//! Close After Done: a session the user armed closes once it has shown Done for three minutes.
//! This file holds the armed set (the `metadata` table), the countdown state (memory), the rule
//! and the presentation fields; `server/close_after_done_runtime.rs` runs the clock and closes.
//!
//! CDXC:Sessions 2026-09-25 DECISION:
//! User (question 1, answer 1A): auto-sleep and Close After Done run in gxserver, always, with the same settings. The runtime's timer (`close-after-done.ts`) ran only while the app window was open and only for the sessions that one app armed; the daemon now owns the armed set, so a phone or the CLI (`ghostex close-after-done`) arms the same timer and it fires with the app closed.
//!
//! CDXC:Sessions 2026-09-25 WHY:
//! The rule and the numbers are the runtime's: a session is Done while it asks for attention, or while it is not working and has an agent identity; the countdown starts when it becomes Done, starts over whenever it stops being Done, and the close is the ordinary `/api/transitionSession` close. The armed set survives a daemon restart (the runtime kept it in client storage); a running countdown does not, and starts over, as the runtime's did after an app restart. Presentation carries `closeAfterDone: true` (the phone already paints it) and `closeAfterDoneDeadlineAt` while the countdown runs.
//!
//! SEE-ALSO: server/src/server/close_after_done_runtime.rs,
//! packages/gx-core/src/sidebar_view/armed_actions.rs (the row and chat countdowns),
//! apps/desktop/src/app/gx_store/terminal_lifecycle/close_after_done.rs (the desktop's toggle).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use chrono::{SecondsFormat, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

/// How long Done must stay visible before the session closes (`GPUI_CLOSE_AFTER_DONE_DELAY_MS`).
pub const CLOSE_AFTER_DONE_DELAY_MS: i64 = 180_000;

const ARMED_METADATA_KEY: &str = "closeAfterDoneSessions";

type SessionKey = (String, String);

/// Running countdowns: the deadline of each armed session that is Done right now.
fn deadlines() -> &'static Mutex<BTreeMap<SessionKey, i64>> {
    static DEADLINES: OnceLock<Mutex<BTreeMap<SessionKey, i64>>> = OnceLock::new();
    DEADLINES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite Close After Done error: {error}"),
    }
}

/// The armed sessions.
pub fn read_armed(db: &Connection) -> Result<BTreeSet<SessionKey>, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [ARMED_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let project_id = entry.get("projectId")?.as_str()?;
            let session_id = entry.get("sessionId")?.as_str()?;
            Some((project_id.to_string(), session_id.to_string()))
        })
        .collect())
}

fn write_armed(db: &Connection, armed: &BTreeSet<SessionKey>) -> Result<(), DomainStateError> {
    let value = Value::Array(
        armed
            .iter()
            .map(|(project_id, session_id)| {
                json!({ "projectId": project_id, "sessionId": session_id })
            })
            .collect(),
    );
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            ARMED_METADATA_KEY,
            value.to_string(),
            Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// `/api/toggleCloseAfterDone`: `{ projectId, sessionId, armed? }`. Without `armed` it toggles.
/// Answers `{ armed, projectId, sessionId }`.
pub fn toggle(
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return Err(DomainStateError::bad_request(
            "toggleCloseAfterDone requires projectId and sessionId.",
        ));
    };
    let is_terminal = repository
        .get_session(&project_id, &session_id)?
        .is_some_and(|session| {
            matches!(
                session.get("kind").and_then(Value::as_str),
                Some("terminal" | "agent")
            )
        });
    if !is_terminal {
        return Err(DomainStateError::not_found(
            "Close After Done is only available for terminal sessions.",
        ));
    }
    let key = (project_id.clone(), session_id.clone());
    let mut armed = read_armed(db)?;
    let arm = params
        .get("armed")
        .and_then(Value::as_bool)
        .unwrap_or(!armed.contains(&key));
    if arm {
        armed.insert(key.clone());
    } else {
        armed.remove(&key);
        clear_deadline(&key);
    }
    write_armed(db, &armed)?;
    Ok(json!({ "armed": arm, "projectId": project_id, "sessionId": session_id }))
}

/// Forgets an armed session (it closed, or it is gone).
pub fn disarm(db: &Connection, key: &SessionKey) -> Result<(), DomainStateError> {
    let mut armed = read_armed(db)?;
    clear_deadline(key);
    if armed.remove(key) {
        write_armed(db, &armed)?;
    }
    Ok(())
}

fn clear_deadline(key: &SessionKey) {
    if let Ok(mut deadlines) = deadlines().lock() {
        deadlines.remove(key);
    }
}

/// `isGpuiCloseAfterDonePresentationSessionDone`, over a presentation session.
pub fn is_done(presentation_session: &Value) -> bool {
    let activity = presentation_session.get("activity").and_then(Value::as_str);
    if activity == Some("attention") {
        return true;
    }
    activity != Some("working")
        && [
            "agentSessionId",
            "agentSessionPath",
            "agentName",
            "agentId",
            "agentIcon",
        ]
        .iter()
        .any(|key| {
            presentation_session
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
        })
}

/// What one clock tick decided for one armed session.
#[derive(Debug, PartialEq, Eq)]
pub enum CountdownStep {
    /// Nothing changed.
    Unchanged,
    /// A countdown started or was taken back: publish the session so clients redraw it.
    Changed,
    /// The countdown ran out while the session is still Done: close it.
    Close,
}

/// Advances one armed session's countdown (`refreshCloseAfterDoneTimer`, `completeCloseAfterDoneTimer`).
pub fn step(key: &SessionKey, done: bool, now_ms: i64) -> CountdownStep {
    let Ok(mut deadlines) = deadlines().lock() else {
        return CountdownStep::Unchanged;
    };
    match (done, deadlines.get(key).copied()) {
        (false, None) => CountdownStep::Unchanged,
        (false, Some(_)) => {
            deadlines.remove(key);
            CountdownStep::Changed
        }
        (true, None) => {
            deadlines.insert(key.clone(), now_ms + CLOSE_AFTER_DONE_DELAY_MS);
            CountdownStep::Changed
        }
        (true, Some(deadline)) if now_ms >= deadline => CountdownStep::Close,
        (true, Some(_)) => CountdownStep::Unchanged,
    }
}

/// Adds `closeAfterDone` (and `closeAfterDoneDeadlineAt` while a countdown runs) to every armed
/// session of a snapshot's `sessions` array.
pub fn insert_close_after_done_presentation_payload(snapshot: &mut Value, db: &Connection) {
    let Ok(armed) = read_armed(db) else {
        return;
    };
    if armed.is_empty() {
        return;
    }
    let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) else {
        return;
    };
    for session in sessions {
        insert_projection(session, &armed);
    }
}

/// The same fields on one presentation session (a delta).
pub fn insert_close_after_done_session_projection(session: &mut Value, db: &Connection) {
    if let Ok(armed) = read_armed(db) {
        insert_projection(session, &armed);
    }
}

fn insert_projection(session: &mut Value, armed: &BTreeSet<SessionKey>) {
    let Some(object) = session.as_object_mut() else {
        return;
    };
    let key = (
        object
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        object
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    );
    if !armed.contains(&key) {
        return;
    }
    object.insert("closeAfterDone".to_string(), Value::Bool(true));
    let deadline = deadlines()
        .lock()
        .ok()
        .and_then(|deadlines| deadlines.get(&key).copied());
    if let Some(deadline_ms) = deadline {
        if let Some(deadline) = Utc.timestamp_millis_opt(deadline_ms).single() {
            object.insert(
                "closeAfterDoneDeadlineAt".to_string(),
                Value::String(deadline.to_rfc3339_opts(SecondsFormat::Millis, true)),
            );
        }
    }
}
