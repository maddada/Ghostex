use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;

const LOG_FILE_MAX_BYTES: u64 = 25 * 1024 * 1024;
const LOG_FILE_MAX_ROTATIONS: usize = 3;
const LOG_FILE_MAX_LINES: usize = 25_000;
const LOG_RETENTION_STARTUP_DELAY_MS: u64 = 60_000;
const DEBUGGING_MODE_CACHE_MS: u64 = 1_000;
const DEFAULT_LOG_QUERY_LIMIT: usize = 200;
const MAX_LOG_QUERY_LIMIT: usize = 5_000;
const LOG_QUERY_FULL_SCAN_MAX_BYTES: u64 = 8 * 1024 * 1024;
const LOG_QUERY_WINDOW_BASE_BYTES: u64 = 2 * 1024 * 1024;
const LOG_QUERY_MAX_WINDOW_BYTES: u64 = 16 * 1024 * 1024;
const LOG_QUERY_ESTIMATED_BYTES_PER_ENTRY: u64 = 1024;
static SCHEDULED_RETENTION_LOG_FILES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static LOG_FILE_WRITE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLogScenario {
    SessionChatDrafts,
    AgentActivity,
    AgentDetection,
    ApiRequests,
    Portless,
    RepositoryClone,
    ServerLifecycle,
    TerminalFocus,
    TypedOperations,
}

impl DiagnosticLogScenario {
    pub fn id(self) -> &'static str {
        match self {
            Self::SessionChatDrafts => "gpui.sessionChat.drafts",
            Self::AgentActivity => "gxserver.agentActivity",
            Self::AgentDetection => "native.agent.detection",
            Self::ApiRequests => "gxserver.requests",
            Self::Portless => "gxserver.portless",
            Self::RepositoryClone => "gxserver.repositoryClone",
            Self::ServerLifecycle => "gxserver.lifecycle",
            Self::TerminalFocus => "native.terminal.focus",
            Self::TypedOperations => "gxserver.typedOperations",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GxserverLogInput {
    pub level: LogLevel,
    pub event: String,
    pub server_id: Option<String>,
    pub request_id: Option<String>,
    pub client: Option<String>,
    pub duration_ms: Option<u128>,
    pub error: Option<String>,
    pub details: Option<Value>,
}

pub struct GxserverLogger {
    paths: GxserverPaths,
    debugging_mode_cache: Mutex<DebuggingModeCache>,
}

#[derive(Clone, Copy, Debug)]
pub struct LogRetentionOptions {
    pub delay_ms: u64,
    pub max_lines: usize,
}

impl Default for LogRetentionOptions {
    fn default() -> Self {
        Self {
            delay_ms: LOG_RETENTION_STARTUP_DELAY_MS,
            max_lines: LOG_FILE_MAX_LINES,
        }
    }
}

#[derive(Debug)]
struct DebuggingModeCache {
    checked_at: Instant,
    debugging_mode: bool,
    enabled_scenarios: HashSet<String>,
}

/*
CDXC:Diagnostics 2026-06-14-20:37:
Persistent Rust logs must be safe for support bundles. Persist explicit warn/error entries and failure-like structured diagnostics unconditionally. Every routine entry must use `log_routine` with an explicit diagnostic scenario, and writes require both Debugging Mode and that unexpired scenario. Rotate before append at the TypeScript size/count, and sanitize at the JSONL writer boundary so future call sites cannot leak paths, URLs, command text, stdout/stderr, tokens, or user-owned names.

CDXC:Diagnostics 2026-06-19-14:45:
Rust logger startup must match TypeScript support-bundle retention: schedule a one-minute delayed cleanup, keep only the active or newest gxserver JSONL split file, delete older rotations, and trim the retained file to 25,000 lines without logging cleanup failures back into the same file.

CDXC:Diagnostics 2026-06-19-18:44:
Retention rewrites the retained JSONL file, so append, rotation, and prune must share a per-log-file writer lock. Do not replace this with stale temp-and-rename pruning unless concurrent appends are blocked or merged before the rewrite commits.

CDXC:Diagnostics 2026-06-22-09:57:
Area 36 privacy review requires persistent server logs to stay metadata-only even when future call sites accidentally pass prompts, environment maps, or uppercase-scheme URLs through structured details. Keep those redactions at the JSONL writer boundary so hook, clone, typed-operation, and lifecycle diagnostics cannot persist user content.
*/
impl GxserverLogger {
    pub fn new(paths: GxserverPaths) -> Self {
        Self::new_with_retention(paths, LogRetentionOptions::default())
    }

    fn new_with_retention(paths: GxserverPaths, retention: LogRetentionOptions) -> Self {
        schedule_gxserver_log_line_retention(&paths, retention);
        Self {
            paths,
            debugging_mode_cache: Mutex::new(DebuggingModeCache {
                checked_at: Instant::now() - Duration::from_millis(DEBUGGING_MODE_CACHE_MS),
                debugging_mode: false,
                enabled_scenarios: HashSet::new(),
            }),
        }
    }

    /// Persists only important diagnostics. Routine callers must use
    /// `log_routine` so an explicit scenario is impossible to forget silently.
    pub fn log(&self, entry: GxserverLogInput) -> Result<()> {
        if !Self::is_important(&entry) {
            return Ok(());
        }
        self.persist(entry)
    }

    pub fn log_routine(
        &self,
        scenario: DiagnosticLogScenario,
        entry: GxserverLogInput,
    ) -> Result<()> {
        if !Self::is_important(&entry) && !self.routine_logging_enabled(scenario) {
            return Ok(());
        }
        self.persist(entry)
    }

    fn persist(&self, entry: GxserverLogInput) -> Result<()> {
        fs::create_dir_all(&self.paths.logs_dir)
            .with_context(|| "create gxserver logs directory")?;
        let line = serde_json::to_string(&normalize_log_entry(entry))?;
        write_gxserver_log_line(&self.paths, &line)
    }

    fn is_important(entry: &GxserverLogInput) -> bool {
        matches!(entry.level, LogLevel::Warn | LogLevel::Error)
            || entry
                .error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty())
            || text_is_important_diagnostic(&entry.event)
            || entry
                .details
                .as_ref()
                .is_some_and(value_contains_important_diagnostic)
    }

    fn routine_logging_enabled(&self, scenario: DiagnosticLogScenario) -> bool {
        let mut cache = self
            .debugging_mode_cache
            .lock()
            .expect("debug cache poisoned");
        if cache.checked_at.elapsed() < Duration::from_millis(DEBUGGING_MODE_CACHE_MS) {
            return cache.debugging_mode && cache.enabled_scenarios.contains(scenario.id());
        }
        let settings = read_diagnostic_logging_settings_file(&self.paths);
        cache.checked_at = Instant::now();
        cache.debugging_mode = settings.debugging_mode;
        cache.enabled_scenarios = settings.enabled_scenarios;
        cache.debugging_mode && cache.enabled_scenarios.contains(scenario.id())
    }
}

fn text_is_important_diagnostic(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "error", "fail", "warning", "crash", "abort", "fatal", "panic",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn value_contains_important_diagnostic(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            let diagnostic_key = [
                "error", "fail", "warning", "crash", "abort", "fatal", "panic",
            ]
            .iter()
            .any(|marker| key.contains(marker));
            let diagnostic_level = matches!(key.as_str(), "level" | "severity" | "status")
                && value.as_str().is_some_and(text_is_important_diagnostic);
            (diagnostic_key && diagnostic_value_present(value))
                || diagnostic_level
                || value_contains_important_diagnostic(value)
        }),
        Value::Array(values) => values.iter().any(value_contains_important_diagnostic),
        _ => false,
    }
}

fn diagnostic_value_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => {
            let value = value.trim();
            !value.is_empty()
                && !matches!(
                    value.to_ascii_lowercase().as_str(),
                    "0" | "false" | "none" | "ok" | "passed" | "success"
                )
        }
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
    }
}

pub fn schedule_gxserver_log_line_retention(paths: &GxserverPaths, options: LogRetentionOptions) {
    let schedule_key = format!("{}:{}", paths.log_file.display(), options.max_lines);
    let scheduled = SCHEDULED_RETENTION_LOG_FILES.get_or_init(|| Mutex::new(HashSet::new()));
    {
        let mut guard = scheduled
            .lock()
            .expect("gxserver log retention schedule poisoned");
        if !guard.insert(schedule_key) {
            return;
        }
    }
    let paths = paths.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(options.delay_ms));
        let _ = prune_gxserver_log_lines(&paths, options.max_lines);
    });
}

pub fn prune_gxserver_log_lines(paths: &GxserverPaths, max_lines: usize) -> Result<()> {
    prune_gxserver_log_lines_with_before_rewrite(paths, max_lines, || {})
}

fn prune_gxserver_log_lines_with_before_rewrite(
    paths: &GxserverPaths,
    max_lines: usize,
    before_rewrite: impl FnOnce(),
) -> Result<()> {
    let write_lock = log_file_write_lock(&paths.log_file);
    let _write_guard = write_lock.lock().expect("gxserver log writer poisoned");
    let log_files = gxserver_log_files(&paths.log_file);
    let Some(retained_log_file) = retained_gxserver_log_file(&paths.log_file, &log_files)? else {
        return Ok(());
    };
    for log_file in log_files
        .into_iter()
        .filter(|log_file| log_file != &retained_log_file)
    {
        match fs::remove_file(&log_file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| "prune gxserver log rotations"),
        }
    }
    prune_log_file_to_max_lines_with_before_rewrite(&retained_log_file, max_lines, before_rewrite)
}

pub fn log_level_from_status(status: u16) -> LogLevel {
    if status >= 500 {
        LogLevel::Error
    } else if status >= 400 {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

#[derive(Debug)]
pub enum LogQueryError {
    Input(String),
    Io(io::Error),
}

impl From<io::Error> for LogQueryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogQueryOrder {
    Asc,
    Desc,
}

struct LogQueryParams {
    client: Option<String>,
    event: Option<String>,
    event_prefix: Option<String>,
    level: Option<Vec<String>>,
    limit: Option<usize>,
    order: Option<LogQueryOrder>,
    project_id: Option<String>,
    server_id: Option<String>,
    session_id: Option<String>,
    since: Option<String>,
    since_ms: Option<i64>,
    until_ms: Option<i64>,
}

struct LogLineScan {
    entries: Vec<Value>,
    malformed_line_count: usize,
    scanned_line_count: usize,
}

struct LogQueryRead {
    entries: Vec<Value>,
    malformed_line_count: usize,
    scanned_line_count: usize,
    complete: bool,
    scanned_bytes: u64,
}

/*
CDXC:Diagnostics 2026-06-19-14:45:
`/api/queryLogs` is a read-only local support API over the resolved Ghostex gxserver log. Match the TypeScript request filters, default/maximum limits, reverse/order aliasing, malformed-line tolerance, and bounded head/tail scanning so clients do not scrape support logs directly.
*/
pub fn query_gxserver_logs(
    paths: &GxserverPaths,
    raw_params: &Map<String, Value>,
) -> std::result::Result<Value, LogQueryError> {
    let params = parse_query_logs_params(raw_params)?;
    let limit = params.limit.unwrap_or(DEFAULT_LOG_QUERY_LIMIT);
    let order = params.order.unwrap_or(LogQueryOrder::Asc);
    let Some(file_size_bytes) = read_log_file_size(&paths.log_file)? else {
        return Ok(json!({
            "entries": [],
            "logFileSizeBytes": 0,
            "malformedLineCount": 0,
            "malformedLineCountIsExact": true,
            "scannedBytes": 0,
            "scannedLineCount": 0,
            "totalMatched": 0,
            "totalMatchedIsExact": true,
            "truncated": false,
        }));
    };

    let read = if file_size_bytes <= LOG_QUERY_FULL_SCAN_MAX_BYTES {
        read_complete_gxserver_log_entries(&paths.log_file, &params, file_size_bytes)?
    } else {
        read_bounded_gxserver_log_entries(&paths.log_file, &params, file_size_bytes, limit, order)?
    };
    let total_matched = read.entries.len();
    let entries = order_entries(read.entries, order)
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    let mut result = Map::new();
    result.insert("entries".to_string(), Value::Array(entries));
    result.insert("logFileSizeBytes".to_string(), json!(file_size_bytes));
    result.insert(
        "malformedLineCount".to_string(),
        json!(read.malformed_line_count),
    );
    result.insert(
        "malformedLineCountIsExact".to_string(),
        json!(read.complete),
    );
    result.insert("scannedBytes".to_string(), json!(read.scanned_bytes));
    result.insert(
        "scannedLineCount".to_string(),
        json!(read.scanned_line_count),
    );
    result.insert("totalMatched".to_string(), json!(total_matched));
    result.insert("totalMatchedIsExact".to_string(), json!(read.complete));
    result.insert("truncated".to_string(), json!(!read.complete));
    if !read.complete {
        result.insert("truncatedReason".to_string(), json!("fileWindowExceeded"));
    }
    Ok(Value::Object(result))
}

fn normalize_log_entry(entry: GxserverLogInput) -> Value {
    let mut object = Map::new();
    object.insert(
        "ts".to_string(),
        json!(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    );
    object.insert("level".to_string(), json!(entry.level));
    object.insert("event".to_string(), json!(sanitize_log_text(&entry.event)));
    if let Some(server_id) = entry.server_id {
        object.insert("serverId".to_string(), json!(server_id));
    }
    if let Some(request_id) = entry.request_id {
        object.insert("requestId".to_string(), json!(request_id));
    }
    if let Some(client) = entry.client {
        object.insert("client".to_string(), json!(sanitize_log_text(&client)));
    }
    if let Some(duration_ms) = entry.duration_ms {
        object.insert("durationMs".to_string(), json!(duration_ms));
    }
    if let Some(error) = entry.error {
        object.insert("error".to_string(), json!(sanitize_log_text(&error)));
    }
    if let Some(details) = entry.details {
        object.insert(
            "details".to_string(),
            sanitize_log_value("details", details),
        );
    }
    Value::Object(object)
}

fn sanitize_log_value(key: &str, value: Value) -> Value {
    let key = key.to_ascii_lowercase();
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => value,
        Value::String(text) => sanitize_string_field(&key, &text),
        Value::Array(items) => {
            if is_environment_key(&key) || is_sensitive_collection_key(&key) {
                json!({ "count": items.len(), "redacted": true })
            } else {
                Value::Array(
                    items
                        .into_iter()
                        .map(|item| sanitize_log_value(&key, item))
                        .collect(),
                )
            }
        }
        Value::Object(object) => {
            if is_environment_key(&key) || is_sensitive_collection_key(&key) {
                json!({ "redacted": true })
            } else {
                Value::Object(
                    object
                        .into_iter()
                        .map(|(entry_key, entry_value)| {
                            let sanitized = sanitize_log_value(&entry_key, entry_value);
                            (entry_key, sanitized)
                        })
                        .collect(),
                )
            }
        }
    }
}

fn sanitize_string_field(key: &str, value: &str) -> Value {
    if is_secret_key(key) {
        return json!("[redacted:secret]");
    }
    if is_environment_key(key) {
        return json!("[redacted]");
    }
    if is_identifier_key(key) && is_safe_identifier(value) {
        return json!(value);
    }
    if is_url_key(key) || looks_like_url(value) {
        return summarize_url(value);
    }
    if is_path_key(key) || looks_like_path(value) {
        return json!("[redacted:path]");
    }
    if is_sensitive_text_key(key) {
        return json!("[redacted]");
    }
    json!(sanitize_log_text(value))
}

fn sanitize_log_text(value: &str) -> String {
    let value = redact_json_string_fields(value);
    let value = redact_urls(&value);
    let value = redact_paths(&value);
    redact_secret_tokens(&value)
}

fn redact_json_string_fields(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut scan = 0;
    let mut copied_until = 0;
    while let Some(key_start) = find_from(value, '"', scan) {
        let Some(key_end) = find_unescaped_quote(value, key_start + 1) else {
            break;
        };
        let key = &value[key_start + 1..key_end];
        let mut cursor = skip_ascii_whitespace(value, key_end + 1);
        if value.as_bytes().get(cursor) != Some(&b':') {
            scan = key_end + 1;
            continue;
        }
        cursor = skip_ascii_whitespace(value, cursor + 1);
        if value.as_bytes().get(cursor) != Some(&b'"') {
            scan = cursor;
            continue;
        }
        let value_start = cursor + 1;
        let Some(value_end) = find_unescaped_quote(value, value_start) else {
            break;
        };
        let normalized_key = key.to_ascii_lowercase();
        if is_secret_key(&normalized_key)
            || is_url_key(&normalized_key)
            || is_path_key(&normalized_key)
            || is_sensitive_text_key(&normalized_key)
        {
            output.push_str(&value[copied_until..value_start]);
            output.push_str(redaction_for_key(&normalized_key));
            copied_until = value_end;
        }
        scan = value_end + 1;
    }
    output.push_str(&value[copied_until..]);
    output
}

fn redact_urls(value: &str) -> String {
    redact_matching_segments(value, &["http://", "https://"], "[redacted:url]", true)
}

fn redact_paths(value: &str) -> String {
    redact_matching_segments(
        value,
        &[
            "~/",
            "/Users/",
            "/Volumes/",
            "/private/",
            "/tmp/",
            "/var/folders/",
        ],
        "[redacted:path]",
        false,
    )
}

fn redact_matching_segments(
    value: &str,
    prefixes: &[&str],
    replacement: &str,
    case_insensitive: bool,
) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let next = prefixes
            .iter()
            .filter_map(|prefix| {
                find_prefix(&value[cursor..], prefix, case_insensitive)
                    .map(|index| (cursor + index, *prefix))
            })
            .min_by_key(|(index, _)| *index);
        let Some((start, prefix)) = next else {
            output.push_str(&value[cursor..]);
            break;
        };
        output.push_str(&value[cursor..start]);
        output.push_str(replacement);
        cursor = segment_end(value, start + prefix.len());
    }
    output
}

fn find_prefix(value: &str, prefix: &str, case_insensitive: bool) -> Option<usize> {
    if case_insensitive {
        value.to_ascii_lowercase().find(prefix)
    } else {
        value.find(prefix)
    }
}

fn segment_end(value: &str, start: usize) -> usize {
    for (offset, character) in value[start..].char_indices() {
        if character.is_whitespace()
            || matches!(character, '"' | '\'' | ')' | '(' | ']' | '[' | '}')
        {
            return start + offset;
        }
    }
    value.len()
}

fn redact_secret_tokens(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for word in value.split_whitespace() {
        let replacement = if contains_secret_marker(word) {
            "[redacted:secret]"
        } else {
            word
        };
        if !sanitized.is_empty() {
            sanitized.push(' ');
        }
        sanitized.push_str(replacement);
    }
    sanitized
}

fn find_from(value: &str, needle: char, start: usize) -> Option<usize> {
    value[start..].find(needle).map(|index| start + index)
}

fn find_unescaped_quote(value: &str, start: usize) -> Option<usize> {
    let mut escaped = false;
    for (offset, byte) in value.as_bytes()[start..].iter().enumerate() {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some(start + offset);
        }
    }
    None
}

fn skip_ascii_whitespace(value: &str, mut cursor: usize) -> usize {
    while value
        .as_bytes()
        .get(cursor)
        .map(|byte| byte.is_ascii_whitespace())
        .unwrap_or(false)
    {
        cursor += 1;
    }
    cursor
}

fn redaction_for_key(key: &str) -> &'static str {
    if is_secret_key(key) {
        "[redacted:secret]"
    } else if is_url_key(key) {
        "[redacted:url]"
    } else if is_path_key(key) {
        "[redacted:path]"
    } else {
        "[redacted]"
    }
}

fn summarize_url(value: &str) -> Value {
    match url::Url::parse(value) {
        Ok(url) => json!({
            "host": url.host_str().unwrap_or_default(),
            "protocol": url.scheme(),
            "redacted": true,
            "type": "url",
        }),
        Err(_) => json!({ "redacted": true, "type": "url" }),
    }
}

#[derive(Default)]
struct DiagnosticLoggingSettingsSnapshot {
    debugging_mode: bool,
    enabled_scenarios: HashSet<String>,
}

fn read_diagnostic_logging_settings_file(
    paths: &GxserverPaths,
) -> DiagnosticLoggingSettingsSnapshot {
    let settings_path = paths.app_config_dir.join("native-sidebar-settings.json");
    let Ok(text) = fs::read_to_string(settings_path) else {
        return DiagnosticLoggingSettingsSnapshot::default();
    };
    let Ok(settings) = serde_json::from_str::<Value>(&text) else {
        return DiagnosticLoggingSettingsSnapshot::default();
    };
    let debugging_mode = settings.get("debuggingMode").and_then(Value::as_bool) == Some(true);
    let enabled_scenarios = settings
        .get("diagnosticLogging")
        .and_then(Value::as_object)
        .and_then(|logging| logging.get("scenarios"))
        .and_then(Value::as_object)
        .map(|scenarios| {
            scenarios
                .iter()
                .filter(|(_, state)| diagnostic_scenario_state_enabled(state))
                .map(|(scenario_id, _)| scenario_id.clone())
                .collect()
        })
        .unwrap_or_default();
    DiagnosticLoggingSettingsSnapshot {
        debugging_mode,
        enabled_scenarios,
    }
}

fn diagnostic_scenario_state_enabled(state: &Value) -> bool {
    if state.as_bool() == Some(true) {
        return true;
    }
    let Some(state) = state.as_object() else {
        return false;
    };
    if state.get("enabled").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    let Some(expires_at) = state
        .get("expiresAt")
        .and_then(Value::as_str)
        .filter(|expires_at| !expires_at.trim().is_empty())
    else {
        return true;
    };
    DateTime::parse_from_rfc3339(expires_at).is_ok_and(|expires_at| expires_at > Utc::now())
}

pub(crate) fn read_routine_diagnostic_enabled(settings_path: &Path, scenario_id: &str) -> bool {
    let Ok(text) = fs::read_to_string(settings_path) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    settings.get("debuggingMode").and_then(Value::as_bool) == Some(true)
        && settings
            .get("diagnosticLogging")
            .and_then(Value::as_object)
            .and_then(|logging| logging.get("scenarios"))
            .and_then(Value::as_object)
            .and_then(|scenarios| scenarios.get(scenario_id))
            .is_some_and(diagnostic_scenario_state_enabled)
}

fn write_gxserver_log_line(paths: &GxserverPaths, line: &str) -> Result<()> {
    let write_lock = log_file_write_lock(&paths.log_file);
    let _write_guard = write_lock.lock().expect("gxserver log writer poisoned");
    rotate_log_if_needed(&paths.log_file, line.len() as u64 + 1)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log_file)
        .with_context(|| "open gxserver log file")?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn log_file_write_lock(log_file: &Path) -> Arc<Mutex<()>> {
    let locks = LOG_FILE_WRITE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = locks
        .lock()
        .expect("gxserver log writer lock registry poisoned");
    guard
        .entry(log_file.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn rotate_log_if_needed(log_file: &Path, incoming_byte_count: u64) -> Result<()> {
    let size = fs::metadata(log_file)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if size + incoming_byte_count <= LOG_FILE_MAX_BYTES {
        return Ok(());
    }
    let _ = fs::remove_file(rotated_log_file(log_file, LOG_FILE_MAX_ROTATIONS));
    for index in (1..LOG_FILE_MAX_ROTATIONS).rev() {
        let source = rotated_log_file(log_file, index);
        let destination = rotated_log_file(log_file, index + 1);
        match fs::rename(&source, &destination) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| "rotate gxserver log"),
        }
    }
    match fs::rename(log_file, rotated_log_file(log_file, 1)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| "rotate gxserver log"),
    }
}

fn rotated_log_file(log_file: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", log_file.display(), index))
}

fn gxserver_log_files(log_file: &Path) -> Vec<PathBuf> {
    let mut files = vec![log_file.to_path_buf()];
    files.extend((1..=LOG_FILE_MAX_ROTATIONS).map(|index| rotated_log_file(log_file, index)));
    files
}

fn retained_gxserver_log_file(
    active_log_file: &Path,
    log_files: &[PathBuf],
) -> Result<Option<PathBuf>> {
    if is_file_if_exists(active_log_file)? {
        return Ok(Some(active_log_file.to_path_buf()));
    }
    let mut retained: Option<(PathBuf, std::time::SystemTime)> = None;
    for log_file in log_files {
        let metadata = match fs::metadata(log_file) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).with_context(|| "stat gxserver log rotation"),
        };
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        if retained
            .as_ref()
            .map(|(_, retained_modified)| modified > *retained_modified)
            .unwrap_or(true)
        {
            retained = Some((log_file.clone(), modified));
        }
    }
    Ok(retained.map(|(path, _)| path))
}

fn is_file_if_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| "stat gxserver log file"),
    }
}

fn prune_log_file_to_max_lines_with_before_rewrite(
    log_file: &Path,
    max_lines: usize,
    before_rewrite: impl FnOnce(),
) -> Result<()> {
    if max_lines == 0 {
        return Ok(());
    }
    let content = match fs::read_to_string(log_file) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| "read gxserver log for retention"),
    };
    let mut lines = if content.ends_with('\n') {
        content[..content.len() - 1]
            .split('\n')
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        content.split('\n').map(str::to_string).collect::<Vec<_>>()
    };
    if lines.len() <= max_lines {
        return Ok(());
    }
    let start = lines.len() - max_lines;
    lines.drain(0..start);
    before_rewrite();
    fs::write(log_file, format!("{}\n", lines.join("\n")))
        .with_context(|| "write retained gxserver log lines")
}

fn parse_query_logs_params(
    raw_params: &Map<String, Value>,
) -> std::result::Result<LogQueryParams, LogQueryError> {
    let level = parse_level_filter(raw_params.get("level"))?;
    let event = parse_optional_string(raw_params.get("event"), "event")?;
    let event_prefix = parse_optional_string(raw_params.get("eventPrefix"), "eventPrefix")?;
    let server_id = parse_optional_string(raw_params.get("serverId"), "serverId")?;
    let project_id = parse_optional_string(raw_params.get("projectId"), "projectId")?;
    let session_id = parse_optional_string(raw_params.get("sessionId"), "sessionId")?;
    let client = parse_optional_string(raw_params.get("client"), "client")?;
    let since = parse_optional_timestamp(raw_params.get("since"), "since")?;
    let until = parse_optional_timestamp(raw_params.get("until"), "until")?;
    let limit = parse_optional_limit(raw_params.get("limit"))?;
    let order = parse_order(raw_params.get("order"))?;
    let reverse = parse_optional_boolean(raw_params.get("reverse"), "reverse")?;
    let resolved_order = resolve_order(order, reverse)?;
    let since_ms = since.as_deref().and_then(parse_timestamp_ms);
    let until_ms = until.as_deref().and_then(parse_timestamp_ms);
    Ok(LogQueryParams {
        client,
        event,
        event_prefix,
        level,
        limit,
        order: resolved_order,
        project_id,
        server_id,
        session_id,
        since,
        since_ms,
        until_ms,
    })
}

fn read_log_file_size(log_file: &Path) -> std::result::Result<Option<u64>, LogQueryError> {
    match fs::metadata(log_file) {
        Ok(metadata) => Ok(Some(metadata.len())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LogQueryError::Io(error)),
    }
}

fn read_complete_gxserver_log_entries(
    log_file: &Path,
    params: &LogQueryParams,
    file_size_bytes: u64,
) -> std::result::Result<LogQueryRead, LogQueryError> {
    let text = fs::read_to_string(log_file)?;
    let scanned = scan_log_lines(split_log_lines(&text), params);
    Ok(LogQueryRead {
        entries: scanned.entries,
        malformed_line_count: scanned.malformed_line_count,
        scanned_line_count: scanned.scanned_line_count,
        complete: true,
        scanned_bytes: file_size_bytes,
    })
}

fn read_bounded_gxserver_log_entries(
    log_file: &Path,
    params: &LogQueryParams,
    file_size_bytes: u64,
    limit: usize,
    order: LogQueryOrder,
) -> std::result::Result<LogQueryRead, LogQueryError> {
    let window_bytes = file_size_bytes.min(log_query_window_bytes(limit));
    let text = if order == LogQueryOrder::Desc || params.since.is_some() {
        read_log_text_window(
            log_file,
            file_size_bytes - window_bytes,
            window_bytes,
            file_size_bytes,
            LogTextWindowMode::Tail,
        )?
    } else {
        read_log_text_window(
            log_file,
            0,
            window_bytes,
            file_size_bytes,
            LogTextWindowMode::Head,
        )?
    };
    let scanned = scan_log_lines(split_log_lines(&text), params);
    Ok(LogQueryRead {
        entries: scanned.entries,
        malformed_line_count: scanned.malformed_line_count,
        scanned_line_count: scanned.scanned_line_count,
        complete: window_bytes >= file_size_bytes,
        scanned_bytes: window_bytes,
    })
}

fn log_query_window_bytes(limit: usize) -> u64 {
    LOG_QUERY_MAX_WINDOW_BYTES
        .min(LOG_QUERY_WINDOW_BASE_BYTES.max(limit as u64 * LOG_QUERY_ESTIMATED_BYTES_PER_ENTRY))
}

#[derive(Clone, Copy)]
enum LogTextWindowMode {
    Head,
    Tail,
}

fn read_log_text_window(
    log_file: &Path,
    start_offset: u64,
    byte_length: u64,
    file_size_bytes: u64,
    mode: LogTextWindowMode,
) -> std::result::Result<String, LogQueryError> {
    let mut file = File::open(log_file)?;
    file.seek(SeekFrom::Start(start_offset))?;
    let mut buffer = vec![0_u8; byte_length as usize];
    let bytes_read = file.read(&mut buffer)?;
    let mut text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
    if matches!(mode, LogTextWindowMode::Tail) && start_offset > 0 {
        text = match text.find('\n') {
            Some(index) => text[index + 1..].to_string(),
            None => String::new(),
        };
    }
    if matches!(mode, LogTextWindowMode::Head)
        && start_offset + bytes_read as u64 >= file_size_bytes
    {
        return Ok(text);
    }
    if matches!(mode, LogTextWindowMode::Head) {
        text = match text.rfind('\n') {
            Some(index) => text[..=index].to_string(),
            None => String::new(),
        };
    }
    Ok(text)
}

fn split_log_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn scan_log_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    params: &LogQueryParams,
) -> LogLineScan {
    let mut entries = Vec::new();
    let mut malformed_line_count = 0;
    let mut scanned_line_count = 0;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        scanned_line_count += 1;
        match parse_log_line(line) {
            Some(entry) if matches_query(&entry, params) => entries.push(entry),
            Some(_) => {}
            None => malformed_line_count += 1,
        }
    }
    LogLineScan {
        entries,
        malformed_line_count,
        scanned_line_count,
    }
}

fn parse_log_line(line: &str) -> Option<Value> {
    let parsed = serde_json::from_str::<Value>(line).ok()?;
    is_gxserver_log_entry(&parsed).then_some(parsed)
}

fn matches_query(entry: &Value, params: &LogQueryParams) -> bool {
    if let Some(levels) = &params.level {
        if !entry
            .get("level")
            .and_then(Value::as_str)
            .map(|level| levels.iter().any(|expected| expected == level))
            .unwrap_or(false)
        {
            return false;
        }
    }
    if let Some(event) = &params.event {
        if entry.get("event").and_then(Value::as_str) != Some(event.as_str()) {
            return false;
        }
    }
    if let Some(event_prefix) = &params.event_prefix {
        if !entry
            .get("event")
            .and_then(Value::as_str)
            .map(|event| event.starts_with(event_prefix))
            .unwrap_or(false)
        {
            return false;
        }
    }
    if let Some(server_id) = &params.server_id {
        if entry.get("serverId").and_then(Value::as_str) != Some(server_id.as_str()) {
            return false;
        }
    }
    if let Some(project_id) = &params.project_id {
        if entry.get("projectId").and_then(Value::as_str) != Some(project_id.as_str()) {
            return false;
        }
    }
    if let Some(session_id) = &params.session_id {
        if entry.get("sessionId").and_then(Value::as_str) != Some(session_id.as_str()) {
            return false;
        }
    }
    if let Some(client) = &params.client {
        if entry.get("client").and_then(Value::as_str) != Some(client.as_str()) {
            return false;
        }
    }
    if params.since_ms.is_some() || params.until_ms.is_some() {
        let Some(timestamp_ms) = entry
            .get("ts")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_ms)
        else {
            return false;
        };
        if params
            .since_ms
            .map(|since_ms| timestamp_ms < since_ms)
            .unwrap_or(false)
        {
            return false;
        }
        if params
            .until_ms
            .map(|until_ms| timestamp_ms > until_ms)
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

fn order_entries(mut entries: Vec<Value>, order: LogQueryOrder) -> Vec<Value> {
    entries.sort_by(|left, right| {
        let left_ts = left.get("ts").and_then(Value::as_str).unwrap_or_default();
        let right_ts = right.get("ts").and_then(Value::as_str).unwrap_or_default();
        let ordering = match (parse_timestamp_ms(left_ts), parse_timestamp_ms(right_ts)) {
            (Some(left_ms), Some(right_ms)) => left_ms.cmp(&right_ms),
            _ => left_ts.cmp(right_ts),
        };
        match order {
            LogQueryOrder::Asc => ordering,
            LogQueryOrder::Desc => ordering.reverse(),
        }
    });
    entries
}

fn parse_level_filter(
    value: Option<&Value>,
) -> std::result::Result<Option<Vec<String>>, LogQueryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(level) = value.as_str() {
        return Ok(Some(vec![parse_level(level)?.to_string()]));
    }
    if let Some(levels) = value.as_array().filter(|levels| !levels.is_empty()) {
        return levels
            .iter()
            .map(|level| parse_level(level.as_str().unwrap_or_default()).map(str::to_string))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Some);
    }
    Err(LogQueryError::Input(
        "level must be a log level or a non-empty log level array.".to_string(),
    ))
}

fn parse_level(value: &str) -> std::result::Result<&'static str, LogQueryError> {
    match value {
        "debug" => Ok("debug"),
        "info" => Ok("info"),
        "warn" => Ok("warn"),
        "error" => Ok("error"),
        _ => Err(LogQueryError::Input(
            "level must be one of debug, info, warn, or error.".to_string(),
        )),
    }
}

fn parse_optional_string(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<String>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
        Some(_) => Err(LogQueryError::Input(format!(
            "{field} must be a non-empty string."
        ))),
    }
}

fn parse_optional_timestamp(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<String>, LogQueryError> {
    let timestamp = parse_optional_string(value, field)?;
    if timestamp
        .as_deref()
        .map(|timestamp| parse_timestamp_ms(timestamp).is_none())
        .unwrap_or(false)
    {
        return Err(LogQueryError::Input(format!(
            "{field} must be a parseable timestamp."
        )));
    }
    Ok(timestamp)
}

fn parse_optional_limit(
    value: Option<&Value>,
) -> std::result::Result<Option<usize>, LogQueryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let limit = json_integer(value).filter(|limit| *limit >= 1);
    let Some(limit) = limit.and_then(|limit| usize::try_from(limit).ok()) else {
        return Err(LogQueryError::Input(format!(
            "limit must be an integer from 1 to {MAX_LOG_QUERY_LIMIT}."
        )));
    };
    if limit > MAX_LOG_QUERY_LIMIT {
        return Err(LogQueryError::Input(format!(
            "limit must be an integer from 1 to {MAX_LOG_QUERY_LIMIT}."
        )));
    }
    Ok(Some(limit))
}

fn parse_order(value: Option<&Value>) -> std::result::Result<Option<LogQueryOrder>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) if value == "asc" => Ok(Some(LogQueryOrder::Asc)),
        Some(Value::String(value)) if value == "desc" => Ok(Some(LogQueryOrder::Desc)),
        Some(_) => Err(LogQueryError::Input(
            "order must be asc or desc.".to_string(),
        )),
    }
}

fn parse_optional_boolean(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<bool>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(LogQueryError::Input(format!("{field} must be a boolean."))),
    }
}

fn resolve_order(
    order: Option<LogQueryOrder>,
    reverse: Option<bool>,
) -> std::result::Result<Option<LogQueryOrder>, LogQueryError> {
    if let (Some(order), Some(reverse)) = (order, reverse) {
        let reverse_order = if reverse {
            LogQueryOrder::Desc
        } else {
            LogQueryOrder::Asc
        };
        if order != reverse_order {
            return Err(LogQueryError::Input(
                "order and reverse specify conflicting log order.".to_string(),
            ));
        }
    }
    Ok(order.or_else(|| {
        reverse.map(|reverse| {
            if reverse {
                LogQueryOrder::Desc
            } else {
                LogQueryOrder::Asc
            }
        })
    }))
}

fn json_integer(value: &Value) -> Option<i64> {
    let number = value.as_number()?;
    if let Some(value) = number.as_i64() {
        return Some(value);
    }
    if let Some(value) = number.as_u64() {
        return i64::try_from(value).ok();
    }
    let value = number.as_f64()?;
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        Some(value as i64)
    } else {
        None
    }
}

fn is_gxserver_log_entry(value: &Value) -> bool {
    value
        .as_object()
        .map(|object| {
            object.get("ts").and_then(Value::as_str).is_some()
                && object.get("event").and_then(Value::as_str).is_some()
                && matches!(
                    object.get("level").and_then(Value::as_str),
                    Some("debug" | "info" | "warn" | "error")
                )
        })
        .unwrap_or(false)
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.timestamp_millis())
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
}

fn is_identifier_key(key: &str) -> bool {
    key == "id"
        || key.ends_with("id")
        || key.ends_with("ids")
        || key.ends_with("ref")
        || key.ends_with("refs")
}

fn is_safe_identifier(value: &str) -> bool {
    value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn is_secret_key(key: &str) -> bool {
    key.contains("token")
        || key.contains("bearer")
        || key.contains("secret")
        || key.contains("credential")
        || key.contains("password")
        || key.contains("cookie")
        || key.contains("authorization")
        || key.contains("auth")
}

fn is_url_key(key: &str) -> bool {
    key == "url" || key.ends_with("url") || key.contains("uri") || key == "href" || key == "origin"
}

fn is_path_key(key: &str) -> bool {
    key == "path"
        || key == "cwd"
        || key.ends_with("path")
        || key.ends_with("dir")
        || key.ends_with("directory")
        || key.ends_with("root")
        || key.ends_with("file")
        || key.ends_with("filename")
        || key.contains("workspace")
}

fn is_sensitive_text_key(key: &str) -> bool {
    key == "title"
        || key.ends_with("title")
        || key == "name"
        || key.ends_with("name")
        || key == "message"
        || key == "details"
        || key.ends_with("details")
        || key == "input"
        || key == "text"
        || key.ends_with("text")
        || key == "comment"
        || key == "description"
        || key == "label"
        || key == "prompt"
        || key.ends_with("prompt")
        || key == "prompts"
        || key.ends_with("prompts")
        || key == "preview"
        || key.ends_with("preview")
        || key == "command"
        || key.ends_with("command")
        || key == "stdout"
        || key == "stderr"
        || key == "body"
        || key.ends_with("body")
}

fn is_sensitive_collection_key(key: &str) -> bool {
    key == "args" || key.ends_with("args") || key == "arguments" || key.ends_with("arguments")
}

fn is_environment_key(key: &str) -> bool {
    key == "env"
        || key == "envvars"
        || key == "envvariables"
        || key == "environment"
        || key == "environmentvariables"
        || key.ends_with("env")
        || key.ends_with("envvars")
        || key.ends_with("envvariables")
        || key.ends_with("environment")
        || key.ends_with("environmentvariables")
}

fn looks_like_url(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with("~/")
        || value.starts_with("/Users/")
        || value.starts_with("/Volumes/")
        || value.starts_with("/private/")
        || value.starts_with("/tmp/")
        || value.starts_with("/var/folders/")
}

fn contains_secret_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("bearer")
        || lower.contains("token")
        || lower.contains("authorization")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("credential")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;
    use std::sync::{mpsc, Arc, Barrier};

    #[test]
    fn warn_log_redacts_private_values() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let logger = GxserverLogger::new(paths.clone());
        logger
            .log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "test".to_string(),
                server_id: Some("S1a".to_string()),
                request_id: Some("request-1".to_string()),
                client: None,
                duration_ms: None,
                error: Some("failed /Users/alice/project token=secret".to_string()),
                details: Some(json!({
                    "path": "/Users/alice/project",
                    "url": "https://example.com/private?token=secret",
                    "command": "cat ~/.ssh/id_rsa",
                    "args": ["--token", "secret"],
                    "projectName": "Private Alpha",
                    "stdout": "raw command output",
                    "stderr": "raw error output",
                    "projectId": "P1abc"
                })),
            })
            .expect("log");
        let text = fs::read_to_string(paths.log_file).expect("read log");
        assert!(!text.contains("/Users/alice"));
        assert!(!text.contains("id_rsa"));
        assert!(!text.contains("token=secret"));
        assert!(!text.contains("Private Alpha"));
        assert!(!text.contains("raw command output"));
        assert!(!text.contains("raw error output"));
        assert!(text.contains("P1abc"));
    }

    #[test]
    fn persistent_log_boundary_redacts_prompts_env_urls_titles_commands_and_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        let logger = GxserverLogger::new(paths.clone());
        logger
            .log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "privacy.boundary".to_string(),
                server_id: Some("S7k".to_string()),
                request_id: Some("request-privacy".to_string()),
                client: None,
                duration_ms: None,
                error: Some(
                    "failed while running /Users/person/dev/private-project with HTTPS://Example.test/private?token=SECRET"
                        .to_string(),
                ),
                details: Some(json!({
                    "args": ["commit", "-m", "private command subject"],
                    "authToken": "SECRET",
                    "commandText": "git commit -m 'private command subject'",
                    "env": {
                        "CUSTOMER_NAME": "Acme Private",
                        "GHOSTEX_TOKEN": "SECRET"
                    },
                    "environment": "PATH=/Users/person/dev/private-project TOKEN=SECRET",
                    "projectName": "Private Project",
                    "prompt": "Summarize private customer incident",
                    "prompts": ["private prompt one", "private prompt two"],
                    "rawUrl": "HTTPS://Example.test/private?token=SECRET",
                    "sessionName": "Customer Debug Session",
                    "stderr": "private stderr output",
                    "stdout": "private stdout output",
                    "terminalTitle": "Private Terminal Title",
                    "url": "HTTPS://Example.test/private?token=SECRET",
                    "workspaceRoot": "/Users/person/dev/private-project"
                })),
            })
            .expect("log");

        let text = fs::read_to_string(paths.log_file).expect("read log");
        for forbidden in [
            "/Users/person",
            "Acme Private",
            "Customer Debug Session",
            "Example.test/private",
            "HTTPS://",
            "Private Project",
            "Private Terminal Title",
            "Summarize private customer incident",
            "git commit",
            "private command subject",
            "private prompt one",
            "private stderr output",
            "private stdout output",
            "SECRET",
            "TOKEN=SECRET",
            "private-project",
        ] {
            assert!(
                !text.contains(forbidden),
                "persistent log leaked {forbidden}: {text}"
            );
        }
        assert!(text.contains("[redacted]"));
        assert!(text.contains("[redacted:path]"));
        assert!(text.contains("[redacted:url]"));
        assert!(text.contains("[redacted:secret]"));
    }

    #[test]
    fn routine_logs_are_gated_by_debugging_mode() {
        let disabled_temp = tempfile::tempdir().expect("disabled tempdir");
        let disabled_paths = get_gxserver_paths(Some(disabled_temp.path().to_path_buf()));
        let disabled_logger = test_logger(disabled_paths.clone());
        disabled_logger
            .log_routine(
                DiagnosticLogScenario::ServerLifecycle,
                GxserverLogInput {
                    level: LogLevel::Info,
                    event: "routine.info".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                },
            )
            .expect("info log");
        disabled_logger
            .log_routine(
                DiagnosticLogScenario::ServerLifecycle,
                GxserverLogInput {
                    level: LogLevel::Debug,
                    event: "routine.debug".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                },
            )
            .expect("debug log");
        disabled_logger
            .log_routine(
                DiagnosticLogScenario::ServerLifecycle,
                GxserverLogInput {
                    level: LogLevel::Info,
                    event: "routine.health".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: Some(json!({ "errorCount": 0, "status": "ok" })),
                },
            )
            .expect("zero-error routine log");
        assert!(!disabled_paths.log_file.exists());
        disabled_logger
            .log(GxserverLogInput {
                level: LogLevel::Info,
                event: "important.failure".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some("important failure".to_string()),
                details: None,
            })
            .expect("important log");
        let important_text =
            fs::read_to_string(&disabled_paths.log_file).expect("read important log");
        assert!(important_text.contains("important.failure"));

        let enabled_temp = tempfile::tempdir().expect("enabled tempdir");
        let enabled_paths = get_gxserver_paths(Some(enabled_temp.path().to_path_buf()));
        let enabled_logger = test_logger_with_debugging_mode(enabled_paths.clone(), true);
        enabled_logger
            .log_routine(
                DiagnosticLogScenario::ServerLifecycle,
                GxserverLogInput {
                    level: LogLevel::Info,
                    event: "routine.info".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                },
            )
            .expect("info log enabled");
        enabled_logger
            .log_routine(
                DiagnosticLogScenario::ServerLifecycle,
                GxserverLogInput {
                    level: LogLevel::Debug,
                    event: "routine.debug".to_string(),
                    server_id: None,
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: None,
                    details: None,
                },
            )
            .expect("debug log enabled");
        enabled_logger
            .log(GxserverLogInput {
                level: LogLevel::Info,
                event: "routine.unscoped".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: None,
            })
            .expect("unscoped routine log ignored");
        let text = fs::read_to_string(enabled_paths.log_file).expect("read enabled logs");
        assert!(text.contains("\"routine.info\""));
        assert!(text.contains("\"routine.debug\""));
        assert!(!text.contains("routine.unscoped"));
    }

    #[test]
    fn log_query_filters_malformed_identities_timestamps_limit_and_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        fs::write(
            &paths.log_file,
            [
                log_line(json!({
                    "client": "cli",
                    "event": "agent.detected",
                    "level": "info",
                    "projectId": "P3a91",
                    "serverId": "S7k",
                    "sessionId": "G8v20",
                    "ts": "2026-05-30T10:00:00.000Z"
                })),
                "not-json".to_string(),
                log_line(json!({
                    "client": "api",
                    "event": "agent.activity.working",
                    "level": "debug",
                    "projectId": "P3a91",
                    "serverId": "S7k",
                    "sessionId": "G8v20",
                    "ts": "2026-05-30T10:01:00.000Z"
                })),
                log_line(json!({
                    "client": "api",
                    "event": "zmx.kill.failed",
                    "level": "error",
                    "projectId": "P4b12",
                    "serverId": "S7k",
                    "sessionId": "G9v21",
                    "ts": "2026-05-30T10:02:00.000Z"
                })),
            ]
            .join("\n")
                + "\n",
        )
        .expect("write log");

        let params = params(json!({
            "eventPrefix": "agent.",
            "level": ["debug", "info"],
            "limit": 1,
            "order": "desc",
            "projectId": "P3a91",
            "since": "2026-05-30T09:59:00.000Z",
            "until": "2026-05-30T10:01:30.000Z"
        }));
        let result = query_gxserver_logs(&paths, &params).expect("query logs");

        assert_eq!(result["malformedLineCount"], json!(1));
        assert_eq!(result["malformedLineCountIsExact"], json!(true));
        assert_eq!(result["totalMatched"], json!(2));
        assert_eq!(result["totalMatchedIsExact"], json!(true));
        assert_eq!(result["truncated"], json!(false));
        let entries = result["entries"].as_array().expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["event"], json!("agent.activity.working"));
    }

    #[test]
    fn large_descending_log_query_reads_bounded_tail_window() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        let mut lines = Vec::new();
        let mut byte_length = 0_u64;
        let mut line_count = 0_usize;
        while byte_length <= LOG_QUERY_FULL_SCAN_MAX_BYTES + 1024 * 1024 {
            let line = log_line(json!({
                "event": format!("tail.{line_count}"),
                "level": "info",
                "message": "x".repeat(768),
                "ts": DateTime::from_timestamp(1_780_138_800 + line_count as i64, 0)
                    .expect("timestamp")
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            }));
            byte_length += line.len() as u64 + 1;
            lines.push(line);
            line_count += 1;
        }
        fs::write(&paths.log_file, format!("{}\n", lines.join("\n"))).expect("write log");

        let result = query_gxserver_logs(
            &paths,
            &params(json!({
                "eventPrefix": "tail.",
                "limit": 3,
                "order": "desc"
            })),
        )
        .expect("query logs");

        let events = result["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|entry| entry["event"].as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                format!("tail.{}", line_count - 1),
                format!("tail.{}", line_count - 2),
                format!("tail.{}", line_count - 3),
            ]
        );
        assert_eq!(result["truncated"], json!(true));
        assert_eq!(result["truncatedReason"], json!("fileWindowExceeded"));
        assert_eq!(result["totalMatchedIsExact"], json!(false));
        assert_eq!(result["malformedLineCountIsExact"], json!(false));
        assert!(
            result["scannedBytes"].as_u64().unwrap() < result["logFileSizeBytes"].as_u64().unwrap()
        );
        assert!(result["scannedLineCount"].as_u64().unwrap() < line_count as u64);
    }

    #[test]
    fn log_query_rejects_invalid_limit_and_conflicting_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        let limit_error =
            query_gxserver_logs(&paths, &params(json!({ "limit": 0 }))).expect_err("limit error");
        assert!(
            matches!(limit_error, LogQueryError::Input(message) if message == "limit must be an integer from 1 to 5000.")
        );

        let order_error =
            query_gxserver_logs(&paths, &params(json!({ "order": "asc", "reverse": true })))
                .expect_err("order error");
        assert!(
            matches!(order_error, LogQueryError::Input(message) if message == "order and reverse specify conflicting log order.")
        );
    }

    #[test]
    fn log_retention_keeps_active_split_file_and_deletes_older_rotations() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        fs::write(&paths.log_file, "old-active\nnew-active-1\nnew-active-2\n").expect("active");
        fs::write(
            rotated_log_file(&paths.log_file, 1),
            "old-rotated\nnew-rotated\n",
        )
        .expect("rotated 1");
        fs::write(rotated_log_file(&paths.log_file, 2), "older-rotated\n").expect("rotated 2");

        prune_gxserver_log_lines(&paths, 2).expect("prune");

        assert_eq!(
            fs::read_to_string(&paths.log_file).expect("read active"),
            "new-active-1\nnew-active-2\n"
        );
        assert!(!rotated_log_file(&paths.log_file, 1).exists());
        assert!(!rotated_log_file(&paths.log_file, 2).exists());
    }

    #[test]
    fn retention_prune_blocks_logger_appends_until_rewrite_finishes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        fs::write(&paths.log_file, "old-0\nold-1\nold-2\nold-3\n").expect("active");
        let logger = Arc::new(test_logger(paths.clone()));
        let (append_started_tx, append_started_rx) = mpsc::channel();
        let (append_done_tx, append_done_rx) = mpsc::channel();
        let (append_handle_tx, append_handle_rx) = mpsc::channel();
        let logger_for_append = Arc::clone(&logger);

        prune_gxserver_log_lines_with_before_rewrite(&paths, 2, move || {
            let append_handle = thread::spawn(move || {
                append_started_tx.send(()).expect("append started");
                logger_for_append
                    .log(GxserverLogInput {
                        level: LogLevel::Warn,
                        event: "retention.append.during-prune".to_string(),
                        server_id: None,
                        request_id: None,
                        client: None,
                        duration_ms: None,
                        error: None,
                        details: None,
                    })
                    .expect("append log");
                let _ = append_done_tx.send(());
            });
            append_handle_tx.send(append_handle).expect("append handle");
            append_started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("append attempted");
            match append_done_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(()) => panic!("logger append completed while retention held the write lock"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => panic!("append done channel failed: {error}"),
            }
        })
        .expect("prune");

        append_handle_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("append handle")
            .join()
            .expect("append thread");
        let text = fs::read_to_string(&paths.log_file).expect("read active");
        assert!(text.contains("\"retention.append.during-prune\""));
    }

    #[test]
    fn concurrent_retention_prune_keeps_logger_appends() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        let old_lines = (0..200)
            .map(|index| {
                log_line(json!({
                    "event": format!("old.{index}"),
                    "level": "warn"
                }))
            })
            .collect::<Vec<_>>();
        fs::write(&paths.log_file, format!("{}\n", old_lines.join("\n"))).expect("active");
        let append_count = 32;
        let prune_count = 8;
        let max_lines = append_count + 8;
        let logger = Arc::new(test_logger(paths.clone()));
        let start = Arc::new(Barrier::new(append_count + prune_count + 1));
        let mut handles = Vec::new();

        for index in 0..append_count {
            let logger = Arc::clone(&logger);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                logger
                    .log(GxserverLogInput {
                        level: LogLevel::Warn,
                        event: format!("retention.append.{index}"),
                        server_id: None,
                        request_id: None,
                        client: None,
                        duration_ms: None,
                        error: None,
                        details: None,
                    })
                    .expect("append log");
            }));
        }
        for _ in 0..prune_count {
            let paths = paths.clone();
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                prune_gxserver_log_lines(&paths, max_lines).expect("prune");
            }));
        }

        start.wait();
        for handle in handles {
            handle.join().expect("worker thread");
        }
        let text = fs::read_to_string(&paths.log_file).expect("read active");
        for index in 0..append_count {
            assert!(
                text.contains(&format!("\"retention.append.{index}\"")),
                "missing append {index}"
            );
        }
    }

    #[test]
    fn logger_startup_schedules_line_retention() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        fs::create_dir_all(&paths.logs_dir).expect("logs dir");
        fs::write(&paths.log_file, "old\nnew-1\nnew-2\n").expect("active");

        let _logger = GxserverLogger::new_with_retention(
            paths.clone(),
            LogRetentionOptions {
                delay_ms: 1,
                max_lines: 2,
            },
        );
        wait_for_log_file_text(&paths.log_file, "new-1\nnew-2\n");
    }

    fn test_logger(paths: GxserverPaths) -> GxserverLogger {
        test_logger_with_debugging_mode(paths, false)
    }

    fn test_logger_with_debugging_mode(
        paths: GxserverPaths,
        debugging_mode: bool,
    ) -> GxserverLogger {
        GxserverLogger {
            paths,
            debugging_mode_cache: Mutex::new(DebuggingModeCache {
                checked_at: Instant::now(),
                debugging_mode,
                enabled_scenarios: debugging_mode
                    .then(|| DiagnosticLogScenario::ServerLifecycle.id().to_string())
                    .into_iter()
                    .collect(),
            }),
        }
    }

    fn wait_for_log_file_text(log_file: &Path, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let current = fs::read_to_string(log_file).unwrap_or_default();
            if current == expected {
                return;
            }
            if Instant::now() >= deadline {
                assert_eq!(current, expected);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn log_line(value: Value) -> String {
        serde_json::to_string(&value).expect("json line")
    }

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().expect("params object").clone()
    }
}
