//! When each coordinator's last two turns started, so its work items can tell work someone is on
//! from work nobody is on (work.rs `derive`, the `idle` status).
//!
//! CDXC:Coordinators 2026-11-11 DECISION:
//! User, with 18 items under In progress of which 4 were being worked on ("why do i see 18 in progress???", then "the user should have accurate statuses in the work board/list always!"): the coordinator kept finished, delivered work open "until the user had looked at it". In progress now holds only an item with a thread working, waiting on the user or still owing work, or one the coordinator itself changed in its current or last turn. Any other open item the coordinator calls In progress is "Not being worked on" (`idle`): its own quiet group under In progress, out of every "in progress" count, with its last line, until the coordinator finishes it, parks it, asks about it, says what it waits on or changes it again. It is never finished by Ghostex. `ghostex coordinator item list` and `status` name such items and ask the coordinator to finish them or say what they wait on.
//! A turn is one working stint of the coordinator's session (`workingStartedAt`), read on every panel refresh (panel.rs), so the record needs no change to the chat send or the hooks. A stint the refresh misses (a turn of a few seconds) only makes the window longer, never shorter. Until two turns are known (a coordinator that has not worked since the update), the window is the last `NO_TURNS_WINDOW_HOURS`.
//! SEE-ALSO: server/src/coordinators/work.rs (`derive`), packages/gx-chat-core/src/extras/work_panel.rs (the group), skills/ghostex-coordinator/SKILL.md ("Work items").

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::records::{now_iso, SessionKey};

/// `{ "<projectId>:<sessionId>": ["<previous turn start>", "<current turn start>"] }`.
const TURNS_METADATA_KEY: &str = "coordinatorTurnStarts";

/// How far back a coordinator's own change counts while fewer than two of its turns are known.
pub const NO_TURNS_WINDOW_HOURS: i64 = 2;

fn record_key(key: &SessionKey) -> String {
    format!("{}:{}", key.0, key.1)
}

/// Every coordinator's last two turn starts.
pub fn read_turn_starts(db: &Connection) -> HashMap<String, (Option<String>, String)> {
    let stored: Option<String> = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [TURNS_METADATA_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let Some(Value::Object(map)) = stored.and_then(|text| serde_json::from_str(&text).ok()) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(key, value)| {
            let pair = value.as_array()?;
            let current = pair.get(1)?.as_str()?.to_string();
            let previous = pair
                .first()
                .and_then(Value::as_str)
                .filter(|at| !at.is_empty())
                .map(str::to_string);
            Some((key, (previous, current)))
        })
        .collect()
}

/// Notes that `key`'s current turn started at `started_at`; returns true when that is a new turn
/// (the record is written then).
pub fn note_turn_start(
    db: &Connection,
    starts: &mut HashMap<String, (Option<String>, String)>,
    key: &SessionKey,
    started_at: &str,
) -> bool {
    let record = record_key(key);
    let next = match starts.get(&record) {
        Some((_, current)) if current == started_at => return false,
        Some((_, current)) => (Some(current.clone()), started_at.to_string()),
        None => (None, started_at.to_string()),
    };
    starts.insert(record, next);
    let value: Map<String, Value> = starts
        .iter()
        .map(|(key, (previous, current))| {
            (
                key.clone(),
                json!([previous.clone().unwrap_or_default(), current]),
            )
        })
        .collect();
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![
            TURNS_METADATA_KEY,
            Value::Object(value).to_string(),
            now_iso()
        ],
    );
    true
}

/// The moment a coordinator's own change to an item must be at or after for the item to count as
/// being worked on: the start of its last turn before the current one, or `NO_TURNS_WINDOW_HOURS`
/// ago while fewer than two turns are known.
pub fn idle_floor(
    starts: &HashMap<String, (Option<String>, String)>,
    key: &SessionKey,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    starts
        .get(&record_key(key))
        .and_then(|(previous, _)| previous.as_deref())
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc))
        .unwrap_or_else(|| {
            // In 5-minute steps, so the panel's cache (panel.rs `PanelInputs`) is not rebuilt for
            // a floor that moved by seconds.
            let floor = (now - Duration::hours(NO_TURNS_WINDOW_HOURS)).timestamp();
            DateTime::from_timestamp(floor - floor.rem_euclid(300), 0).unwrap_or(now)
        })
}

/// [`idle_floor`] read from the database, for one coordinator.
pub fn idle_floor_for(db: &Connection, key: &SessionKey, now: DateTime<Utc>) -> DateTime<Utc> {
    idle_floor(&read_turn_starts(db), key, now)
}
