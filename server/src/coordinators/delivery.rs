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

/// The pending message a thread keeps for a message with no text (images only): what an
/// "undelivered" report quotes. It is never matched as text (`pending_message_needles`).
///
/// CDXC:Coordinators 2026-11-13 DECISION:
/// GitHub #554 ("An image-only message to a done thread gets no delivery watch"): the watch found
/// a message by its text, so a message of only images got none, and a reply to it too fast for a
/// supervisor tick was not reported. The user picked "Build the 2 left now". Agents record such a
/// message differently (Claude writes a user row with an image, Codex writes no user row at all),
/// so it is found by time instead (`transcript_records_images_only_message`): a user row with an
/// image from the send on, or any row the agent itself wrote after the send. The thread was done
/// (idle) when the message reopened it, so a row after the send is that message's turn.
pub const IMAGES_ONLY_PENDING_MESSAGE: &str = "(images only)";

/// The needles of a thread's pending message: none for an images-only one.
pub fn pending_message_needles(excerpt: &str) -> Vec<String> {
    if excerpt == IMAGES_ONLY_PENDING_MESSAGE {
        return Vec::new();
    }
    delivery_needles(excerpt)
}

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

/// A thread's transcript file as `resolve_thread_transcript` found it, kept between calls with the
/// conversation (`agentSessionId`, `agentSessionPath`) it was found for.
///
/// CDXC:LeadLinks 2026-11-26 FIX:
/// GitHub #755 and #713: after a clear, a linked worker never reported to its lead again. The
/// cached file was looked up again only once it was gone, and a cleared conversation's file stays
/// on disk, so the lead's watcher kept reading the old conversation and never found its message
/// in it (`reportOwed` stayed on). The file is now looked up again whenever the session names
/// another conversation, so a link made before the new conversation shows up, or one kept across
/// a clear, follows the session into it.
#[derive(Debug, Clone, Default)]
pub struct ThreadTranscriptPath {
    file: Option<PathBuf>,
    conversation: Option<(Option<String>, Option<String>)>,
}

impl ThreadTranscriptPath {
    /// The transcript file found by the last `resolve_thread_transcript`, when there is one.
    pub fn file(&self) -> Option<&std::path::Path> {
        self.file.as_deref()
    }
}

/// BLOCKING (transcript path resolution). The thread's transcript agent, with `cache` holding its
/// transcript file when one is on disk; the file is looked up again when it is gone or the session
/// names another conversation.
pub fn resolve_thread_transcript(
    session: &Value,
    cache: &mut ThreadTranscriptPath,
) -> Option<crate::session_chat::SessionChatTranscriptAgent> {
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session)?;
    let transcript_agent =
        crate::session_chat::resolve_session_chat_transcript_agent(Some(agent.as_str()))?;
    let runtime = |key: &str| {
        session
            .pointer(&format!("/runtimeSettings/{key}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let conversation = (runtime("agentSessionId"), runtime("agentSessionPath"));
    let current = cache.conversation.as_ref() == Some(&conversation)
        && cache.file.as_ref().is_some_and(|path| path.is_file());
    if !current {
        cache.file = crate::session_chat_paths::resolve_session_chat_transcript_path(
            transcript_agent,
            conversation.0.as_deref(),
            conversation.1.as_deref(),
        );
        cache.conversation = Some(conversation);
    }
    Some(transcript_agent)
}

/// BLOCKING (transcript path resolution and a bounded tail read).
/// `Some(true)` when the thread's transcript records a user turn sent at or after `since_ms` that
/// carries `needles`, `Some(false)` when it was read and does not, `None` when there is no
/// transcript to read yet. `path` caches the resolved file between checks.
#[track_caller]
pub fn transcript_records_message(
    session: &Value,
    needles: &[String],
    since_ms: i64,
    path: &mut ThreadTranscriptPath,
) -> Option<bool> {
    if needles.is_empty() {
        return None;
    }
    let earliest = since_ms - DELIVERY_CLOCK_SLACK_MS;
    transcript_has_row(session, since_ms, path, |message| {
        records_message(message, needles, earliest)
    })
}

/// BLOCKING, as `transcript_records_message`, for a message of only images sent at `since_ms`:
/// `Some(true)` once the transcript has a user row with an image from the send on, or a row the
/// agent wrote after it (see `IMAGES_ONLY_PENDING_MESSAGE`).
#[track_caller]
pub fn transcript_records_images_only_message(
    session: &Value,
    since_ms: i64,
    path: &mut ThreadTranscriptPath,
) -> Option<bool> {
    let earliest = since_ms - DELIVERY_CLOCK_SLACK_MS;
    transcript_has_row(session, since_ms, path, |message| {
        message.source == SessionChatSource::Transcript
            && match message.role {
                SessionChatRole::User => {
                    message
                        .timestamp
                        .is_some_and(|timestamp| timestamp >= earliest)
                        && message
                            .blocks
                            .iter()
                            .any(|block| matches!(block, SessionChatBlock::ImageRef { .. }))
                }
                SessionChatRole::Assistant | SessionChatRole::Reasoning | SessionChatRole::Tool => {
                    message
                        .timestamp
                        .is_some_and(|timestamp| timestamp >= since_ms)
                }
                SessionChatRole::System => false,
            }
    })
}

/// BLOCKING. Whether the thread's transcript has a row `matches` accepts, paging back from the
/// newest row until rows older than the send (`since_ms`, with the clock slack); `None` when there
/// is no transcript to read yet.
#[track_caller]
fn transcript_has_row(
    session: &Value,
    since_ms: i64,
    path: &mut ThreadTranscriptPath,
    matches: impl Fn(&crate::session_chat::SessionChatMessage) -> bool,
) -> Option<bool> {
    let transcript_agent = resolve_thread_transcript(session, path)?;
    let path = path.file()?;
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
        if messages.iter().any(&matches) {
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
