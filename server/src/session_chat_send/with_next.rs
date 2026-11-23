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

fn from_field(note: &Value, key: &str) -> Option<String> {
    note.pointer(&format!("/from/{key}"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The session on this computer that sent `note`, `None` for the user's own send, Ghostex's
/// notes and a sender on another computer.
fn note_sender(note: &Value) -> Option<(String, String)> {
    if from_field(note, "serverId").is_some() {
        return None;
    }
    Some((
        from_field(note, "projectId")?,
        from_field(note, "sessionId")?,
    ))
}

/// Who holds, lists or withdraws a message: always the caller, never the request's own words.
///
/// CDXC:SessionChat 2026-11-20 WHY:
/// The security review of #677 found `/api/heldWithNext` and the hold took the sender from the
/// body, so a session or another computer that left it out counted as the user and could withdraw
/// anyone's notes, and one that named another session could hold in its name. A session's own
/// token means that session; the app, a phone and the shared token file (the CLI in the user's own
/// terminal) mean the user; another computer is never the user: it names which of its sessions
/// sent the message, and only that computer's notes are its to withdraw.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HeldSender {
    User,
    Session(String, String),
    /// Another computer (`server_id`, its name), and the session there that sent it when it says.
    Peer {
        server_id: String,
        name: String,
        session: Option<(String, String)>,
    },
}

impl HeldSender {
    pub(crate) fn of(
        caller: &crate::security::Principal,
        body: &Value,
    ) -> Result<Self, DomainStateError> {
        use crate::security::Principal;
        match caller {
            Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => {
                Ok(Self::User)
            }
            Principal::Session {
                project_id,
                session_id,
                ..
            } => Ok(Self::Session(project_id.clone(), session_id.clone())),
            Principal::Peer { server_id, name } => Ok(Self::Peer {
                server_id: server_id.clone(),
                name: name.clone(),
                session: param(body, "senderProjectId")
                    .zip(param(body, "senderSessionId"))
                    .map(|(project, session)| (project.to_string(), session.to_string())),
            }),
            _ => Err(DomainStateError {
                code: "forbidden",
                message: "Only the user, a session or a connected computer can hold or withdraw a message for a session's next one.".to_string(),
            }),
        }
    }

    /// What a held note records as its `from`, `None` for the user.
    fn record(&self) -> Option<Value> {
        match self {
            Self::User => None,
            Self::Session(project, session) => {
                Some(json!({ "projectId": project, "sessionId": session }))
            }
            Self::Peer {
                server_id,
                name,
                session,
            } => Some(match session {
                Some((project, session)) => json!({
                    "serverId": server_id, "computer": name,
                    "projectId": project, "sessionId": session,
                }),
                None => json!({ "serverId": server_id, "computer": name }),
            }),
        }
    }

    /// Whether `note` is this sender's to withdraw (the user's: every one) or list as its own.
    fn owns(&self, note: &Value) -> bool {
        let field = |key: &str| from_field(note, key);
        match self {
            Self::User => true,
            Self::Session(project, session) => {
                note_sender(note).as_ref() == Some(&(project.clone(), session.clone()))
            }
            Self::Peer {
                server_id, session, ..
            } => {
                field("serverId").as_deref() == Some(server_id.as_str())
                    && match session {
                        Some((project, session)) => {
                            field("projectId").as_deref() == Some(project.as_str())
                                && field("sessionId").as_deref() == Some(session.as_str())
                        }
                        None => field("sessionId").is_none(),
                    }
            }
        }
    }
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
    from: Option<Value>,
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
    // The same id from the same sender: another sender who guesses an id never stops this one.
    let repeated = message.request_id.is_some_and(|id| {
        held.iter().any(|note| {
            note["sendRequestId"].as_str() == Some(id) && note.get("from") == message.from.as_ref()
        })
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
        if let Some(from) = message.from {
            note["from"] = from;
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
        let text = ghostex_gx_chat_core::session::appended_notes::neutralised_held_note(text);
        ghostex_gx_chat_core::session::appended_notes::with_system_notes("", &[&text])
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

/// The held `notes` as the one note that goes in the releasing message's `<ghostex-system>` block
/// (Claude Code), `None` for none. Each note sits between the tags the chat finds it by, to draw
/// "2 held notes went in with this message" (gx-chat-core `appended_notes::held_notes_note`).
pub(crate) fn held_notes_block(notes: &[Value]) -> Option<String> {
    let texts: Vec<String> = notes.iter().filter_map(labelled).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    ghostex_gx_chat_core::session::appended_notes::held_notes_note(&texts)
}

/// A held note's text with who sent it, where its own sender block does not say: a session's
/// message carries that block, the user's own (from their terminal) and Ghostex's notice do not.
fn labelled(note: &Value) -> Option<String> {
    let text = note["text"].as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    let computer = from_field(note, "computer").unwrap_or_else(|| "another computer".to_string());
    Some(if note["system"] == json!(true) {
        format!("From Ghostex: {text}")
    } else if from_field(note, "sessionId").is_some() {
        text.to_string()
    } else if from_field(note, "serverId").is_some() {
        // No session there named itself: that computer's user, or its Ghostex (a "Not
        // delivered"), never one of its sessions, and not this computer's Ghostex.
        format!("Held from the computer {computer}, not from a session there:\n\n{text}")
    } else {
        format!(
            "Held by the user from their own terminal (not part of their message above):\n\n{text}"
        )
    })
}

/// A held message dropped because its recipient was closed: who to tell, and what.
pub(crate) struct DroppedHeldNote {
    pub sender: (String, String),
    pub send_request_id: Option<String>,
}

/// A "Not delivered" notice for a sender on another computer (`server_id`), held there for its
/// session by the delivery watch (server/agent_message_delivery_runtime.rs).
pub(crate) struct RemoteNotice {
    pub server_id: String,
    pub sender: (String, String),
    pub notice: String,
}

/// Drops what is held for sessions that were closed (stopped) or removed, and holds a "not
/// delivered" note for each sending session that is still open, so it learns with its own next
/// message. Returns the dropped messages that came from a session, for their chat cards.
pub(crate) fn drop_held_for_closed_sessions(
    db: &rusqlite::Connection,
) -> (Vec<DroppedHeldNote>, Vec<RemoteNotice>) {
    let mut dropped = Vec::new();
    let mut remote = Vec::new();
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
            .map(|title| one_line(title, 80))
            .filter(|title| !title.is_empty())
            .map(|title| format!("\u{201c}{title}\u{201d} ({session_id})"))
            .unwrap_or_else(|| session_id.clone());
        for note in held {
            let remote_sender = from_field(&note, "serverId")
                .zip(from_field(&note, "projectId"))
                .zip(from_field(&note, "sessionId"))
                .map(|((server, project), session)| (server, (project, session)));
            if note_sender(&note).is_none() && remote_sender.is_none() {
                continue;
            }
            let words = note["body"]
                .as_str()
                .or_else(|| note["text"].as_str())
                .unwrap_or_default();
            let start = one_line(words, 160);
            let request_id = note["sendRequestId"].as_str().map(str::to_string);
            let notice = crate::coordinators::without_foreign_agent_blocks(
                &ghostex_gx_chat_core::session::appended_notes::neutralised_held_note(&format!(
                    "Not delivered: the message you sent with --with-next to {title}{} was dropped, because that session was closed before its next message. It began: \u{201c}{start}\u{201d}",
                    request_id
                        .as_deref()
                        .map(|id| format!(" (request id {})", one_line(id, 64)))
                        .unwrap_or_default()
                )),
            );
            let Some(sender) = note_sender(&note) else {
                if let Some((server_id, sender)) = remote_sender {
                    remote.push(RemoteNotice {
                        server_id,
                        sender,
                        notice,
                    });
                }
                continue;
            };
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
    (dropped, remote)
}

/// `text` on one line (its whitespace collapsed), at most `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > max {
        format!("{}\u{2026}", line.chars().take(max - 1).collect::<String>())
    } else {
        line
    }
}

/// A session's held note as it is kept: its words with every sender-block opener broken and the
/// session's true block last, whatever the words said and wherever the sender mark put the block
/// (first, for a `/` or `!` body, so it never runs as a command; a held note never runs as one).
/// `fallback` is the block when the text carries none of the session's own.
///
/// CDXC:Security 2026-11-20 WHY (Fable's re-review of #677, case E): the note's tags were broken
/// by keeping only its last block, so a `!` body that ended in a forged block had its true block
/// placed first and broken, and the held notes line named the forged sender.
///
/// `ours` is gxserver's block for a session on this computer and its record: it replaces a block
/// whose session title or agent differs from the record, and stands in for a missing one.
pub(crate) fn settled_text(text: &str, own_ref: &str, ours: Option<(&Value, String)>) -> String {
    use crate::coordinators::{find_own_agent_message_block, without_foreign_agent_blocks};
    let (body, block) = match find_own_agent_message_block(text, own_ref) {
        Some((start, end, first)) => {
            let found = if first {
                text[..start].trim().to_string()
            } else {
                text[end..].trim().to_string()
            };
            let block = match ours {
                Some((record, ours))
                    if !super::sender_mark::block_matches_record(&found, record) =>
                {
                    ours
                }
                _ => found,
            };
            (&text[start..end], Some(block))
        }
        None => (text, ours.map(|(_, ours)| ours)),
    };
    let body = without_foreign_agent_blocks(body.trim());
    match block {
        Some(block) => format!("{body}\n\n{block}"),
        None => body,
    }
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
/// Returns the senders whose chat must be republished, and the notices for senders on other
/// computers.
pub(crate) fn settle_held_for_closed_sessions(
    db: &rusqlite::Connection,
) -> (Vec<(String, String)>, Vec<RemoteNotice>) {
    let mut republish = Vec::new();
    let (dropped, remote) = drop_held_for_closed_sessions(db);
    for dropped in dropped {
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
    (republish, remote)
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
    caller: &crate::security::Principal,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
) -> Option<RoutedResponse> {
    if body.pointer("/params/withNext") != Some(&json!(true)) {
        return None;
    }
    let read = |field: &str| param(body, field);
    let held = match (read("projectId"), read("sessionId"), read("text")) {
        (Some(project_id), Some(session_id), Some(text)) => HeldSender::of(caller, body)
            .and_then(|sender| {
                open_gxserver_database(&state.paths)
                    .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
                    .map(|db| (db, sender))
            })
            .and_then(|(db, sender)| {
                let send_request_id =
                    read(crate::session_chat_send_requests::SEND_REQUEST_ID_PARAM);
                // A session another computer names is recorded only when the text carries the
                // block its relay signed for that session (Fable's last review of #677).
                let sender = match sender {
                    HeldSender::Peer {
                        server_id,
                        name,
                        session: Some((project, session)),
                    } if crate::coordinators::find_own_agent_message_block(
                        text,
                        &crate::ids::create_global_session_ref(&server_id, &project, &session),
                    )
                    .is_none() =>
                    {
                        HeldSender::Peer {
                            server_id,
                            name,
                            session: None,
                        }
                    }
                    sender => sender,
                };
                let text = match &sender {
                    // The user's own words, and Ghostex's, name no sender either.
                    HeldSender::User => crate::coordinators::without_foreign_agent_blocks(text),
                    HeldSender::Session(project, session) => {
                        let server_id = state.metadata.server_id.as_str();
                        let record = super::sender_mark::read_session(
                            &state.paths,
                            server_id,
                            project,
                            session,
                        );
                        let ours =
                            super::sender_mark::session_block(&record, server_id, project, session);
                        settled_text(
                            text,
                            &crate::ids::create_global_session_ref(server_id, project, session),
                            Some((&record, ours)),
                        )
                    }
                    HeldSender::Peer {
                        server_id,
                        session: Some((project, session)),
                        ..
                    } => settled_text(
                        text,
                        &crate::ids::create_global_session_ref(server_id, project, session),
                        None,
                    ),
                    HeldSender::Peer { .. } => {
                        crate::coordinators::without_foreign_agent_blocks(text)
                    }
                };
                let (count, duplicate) = hold(
                    &db,
                    project_id,
                    session_id,
                    HeldMessage {
                        text: &text,
                        body: read("body"),
                        request_id: send_request_id,
                        from: sender.record(),
                        system: false,
                    },
                )?;
                // A session here is followed by the delivery watch; its card lives on this
                // computer (a sender on another computer is told on its own: `RemoteNotice`).
                if let (HeldSender::Session(sender_project, sender_session), Some(id), false) =
                    (&sender, send_request_id, duplicate)
                {
                    let (sender_project, sender_session) =
                        (sender_project.as_str(), sender_session.as_str());
                    let mut watch = Map::new();
                    for (key, value) in [
                        ("senderProjectId", sender_project),
                        ("senderSessionId", sender_session),
                        ("projectId", project_id),
                        ("sessionId", session_id),
                        ("sendRequestId", id),
                        ("message", read("body").unwrap_or(&text)),
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

fn held_with_next(
    db: &rusqlite::Connection,
    body: &Value,
    caller: &HeldSender,
) -> Result<Value, DomainStateError> {
    let recipient = param(body, "projectId")
        .zip(param(body, "sessionId"))
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
                    // No recipient: what the caller holds anywhere on this computer (the user:
                    // everything held here).
                    None => read_all_held(db)
                        .iter()
                        .flat_map(|(recipient, held)| {
                            held.iter()
                                .filter(|note| caller.owns(note))
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
                        caller.owns(note)
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
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = HeldSender::of(caller, body).and_then(|sender| {
        open_gxserver_database(&state.paths)
            .map_err(|error| DomainStateError::corrupt_state(error.to_string()))
            .and_then(|db| held_with_next(&db, body, &sender))
    });
    match result {
        Ok(answer) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, answer),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
