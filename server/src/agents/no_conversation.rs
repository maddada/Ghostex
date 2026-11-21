//! Whether a session has no conversation yet, so another agent or model takes it over in place
//! (`/api/switchDraftAgent`) instead of through a handover.
//!
//! CDXC:Drafts 2026-11-21 DECISION:
//! User, after picking Antigravity in the model picker of a new Claude coordinator whose first message was still unsent and getting the Handoff / Export dialog: "It shouldn't show handover. I still didn't send the first message... In this case, it should just switch the models directly. Please make sure this is applied for all cases like this, not just for this case." A session with no conversation switches agent (any pair) or model in place: the same session restarts on the new agent, the chat box keeps its text, and the session keeps its place, title, project and coordinator role. The handover stays for sessions that have a conversation. gxserver decides here, and the chat read hands such a session the agent list (`availableAgents`) that makes every chat (desktop, web build, phone, phones already out) switch instead of offering the handover.
//!
//! "No conversation yet" is positively known, never guessed from a missing signal:
//! 1. A draft (`draftStatus`, agents/drafts.rs): the user has sent nothing.
//! 2. A coordinator whose first message still waits for Start (session_chat_first_message.rs):
//!    the record is gone the moment it is sent, from the chat or from the terminal.
//! 3. Any other agent session the agent itself has not recorded a prompt for: no first user
//!    message from a hook or a chat send, its transcript file named and not on disk (agents write
//!    none before the first prompt), and not working now. A session whose transcript cannot be
//!    named does not count.
//! Ghostex's own rows in the chat (notices, cards) are not a conversation: none of the three reads
//! them. A session in a box never counts; its agent runs in the box.

use rusqlite::Connection;
use serde_json::Value;

use super::{read_text_value, session_is_draft};

pub(crate) fn session_has_no_conversation(db: &Connection, session: &Value) -> bool {
    if session_is_draft(session) {
        return true;
    }
    if session.get("kind").and_then(Value::as_str) != Some("agent")
        || crate::agentbox::is_agentbox_session(session)
    {
        return false;
    }
    let (Some(project_id), Some(session_id)) = (
        read_text_value(session, "projectId"),
        read_text_value(session, "sessionId"),
    ) else {
        return false;
    };
    crate::session_chat_first_message::read(db, &project_id, &session_id).is_some()
        || agent_recorded_no_prompt(session)
}

fn agent_recorded_no_prompt(session: &Value) -> bool {
    let runtime = |key: &str| {
        session
            .pointer(&format!("/runtimeSettings/{key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    if runtime("firstUserMessage").is_some() {
        return false;
    }
    let Some(transcript) = runtime("agentSessionPath") else {
        return false;
    };
    let transcript_absent = std::fs::metadata(transcript)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    transcript_absent
        && !matches!(
            crate::presentation::presentation_activity(session, &now).as_str(),
            "working" | "attention"
        )
}
