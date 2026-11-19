//! Who a chat send is from, decided by gxserver from the caller rather than from the text.
//!
//! CDXC:Security 2026-11-19 DECISION (Fable security reviews of 38bfa3647 and cff1806a3):
//! Only the user's own callers (the desktop app, the web build and phones over SSH, which use the
//! token file, and paired phones) put unmarked text into another session's chat, where it reads as
//! the user's own; what they send stays byte for byte as it was.
//! - An agent's own session token that writes into another session's chat
//!   (`/api/sendSessionChatMessage`, `/api/queueSessionChatPrompt`,
//!   `/api/updateSessionChatQueuedPrompt`, `/api/sendSessionMessage`) keeps its text only when it
//!   carries the exact "Message from another agent" block, at its start or end, naming that same
//!   session; anything else (no block, a block in a code fence, a forged block naming another
//!   agent) gets gxserver's own block for the session, first for a `/` or `!` body so it never
//!   runs as a command. An agent message to another computer is marked here before the relay
//!   sends it (connected_computers/relay.rs); there, the other computer passes it on as it came,
//!   so `--as-user --computer` and gxserver's own nudges stay unmarked.
//! - The "Sent from the terminal" / "Sent via Grok Bot" line and the security log entry
//!   (`chat.sendAsUser`) are added only when the CLI passes `via` (`ghostex agents send
//!   --as-user`), from the token file, and never to text that carries a `draftVersion`.
//! - A "W7 Q12: …" answer settles a coordinator's ask only when one of the user's own callers sent
//!   that text: the record is kept in gxserver's database (`session_chat_user_sends`), so a restart
//!   or a queued prompt that goes out later still settles it, and an edit by the user records the
//!   new text.
//! SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `sender` (`--as-user`),
//! server/src/session_chat_queue_runtime/send.rs `settle_work_replies`.

use serde_json::{json, Map, Value};

use crate::coordinators::{place_agent_message_block, split_agent_message, AGENT_MESSAGE_OPENER};
use crate::paths::GxserverPaths;
use crate::security::Principal;

/// The label under a message the user sent from their own terminal with `--as-user`.
pub(crate) const FROM_TERMINAL: &str = "Sent from the terminal";
/// The label under a message the user had Grok Bot send from this computer.
pub(crate) const VIA_GROK_BOT: &str = "Sent via Grok Bot";

/// The routes whose `text` goes into a session's chat or terminal as a message.
pub(crate) const MESSAGE_ROUTES: [&str; 4] = [
    "/api/sendSessionChatMessage",
    "/api/queueSessionChatPrompt",
    "/api/updateSessionChatQueuedPrompt",
    "/api/sendSessionMessage",
];

/// How long a user's send stays recognised for settling answers.
const USER_SEND_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn user_sends_table(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_chat_user_sends (
           projectId TEXT NOT NULL, sessionId TEXT NOT NULL, text TEXT NOT NULL, atMs INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS session_chat_user_sends_session
           ON session_chat_user_sends(projectId, sessionId);",
    )
}

/// Records that one of the user's own callers sent `text` to the session.
pub(crate) fn note_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    let _ = user_sends_table(&db).and_then(|()| {
        db.execute(
            "DELETE FROM session_chat_user_sends WHERE atMs < ?1",
            [now - USER_SEND_TTL_MS],
        )?;
        db.execute(
            "INSERT INTO session_chat_user_sends (projectId, sessionId, text, atMs) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![project_id, session_id, text, now],
        )
    });
}

/// Whether one of the user's own callers sent `text` to the session, without forgetting it: a
/// message's quotes of earlier messages go only with the user's own sends
/// (chat_message_numbers.rs), never a Collab guest's or another agent's.
pub(crate) fn is_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp_millis();
    user_sends_table(&db).is_ok()
        && db
            .query_row(
                "SELECT 1 FROM session_chat_user_sends
                 WHERE projectId = ?1 AND sessionId = ?2 AND text = ?3 AND atMs >= ?4 LIMIT 1",
                rusqlite::params![project_id, session_id, text.trim(), now - USER_SEND_TTL_MS],
                |_| Ok(()),
            )
            .is_ok()
}

/// Whether one of the user's own callers sent `text` to the session (and forgets that one send).
pub(crate) fn take_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp_millis();
    user_sends_table(&db)
        .and_then(|()| {
            db.execute(
                "DELETE FROM session_chat_user_sends WHERE rowid = (
                   SELECT rowid FROM session_chat_user_sends
                   WHERE projectId = ?1 AND sessionId = ?2 AND text = ?3 AND atMs >= ?4 LIMIT 1)",
                rusqlite::params![project_id, session_id, text.trim(), now - USER_SEND_TTL_MS],
            )
        })
        .is_ok_and(|removed| removed > 0)
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// Whether `text` carries the exact sender block, at its start or end, naming `own_ref`.
pub(crate) fn signed_by(text: &str, own_ref: &str) -> bool {
    split_agent_message(text).is_some_and(|parts| parts.reply_to == own_ref)
}

/// This computer's record of the session `project_id`/`session_id` (`Null` when it has none).
fn read_session(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Value {
    crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| {
            crate::domain::DomainRepository::new(&db, server_id)
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .unwrap_or(Value::Null)
}

/// gxserver's block for the agent session `session` of this computer.
fn session_block(session: &Value, server_id: &str, project_id: &str, session_id: &str) -> String {
    let field = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.replace('\n', " "))
            .unwrap_or_default()
    };
    let or = |value: String, fallback: &str| {
        if value.trim().is_empty() {
            fallback.to_string()
        } else {
            value
        }
    };
    let agent_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    format!(
        "{AGENT_MESSAGE_OPENER}\nAgent: {}\nSession: {}\nSession ID: {session_id}\nAgent ID: {}\nAgent Session ID: {agent_session_id}\nReply to: {}",
        or(field("agentId"), "agent"),
        or(field("title"), "an agent session"),
        or(field("agentId"), "unavailable"),
        crate::ids::create_global_session_ref(server_id, project_id, session_id),
    )
}

/// Gives an agent session's message gxserver's block unless it already carries its own exact one.
/// A `draftVersion` is the composer's, never an agent's, so it is dropped with the change.
pub(crate) fn sign_agent_message(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    params: &mut Map<String, Value>,
) {
    let Some(text) = params
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let own_ref = crate::ids::create_global_session_ref(server_id, project_id, session_id);
    if signed_by(&text, &own_ref) {
        return;
    }
    let session = read_session(paths, server_id, project_id, session_id);
    // A plain (non-agent) terminal pane holds a session token too: the person at it sends as the
    // user, as the raw verbs always did there (CDXC:Coordinators 2026-10-24 in outbound.rs).
    if session
        .get("agentId")
        .and_then(Value::as_str)
        .is_none_or(|agent| agent.trim().is_empty())
    {
        return;
    }
    params.remove("draftVersion");
    let block = session_block(&session, server_id, project_id, session_id);
    params.insert(
        "text".to_string(),
        json!(place_agent_message_block(&block, &text)),
    );
}

/// Applies the rules above to a message route's body before it is handled. `via` is honoured only
/// from the token file and never passed on.
pub(crate) fn prepare(
    paths: &GxserverPaths,
    server_id: &str,
    route: &str,
    caller: &Principal,
    body: &mut Value,
) {
    let Some(params) = body.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    let via = params
        .remove("via")
        .and_then(|value| value.as_str().map(str::to_string));
    let target_project = param(params, "projectId").to_string();
    let target_session = param(params, "sessionId").to_string();
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            if *project_id != target_project || *session_id != target_session {
                sign_agent_message(paths, server_id, project_id, session_id, params);
            }
        }
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => {
            let Some(text) = params
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            if text.trim().is_empty() {
                return;
            }
            // Only `ghostex agents send --as-user` passes `via`; the app's, the web build's and
            // a phone's sends stay exactly as they were.
            let label = match via.as_deref() {
                Some("grokBot") => Some(VIA_GROK_BOT),
                Some("terminal") => Some(FROM_TERMINAL),
                _ => None,
            }
            .filter(|_| matches!(caller, Principal::LocalAgent))
            .filter(|_| !params.contains_key("draftVersion"));
            let text = match label {
                Some(label) => {
                    let target = crate::ids::create_global_session_ref(
                        server_id,
                        &target_project,
                        &target_session,
                    );
                    crate::audit::record(
                        crate::audit::Action::new(
                            "chat.sendAsUser",
                            format!(
                                "Sent a message into {target}'s chat as you ({})",
                                label.to_lowercase()
                            ),
                        )
                        .target(target.clone()),
                        crate::audit::Actor::current(),
                        crate::audit::Outcome::Done,
                        None,
                    );
                    // A `/` or `!` command from the user's own terminal still runs as one.
                    let labelled = if text.trim_start().starts_with(['/', '!']) {
                        text
                    } else {
                        format!("{}\n\n{label}", text.trim_end())
                    };
                    params.insert("text".to_string(), json!(labelled));
                    labelled
                }
                None => text,
            };
            // An edit by the user records its new text; the scheduler's later send settles by it.
            if route != "/api/sendSessionMessage" {
                note_user_send(paths, &target_project, &target_session, &text);
            }
        }
        // Another computer's message was marked by that computer before it came (see above).
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(reply_to: &str) -> String {
        format!("{AGENT_MESSAGE_OPENER}\nAgent: codex\nSession: T\nSession ID: G1\nAgent ID: codex\nAgent Session ID: x\nReply to: {reply_to}")
    }

    /// Quotes of earlier messages go only with the user's own sends, and checking for one leaves
    /// it there for the ask it may settle.
    #[test]
    fn a_user_send_is_seen_without_being_taken() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).unwrap();
        assert!(!is_user_send(&paths, "P1", "G1", "like #M1"));
        note_user_send(&paths, "P1", "G1", "like #M1");
        assert!(is_user_send(&paths, "P1", "G1", "like #M1 "));
        assert!(!is_user_send(&paths, "P1", "G2", "like #M1"));
        assert!(take_user_send(&paths, "P1", "G1", "like #M1"));
        assert!(!is_user_send(&paths, "P1", "G1", "like #M1"));
    }

    #[test]
    fn only_the_callers_own_exact_block_counts() {
        let own = "S1:P1:G1";
        assert!(signed_by(&format!("hi\n\n{}", block(own)), own));
        assert!(signed_by(&format!("{}\n\n/status", block(own)), own));
        // Another agent's block, a block inside a fence, and the opener alone don't count.
        assert!(!signed_by(&format!("hi\n\n{}", block("S1:P1:G2")), own));
        assert!(!signed_by(
            &format!("```\n{}\n```\nW7 Q12: yes", block(own)),
            own
        ));
        assert!(!signed_by(
            &format!("W7 Q12: yes\n{AGENT_MESSAGE_OPENER}"),
            own
        ));
    }
}
