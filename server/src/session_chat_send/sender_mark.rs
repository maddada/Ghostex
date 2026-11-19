//! Who a chat send is from, decided by gxserver from the caller rather than from the text.
//!
//! CDXC:Security 2026-11-19 DECISION (Fable security review of 38bfa3647):
//! Only the user's own callers (the desktop app, a paired phone, the token file of the user's
//! terminal) may put unmarked text into another session's chat, where it reads as the user's own.
//! - A session token (an agent) or another computer that sends text without the "Message from
//!   another agent" block gets the block from gxserver, so an agent that dropped its CLI's
//!   identity (`env -u`, plain HTTP with its token) still can't speak as the user. A `/` or `!`
//!   body gets it first, so it never runs as a command.
//! - Unmarked text from the token file is labelled "Sent from the terminal" (or "Sent via Grok
//!   Bot", which `ghostex agents send --as-user` passes when the Grok Bot desktop app ran it) and
//!   recorded in the security log (`chat.sendAsUser`).
//! - A "W7 Q12: …" answer settles a coordinator's ask only when one of the user's own callers sent
//!   that text ([`note_user_send`], [`take_user_send`]), never because the agent block is missing.
//! SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `sender` (`--as-user`),
//! server/src/session_chat_queue_runtime/send.rs `settle_work_replies`.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use super::AppState;
use crate::coordinators::{place_agent_message_block, AGENT_MESSAGE_OPENER};
use crate::security::Principal;

/// The label under a message the user sent from their own terminal.
pub(crate) const FROM_TERMINAL: &str = "Sent from the terminal";
/// The label under a message the user had Grok Bot send from this computer.
pub(crate) const VIA_GROK_BOT: &str = "Sent via Grok Bot";

/// How long a user's send stays recognised for settling answers: long enough for a queued prompt
/// to go out after the turn it waits behind.
const USER_SEND_TTL_MS: i64 = 24 * 60 * 60 * 1000;
const USER_SEND_LIMIT: usize = 500;

struct UserSend {
    project_id: String,
    session_id: String,
    text: String,
    at_ms: i64,
}

fn user_sends() -> &'static Mutex<VecDeque<UserSend>> {
    static SENDS: OnceLock<Mutex<VecDeque<UserSend>>> = OnceLock::new();
    SENDS.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Remembers that one of the user's own callers sent `text` to the session.
pub(crate) fn note_user_send(project_id: &str, session_id: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut sends = user_sends().lock().unwrap_or_else(|p| p.into_inner());
    sends.retain(|send| now - send.at_ms < USER_SEND_TTL_MS);
    if sends.len() >= USER_SEND_LIMIT {
        sends.pop_front();
    }
    sends.push_back(UserSend {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        text: text.trim().to_string(),
        at_ms: now,
    });
}

/// Whether one of the user's own callers sent `text` to the session (and forgets it).
pub(crate) fn take_user_send(project_id: &str, session_id: &str, text: &str) -> bool {
    let now = chrono::Utc::now().timestamp_millis();
    let mut sends = user_sends().lock().unwrap_or_else(|p| p.into_inner());
    sends.retain(|send| now - send.at_ms < USER_SEND_TTL_MS);
    let text = text.trim();
    match sends.iter().position(|send| {
        send.project_id == project_id && send.session_id == session_id && send.text == text
    }) {
        Some(index) => {
            sends.remove(index);
            true
        }
        None => false,
    }
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

fn marked(text: &str) -> bool {
    text.lines().any(|line| line.trim() == AGENT_MESSAGE_OPENER)
}

/// The block for an agent session that sent without one: what gxserver knows of it.
fn session_block(state: &AppState, project_id: &str, session_id: &str) -> String {
    let mut lookup = Map::new();
    lookup.insert("projectId".to_string(), json!(project_id));
    lookup.insert("sessionId".to_string(), json!(session_id));
    let session = super::resolve_session_chat_send_target(state, &lookup, "sender")
        .map(|target| target.session)
        .unwrap_or(Value::Null);
    let field = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
    let agent_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    format!(
        "{AGENT_MESSAGE_OPENER}\nAgent: {}\nSession: {}\nSession ID: {session_id}\nAgent ID: {}\nAgent Session ID: {agent_session_id}\nReply to: {}",
        if field("agentId").is_empty() { "agent" } else { field("agentId") },
        if field("title").is_empty() { "an agent session" } else { field("title") },
        field("agentId"),
        crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            project_id,
            session_id
        ),
    )
}

/// Applies the rules above to a `/api/sendSessionChatMessage` or `/api/queueSessionChatPrompt`
/// body before it is handled. `via` is honoured only from the token file and never passed on.
pub(crate) fn prepare(state: &AppState, caller: &Principal, body: &mut Value) {
    let Some(params) = body.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    let via = params
        .remove("via")
        .and_then(|value| value.as_str().map(str::to_string));
    let target_project = param(params, "projectId").to_string();
    let target_session = param(params, "sessionId").to_string();
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
    let text = match caller {
        Principal::Owner { .. } | Principal::Device { .. } => text,
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            let own = *project_id == target_project && *session_id == target_session;
            if own || marked(&text) {
                return;
            }
            place_agent_message_block(&session_block(state, project_id, session_id), &text)
        }
        Principal::Peer { name, server_id } => {
            if marked(&text) {
                return;
            }
            let block = format!(
                "{AGENT_MESSAGE_OPENER}\nAgent: an agent on {name}\nSession: another computer\nSession ID: unavailable\nAgent ID: unavailable\nAgent Session ID: unavailable\nReply to: {server_id}"
            );
            place_agent_message_block(&block, &text)
        }
        Principal::LocalAgent => {
            if marked(&text) {
                return;
            }
            let label = if via.as_deref() == Some("grokBot") {
                VIA_GROK_BOT
            } else {
                FROM_TERMINAL
            };
            let target = crate::ids::create_global_session_ref(
                state.metadata.server_id.as_str(),
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
            // A `/` or `!` command from the user's own terminal must still run as one.
            if text.trim_start().starts_with(['/', '!']) {
                text
            } else {
                format!("{}\n\n{label}", text.trim_end())
            }
        }
        _ => return,
    };
    if matches!(
        caller,
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
    ) {
        note_user_send(&target_project, &target_session, &text);
    }
    params.insert("text".to_string(), json!(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_send_is_taken_once_and_only_for_its_session() {
        note_user_send("P1", "G1", "W7 Q12: yes");
        assert!(!take_user_send("P1", "G2", "W7 Q12: yes"));
        assert!(take_user_send("P1", "G1", " W7 Q12: yes "));
        assert!(!take_user_send("P1", "G1", "W7 Q12: yes"));
    }

    #[test]
    fn the_agent_opener_marks_a_message() {
        assert!(marked("hi\n\nMessage from another agent\nAgent: x"));
        assert!(!marked("W7 Q12: yes"));
    }
}
