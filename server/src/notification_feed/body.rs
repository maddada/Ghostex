use serde_json::Value;

use crate::presentation::read_runtime_text;
use crate::session_chat::{SessionChatBlock, SessionChatRole, SessionChatTailPage};
use crate::session_chat_follower::session_chat_agent_for_session;

/// Messages scanned from the transcript tail when looking for the newest assistant text.
const NOTIFICATION_BODY_SCAN_LIMIT: usize = 60;

/// The text of the agent's newest assistant message, so a feed row says what the agent said instead of only that it stopped.
/// Reads the provider transcript tail the same way the chat prompt scanner does; every miss (no agent, no transcript, no assistant text in the window) returns None and the caller falls back to a kind label.
pub(crate) fn last_assistant_message_text(session: &Value) -> Option<String> {
    // A Claude cloud session has no transcript here; its latest status line stands in.
    if let Some(text) = crate::claude_cloud::notification_text(session) {
        return Some(text);
    }
    last_assistant_message(session).map(|(text, _)| text)
}

/// [`last_assistant_message_text`] with the message's transcript timestamp in milliseconds.
pub(crate) fn last_assistant_message(session: &Value) -> Option<(String, Option<i64>)> {
    last_assistant_message_joined(session, " ")
}

/// The agent's newest assistant message as it wrote it: its text blocks one paragraph apart, so
/// its lines keep the Markdown structure the marked sections are read from.
pub(crate) fn last_assistant_markdown(session: &Value) -> Option<String> {
    last_assistant_message_joined(session, "\n\n").map(|(text, _)| text)
}

fn last_assistant_message_joined(
    session: &Value,
    separator: &str,
) -> Option<(String, Option<i64>)> {
    let transcript_agent = crate::session_chat::resolve_session_chat_transcript_agent(
        session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    let SessionChatTailPage::Page { messages, .. } =
        crate::session_chat::read_session_chat_tail_page(
            transcript_agent,
            &path,
            NOTIFICATION_BODY_SCAN_LIMIT,
            None,
        )
        .ok()?
    else {
        return None;
    };
    messages
        .iter()
        .rev()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .find_map(|message| {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    SessionChatBlock::Text { text } => Some(text.trim()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(separator);
            (!text.trim().is_empty()).then_some((text, message.timestamp))
        })
}
