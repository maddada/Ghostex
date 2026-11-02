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
#[cfg(target_os = "macos")]
const MAX_CRASH_STACK_FRAMES: usize = 64;
const MAX_CRASH_FRAME_CHARS: usize = 300;
const MAX_CRASH_BACKTRACE_LINES: usize = 160;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuiSupportLog {
    HostLifecycle,
    RemoteGxserverInstall,
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
    ProjectBoard,
    AppModal,
    CrashReports,
    Performance,
}

impl GpuiSupportLog {
    fn file_name(self) -> &'static str {
        match self {
            Self::HostLifecycle => "gpui-host-lifecycle.log",
            Self::RemoteGxserverInstall => "gpui-remote-gxserver-install-debug.log",
            Self::SidebarRefresh => "gpui-sidebar-refresh-debug.log",
            Self::TerminalFocus => "gpui-terminal-focus-debug.log",
            Self::SessionChat => "gpui-session-chat-debug.jsonl",
            Self::ProjectBoard => "gpui-project-board-debug.log",
            Self::AppModal => "gpui-app-modal-debug.jsonl",
            Self::CrashReports => "gpui-crash-reports.log",
            Self::Performance => "gpui-performance.jsonl",
        }
    }

    fn scenario(self) -> Option<GpuiDiagnosticScenario> {
        match self {
            Self::HostLifecycle => Some(GpuiDiagnosticScenario::HostLifecycle),
            Self::RemoteGxserverInstall => Some(GpuiDiagnosticScenario::RemoteGxserverInstall),
            Self::SidebarRefresh => Some(GpuiDiagnosticScenario::SidebarRefresh),
            Self::TerminalFocus => Some(GpuiDiagnosticScenario::TerminalFocus),
            Self::SessionChat => Some(GpuiDiagnosticScenario::SessionChat),
            Self::ProjectBoard => Some(GpuiDiagnosticScenario::ProjectBoard),
            Self::AppModal => Some(GpuiDiagnosticScenario::AppModal),
            // Crash reports are always-on failure diagnostics, and `--profile` is the performance
            // log's own opt-in (`append_profile_sample`).
            Self::CrashReports | Self::Performance => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GpuiDiagnosticScenario {
    HostLifecycle,
    RemoteGxserverInstall,
    SidebarRefresh,
    TerminalFocus,
    SessionChat,
    ProjectBoard,
    AppModal,
}

impl GpuiDiagnosticScenario {
    /// The id the settings file spells this scenario with, which is also what
    /// [`append_for_scenario`] takes for a log whose own scenario is a different one.
    pub fn scenario_id(self) -> &'static str {
        match self {
            Self::HostLifecycle => "native.host.lifecycle",
            Self::RemoteGxserverInstall => "native.remote.gxserver.install",
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
    if let Some(scenario) = log.scenario()
        && !important
        && !scenario_enabled(scenario)
    {
        return;
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

/// Appends one `--profile` timing sample. Launching with `--profile` is this log's only opt-in,
/// so it skips the Settings switches.
pub fn append_profile_sample(details: serde_json::Value) {
    append_unconditionally(
        GpuiSupportLog::Performance,
        "gpui.performance.sample",
        details,
    );
}

fn debug_ui_controls_enabled() -> bool {
    shared_settings::shared_sidebar_settings_snapshot().debugging_mode()
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
    write_entry(log, event, sanitize_json_value(details, 0));
}

/// Writes `details` as given: callers have already made them safe to keep.
fn write_entry(log: GpuiSupportLog, event: &str, details: serde_json::Value) {
    let payload = serde_json::json!({
        "event": sanitize_string_value(event),
        "details": details,
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

/// When this process started, as close to `main` as it can be taken, for launch timings.
static LAUNCHED_AT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Called first thing in `main`.
pub fn note_launch() {
    LAUNCHED_AT.get_or_init(std::time::Instant::now);
}

/// Milliseconds since `note_launch`.
pub fn ms_since_launch() -> u64 {
    LAUNCHED_AT
        .get()
        .map_or(0, |launched| launched.elapsed().as_millis() as u64)
}

pub fn logs_directory() -> PathBuf {
    shared_settings::ghostex_storage_paths().logs_dir.clone()
}

/// CDXC:SessionChat 2026-09-16 SEE-ALSO:
/// server/src/session_chat_send_diagnostics.rs records server refusals and the active screen. This file keeps client failures too, including draft saves and requests that never reach gxserver.
pub fn append_session_chat_send_failure(details: serde_json::Value) {
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITER.lock().unwrap_or_else(|error| error.into_inner());
    let write = || -> std::io::Result<()> {
        let directory = logs_directory();
        fs::create_dir_all(&directory)?;
        let path = directory.join("gpui-session-chat-send-failures.jsonl");
        let line = serde_json::json!({
            "ts": timestamp_now(),
            "event": "sessionChat.sendFailure",
            "details": details,
        })
        .to_string();
        rotate_log_if_needed(&path, line.len() as u64 + 1)?;
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        writeln!(options.open(path)?, "{line}")
    };
    if let Err(error) = write() {
        eprintln!("session chat send diagnostic could not be written: {error}");
        append(
            GpuiSupportLog::SessionChat,
            "sessionChat.sendDiagnosticWriteFailed",
            serde_json::json!({ "error": error.to_string() }),
        );
    }
}

/// Installs the process panic hook writing crash reports to
/// `gpui-crash-reports.log` before delegating to the previous hook. macOS
/// counterpart: NativeCrashDiagnostics; GPUI previously lost panics to stderr.
pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        write_entry(
            GpuiSupportLog::CrashReports,
            "gpui.panic",
            panic_crash_details(
                panic_info
                    .location()
                    .map(|location| (location.file(), location.line())),
                &message,
                &backtrace,
            ),
        );
        previous_hook(panic_info);
    }));
}

/// Full lines kept per `log` call site before its repeats are only counted.
const LOG_RECORD_FULL_LINES_PER_SITE: u64 = 3;

/// CDXC:Diagnostics 2026-10-26 WHY:
/// The app installed no `log` logger, so every `log::error!` from GPUI and the other crates went
/// nowhere; on 2026-10-25 the renderer failed every frame ("missing path intermediate texture")
/// with a frozen window and an empty log. Errors from any crate and warnings from GPUI's own
/// crates (`gpui*` targets: lost GPU device, fonts that fail to load, a11y tree faults) now go to
/// `gpui-host-lifecycle.log` like the host's other unconditional errors. Warnings from the other
/// crates stay off: usvg, html5ever, notify and the like warn about routine malformed input.
/// Info and debug are cut by `log::set_max_level`, so those macros cost one atomic load. A call
/// site writes its first three records in full, then one line with the count at 10, 100,
/// 1,000 and so on, so a per-frame error adds a handful of lines, never a flood. A call site is
/// its owned (target, file, line): `ResultExt::log_err` (gpui_util), GPUI's main error path,
/// builds records whose file and module path are not `'static`, so a key on the static ones
/// lost the file and made `log_err` errors from different files on the same line share a count.
struct SupportLogRecords {
    repeats: std::sync::Mutex<std::collections::HashMap<(String, String, u32), u64>>,
}

impl log::Log for SupportLogRecords {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Error
            || (metadata.level() == log::Level::Warn && metadata.target().starts_with("gpui"))
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let file = record.file().or(record.module_path()).unwrap_or("unknown");
        let line = record.line().unwrap_or(0);
        let occurrence = {
            let mut repeats = self
                .repeats
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let count = repeats
                .entry((record.target().to_owned(), file.to_owned(), line))
                .or_insert(0);
            *count += 1;
            *count
        };
        let repeated = occurrence > LOG_RECORD_FULL_LINES_PER_SITE;
        if repeated && !is_power_of_ten(occurrence) {
            return;
        }
        let level = if record.level() == log::Level::Error {
            "error"
        } else {
            "warning"
        };
        let event = if repeated {
            format!("gpui.log.{level}Repeated")
        } else {
            format!("gpui.log.{level}")
        };
        write_entry(
            GpuiSupportLog::HostLifecycle,
            &event,
            serde_json::json!({
                "target": redact_crash_text(record.target(), MAX_SANITIZED_STRING_CHARS),
                "location": format!("{}:{line}", redact_source_location(file)),
                "message": redact_crash_text(&record.args().to_string(), MAX_CRASH_MESSAGE_CHARS),
                "occurrence": occurrence,
            }),
        );
    }

    fn flush(&self) {}
}

fn is_power_of_ten(mut value: u64) -> bool {
    while value >= 10 && value % 10 == 0 {
        value /= 10;
    }
    value == 1
}

/// Routes `log` records into the support logs (see [`SupportLogRecords`]). Called once at
/// startup, next to [`install_panic_hook`].
pub fn install_log_records_sink() {
    static LOGGER: std::sync::OnceLock<SupportLogRecords> = std::sync::OnceLock::new();
    let logger = LOGGER.get_or_init(|| SupportLogRecords {
        repeats: std::sync::Mutex::new(std::collections::HashMap::new()),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiInstallUncaughtExceptionHandler();
}

/// Installs the uncaught Objective-C exception handler (native/macos/GpuiCrashReports.m), the
/// [`install_panic_hook`] counterpart for exceptions AppKit or other Objective-C code throws
/// and nothing catches. It records them in `gpui-crash-reports.log` and lets the process end.
pub fn install_uncaught_exception_handler() {
    #[cfg(target_os = "macos")]
    unsafe {
        GhostexGpuiInstallUncaughtExceptionHandler()
    };
}

/// Called by GpuiCrashReports.m right before an uncaught Objective-C exception ends the process.
/// The generic sanitizer would replace the reason and every frame whole (each is longer than a
/// bounded id), so this keeps the text and replaces only path-like words.
#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
pub extern "C" fn GhostexGpuiRecordUncaughtException(
    name: *const std::ffi::c_char,
    reason: *const std::ffi::c_char,
    call_stack: *const std::ffi::c_char,
    executable_load_address: u64,
) {
    let text = |value: *const std::ffi::c_char| {
        if value.is_null() {
            String::new()
        } else {
            // SAFETY: GpuiCrashReports.m passes NUL-terminated UTF-8 strings (or null) that live
            // for the duration of this call.
            unsafe { std::ffi::CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned()
        }
    };
    let call_stack: Vec<serde_json::Value> = text(call_stack)
        .lines()
        .take(MAX_CRASH_STACK_FRAMES)
        .map(|frame| redact_crash_text(frame, MAX_CRASH_FRAME_CHARS).into())
        .collect();
    write_entry(
        GpuiSupportLog::CrashReports,
        "gpui.uncaughtException",
        serde_json::json!({
            "name": redact_crash_text(&text(name), MAX_SANITIZED_STRING_CHARS),
            "reason": redact_crash_text(&text(reason), MAX_CRASH_MESSAGE_CHARS),
            "callStack": call_stack,
            "executableLoadAddress": format!("{executable_load_address:#x}"),
            "pid": std::process::id(),
        }),
    );
}

/// CDXC:CrashReports 2026-10-24 WHY:
/// The generic sanitizer replaced the panic message, location and backtrace whole (each holds a
/// slash or runs past 120 characters), so every panic was saved as `[redacted]`. Panics now keep
/// them: path-like words in the message and frames become `[path]`, and source locations keep a
/// relative source path (this repository's crates) but only the file name of an absolute one
/// (dependencies and the standard library, whose paths name the build machine's home folder).
fn panic_crash_details(
    location: Option<(&str, u32)>,
    message: &str,
    backtrace: &str,
) -> serde_json::Value {
    let backtrace: Vec<serde_json::Value> = backtrace
        .lines()
        .take(MAX_CRASH_BACKTRACE_LINES)
        .map(|line| {
            let line = line.trim_end();
            let indent = &line[..line.len() - line.trim_start().len()];
            match line.trim_start().strip_prefix("at ") {
                Some(source) => format!("{indent}at {}", redact_source_location(source)),
                None => redact_crash_text(line, MAX_CRASH_FRAME_CHARS),
            }
            .into()
        })
        .collect();
    serde_json::json!({
        "location": location
            .map(|(file, line)| format!("{}:{line}", redact_source_location(file)))
            .unwrap_or_else(|| "unknown".to_string()),
        "message": redact_crash_text(message, MAX_CRASH_MESSAGE_CHARS),
        "backtrace": backtrace,
        "pid": std::process::id(),
    })
}

/// A source file (optionally followed by `:line:column`): kept when relative, reduced to its file
/// name when absolute.
fn redact_source_location(source: &str) -> String {
    let absolute = source.starts_with(['/', '\\', '~']) || source.as_bytes().get(1) == Some(&b':');
    let kept = if absolute {
        source.rsplit(['/', '\\']).next().unwrap_or_default()
    } else {
        source
    };
    kept.chars()
        .filter(|character| !character.is_control())
        .take(MAX_CRASH_FRAME_CHARS)
        .collect()
}

/// Replaces every whitespace-separated word holding a path separator with `[path]`, turns control
/// characters into spaces and caps the length.
fn redact_crash_text(text: &str, max_chars: usize) -> String {
    let mut redacted = String::with_capacity(text.len());
    for word in text.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        if trimmed.contains('/') || trimmed.contains('\\') {
            redacted.push_str("[path]");
            redacted.push_str(&word[trimmed.len()..]);
        } else {
            redacted.push_str(word);
        }
    }
    redacted
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(max_chars)
        .collect()
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
        GpuiSupportLog::SidebarRefresh,
        GpuiSupportLog::TerminalFocus,
        GpuiSupportLog::ProjectBoard,
        GpuiSupportLog::AppModal,
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

/// The longest a diagnostic area stays on (Settings > Debugging, "Turn logs off after").
const DIAGNOSTIC_AREA_MAX_ON: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Gives every diagnostic area that is on with no end time (the retired "Never" choice, or an
/// older client's) an end [`DIAGNOSTIC_AREA_MAX_ON`] from now, saved through the settings
/// service. Runs with the startup retention pass.
///
/// SEE-ALSO: packages/settings-catalog/src/data/diagnostic_logging.rs (CDXC:Diagnostics 2026-11-03).
pub fn bound_open_ended_diagnostic_areas() {
    let snapshot = shared_settings::shared_sidebar_settings_snapshot();
    let mut object = snapshot.object().clone();
    let Some(scenarios) = object
        .get_mut("diagnosticLogging")
        .and_then(|logging| logging.get_mut("scenarios"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return;
    };
    let expires_at = iso8601_from_epoch_seconds(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(DIAGNOSTIC_AREA_MAX_ON)
            .as_secs() as i64,
    );
    let mut changed = false;
    for state in scenarios.values_mut() {
        let open_ended = match state {
            serde_json::Value::Bool(enabled) => *enabled,
            serde_json::Value::Object(fields) => {
                fields.get("enabled").and_then(serde_json::Value::as_bool) == Some(true)
                    && fields
                        .get("expiresAt")
                        .and_then(serde_json::Value::as_str)
                        .is_none_or(|value| value.trim().is_empty())
            }
            _ => false,
        };
        if open_ended {
            *state = serde_json::json!({ "enabled": true, "expiresAt": expires_at });
            changed = true;
        }
    }
    if changed {
        if let Err(error) = shared_settings::write_shared_sidebar_settings_object(object) {
            log::error!(
                "Could not give the open-ended diagnostic log areas an end time: {error:?}"
            );
        }
    }
}

fn iso8601_from_epoch_seconds(epoch_seconds: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_epoch(epoch_seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000Z")
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

#[cfg(test)]
mod crash_text_tests {
    use super::{panic_crash_details, redact_crash_text};

    #[test]
    fn an_exception_reason_survives_and_paths_do_not() {
        // GitHub #291's reason is longer than the generic sanitizer's 120 characters.
        let reason = "Cannot remove an observer <_NSTouchBarFinderObservation 0x600> for the key path \"nextResponder\" from <NSView 0x700> because it is not registered as an observer.";
        assert_eq!(redact_crash_text(reason, 1_000), reason);
        assert_eq!(
            redact_crash_text("open /Users/someone/notes.txt\tC:\\Users\\x failed", 1_000),
            "open [path] [path] failed"
        );
        assert_eq!(redact_crash_text("abcdef", 3), "abc");
    }

    #[test]
    fn a_panic_keeps_its_message_location_and_backtrace() {
        let message = format!(
            "called `Option::unwrap()` on a `None` value while reading /Users/someone/.ghostex/state.json {}",
            "x".repeat(200)
        );
        let backtrace = "   0: std::backtrace::Backtrace::create\n             at /rustc/abc123/library/std/src/backtrace.rs:331:13\n   1: ghostex_gpui::app::render\n             at ./apps/desktop/src/app/render/root.rs:42:9\n   2: <unknown>";
        let details = panic_crash_details(
            Some(("apps/desktop/src/app/render/root.rs", 42)),
            &message,
            backtrace,
        );
        assert_eq!(
            details["location"],
            "apps/desktop/src/app/render/root.rs:42"
        );
        let saved = details["message"].as_str().unwrap_or_default();
        assert!(
            saved.starts_with(
                "called `Option::unwrap()` on a `None` value while reading [path] xxx"
            )
        );
        assert!(!saved.contains("someone"));
        assert_eq!(
            details["backtrace"],
            serde_json::json!([
                "   0: std::backtrace::Backtrace::create",
                "             at backtrace.rs:331:13",
                "   1: ghostex_gpui::app::render",
                "             at ./apps/desktop/src/app/render/root.rs:42:9",
                "   2: <unknown>",
            ])
        );
        let dependency = panic_crash_details(
            Some((
                "/Users/someone/.cargo/registry/src/gpui-0.2/src/window.rs",
                7,
            )),
            "boom",
            "",
        );
        assert_eq!(dependency["location"], "window.rs:7");
        assert_eq!(panic_crash_details(None, "boom", "")["location"], "unknown");
    }
}
