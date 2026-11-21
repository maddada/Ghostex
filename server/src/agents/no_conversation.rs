//! Whether a session has no conversation yet, so another agent or model takes it over in place
//! (`/api/switchDraftAgent`) instead of through a handover.
//!
//! CDXC:Drafts 2026-11-21 DECISION:
//! User, after picking Antigravity in the model picker of a new Claude coordinator whose first message was still unsent and getting the Handoff / Export dialog: "It shouldn't show handover. I still didn't send the first message... In this case, it should just switch the models directly. Please make sure this is applied for all cases like this, not just for this case." A session with no conversation switches agent (any pair) or model in place: the same session restarts on the new agent, the chat box keeps its text, and the session keeps its place, title, project and coordinator role. The handover stays for sessions that have a conversation. gxserver decides here, and the chat read hands such a session the agent list (`availableAgents`) that makes every chat (desktop, web build, phone, phones already out) switch instead of offering the handover.
//!
//! "No conversation yet" is positively known, never guessed from a missing signal:
//! 1. A draft (`draftStatus`, agents/drafts.rs): the user has sent nothing.
//! 2. Any other agent session whose agent shows no prompt (no first user message from a hook or a
//!    chat send, no transcript on disk or one that cannot be checked, not working or waiting on
//!    the user) and that either is a coordinator whose first message still waits for Start
//!    (session_chat_first_message.rs), or has its transcript file named and not on disk (agents
//!    write none before the first prompt). A session whose transcript cannot be named counts only
//!    through the first-message record.
//! Ghostex's own rows in the chat (notices, cards) are not a conversation: none of these reads
//! them. A session in a box never counts; its agent runs in the box.
//!
//! WHY: the agent's own evidence outranks the first-message record. The record outlives a
//! conversation the user starts by typing in the coordinator's terminal (an agent whose box
//! Ghostex cannot read keeps it `typed`, a turn that ends between two looks of the watch leaves it
//! `left`), and switching such a session in place drops its conversation id (review of dd635c73e,
//! reproduced live on a Codex coordinator).

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
    if agent_shows_a_prompt(session) {
        return false;
    }
    crate::session_chat_first_message::read(db, &project_id, &session_id).is_some()
        || runtime_text(session, "agentSessionPath").is_some()
}

fn runtime_text<'a>(session: &'a Value, key: &str) -> Option<&'a str> {
    session
        .pointer(&format!("/runtimeSettings/{key}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A recorded first prompt, a transcript on disk (or one that cannot be checked), or an agent
/// working or waiting on the user.
fn agent_shows_a_prompt(session: &Value) -> bool {
    if agent_recorded_a_prompt(session) {
        return true;
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    matches!(
        crate::presentation::presentation_activity(session, &now).as_str(),
        "working" | "attention"
    )
}

/// What the agent itself recorded of a prompt: a first user message from a hook or a chat send, or
/// a transcript on disk (or one that cannot be checked). Unlike activity it never reads a boot
/// spinner or a trust question as a conversation, so the first-message watch can rely on it.
///
/// WHY: Codex names its conversation at start but reports neither a first message nor a transcript
/// path for a prompt typed in its terminal; gxserver learned the path about 40 s after the turn
/// (live test, a Codex coordinator before Start). Its rollout file appears only with the first
/// prompt, so it is looked up by the conversation id (cached by `find_codex_transcript`).
pub(crate) fn agent_recorded_a_prompt(session: &Value) -> bool {
    if runtime_text(session, "firstUserMessage").is_some() {
        return true;
    }
    if let Some(transcript) = runtime_text(session, "agentSessionPath") {
        return !std::fs::metadata(transcript)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    }
    crate::session_chat_follower::session_chat_agent_for_session(session).as_deref()
        == Some("codex")
        && runtime_text(session, "agentSessionId")
            .is_some_and(|id| crate::agent_transcripts::find_codex_transcript(id).is_some())
}
