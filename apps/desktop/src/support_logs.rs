//! GPUI support logging under the resolved Ghostex logs directory, porting the macOS writer
//! conventions (SidebarRefreshDebugLog.swift and siblings):
//!
//! - JSON-line entries `[timestamp] {"event":..., "details":...}`.
//! - Routine breadcrumbs require the shared `debuggingMode` ("Show debug UI
//!   controls") gate and the matching `diagnosticLogging.scenarios` entry.
//!   Warning/error/failure-like events and crash reports always persist.
//! - 25 MB per-file cap with three rotations, plus a one-minute-after-startup
//!   retention pass trimming each retained file to 25,000 lines.
//! - Every GPUI file is `gpui-*`-prefixed so the macOS app's writers in the
//!   same shared logs directory are never rotated or pruned by this process.
//!
//! Privacy: callers may pass only bounded ids, enums, counts, and durations.
//! The writer additionally redacts path-like or long string values so no
//! paths, titles, commands, URLs, tokens, or terminal content can persist.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::shared_settings;

const MAX_LOG_FILE_BYTES: u64 = 25 * 1024 * 1024;
const MAX_ROTATED_LOG_FILES: u32 = 3;
const MAX_RETAINED_LOG_LINES: usize = 25_000;
pub const RETENTION_STARTUP_DELAY: Duration = Duration::from_secs(60);
const MAX_SANITIZED_STRING_CHARS: usize = 120;
const MAX_CRASH_MESSAGE_CHARS: usize = 1_000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuiSupportLog {
    HostLifecycle,
    RemoteGxserverInstall,
    SidebarRenderer,
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
    ProjectBoard,
    AppModal,
    TitlebarPopupRepro,
    CrashReports,
}

impl GpuiSupportLog {
    fn file_name(self) -> &'static str {
        match self {
            Self::HostLifecycle => "gpui-host-lifecycle.log",
            Self::RemoteGxserverInstall => "gpui-remote-gxserver-install-debug.log",
            Self::SidebarRenderer => "gpui-sidebar-renderer-debug.jsonl",
            Self::SidebarRefresh => "gpui-sidebar-refresh-debug.log",
            Self::TerminalFocus => "gpui-terminal-focus-debug.log",
            Self::SessionChat => "gpui-session-chat-debug.jsonl",
            Self::ProjectBoard => "gpui-project-board-debug.log",
            // Filename from the shared `gpui.app.modal` scenario definition
            // (shared/ghostex-settings.ts logFiles).
            Self::AppModal => "gpui-app-modal-debug.jsonl",
            Self::TitlebarPopupRepro => "gpui-titlebar-popup-repro.jsonl",
            Self::CrashReports => "gpui-crash-reports.log",
        }
    }

    fn scenario(self) -> Option<GpuiDiagnosticScenario> {
        match self {
            Self::HostLifecycle => Some(GpuiDiagnosticScenario::HostLifecycle),
            Self::RemoteGxserverInstall => Some(GpuiDiagnosticScenario::RemoteGxserverInstall),
            Self::SidebarRenderer => Some(GpuiDiagnosticScenario::SidebarRenderer),
            Self::SidebarRefresh => Some(GpuiDiagnosticScenario::SidebarRefresh),
            Self::TerminalFocus => Some(GpuiDiagnosticScenario::TerminalFocus),
            Self::SessionChat => Some(GpuiDiagnosticScenario::SessionChat),
            Self::ProjectBoard => Some(GpuiDiagnosticScenario::ProjectBoard),
            Self::AppModal => Some(GpuiDiagnosticScenario::AppModal),
            Self::TitlebarPopupRepro => None,
            // Crash reports are always-on failure diagnostics.
            Self::CrashReports => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuiDiagnosticScenario {
    HostLifecycle,
    RemoteGxserverInstall,
    SidebarRenderer,
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
    ProjectBoard,
    AppModal,
}

impl GpuiDiagnosticScenario {
    fn scenario_id(self) -> &'static str {
        match self {
            Self::HostLifecycle => "native.host.lifecycle",
            Self::RemoteGxserverInstall => "native.remote.gxserver.install",
            Self::SidebarRenderer => "gpui.sidebar.renderer",
            Self::SidebarRefresh => "native.sidebar.refresh",
            Self::TerminalFocus => "native.terminal.focus",
            Self::SessionChat => "gpui.sessionChat.viewState",
            Self::ProjectBoard => "native.project.board",
            Self::AppModal => "gpui.app.modal",
        }
    }
}

pub fn scenario_enabled(scenario: GpuiDiagnosticScenario) -> bool {
    scenario_id_enabled(scenario.scenario_id())
}

pub fn scenario_id_enabled(scenario_id: &str) -> bool {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    let Some(state) = settings
        .object()
        .get("diagnosticLogging")
        .and_then(serde_json::Value::as_object)
        .and_then(|logging| logging.get("scenarios"))
        .and_then(serde_json::Value::as_object)
        .and_then(|scenarios| scenarios.get(scenario_id))
    else {
        return false;
    };
    if let Some(enabled) = state.as_bool() {
        return enabled;
    }
    let Some(state) = state.as_object() else {
        return false;
    };
    if state.get("enabled").and_then(serde_json::Value::as_bool) != Some(true) {
        return false;
    }
    let Some(expires_at) = state
        .get("expiresAt")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return true;
    };
    iso8601_to_epoch_seconds(expires_at).is_some_and(|expiry| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .is_ok_and(|now| (now.as_secs() as i64) < expiry)
    })
}

/// Appends a support event. Routine breadcrumbs require both "Show debug UI
/// controls" and the log's scenario; important diagnostics bypass both gates.
pub fn append(log: GpuiSupportLog, event: &str, details: serde_json::Value) {
    let important = is_important_diagnostic(log, event, &details);
    if !important && !debug_ui_controls_enabled() {
        return;
    }
    if let Some(scenario) = log.scenario() {
        if !important && !scenario_enabled(scenario) {
            return;
        }
    }
    append_unconditionally(log, event, details);
}

/// Appends a routine event whose scenario is selected by its caller. Missing,
/// disabled, or expired scenarios never write; important diagnostics remain
/// unconditional.
pub fn append_for_scenario(
    log: GpuiSupportLog,
    scenario_id: &str,
    event: &str,
    details: serde_json::Value,
) {
    let important = is_important_diagnostic(log, event, &details);
    if !important && (!debug_ui_controls_enabled() || !scenario_id_enabled(scenario_id)) {
        return;
    }
    append_unconditionally(log, event, details);
}

/// Privacy-shaped temporary instrumentation. Routine temporary events follow
/// the same explicit scenario as their destination log.
pub fn append_temporary(log: GpuiSupportLog, event: &str, details: serde_json::Value) {
    append(log, event, details);
}

/// Always-on, privacy-sanitized breadcrumbs for a user-requested reproduction.
/// Keep callers narrowly scoped and remove them after the underlying issue is
/// diagnosed; unlike routine support logs these do not depend on Debugging Mode.
#[allow(dead_code)] // no live caller: its only caller is the superseded titlebar popup repro logger
pub fn append_repro(log: GpuiSupportLog, event: &str, details: serde_json::Value) {
    append_unconditionally(log, event, details);
}

/// TEMPORARY latency instrumentation helper: wall-clock milliseconds since the
/// UNIX epoch, so the `TEMP.gpui.sessionSwitchLatency.*` markers can be ordered
/// against each other and against any other timestamped log. Remove with them.
pub fn temporary_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_millis() as u64)
        .unwrap_or_default()
}

fn debug_ui_controls_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
}

pub fn temporary_fluid_voice_text_shape(text: &str) -> serde_json::Value {
    serde_json::json!({
        "asciiLowercaseAOnly": !text.is_empty() && text.bytes().all(|byte| byte == b'a'),
        "byteLength": text.len(),
        "containsLineBreak": text.contains(['\r', '\n']),
        "containsNonAscii": !text.is_ascii(),
        "containsWhitespace": text.chars().any(char::is_whitespace),
        "unicodeScalarCount": text.chars().count(),
        "utf16CodeUnitCount": text.encode_utf16().count(),
    })
}

pub fn temporary_fluid_voice_bytes_shape(bytes: &[u8]) -> serde_json::Value {
    let utf8 = std::str::from_utf8(bytes).ok();
    serde_json::json!({
        "asciiLowercaseAOnly": !bytes.is_empty() && bytes.iter().all(|byte| *byte == b'a'),
        "byteLength": bytes.len(),
        "containsLineBreak": bytes.iter().any(|byte| matches!(*byte, b'\r' | b'\n')),
        "containsNonAscii": !bytes.is_ascii(),
        "containsWhitespace": utf8.is_some_and(|text| text.chars().any(char::is_whitespace)),
        "unicodeScalarCount": utf8.map(|text| text.chars().count()),
        "utf16CodeUnitCount": utf8.map(|text| text.encode_utf16().count()),
        "utf8Valid": utf8.is_some(),
    })
}

fn is_important_diagnostic(log: GpuiSupportLog, event: &str, details: &serde_json::Value) -> bool {
    log == GpuiSupportLog::CrashReports
        || event_is_important_diagnostic(event)
        || details_contain_important_diagnostic(details)
}

/// Warning/error/failure events persist without either routine-log gate.
fn event_is_important_diagnostic(event: &str) -> bool {
    let lowered = event.to_ascii_lowercase();
    [
        "error", "fail", "warning", "crash", "abort", "fatal", "panic",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn details_contain_important_diagnostic(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            let diagnostic_key = [
                "error", "fail", "warning", "crash", "abort", "fatal", "panic",
            ]
            .iter()
            .any(|marker| key.contains(marker));
            let diagnostic_level = matches!(key.as_str(), "level" | "severity" | "status")
                && value.as_str().is_some_and(event_is_important_diagnostic);
            (diagnostic_key && diagnostic_value_present(value))
                || diagnostic_level
                || details_contain_important_diagnostic(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(details_contain_important_diagnostic),
        _ => false,
    }
}

fn diagnostic_value_present(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        serde_json::Value::String(value) => {
            let value = value.trim();
            !value.is_empty()
                && !matches!(
                    value.to_ascii_lowercase().as_str(),
                    "0" | "false" | "none" | "ok" | "passed" | "success"
                )
        }
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
    }
}

fn append_unconditionally(log: GpuiSupportLog, event: &str, details: serde_json::Value) {
    let payload = serde_json::json!({
        "event": sanitize_string_value(event),
        "details": sanitize_json_value(details, 0),
    });
    let line = format!("[{}] {}\n", timestamp_now(), payload);
    let logs_directory = logs_directory();
    if fs::create_dir_all(&logs_directory).is_err() {
        return;
    }
    let log_path = logs_directory.join(log.file_name());
    let _ = rotate_log_if_needed(&log_path, line.len() as u64);
    let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    else {
        return;
    };
    let _ = file.write_all(line.as_bytes());
}

pub fn logs_directory() -> PathBuf {
    shared_settings::ghostex_storage_paths().logs_dir.clone()
}

/// Installs the process panic hook writing crash reports to
/// `gpui-crash-reports.log` before delegating to the previous hook. macOS
/// counterpart: NativeCrashDiagnostics; GPUI previously lost panics to stderr.
pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        let message: String = message.chars().take(MAX_CRASH_MESSAGE_CHARS).collect();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        // Crash payloads carry code locations and developer-authored panic
        // text plus frame symbols; user paths inside frames are redacted by
        // keeping only the first frames and capping total size.
        let backtrace: String = backtrace.chars().take(8_000).collect();
        append_unconditionally(
            GpuiSupportLog::CrashReports,
            "gpui.panic",
            serde_json::json!({
                "location": location,
                "message": message,
                "backtrace": backtrace,
                "pid": std::process::id(),
            }),
        );
        previous_hook(panic_info);
    }));
}

/// One-minute-after-startup retention pass (macOS
/// `scheduleSupportLogLineRetentionAfterStartup` parity), scoped strictly to
/// this app's `gpui-*` files so the macOS app's writers in the same shared
/// directory are untouched.
pub fn prune_gpui_support_logs() {
    let logs_directory = logs_directory();
    for log in [
        GpuiSupportLog::HostLifecycle,
        GpuiSupportLog::RemoteGxserverInstall,
        GpuiSupportLog::SidebarRenderer,
        GpuiSupportLog::SidebarRefresh,
        GpuiSupportLog::TerminalFocus,
        GpuiSupportLog::ProjectBoard,
        GpuiSupportLog::AppModal,
        GpuiSupportLog::TitlebarPopupRepro,
        GpuiSupportLog::CrashReports,
    ] {
        let retained = logs_directory.join(log.file_name());
        for index in 1..=MAX_ROTATED_LOG_FILES {
            let rotated = rotated_log_path(&retained, index);
            if rotated.exists() {
                let _ = fs::remove_file(rotated);
            }
        }
        let _ = trim_log_to_retained_lines(&retained, MAX_RETAINED_LOG_LINES);
    }
}

fn trim_log_to_retained_lines(path: &Path, max_lines: usize) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let contents = fs::read_to_string(path)?;
    let lines: Vec<&str> = contents.lines().collect();
    if lines.len() <= max_lines {
        return Ok(());
    }
    let retained = lines[lines.len() - max_lines..].join("\n");
    fs::write(path, format!("{retained}\n"))
}

fn rotate_log_if_needed(path: &Path, incoming_bytes: u64) -> std::io::Result<()> {
    let size = fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if size + incoming_bytes <= MAX_LOG_FILE_BYTES {
        return Ok(());
    }
    let oldest = rotated_log_path(path, MAX_ROTATED_LOG_FILES);
    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }
    for index in (1..MAX_ROTATED_LOG_FILES).rev() {
        let source = rotated_log_path(path, index);
        if source.exists() {
            fs::rename(&source, rotated_log_path(path, index + 1))?;
        }
    }
    let first = rotated_log_path(path, 1);
    if first.exists() {
        fs::remove_file(&first)?;
    }
    fs::rename(path, first)
}

fn rotated_log_path(path: &Path, index: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!("{file_name}.{index}"))
}

/// Conservative sanitizer at the writer boundary: strings that look path-like
/// or exceed the bounded-id length are redacted; numbers/bools pass through;
/// arrays/objects recurse with depth and size caps.
fn sanitize_json_value(value: serde_json::Value, depth: usize) -> serde_json::Value {
    if depth > 4 {
        return serde_json::Value::String("[depth-capped]".to_string());
    }
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(sanitize_string_value(&text)),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .take(32)
                .map(|item| sanitize_json_value(item, depth + 1))
                .collect(),
        ),
        serde_json::Value::Object(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .take(32)
                .map(|(key, item)| {
                    (
                        sanitize_string_value(&key),
                        sanitize_json_value(item, depth + 1),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

fn sanitize_string_value(text: &str) -> String {
    if text.chars().count() > MAX_SANITIZED_STRING_CHARS
        || text.contains('/')
        || text.contains('\\')
        || text.chars().any(char::is_control)
    {
        return "[redacted]".to_string();
    }
    text.to_string()
}

fn timestamp_now() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = duration.subsec_millis();
    let (year, month, day, hour, minute, second) = civil_from_epoch(duration.as_secs() as i64);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{millis:03} +0000")
}

fn iso8601_to_epoch_seconds(value: &str) -> Option<i64> {
    // Accepts the ISO-8601 UTC forms shared settings persist
    // (yyyy-MM-ddTHH:mm:ss[.SSS]Z). Non-UTC offsets fail closed (scenario off).
    let value = value.trim();
    let rest = value.strip_suffix('Z')?;
    let (date, time) = rest.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let time = time.split('.').next()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Howard Hinnant's civil-days algorithm.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_epoch(epoch_seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = epoch_seconds.div_euclid(86_400);
    let seconds_of_day = epoch_seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (
        year,
        month,
        day,
        (seconds_of_day / 3_600) as u32,
        ((seconds_of_day % 3_600) / 60) as u32,
        (seconds_of_day % 60) as u32,
    )
}
