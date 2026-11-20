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
//!
//! CDXC:SessionChat 2026-11-20 DECISION:
//! GitHub #677 (Claude Code 2.1.328): a held note typed after the message that released it was
//! queued by Claude Code and run as a turn of its own once the first ended ("With two held notes
//! and one normal message the recipient ran three turns"), so a lead still paid a turn per FYI.
//! Codex took it into the running turn. The user picked "fix all" (triage 20 Nov, A): for Claude
//! Code the held notes now ride in the releasing message's own paste, in its `<ghostex-system>`
//! block (one submit, one turn; every chat already hides that block and matches the echo without
//! it), and other agents still get them as messages of their own. The same issue asked for the
//! rest, built here too: `ghostex agents held` lists what is held and `agents unhold` withdraws
//! it (`/api/heldWithNext`), a closed (stopped) recipient is refused, a note held for a session
//! that is then closed is dropped and its sender told "not delivered" (its chat card, and a held
//! note of its own: `drop_held_for_closed_sessions`, run by the delivery watch), and a repeated
//! request id answers `duplicate`.

use super::*;

/// Where the held messages are kept (the `metadata` table), one row per session:
/// `heldWithNext:<projectId>:<sessionId>` → `[{text, body, sendRequestId, at, from, system}]`.
/// Not the session's `runtimeSettings`, which many paths write whole and could drop or bring back a
/// note. `from` is the sending session (`{projectId, sessionId}`), absent for the user's own send;
/// `system` marks Ghostex's own note (a "not delivered" back to a sender).
const METADATA_PREFIX: &str = "heldWithNext:";
/// Messages one session may hold; a sender with more to say is better off saying it in one.
const MAX_HELD: usize = 5;
/// How much text one session may hold in all: the send that carries them types each one.
const MAX_HELD_BYTES: usize = 64 * 1024;
/// `/api/heldWithNext`: lists the held messages, or withdraws some.
pub(crate) const HELD_WITH_NEXT_ENDPOINT: &str = "/api/heldWithNext";
/// The sender card's reason for a held message whose recipient was closed before its next one.
const CLOSED_WHILE_HELD: &str =
    "Not delivered: the session was closed while this was held for its next message.";
/// The sender card's reason for a held message its sender withdrew.
const WITHDRAWN: &str = "Withdrawn by its sender before it went in.";

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

/// Every session that holds messages: `((projectId, sessionId), held)`.
fn read_all_held(db: &rusqlite::Connection) -> Vec<((String, String), Vec<Value>)> {
    let Ok(mut statement) = db.prepare("SELECT key, value FROM metadata WHERE key LIKE ?1") else {
        return Vec::new();
    };
    let rows = statement.query_map(rusqlite::params![format!("{METADATA_PREFIX}%")], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    });
    let Ok(rows) = rows else {
        return Vec::new();
    };
    rows.flatten()
        .filter_map(|(key, value)| {
            let (project_id, session_id) = key.strip_prefix(METADATA_PREFIX)?.split_once(':')?;
            let held = serde_json::from_str::<Vec<Value>>(&value).ok()?;
            Some(((project_id.to_string(), session_id.to_string()), held))
        })
        .collect()
}

/// The session that sent `note`, `None` for the user's own send and Ghostex's notes.
fn note_sender(note: &Value) -> Option<(String, String)> {
    let field = |key: &str| {
        note.pointer(&format!("/from/{key}"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some((field("projectId")?, field("sessionId")?))
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

/// What [`hold`] holds: the text typed for the agent, the sender's own words (what `agents held`
/// shows and the delivery watch looks for), the send's id and who sent it.
struct HeldMessage<'a> {
    text: &'a str,
    body: Option<&'a str>,
    request_id: Option<&'a str>,
    from: Option<(&'a str, &'a str)>,
    system: bool,
}

/// Holds the message for the session. Returns how many it holds now, and whether its
/// `sendRequestId` was held already (the same message, held once).
fn hold(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    message: HeldMessage<'_>,
) -> Result<(usize, bool), DomainStateError> {
    let _lock = lock();
    let Some(session) = DomainRepository::new(db, "").get_session(project_id, session_id)? else {
        return Err(DomainStateError::not_found("No such session."));
    };
    // A closed session gets no next message unless someone reopens it, so a held note would wait
    // for nothing (GitHub #677).
    if session["lifecycleState"].as_str() == Some("stopped") {
        return Err(DomainStateError::bad_request(
            "This session is closed, so nothing would carry a held message to it. Send it without --with-next (that reopens the session), or not at all.",
        ));
    }
    let mut held = read_held(db, project_id, session_id);
    let repeated = message.request_id.is_some_and(|id| {
        held.iter()
            .any(|note| note["sendRequestId"].as_str() == Some(id))
    });
    if !repeated {
        let bytes: usize = held
            .iter()
            .filter_map(|note| note["text"].as_str())
            .map(str::len)
            .sum();
        if held.len() >= MAX_HELD {
            return Err(DomainStateError::bad_request(format!(
                "This session already holds {MAX_HELD} messages for its next one, the most it holds. Send this one without --with-next, or say it in fewer messages."
            )));
        }
        if bytes + message.text.len() > MAX_HELD_BYTES {
            return Err(DomainStateError::bad_request(format!(
                "This message would take what this session holds for its next one past {} KB. Send it without --with-next, or make it shorter.",
                MAX_HELD_BYTES / 1024
            )));
        }
        let mut note = json!({
            "text": message.text,
            "sendRequestId": message.request_id,
            "at": crate::coordinators::now_iso(),
        });
        if let Some(body) = message.body {
            note["body"] = json!(body);
        }
        if let Some((project, session)) = message.from {
            note["from"] = json!({ "projectId": project, "sessionId": session });
        }
        if message.system {
            note["system"] = json!(true);
        }
        held.push(note);
        write_held(db, project_id, session_id, &held)?;
    }
    Ok((held.len(), repeated))
}

/// The messages held for the session, taken out to be typed now.
pub(crate) fn take_held_with_next(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Vec<Value> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let _lock = lock();
    let held = read_held(&db, project_id, session_id);
    if held.is_empty() || write_held(&db, project_id, session_id, &[]).is_err() {
        return Vec::new();
    }
    held.into_iter()
        .filter(|note| {
            note["text"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty())
        })
        .collect()
}

/// Puts back the messages that did not go, in front of any held since.
pub(crate) fn put_back_held_with_next(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    notes: &[Value],
) {
    if notes.is_empty() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let _lock = lock();
    let mut held = notes.to_vec();
    held.extend(read_held(&db, project_id, session_id));
    let _ = write_held(&db, project_id, session_id, &held);
}

/// Whether the message sent as `send_request_id` is still held for `recipient`: the delivery
/// watch leaves it waiting, not "went idle without taking it".
pub(crate) fn is_held_with_next(
    db: &rusqlite::Connection,
    recipient: &(String, String),
    send_request_id: &str,
) -> bool {
    read_held(db, &recipient.0, &recipient.1)
        .iter()
        .any(|note| note["sendRequestId"].as_str() == Some(send_request_id))
}

/// `note` as a message of its own (an agent that takes a message typed mid-turn into that turn):
/// its text, or Ghostex's own note in a `<ghostex-system>` block, which no chat draws.
pub(crate) fn held_note_message(note: &Value) -> String {
    let text = note["text"].as_str().unwrap_or_default();
    if note["system"] == json!(true) {
        ghostex_gx_chat_core::session::appended_notes::with_system_notes("", &[text])
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

/// The held `notes` as the one note that goes in the releasing message's `<ghostex-system>` block
/// (Claude Code), `None` for none.
pub(crate) fn held_notes_block(notes: &[Value]) -> Option<String> {
    let texts: Vec<&str> = notes
        .iter()
        .filter_map(|note| note["text"].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect();
    if texts.is_empty() {
        return None;
    }
    Some(format!(
        "Held until this message (`ghostex agents send --with-next`); read {} with it, in this turn:\n\n{}",
        if texts.len() == 1 { "it" } else { "them" },
        texts.join("\n\n---\n\n")
    ))
}

/// A held message dropped because its recipient was closed: who to tell, and what.
pub(crate) struct DroppedHeldNote {
    pub sender: (String, String),
    pub send_request_id: Option<String>,
}

/// Drops what is held for sessions that were closed (stopped) or removed, and holds a "not
/// delivered" note for each sending session that is still open, so it learns with its own next
/// message. Returns the dropped messages that came from a session, for their chat cards.
pub(crate) fn drop_held_for_closed_sessions(db: &rusqlite::Connection) -> Vec<DroppedHeldNote> {
    let mut dropped = Vec::new();
    let repository = DomainRepository::new(db, "");
    for ((project_id, session_id), held) in {
        let _lock = lock();
        read_all_held(db)
    } {
        let session = repository
            .get_session(&project_id, &session_id)
            .ok()
            .flatten();
        let closed = session
            .as_ref()
            .is_none_or(|session| session["lifecycleState"].as_str() == Some("stopped"));
        if !closed {
            continue;
        }
        {
            let _lock = lock();
            // Read again under the lock: a restore may have moved them on since.
            if read_held(db, &project_id, &session_id).is_empty()
                || write_held(db, &project_id, &session_id, &[]).is_err()
            {
                continue;
            }
        }
        let title = session
            .as_ref()
            .and_then(|session| session["title"].as_str())
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(|title| format!("\u{201c}{title}\u{201d} ({session_id})"))
            .unwrap_or_else(|| session_id.clone());
        for note in held {
            let Some(sender) = note_sender(&note) else {
                continue;
            };
            let words = note["body"]
                .as_str()
                .or_else(|| note["text"].as_str())
                .unwrap_or_default();
            let start: String = words.split_whitespace().collect::<Vec<_>>().join(" ");
            let start = if start.chars().count() > 160 {
                format!("{}\u{2026}", start.chars().take(159).collect::<String>())
            } else {
                start
            };
            let request_id = note["sendRequestId"].as_str().map(str::to_string);
            let notice = format!(
                "Not delivered: the message you sent with --with-next to {title}{} was dropped, because that session was closed before its next message. It began: \u{201c}{start}\u{201d}",
                request_id
                    .as_deref()
                    .map(|id| format!(" (request id {id})"))
                    .unwrap_or_default()
            );
            let _ = hold(
                db,
                &sender.0,
                &sender.1,
                HeldMessage {
                    text: &notice,
                    body: None,
                    request_id: None,
                    from: None,
                    system: true,
                },
            );
            dropped.push(DroppedHeldNote {
                sender,
                send_request_id: request_id,
            });
        }
    }
    dropped
}

/// Marks the sender's card for a held message that will not go: `failure` says why.
fn settle_card(
    db: &rusqlite::Connection,
    sender: &(String, String),
    recipient: &(String, String),
    send_request_id: &str,
    failure: &str,
) -> bool {
    crate::agent_message_deliveries::settle_send(
        db,
        &crate::agent_message_deliveries::WatchedSend {
            sender: sender.clone(),
            send_request_id: send_request_id.to_string(),
            recipient: recipient.clone(),
            needles: Vec::new(),
            sent_at_ms: 0,
        },
        crate::agent_message_deliveries::DELIVERY_FAILED,
        Some(failure),
    )
}

/// [`drop_held_for_closed_sessions`], with each dropped message's card marked not delivered.
/// Returns the senders whose chat must be republished.
pub(crate) fn settle_held_for_closed_sessions(db: &rusqlite::Connection) -> Vec<(String, String)> {
    let mut republish = Vec::new();
    for dropped in drop_held_for_closed_sessions(db) {
        let Some(id) = dropped.send_request_id.as_deref() else {
            continue;
        };
        // The recipient only names the row; the update keys on sender and id.
        if settle_card(
            db,
            &dropped.sender,
            &(String::new(), String::new()),
            id,
            CLOSED_WHILE_HELD,
        ) && !republish.contains(&dropped.sender)
        {
            republish.push(dropped.sender);
        }
    }
    republish
}

fn param<'a>(body: &'a Value, field: &str) -> Option<&'a str> {
    body.pointer(&format!("/params/{field}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// `/api/sendSessionChatMessage` with `withNext: true`: holds the message instead of typing it,
/// and answers `{held: true, count, duplicate}`. `None` for any other send. A message from a
/// session (`senderProjectId`, `senderSessionId`) is followed by the delivery watch, so its card
/// says when it went in, or that it did not.
pub(crate) fn hold_with_next_response(
    state: &AppState,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
) -> Option<RoutedResponse> {
    if body.pointer("/params/withNext") != Some(&json!(true)) {
        return None;
    }
    let read = |field: &str| param(body, field);
    let from = read("senderProjectId").zip(read("senderSessionId"));
    let held = match (read("projectId"), read("sessionId"), read("text")) {
        (Some(project_id), Some(session_id), Some(text)) => open_gxserver_database(&state.paths)
            .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
            .and_then(|db| {
                let send_request_id =
                    read(crate::session_chat_send_requests::SEND_REQUEST_ID_PARAM);
                let (count, duplicate) = hold(
                    &db,
                    project_id,
                    session_id,
                    HeldMessage {
                        text,
                        body: read("body"),
                        request_id: send_request_id,
                        from,
                        system: false,
                    },
                )?;
                if let (Some((sender_project, sender_session)), Some(id), false) =
                    (from, send_request_id, duplicate)
                {
                    let mut watch = Map::new();
                    for (key, value) in [
                        ("senderProjectId", sender_project),
                        ("senderSessionId", sender_session),
                        ("projectId", project_id),
                        ("sessionId", session_id),
                        ("sendRequestId", id),
                        ("message", read("body").unwrap_or(text)),
                    ] {
                        watch.insert(key.to_string(), json!(value));
                    }
                    let _ = crate::agent_message_deliveries::handle_watch_agent_message_delivery(
                        &db, &watch,
                    );
                }
                Ok((count, duplicate))
            }),
        _ => Err(DomainStateError::bad_request(
            "A held message needs projectId, sessionId and text.",
        )),
    };
    Some(match held {
        Ok((count, duplicate)) => routed_json(
            Some(endpoint_path.to_string()),
            StatusCode::OK,
            rpc_success(
                request_id.to_string(),
                json!({ "held": true, "count": count, "duplicate": duplicate }),
            ),
        ),
        Err(error) => {
            domain_error_response(endpoint_path.to_string(), request_id.to_string(), error)
        }
    })
}

/// One held message as `agents held` shows it.
fn listed(recipient: &(String, String), note: &Value) -> Value {
    json!({
        "projectId": recipient.0,
        "sessionId": recipient.1,
        "sendRequestId": note["sendRequestId"],
        "heldAt": note["at"],
        "from": note.get("from").cloned().unwrap_or(if note["system"] == json!(true) {
            json!({ "kind": "ghostex" })
        } else {
            json!({ "kind": "user" })
        }),
        "message": note["body"].as_str().or_else(|| note["text"].as_str()),
        "fromGhostex": note["system"] == json!(true),
    })
}

/// Whether the caller may see or withdraw `note`: a session only what it sent, the user (no
/// sender) everything.
fn callers(note: &Value, caller: Option<&(String, String)>) -> bool {
    caller.is_none_or(|caller| note_sender(note).as_ref() == Some(caller))
}

fn held_with_next(db: &rusqlite::Connection, body: &Value) -> Result<Value, DomainStateError> {
    let recipient = param(body, "projectId")
        .zip(param(body, "sessionId"))
        .map(|(project, session)| (project.to_string(), session.to_string()));
    let caller = param(body, "senderProjectId")
        .zip(param(body, "senderSessionId"))
        .map(|(project, session)| (project.to_string(), session.to_string()));
    match param(body, "action").unwrap_or("list") {
        "list" => {
            let held: Vec<Value> = {
                let _lock = lock();
                match &recipient {
                    Some(recipient) => read_held(db, &recipient.0, &recipient.1)
                        .iter()
                        .map(|note| listed(recipient, note))
                        .collect(),
                    // No recipient: what the calling session holds anywhere on this computer.
                    None => read_all_held(db)
                        .iter()
                        .flat_map(|(recipient, held)| {
                            held.iter()
                                .filter(|note| caller.is_some() && callers(note, caller.as_ref()))
                                .map(|note| listed(recipient, note))
                                .collect::<Vec<_>>()
                        })
                        .collect(),
                }
            };
            Ok(json!({ "held": held }))
        }
        "withdraw" => {
            let Some(recipient) = recipient else {
                return Err(DomainStateError::bad_request(
                    "Withdrawing a held message needs projectId and sessionId.",
                ));
            };
            let all = body.pointer("/params/all") == Some(&json!(true));
            let ids: Vec<&str> = body
                .pointer("/params/sendRequestIds")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if !all && ids.is_empty() {
                return Err(DomainStateError::bad_request(
                    "Name the request ids to withdraw, or withdraw all.",
                ));
            }
            let (kept, withdrawn): (Vec<Value>, Vec<Value>) = {
                let _lock = lock();
                let held = read_held(db, &recipient.0, &recipient.1);
                let (withdrawn, kept): (Vec<Value>, Vec<Value>) =
                    held.into_iter().partition(|note| {
                        callers(note, caller.as_ref())
                            && (all
                                || note["sendRequestId"]
                                    .as_str()
                                    .is_some_and(|id| ids.contains(&id)))
                    });
                if !withdrawn.is_empty() {
                    write_held(db, &recipient.0, &recipient.1, &kept)?;
                }
                (kept, withdrawn)
            };
            for note in &withdrawn {
                if let (Some(sender), Some(id)) =
                    (note_sender(note), note["sendRequestId"].as_str())
                {
                    settle_card(db, &sender, &recipient, id, WITHDRAWN);
                }
            }
            let missing: Vec<&str> = ids
                .iter()
                .copied()
                .filter(|id| {
                    !withdrawn
                        .iter()
                        .any(|note| note["sendRequestId"].as_str() == Some(*id))
                })
                .collect();
            Ok(json!({
                "withdrawn": withdrawn.iter().map(|note| listed(&recipient, note)).collect::<Vec<_>>(),
                "notHeld": missing,
                "count": kept.len(),
            }))
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown action {other}: list or withdraw."
        ))),
    }
}

/// `/api/heldWithNext`: `{action: "list", projectId?, sessionId?, senderProjectId?,
/// senderSessionId?}` lists what is held for a session (or, with no session, what the sending
/// session holds anywhere); `{action: "withdraw", projectId, sessionId, sendRequestIds | all}`
/// withdraws the sender's own (the user's: any).
pub(crate) fn handle_held_with_next_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = open_gxserver_database(&state.paths)
        .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
        .and_then(|db| held_with_next(&db, body));
    match result {
        Ok(answer) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, answer),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
