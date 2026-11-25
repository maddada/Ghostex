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

/// The agent's newest reply as it wrote it (its text blocks one paragraph apart, so its lines keep
/// the Markdown structure the marked sections are read from), only while it is the newest text in
/// the transcript:
/// `None` when a prompt written after it (the reply to it not written yet) is newer, so a turn's
/// handover is never read from the turn before it.
pub(crate) fn newest_reply_markdown(session: &Value) -> Option<String> {
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
    let text_of = |message: &crate::session_chat::SessionChatMessage| {
        message
            .blocks
            .iter()
            .filter_map(|block| match block {
                SessionChatBlock::Text { text } => Some(text.trim()),
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let (message, text) = messages
        .iter()
        .rev()
        .filter(|message| !message.queued)
        .map(|message| (message, text_of(message)))
        .find(|(_, text)| !text.is_empty())?;
    (message.role == SessionChatRole::Assistant).then_some(text)
}

/// What a coordinator thread's "finished its turn" report can say, from one read of its
/// transcript tail.
pub(crate) enum FinishedTurnReply {
    /// The newest reply, written after the thread's last report (or with no time to compare).
    New(String),
    /// The newest reply is the one already forwarded, and nothing but system rows came after it:
    /// no turn ended since the last report.
    AlreadyReported,
    /// No reply newer than the last report could be read, though rows after it show a turn ran.
    Unreadable,
}

/// The newest reply for a thread last reported at `reported_ms`.
///
/// CDXC:Coordinators 2026-11-25 FIX:
/// GitHub #748: an OpenCode thread whose final report had reached its coordinator intact kept
/// sending "finished its turn, but its final message could not be read": five over two minutes
/// while leftover shells flipped it working and idle again, one after two hours of quiet when the
/// coordinator's next message was confirmed before its turn showed up, one after plain system
/// rows. Each flip set `observedWorking` again, and the report's lookup then filtered out the reply
/// it had already sent and called the turn unreadable. A reply already forwarded with nothing but
/// system rows after it now says so (`AlreadyReported`), and no report is sent for it; a turn that
/// did run since (a prompt, a tool row or any assistant row newer than the report) and left no
/// reply is still reported as unreadable.
pub(crate) fn finished_turn_reply(session: &Value, reported_ms: Option<i64>) -> FinishedTurnReply {
    let Some(messages) = tail_messages(session) else {
        return FinishedTurnReply::Unreadable;
    };
    let messages: Vec<_> = messages.iter().filter(|message| !message.queued).collect();
    let Some(position) = messages.iter().rposition(|message| {
        message.role == SessionChatRole::Assistant && !reply_text(message, " ").is_empty()
    }) else {
        return FinishedTurnReply::Unreadable;
    };
    let reply = messages[position];
    match (reply.timestamp, reported_ms) {
        (Some(written), Some(reported)) if written <= reported => {
            let nothing_since = messages[position + 1..].iter().all(|message| {
                message.role == SessionChatRole::System
                    || message.timestamp.is_some_and(|at| at <= reported)
            });
            if nothing_since {
                FinishedTurnReply::AlreadyReported
            } else {
                FinishedTurnReply::Unreadable
            }
        }
        _ => FinishedTurnReply::New(reply_text(reply, " ")),
    }
}

fn reply_text(message: &crate::session_chat::SessionChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.trim()),
            _ => None,
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}

fn tail_messages(session: &Value) -> Option<Vec<crate::session_chat::SessionChatMessage>> {
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
    Some(messages)
}

fn last_assistant_message_joined(
    session: &Value,
    separator: &str,
) -> Option<(String, Option<i64>)> {
    let messages = tail_messages(session)?;
    messages
        .iter()
        .rev()
        .filter(|message| message.role == SessionChatRole::Assistant && !message.queued)
        .find_map(|message| {
            let text = reply_text(message, separator);
            (!text.trim().is_empty()).then_some((text, message.timestamp))
        })
}
