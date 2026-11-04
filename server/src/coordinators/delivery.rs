//! Whether a message a coordinator handed to one of its threads reached it: the text helpers the
//! CLI and the supervisor share, and the supervisor's transcript check.

use std::path::PathBuf;

use serde_json::Value;

use crate::session_chat::{SessionChatBlock, SessionChatRole, SessionChatSource};
use crate::session_chat_tail::SessionChatTailPage;

/// The start of a pending message kept on the thread: enough for the needle and for the excerpt
/// an "undelivered" report quotes.
pub const COORDINATOR_PENDING_MESSAGE_MAX_CHARS: usize = 300;
/// A thread's transcript is written on gxserver's own machine, so only the gap between taking
/// the send time and the agent writing its row needs covering.
const DELIVERY_CLOCK_SLACK_MS: i64 = 5_000;
/// Transcript rows read per page, and the most pages one check reads back to the send.
const DELIVERY_PAGE_ROWS: usize = 100;
const DELIVERY_MAX_PAGES: usize = 20;

pub use ghostex_gx_protocol::agent_message_delivery::{delivery_needles, normalize_delivery_text};

/// Whether normalized transcript text carries every needle of a message.
pub fn holds_message(normalized_text: &str, needles: &[String]) -> bool {
    !needles.is_empty()
        && needles
            .iter()
            .all(|needle| normalized_text.contains(needle.as_str()))
}

/// The start of a message as the thread record keeps it: one line, bounded.
pub fn pending_message_excerpt(body: &str) -> String {
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(COORDINATOR_PENDING_MESSAGE_MAX_CHARS)
        .collect()
}

/// BLOCKING (transcript path resolution and a bounded tail read).
/// `Some(true)` when the thread's transcript records a user turn sent at or after `since_ms` that
/// carries `needles`, `Some(false)` when it was read and does not, `None` when there is no
/// transcript to read yet. `path` caches the resolved file between checks.
pub fn transcript_records_message(
    session: &Value,
    needles: &[String],
    since_ms: i64,
    path: &mut Option<PathBuf>,
) -> Option<bool> {
    if needles.is_empty() {
        return None;
    }
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session)?;
    let transcript_agent =
        crate::session_chat::resolve_session_chat_transcript_agent(Some(agent.as_str()))?;
    if !path.as_ref().is_some_and(|path| path.is_file()) {
        let runtime = |key: &str| {
            session
                .pointer(&format!("/runtimeSettings/{key}"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        *path = crate::session_chat_paths::resolve_session_chat_transcript_path(
            transcript_agent,
            runtime("agentSessionId").as_deref(),
            runtime("agentSessionPath").as_deref(),
        );
    }
    let path = path.as_ref()?;
    let earliest = since_ms - DELIVERY_CLOCK_SLACK_MS;
    let mut before_offset = None;
    // CDXC:Coordinators 2026-10-04 WHY: a thread that took a follow-up and then worked a long turn has dozens of rows after it; reading only the newest rows reported a delivered message as lost, so the check pages back until it reaches rows older than the send.
    for _ in 0..DELIVERY_MAX_PAGES {
        let page = crate::session_chat_tail::read_session_chat_tail_page(
            transcript_agent,
            path,
            DELIVERY_PAGE_ROWS,
            before_offset,
        )
        .ok()?;
        let SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset: older,
            ..
        } = page
        else {
            return None;
        };
        if messages
            .iter()
            .any(|message| records_message(message, needles, earliest))
        {
            return Some(true);
        }
        let reached_send = messages
            .first()
            .and_then(|message| message.timestamp)
            .is_some_and(|timestamp| timestamp < earliest);
        if reached_send || !has_more || before_offset == Some(older) {
            return Some(false);
        }
        before_offset = Some(older);
    }
    Some(false)
}

fn records_message(
    message: &crate::session_chat::SessionChatMessage,
    needles: &[String],
    earliest: i64,
) -> bool {
    message.role == SessionChatRole::User
        && message.source == SessionChatSource::Transcript
        && message
            .timestamp
            .is_none_or(|timestamp| timestamp >= earliest)
        && holds_message(
            &message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    SessionChatBlock::Text { text } => Some(normalize_delivery_text(text)),
                    _ => None,
                })
                .collect::<String>(),
            needles,
        )
}
