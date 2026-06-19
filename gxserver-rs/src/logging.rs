use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
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

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
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
    enabled: bool,
}

/*
CDXC:GxserverLogs 2026-06-14-20:37:
Persistent Rust logs must be safe for support bundles. Persist only warn/error unless Debugging Mode is enabled, rotate before append at the TypeScript size/count, and sanitize at the JSONL writer boundary so future call sites cannot leak paths, URLs, command text, stdout/stderr, tokens, or user-owned names.

CDXC:GxserverLogs 2026-06-19-14:45:
Rust logger startup must match TypeScript support-bundle retention: schedule a one-minute delayed cleanup, keep only the active or newest gxserver JSONL split file, delete older rotations, and trim the retained file to 25,000 lines without logging cleanup failures back into the same file.
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
                enabled: false,
            }),
        }
    }

    pub fn log(&self, entry: GxserverLogInput) -> Result<()> {
        if !self.should_persist(entry.level) {
            return Ok(());
        }
        fs::create_dir_all(&self.paths.logs_dir)
            .with_context(|| "create gxserver logs directory")?;
        let line = serde_json::to_string(&normalize_log_entry(entry))?;
        rotate_log_if_needed(&self.paths.log_file, line.as_bytes().len() as u64 + 1)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.paths.log_file)
            .with_context(|| "open gxserver log file")?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    fn should_persist(&self, level: LogLevel) -> bool {
        matches!(level, LogLevel::Warn | LogLevel::Error) || self.debugging_mode_enabled()
    }

    fn debugging_mode_enabled(&self) -> bool {
        let mut cache = self
            .debugging_mode_cache
            .lock()
            .expect("debug cache poisoned");
        if cache.checked_at.elapsed() < Duration::from_millis(DEBUGGING_MODE_CACHE_MS) {
            return cache.enabled;
        }
        cache.checked_at = Instant::now();
        cache.enabled = read_debugging_mode_settings_file(&self.paths);
        cache.enabled
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
    prune_log_file_to_max_lines(&retained_log_file, max_lines)
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
CDXC:GxserverLogs 2026-06-19-14:45:
`/api/queryLogs` is a read-only local support API over `~/.ghostex/logs/gxserver.jsonl`. Match the TypeScript request filters, default/maximum limits, reverse/order aliasing, malformed-line tolerance, and bounded head/tail scanning so clients do not scrape support logs directly.
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
            if is_sensitive_collection_key(&key) {
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
            if is_sensitive_collection_key(&key) {
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
    redact_matching_segments(value, &["http://", "https://"], "[redacted:url]")
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
    )
}

fn redact_matching_segments(value: &str, prefixes: &[&str], replacement: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let next = prefixes
            .iter()
            .filter_map(|prefix| {
                value[cursor..]
                    .find(prefix)
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

fn read_debugging_mode_settings_file(paths: &GxserverPaths) -> bool {
    let settings_path = paths
        .home_dir
        .join(".ghostex")
        .join("state")
        .join("native-sidebar-settings.json");
    let Ok(text) = fs::read_to_string(settings_path) else {
        return false;
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| value.get("debuggingMode").and_then(Value::as_bool))
        == Some(true)
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

fn prune_log_file_to_max_lines(log_file: &Path, max_lines: usize) -> Result<()> {
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

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
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
            byte_length += line.as_bytes().len() as u64 + 1;
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
        std::thread::sleep(Duration::from_millis(50));

        assert_eq!(
            fs::read_to_string(&paths.log_file).expect("read active"),
            "new-1\nnew-2\n"
        );
    }

    fn log_line(value: Value) -> String {
        serde_json::to_string(&value).expect("json line")
    }

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().expect("params object").clone()
    }
}
