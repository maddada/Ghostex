use super::*;

pub(super) const LOG_FILE_MAX_BYTES: u64 = 25 * 1024 * 1024;
pub(super) const LOG_FILE_MAX_ROTATIONS: usize = 3;
const LOG_FILE_MAX_LINES: usize = 25_000;
const LOG_RETENTION_STARTUP_DELAY_MS: u64 = 60_000;
const DEBUGGING_MODE_CACHE_MS: u64 = 1_000;
pub(super) const DEFAULT_LOG_QUERY_LIMIT: usize = 200;
pub(super) const MAX_LOG_QUERY_LIMIT: usize = 5_000;
pub(super) const LOG_QUERY_FULL_SCAN_MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const LOG_QUERY_WINDOW_BASE_BYTES: u64 = 2 * 1024 * 1024;
pub(super) const LOG_QUERY_MAX_WINDOW_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const LOG_QUERY_ESTIMATED_BYTES_PER_ENTRY: u64 = 1024;
static SCHEDULED_RETENTION_LOG_FILES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
pub(super) static LOG_FILE_WRITE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    OnceLock::new();

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
    RepositoryClone,
    ServerLifecycle,
    TerminalFocus,
    TypedOperations,
    ProgramStatusMismatch,
    SessionLinkReads,
    HooklessActivityComparison,
}

impl DiagnosticLogScenario {
    /// The Settings switch that turns this log on. Several variants share one switch since the
    /// Debugging page merged its log switches into one per feature area; the variant still names
    /// what is being logged.
    ///
    /// SEE-ALSO: `DIAGNOSTIC_LOGGING_SCENARIOS` in packages/settings-catalog/src/data/diagnostic_logging.rs
    /// lists the ids Settings can turn on.
    pub fn id(self) -> &'static str {
        match self {
            Self::SessionChatDrafts => "gpui.sessionChat.viewState",
            Self::AgentActivity | Self::AgentDetection => "gxserver.agentActivity",
            Self::ApiRequests | Self::RepositoryClone | Self::TypedOperations => {
                "gxserver.requests"
            }
            Self::ServerLifecycle => "native.host.lifecycle",
            Self::TerminalFocus => "native.terminal.focus",
            Self::ProgramStatusMismatch => "gxserver.programStatusMismatch",
            Self::SessionLinkReads => "gxserver.sessionLinks",
            Self::HooklessActivityComparison => "gxserver.hooklessActivity",
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
    pub(super) paths: GxserverPaths,
    pub(super) debugging_mode_cache: Mutex<DebuggingModeCache>,
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
pub(super) struct DebuggingModeCache {
    pub(super) checked_at: Instant,
    pub(super) debugging_mode: bool,
    pub(super) enabled_scenarios: HashSet<String>,
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

    pub(super) fn new_with_retention(paths: GxserverPaths, retention: LogRetentionOptions) -> Self {
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

    /// Persists a record the user asked to keep for every occurrence, whatever the diagnostics
    /// switches say. Only for rare, metadata-only events: today one line per session close
    /// (`session_close_log.rs`).
    pub fn log_record(&self, entry: GxserverLogInput) -> Result<()> {
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

    /// Whether routine entries for `scenario` would be written now, for work that only runs to
    /// produce them.
    pub(crate) fn routine_logging_enabled(&self, scenario: DiagnosticLogScenario) -> bool {
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

pub(super) fn prune_gxserver_log_lines_with_before_rewrite(
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
