/*
CDXC:Telemetry 2026-08-26:
Durable telemetry state, in the `metadata` table under `analytics.state.v1`,
mirroring the `agents.settings.v1` read/write pair in
`server/src/agents/settings.rs`.

Only the heartbeat needs durability: "once per 24h" has to survive daemon
restarts, or a user who restarts Ghostex five times in an afternoon would send
five heartbeats and skew every per-install metric. The throttles for
`client.connected` and `surface.opened` stay in memory on purpose — they are
noise suppression within one run, and losing them across a restart costs at most
one extra event.
*/

use std::sync::{LazyLock, Mutex};

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

pub const ANALYTICS_STATE_METADATA_KEY: &str = "analytics.state.v1";
pub const LAST_HEARTBEAT_AT_KEY: &str = "lastHeartbeatAt";
pub const LAST_WEEKLY_COUNT_AT_KEY: &str = "lastWeeklyCountAt";
pub const LAST_ACTIVITY_AT_KEY: &str = "lastActivityAt";
pub const BUILT_IN_EXTENSIONS_ON_KEY: &str = "builtInExtensionsOn";
pub const BUILT_IN_EXTENSIONS_KNOWN_KEY: &str = "builtInExtensionsKnown";
pub const LAST_MOBILE_HELLO_AT_KEY: &str = "lastMobileHelloAt";
pub const CACHED_ACCOUNTS_KEY: &str = "cachedAccounts";
pub const SESSION_MODE_SWITCHES_KEY: &str = "sessionModeSwitches";

/// Maximum idle gap after the latest observed client or session activity during
/// which a due daily heartbeat may still emit. Prevents an idle daemon from
/// phoning home overnight or over a weekend.
pub const HEARTBEAT_ACTIVITY_RECENCY_HOURS: i64 = 2;

/// Minimum cadence (days) for `install.weekly_count` (Q29).
pub const WEEKLY_COUNT_STALE_DAYS: i64 = 7;

/// Maximum idle window (days) within which client or agent-session activity
/// qualifies an install for its weekly active count (Q29).
pub const WEEKLY_COUNT_ACTIVITY_DAYS: i64 = 7;

/// Daily switch counters between Terminal and Native mode, grouped by direction and entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionModeSwitchCounts {
    pub to_native_pill: u64,
    pub to_native_more_menu: u64,
    pub to_native_bulk_modal: u64,
    pub to_native_settings: u64,
    pub to_native_other: u64,
    pub to_terminal_more_menu: u64,
    pub to_terminal_settings: u64,
    pub to_terminal_other: u64,
}

impl SessionModeSwitchCounts {
    pub fn is_empty(&self) -> bool {
        self.to_native_total() == 0 && self.to_terminal_total() == 0
    }

    pub fn to_native_total(&self) -> u64 {
        self.to_native_pill
            + self.to_native_more_menu
            + self.to_native_bulk_modal
            + self.to_native_settings
            + self.to_native_other
    }

    pub fn to_terminal_total(&self) -> u64 {
        self.to_terminal_more_menu + self.to_terminal_settings + self.to_terminal_other
    }

    pub fn record(&mut self, direction: &str, source: &str) {
        match (direction, source) {
            ("terminal_to_native", "pill") => {
                self.to_native_pill = self.to_native_pill.saturating_add(1)
            }
            ("terminal_to_native", "more_menu") => {
                self.to_native_more_menu = self.to_native_more_menu.saturating_add(1);
            }
            ("terminal_to_native", "bulk_modal") => {
                self.to_native_bulk_modal = self.to_native_bulk_modal.saturating_add(1);
            }
            ("terminal_to_native", "settings") => {
                self.to_native_settings = self.to_native_settings.saturating_add(1);
            }
            ("terminal_to_native", _) => {
                self.to_native_other = self.to_native_other.saturating_add(1);
            }
            ("native_to_terminal", "more_menu") => {
                self.to_terminal_more_menu = self.to_terminal_more_menu.saturating_add(1);
            }
            ("native_to_terminal", "settings") => {
                self.to_terminal_settings = self.to_terminal_settings.saturating_add(1);
            }
            ("native_to_terminal", _) => {
                self.to_terminal_other = self.to_terminal_other.saturating_add(1);
            }
            _ => {}
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.to_native_pill = self.to_native_pill.saturating_add(other.to_native_pill);
        self.to_native_more_menu = self
            .to_native_more_menu
            .saturating_add(other.to_native_more_menu);
        self.to_native_bulk_modal = self
            .to_native_bulk_modal
            .saturating_add(other.to_native_bulk_modal);
        self.to_native_settings = self
            .to_native_settings
            .saturating_add(other.to_native_settings);
        self.to_native_other = self.to_native_other.saturating_add(other.to_native_other);
        self.to_terminal_more_menu = self
            .to_terminal_more_menu
            .saturating_add(other.to_terminal_more_menu);
        self.to_terminal_settings = self
            .to_terminal_settings
            .saturating_add(other.to_terminal_settings);
        self.to_terminal_other = self
            .to_terminal_other
            .saturating_add(other.to_terminal_other);
    }

    fn from_value(value: &Value) -> Self {
        let num = |key: &str| value.get(key).and_then(Value::as_u64).unwrap_or(0);
        Self {
            to_native_pill: num("toNativePill"),
            to_native_more_menu: num("toNativeMoreMenu"),
            to_native_bulk_modal: num("toNativeBulkModal"),
            to_native_settings: num("toNativeSettings"),
            to_native_other: num("toNativeOther"),
            to_terminal_more_menu: num("toTerminalMoreMenu"),
            to_terminal_settings: num("toTerminalSettings"),
            to_terminal_other: num("toTerminalOther"),
        }
    }

    fn to_value(&self) -> Value {
        json!({
            "toNativePill": self.to_native_pill,
            "toNativeMoreMenu": self.to_native_more_menu,
            "toNativeBulkModal": self.to_native_bulk_modal,
            "toNativeSettings": self.to_native_settings,
            "toNativeOther": self.to_native_other,
            "toTerminalMoreMenu": self.to_terminal_more_menu,
            "toTerminalSettings": self.to_terminal_settings,
            "toTerminalOther": self.to_terminal_other,
        })
    }
}

static PENDING_SWITCH_COUNTS: LazyLock<Mutex<SessionModeSwitchCounts>> =
    LazyLock::new(Mutex::default);

#[derive(Default)]
struct ActivityTracker {
    last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    dirty: bool,
}

static ACTIVITY_TRACKER: LazyLock<Mutex<ActivityTracker>> = LazyLock::new(Mutex::default);

/// Records active Ghostex client or agent-session usage in memory without
/// touching SQLite; flushed to `analytics.state.v1` by the telemetry background
/// tasks so daily heartbeats are strictly activity-gated (Q20).
pub fn record_activity() {
    let now = chrono::Utc::now();
    {
        let mut tracker = ACTIVITY_TRACKER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        tracker.last_activity_at = Some(now);
        tracker.dirty = true;
    }
    super::task::notify_activity();
}

pub fn clear_pending_activity() {
    let mut tracker = ACTIVITY_TRACKER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    tracker.last_activity_at = None;
    tracker.dirty = false;
}

/// Persists the latest in-memory activity timestamp into `analytics.state.v1`
/// when dirty, so activity observed right before a daemon restart survives.
pub fn persist_pending_activity(db: &Connection) {
    let pending = {
        let mut tracker = ACTIVITY_TRACKER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !tracker.dirty {
            return;
        }
        tracker.dirty = false;
        tracker.last_activity_at
    };
    let Some(seen_at) = pending else {
        return;
    };
    write_last_activity_at(
        db,
        &seen_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    );
}

/// Reads only the in-memory activity timestamp without reading SQLite. Used in
/// `weekly_count_only` mode so no activity timestamp is persisted to disk.
pub fn read_in_memory_activity_at() -> Option<chrono::DateTime<chrono::Utc>> {
    ACTIVITY_TRACKER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .last_activity_at
}

pub fn read_last_activity_at(db: &Connection) -> Option<chrono::DateTime<chrono::Utc>> {
    let in_memory = read_in_memory_activity_at();
    let stored = read_analytics_state(db)
        .and_then(|value| {
            value
                .get(LAST_ACTIVITY_AT_KEY)
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(&raw).ok())
        .map(|parsed| parsed.with_timezone(&chrono::Utc));
    match (in_memory, stored) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub fn write_last_activity_at(db: &Connection, seen_at: &str) {
    write_state_key(db, LAST_ACTIVITY_AT_KEY, Value::String(seen_at.to_string()));
}

/// True only when a client or agent session was actively used AFTER the
/// previous heartbeat AND within `max_idle_hours` of `now`. Guarantees an idle
/// daemon never emits passive daily heartbeats or settings snapshots.
pub fn has_recent_activity_since_heartbeat(
    now: chrono::DateTime<chrono::Utc>,
    last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    max_idle_hours: i64,
) -> bool {
    let Some(activity) = last_activity_at else {
        return false;
    };
    if let Some(previous_heartbeat) = last_heartbeat_at {
        if activity <= previous_heartbeat {
            return false;
        }
    }
    let elapsed = now.signed_duration_since(activity);
    elapsed <= chrono::Duration::hours(max_idle_hours) && elapsed >= chrono::Duration::minutes(-5)
}

/// Deletes the entire `analytics.state.v1` row from SQLite and clears all
/// in-memory counters and activity timestamps (called immediately on hard opt-out).
pub fn wipe_all_state(db: &Connection) {
    clear_pending_switches();
    clear_pending_activity();
    if let Err(error) = db.execute(
        "DELETE FROM metadata WHERE key = ?1",
        [ANALYTICS_STATE_METADATA_KEY],
    ) {
        super::debug_log(format!("telemetry state wipe failed: {error}"));
    }
}

/// Wipes all product telemetry detail fields from `analytics.state.v1`
/// (`lastHeartbeatAt`, `lastActivityAt`, `cachedAccounts`, `builtInExtensionsOn`,
/// `builtInExtensionsKnown`, `lastMobileHelloAt`, `sessionModeSwitches`),
/// retaining ONLY `lastWeeklyCountAt` so `install.weekly_count` never fires
/// more than once every 7 days when the UI toggle is OFF (Q29).
pub fn wipe_product_state(db: &Connection) {
    clear_pending_switches();
    let kept_weekly = read_analytics_state(db).and_then(|value| {
        value
            .get(LAST_WEEKLY_COUNT_AT_KEY)
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    match kept_weekly {
        Some(ts) => {
            let serialized = json!({ LAST_WEEKLY_COUNT_AT_KEY: ts }).to_string();
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            if let Err(error) = db.execute(
                r#"
                INSERT INTO metadata (key, value, updatedAt)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
                "#,
                rusqlite::params![ANALYTICS_STATE_METADATA_KEY, serialized, now],
            ) {
                super::debug_log(format!("telemetry product state wipe failed: {error}"));
            }
        }
        None => {
            if let Err(error) = db.execute(
                "DELETE FROM metadata WHERE key = ?1",
                [ANALYTICS_STATE_METADATA_KEY],
            ) {
                super::debug_log(format!("telemetry state wipe failed: {error}"));
            }
        }
    }
}

pub fn read_last_weekly_count_at(db: &Connection) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = read_analytics_state(db)?;
    let raw = value
        .get(LAST_WEEKLY_COUNT_AT_KEY)
        .and_then(Value::as_str)?;
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

pub fn write_last_weekly_count_at(db: &Connection, sent_at: &str) {
    write_state_key(
        db,
        LAST_WEEKLY_COUNT_AT_KEY,
        Value::String(sent_at.to_string()),
    );
}

pub fn weekly_count_is_due(
    last_sent_at: Option<chrono::DateTime<chrono::Utc>>,
    stale_days: i64,
) -> bool {
    let Some(last_sent_at) = last_sent_at else {
        return true;
    };
    chrono::Utc::now().signed_duration_since(last_sent_at) >= chrono::Duration::days(stale_days)
}

/// True only when a Ghostex client or agent session was actively used AFTER the
/// previous weekly count AND within `max_idle_days` (7 days) of `now`.
pub fn has_recent_activity_since_weekly_count(
    now: chrono::DateTime<chrono::Utc>,
    last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    last_weekly_count_at: Option<chrono::DateTime<chrono::Utc>>,
    max_idle_days: i64,
) -> bool {
    let Some(activity) = last_activity_at else {
        return false;
    };
    if let Some(previous) = last_weekly_count_at {
        if activity <= previous {
            return false;
        }
    }
    let elapsed = now.signed_duration_since(activity);
    elapsed <= chrono::Duration::days(max_idle_days) && elapsed >= chrono::Duration::minutes(-5)
}

/// Records one switch in memory without touching SQLite; flushed to `analytics.state.v1` by the
/// telemetry background worker.
pub fn record_pending_switch(direction: &str, source: &str) {
    let mut counts = PENDING_SWITCH_COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counts.record(direction, source);
}

pub fn take_pending_switches() -> SessionModeSwitchCounts {
    let mut counts = PENDING_SWITCH_COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *counts)
}

pub fn clear_pending_switches() {
    let mut counts = PENDING_SWITCH_COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts = SessionModeSwitchCounts::default();
}

/// Persists any in-memory switch counts into `analytics.state.v1` so they survive daemon restarts.
pub fn persist_pending_switches(db: &Connection) {
    let pending = take_pending_switches();
    if pending.is_empty() {
        return;
    }
    let mut stored = read_analytics_state(db)
        .and_then(|state| {
            state
                .get(SESSION_MODE_SWITCHES_KEY)
                .map(SessionModeSwitchCounts::from_value)
        })
        .unwrap_or_default();
    stored.merge(&pending);
    write_state_key(db, SESSION_MODE_SWITCHES_KEY, stored.to_value());
}

/// Returns the combined stored + in-memory switch counts since the last daily rollup and resets
/// both to zero.
pub fn take_daily_switches(db: &Connection) -> SessionModeSwitchCounts {
    let pending = take_pending_switches();
    let mut stored = read_analytics_state(db)
        .and_then(|state| {
            state
                .get(SESSION_MODE_SWITCHES_KEY)
                .map(SessionModeSwitchCounts::from_value)
        })
        .unwrap_or_default();
    stored.merge(&pending);
    if !stored.is_empty() {
        update_state(db, |state| {
            state.remove(SESSION_MODE_SWITCHES_KEY);
        });
    }
    stored
}

/// Clears stored and pending switch counts (called while analytics are opted out).
pub fn clear_session_mode_switches(db: &Connection) {
    clear_pending_switches();
    let Some(state) = read_analytics_state(db) else {
        return;
    };
    if state.get(SESSION_MODE_SWITCHES_KEY).is_none() {
        return;
    }
    update_state(db, |state| {
        state.remove(SESSION_MODE_SWITCHES_KEY);
    });
}

pub fn read_analytics_state(db: &Connection) -> Option<Value> {
    let text = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [ANALYTICS_STATE_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()?;
    serde_json::from_str::<Value>(&text).ok()
}

pub fn read_last_heartbeat_at(db: &Connection) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = read_analytics_state(db)?;
    let raw = value.get(LAST_HEARTBEAT_AT_KEY).and_then(Value::as_str)?;
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

pub fn write_last_heartbeat_at(db: &Connection, sent_at: &str) {
    write_state_key(
        db,
        LAST_HEARTBEAT_AT_KEY,
        Value::String(sent_at.to_string()),
    );
}

/// The built-in extensions known, and those on, at the last settings snapshot, so the next one
/// can tell which the user switched on or off since (CDXC:Telemetry 2026-11-20 in task.rs).
/// `None` before the first snapshot, after analytics were off, or from a state written before the
/// known list was kept.
pub fn read_built_in_extensions(db: &Connection) -> Option<(Vec<String>, Vec<String>)> {
    let value = read_analytics_state(db)?;
    let list = |key: &str| -> Option<Vec<String>> {
        Some(
            value
                .get(key)?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        )
    };
    Some((
        list(BUILT_IN_EXTENSIONS_KNOWN_KEY)?,
        list(BUILT_IN_EXTENSIONS_ON_KEY)?,
    ))
}

pub fn write_built_in_extensions(db: &Connection, known: &[&'static str], on: &[&'static str]) {
    update_state(db, |state| {
        state.insert(BUILT_IN_EXTENSIONS_KNOWN_KEY.to_string(), json!(known));
        state.insert(BUILT_IN_EXTENSIONS_ON_KEY.to_string(), json!(on));
    });
}

/// Forget both lists (analytics are off). Writes only when there is something to forget.
pub fn clear_built_in_extensions(db: &Connection) {
    let Some(state) = read_analytics_state(db) else {
        return;
    };
    if state.get(BUILT_IN_EXTENSIONS_KNOWN_KEY).is_none()
        && state.get(BUILT_IN_EXTENSIONS_ON_KEY).is_none()
    {
        return;
    }
    update_state(db, |state| {
        state.remove(BUILT_IN_EXTENSIONS_KNOWN_KEY);
        state.remove(BUILT_IN_EXTENSIONS_ON_KEY);
    });
}

/// When this install last received an explicit `ghostex client-hello` from a mobile app.
pub fn read_last_mobile_hello_at(db: &Connection) -> Option<chrono::DateTime<chrono::Utc>> {
    let value = read_analytics_state(db)?;
    let raw = value
        .get(LAST_MOBILE_HELLO_AT_KEY)
        .and_then(Value::as_str)?;
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&chrono::Utc))
}

pub fn write_last_mobile_hello_at(db: &Connection, seen_at: &str) {
    write_state_key(
        db,
        LAST_MOBILE_HELLO_AT_KEY,
        Value::String(seen_at.to_string()),
    );
}

pub fn clear_mobile_hello(db: &Connection) {
    let Some(state) = read_analytics_state(db) else {
        return;
    };
    if state.get(LAST_MOBILE_HELLO_AT_KEY).is_none() {
        return;
    }
    update_state(db, |state| {
        state.remove(LAST_MOBILE_HELLO_AT_KEY);
    });
}

/// Cached anonymous account counts and plan enums from the last account discovery,
/// so a daily `settings.snapshot` sent at daemon startup before a client opens
/// still populates `claude_plans` / `claude_top_plan` / `codex_plans` / `codex_top_plan`.
pub fn read_cached_accounts(db: &Connection) -> Option<super::settings_snapshot::AccountsReport> {
    let value = read_analytics_state(db)?;
    let cached = value.get(CACHED_ACCOUNTS_KEY)?.as_object()?;
    let mut report = super::settings_snapshot::AccountsReport::default();
    if let Some(counts) = cached.get("counts").and_then(Value::as_object) {
        for provider in ["claude", "codex", "cursor", "antigravity", "copilot"] {
            if let Some(count) = counts.get(provider).and_then(Value::as_u64) {
                report.counts.insert(provider, count as usize);
            }
        }
    }
    let read_plans = |key: &str, table: &'static [&'static str]| -> Vec<&'static str> {
        let mut plans: Vec<&'static str> = cached
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|plan| super::taxonomy::match_enum(table, plan))
            .collect();
        plans.sort_unstable();
        plans.dedup();
        plans
    };
    report.claude_plans = read_plans("claudePlans", super::taxonomy::CLAUDE_PLANS);
    report.codex_plans = read_plans("codexPlans", super::taxonomy::CODEX_PLANS);
    Some(report)
}

pub fn write_cached_accounts(db: &Connection, report: &super::settings_snapshot::AccountsReport) {
    let counts: serde_json::Map<String, Value> = report
        .counts
        .iter()
        .map(|(provider, count)| ((*provider).to_string(), json!(*count)))
        .collect();
    write_state_key(
        db,
        CACHED_ACCOUNTS_KEY,
        json!({
            "counts": counts,
            "claudePlans": report.claude_plans,
            "codexPlans": report.codex_plans,
        }),
    );
}

pub fn clear_cached_accounts(db: &Connection) {
    let Some(state) = read_analytics_state(db) else {
        return;
    };
    if state.get(CACHED_ACCOUNTS_KEY).is_none() {
        return;
    }
    update_state(db, |state| {
        state.remove(CACHED_ACCOUNTS_KEY);
    });
}

fn write_state_key(db: &Connection, key: &str, value: Value) {
    update_state(db, |state| {
        state.insert(key.to_string(), value);
    });
}

fn update_state(db: &Connection, change: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    let mut state = read_analytics_state(db)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    change(&mut state);
    let Ok(serialized) = serde_json::to_string(&Value::Object(state)) else {
        return;
    };
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if let Err(error) = db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![ANALYTICS_STATE_METADATA_KEY, serialized, now],
    ) {
        super::debug_log(format!("telemetry state write failed: {error}"));
    }
}

/// Convenience for the "is a heartbeat due?" question, kept here so the rule
/// lives next to the value it reads.
pub fn heartbeat_is_due(
    last_sent_at: Option<chrono::DateTime<chrono::Utc>>,
    stale_hours: i64,
) -> bool {
    let Some(last_sent_at) = last_sent_at else {
        return true;
    };
    chrono::Utc::now()
        .signed_duration_since(last_sent_at)
        .num_hours()
        >= stale_hours
}

/// The empty state a brand-new install starts from, spelled out so the shape is
/// discoverable without running the daemon.
pub fn empty_state() -> Value {
    json!({})
}
