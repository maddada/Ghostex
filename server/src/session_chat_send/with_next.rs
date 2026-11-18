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
//! messages live in the session's record (`runtimeSettings.heldWithNext`), so they outlive a
//! restart, and go as messages of their own, so the chat shows each as the agent message it is.
//! SEE-ALSO: server/src/ghostex_cli/agents/delivery.rs (`hold_with_next`).

use super::*;

/// The key in `runtimeSettings`.
const RUNTIME_KEY: &str = "heldWithNext";
/// Messages one session may hold; a sender with more to say is better off saying it in one.
const MAX_HELD: usize = 20;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_held(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<Value> {
    DomainRepository::new(db, "")
        .get_session(project_id, session_id)
        .ok()
        .flatten()
        .and_then(|session| {
            session
                .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
}

fn write_held(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    held: &[Value],
) -> Result<(), DomainStateError> {
    db.execute(
        &format!(
            "UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.{RUNTIME_KEY}', json(?3)) WHERE projectId = ?1 AND sessionId = ?2"
        ),
        rusqlite::params![project_id, session_id, Value::from(held.to_vec()).to_string()],
    )
    .map(|_| ())
    .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
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
        if held.len() >= MAX_HELD {
            return Err(DomainStateError::bad_request(format!(
                "This session already holds {MAX_HELD} messages for its next one. Send this one without --with-next, or say it in fewer messages."
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
