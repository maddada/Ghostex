//! `ghostex agents send --with-next`: a message held for a session and typed right after the next
//! message that session gets (its user's, another session's report, a queued one), so a note that
//! needs nothing from the agent now never starts a turn by itself. Any session, coordinator or
//! not, whether or not Work items is on.
//!
//! CDXC:SessionChat 2026-11-18 DECISION:
//! Issue #670 asked for it between ordinary sessions: a lead session's helpers send it status
//! notes, and `--queue` only waits while the recipient is busy or asleep, so an idle lead was woken
//! by each one. The user approved `--with-next` with the work items plan (Q4134, then Q4135 A).
//! Ghostex already holds a coordinator's own informational notes this way (CDXC:Coordinators
//! 2026-11-18 at `CoordinatorNotes::asks_action` in session_chat_queue_runtime/send.rs). The held
//! messages live in `metadata` (`heldWithNext:<session>`), so they outlive a
//! restart, and go as messages of their own, so the chat shows each as the agent message it is.
//! SEE-ALSO: server/src/ghostex_cli/agents/delivery.rs (`hold_with_next`).

use super::*;

/// Where the held messages are kept (the `metadata` table), one row per session:
/// `heldWithNext:<projectId>:<sessionId>` → `[{text, sendRequestId, at}]`. Not the session's
/// `runtimeSettings`, which many paths write whole and could drop or bring back a note.
const METADATA_PREFIX: &str = "heldWithNext:";
/// Messages one session may hold; a sender with more to say is better off saying it in one.
const MAX_HELD: usize = 5;
/// How much text one session may hold in all: the send that carries them types each one.
const MAX_HELD_BYTES: usize = 64 * 1024;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn metadata_key(project_id: &str, session_id: &str) -> String {
    format!("{METADATA_PREFIX}{project_id}:{session_id}")
}

fn read_held(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<Value> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        rusqlite::params![metadata_key(project_id, session_id)],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
    .unwrap_or_default()
}

fn write_held(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    held: &[Value],
) -> Result<(), DomainStateError> {
    let key = metadata_key(project_id, session_id);
    let result = if held.is_empty() {
        db.execute(
            "DELETE FROM metadata WHERE key = ?1",
            rusqlite::params![key],
        )
    } else {
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
            rusqlite::params![
                key,
                Value::from(held.to_vec()).to_string(),
                crate::coordinators::now_iso()
            ],
        )
    };
    result
        .map(|_| ())
        .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
}

/// A restored session is a new row: what was held for the closed one moves to it
/// (previous_session_restore.rs).
pub(crate) fn move_held_with_next(db: &rusqlite::Connection, from: (&str, &str), to: (&str, &str)) {
    let _lock = lock();
    let held = read_held(db, from.0, from.1);
    if held.is_empty() {
        return;
    }
    if write_held(db, to.0, to.1, &held).is_ok() {
        let _ = write_held(db, from.0, from.1, &[]);
    }
}

/// Holds `text` for the session. Returns how many it holds now. A repeated `sendRequestId` is the
/// same message, held once.
fn hold(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
    request_id: Option<&str>,
) -> Result<usize, DomainStateError> {
    let _lock = lock();
    if DomainRepository::new(db, "")
        .get_session(project_id, session_id)?
        .is_none()
    {
        return Err(DomainStateError::not_found("No such session."));
    }
    let mut held = read_held(db, project_id, session_id);
    let repeated = request_id.is_some_and(|id| {
        held.iter()
            .any(|message| message["sendRequestId"].as_str() == Some(id))
    });
    if !repeated {
        let bytes: usize = held
            .iter()
            .filter_map(|message| message["text"].as_str())
            .map(str::len)
            .sum();
        if held.len() >= MAX_HELD {
            return Err(DomainStateError::bad_request(format!(
                "This session already holds {MAX_HELD} messages for its next one, the most it holds. Send this one without --with-next, or say it in fewer messages."
            )));
        }
        if bytes + text.len() > MAX_HELD_BYTES {
            return Err(DomainStateError::bad_request(format!(
                "This message would take what this session holds for its next one past {} KB. Send it without --with-next, or make it shorter.",
                MAX_HELD_BYTES / 1024
            )));
        }
        held.push(json!({
            "text": text,
            "sendRequestId": request_id,
            "at": crate::coordinators::now_iso(),
        }));
        write_held(db, project_id, session_id, &held)?;
    }
    Ok(held.len())
}

/// The messages held for the session, taken out to be typed now.
pub(crate) fn take_held_with_next(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let _lock = lock();
    let held = read_held(&db, project_id, session_id);
    if held.is_empty() || write_held(&db, project_id, session_id, &[]).is_err() {
        return Vec::new();
    }
    held.iter()
        .filter_map(|message| message["text"].as_str())
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .collect()
}

/// Puts back the messages that did not go, in front of any held since.
pub(crate) fn put_back_held_with_next(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    texts: &[String],
) {
    if texts.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let _lock = lock();
    let mut held: Vec<Value> = texts
        .iter()
        .map(|text| json!({ "text": text, "at": crate::coordinators::now_iso() }))
        .collect();
    held.extend(read_held(&db, project_id, session_id));
    let _ = write_held(&db, project_id, session_id, &held);
}

/// `/api/sendSessionChatMessage` with `withNext: true`: holds the message instead of typing it,
/// and answers `{held: true, count}`. `None` for any other send.
pub(crate) fn hold_with_next_response(
    state: &AppState,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
) -> Option<RoutedResponse> {
    if body.pointer("/params/withNext") != Some(&json!(true)) {
        return None;
    }
    let read = |field: &str| {
        body.pointer(&format!("/params/{field}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let held = match (read("projectId"), read("sessionId"), read("text")) {
        (Some(project_id), Some(session_id), Some(text)) => open_gxserver_database(&state.paths)
            .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
            .and_then(|db| {
                hold(
                    &db,
                    project_id,
                    session_id,
                    text,
                    read(crate::session_chat_send_requests::SEND_REQUEST_ID_PARAM),
                )
            }),
        _ => Err(DomainStateError::bad_request(
            "A held message needs projectId, sessionId and text.",
        )),
    };
    Some(match held {
        Ok(count) => routed_json(
            Some(endpoint_path.to_string()),
            StatusCode::OK,
            rpc_success(
                request_id.to_string(),
                json!({ "held": true, "count": count }),
            ),
        ),
        Err(error) => {
            domain_error_response(endpoint_path.to_string(), request_id.to_string(), error)
        }
    })
}
