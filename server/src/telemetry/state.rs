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

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

pub const ANALYTICS_STATE_METADATA_KEY: &str = "analytics.state.v1";
pub const LAST_HEARTBEAT_AT_KEY: &str = "lastHeartbeatAt";
pub const BUILT_IN_EXTENSIONS_ON_KEY: &str = "builtInExtensionsOn";

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

/// The built-in extensions that were on at the last settings snapshot, so the next one can tell
/// which were switched on or off since (CDXC:Telemetry 2026-11-20 in task.rs). `None` before the
/// first snapshot.
pub fn read_built_in_extensions_on(db: &Connection) -> Option<Vec<String>> {
    let value = read_analytics_state(db)?;
    let list = value.get(BUILT_IN_EXTENSIONS_ON_KEY)?.as_array()?;
    Some(
        list.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    )
}

pub fn write_built_in_extensions_on(db: &Connection, ids: &[&'static str]) {
    write_state_key(db, BUILT_IN_EXTENSIONS_ON_KEY, json!(ids));
}

fn write_state_key(db: &Connection, key: &str, value: Value) {
    let mut state = read_analytics_state(db)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    state.insert(key.to_string(), value);
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
