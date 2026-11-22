//! An app's message in its GBot chat, and the GBot's reply to it. Each message carries a marker
//! (its message id) on its first line, so the reply is read from the chat's own transcript: the
//! GBot's text after the message that carries the marker, once that turn has ended.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::coordinators::{classify_thread_session, ThreadProgress, ThreadState};
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::read_runtime_text;
use crate::session_chat::{SessionChatBlock, SessionChatRole, SessionChatTailPage};
use crate::session_chat_follower::session_chat_agent_for_session;

/// Transcript messages read back when looking for the marker (a reply with many tool calls is
/// many messages).
const TAIL_SCAN_LIMIT: usize = 400;
/// The most of a reply an app gets back (the full text stays in the chat).
pub(crate) const REPLY_MAX_CHARS: usize = 48_000;

/// One message an app sent (`gbots_api_messages`).
#[derive(Clone, Debug)]
pub(crate) struct ApiMessage {
    pub message_id: String,
    pub key_id: String,
    pub bot_id: String,
    pub project_id: String,
    pub session_id: String,
    pub created_at: String,
    pub reply: Option<String>,
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

pub(crate) fn insert_message(
    db: &Connection,
    message: &ApiMessage,
) -> Result<(), DomainStateError> {
    db.execute(
        "INSERT INTO gbots_api_messages (messageId, keyId, botId, projectId, sessionId, marker, kind, createdAt) VALUES (?1, ?2, ?3, ?4, ?5, ?1, 'message', ?6)",
        params![
            message.message_id,
            message.key_id,
            message.bot_id,
            message.project_id,
            message.session_id,
            message.created_at
        ],
    )
    .map_err(sql)?;
    Ok(())
}

pub(crate) fn read_message(
    db: &Connection,
    message_id: &str,
) -> Result<Option<ApiMessage>, DomainStateError> {
    db.query_row(
        "SELECT messageId, keyId, botId, projectId, sessionId, createdAt, reply FROM gbots_api_messages WHERE messageId = ?1",
        [message_id],
        |row| {
            Ok(ApiMessage {
                message_id: row.get(0)?,
                key_id: row.get(1)?,
                bot_id: row.get(2)?,
                project_id: row.get(3)?,
                session_id: row.get(4)?,
                created_at: row.get(5)?,
                reply: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(sql)
}

/// The newest message sent with `key_id` into the chat, if any.
pub(crate) fn newest_message_in(
    db: &Connection,
    key_id: &str,
    project_id: &str,
    session_id: &str,
) -> Result<Option<ApiMessage>, DomainStateError> {
    let id: Option<String> = db
        .query_row(
            "SELECT messageId FROM gbots_api_messages WHERE keyId = ?1 AND projectId = ?2 AND sessionId = ?3 ORDER BY createdAt DESC LIMIT 1",
            params![key_id, project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql)?;
    match id {
        Some(id) => read_message(db, &id),
        None => Ok(None),
    }
}

fn save_reply(db: &Connection, message_id: &str, reply: &str) {
    let _ = db.execute(
        "UPDATE gbots_api_messages SET reply = ?2, repliedAt = ?3 WHERE messageId = ?1",
        params![message_id, reply, crate::coordinators::now_iso()],
    );
}

/// Where an app's message stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReplyState {
    /// Queued, or the GBot is still on it.
    Working,
    /// The chat waits on an approval or a question, which only the user answers.
    NeedsApproval(String),
    /// The GBot's reply.
    Replied(String),
    /// The chat was closed before it answered.
    Gone,
}

/// The text of a transcript message's text blocks.
fn text_of(message: &crate::session_chat::SessionChatMessage) -> String {
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
}

/// The GBot's final message after the message carrying `marker` (its last text before the next
/// message from anyone else; what it said along the way stays in the chat); `None` when the
/// marker is not in the transcript yet (still queued).
fn text_after_marker(session: &Value, marker: &str) -> Option<String> {
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(
        session_chat_agent_for_session(session).as_deref(),
    )?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )?;
    let SessionChatTailPage::Page { messages, .. } =
        crate::session_chat::read_session_chat_tail_page(agent, &path, TAIL_SCAN_LIMIT, None)
            .ok()?
    else {
        return None;
    };
    let messages: Vec<_> = messages
        .into_iter()
        .filter(|message| !message.queued)
        .collect();
    let start = messages.iter().rposition(|message| {
        message.role == SessionChatRole::User && text_of(message).contains(marker)
    })?;
    let mut last = String::new();
    for message in &messages[start + 1..] {
        if message.role == SessionChatRole::User && !text_of(message).is_empty() {
            break;
        }
        if message.role == SessionChatRole::Assistant {
            let text = text_of(message);
            if !text.is_empty() {
                last = text;
            }
        }
    }
    Some(last)
}

/// Where `message` stands now (BLOCKING: reads the chat's transcript). A reply is kept in the row,
/// so asking again later reads it from there.
pub(crate) fn reply_state(
    db: &Connection,
    repository: &DomainRepository<'_>,
    message: &ApiMessage,
) -> ReplyState {
    if let Some(reply) = &message.reply {
        return ReplyState::Replied(reply.clone());
    }
    let Ok(Some(session)) = repository.get_session(&message.project_id, &message.session_id) else {
        return ReplyState::Gone;
    };
    let now = crate::presentation::now_iso();
    let progress = ThreadProgress {
        resolved: false,
        has_run: true,
    };
    let state = classify_thread_session(Some(&session), progress, &now, false);
    if state == ThreadState::Closed {
        return ReplyState::Gone;
    }
    let after = text_after_marker(&session, &message.message_id);
    if state == ThreadState::Waiting {
        let summary = crate::coordinators::waiting_prompt(&session)
            .map(|prompt| prompt.summary)
            .unwrap_or_else(|| "a prompt on its screen".to_string());
        return ReplyState::NeedsApproval(summary);
    }
    match after {
        Some(text)
            if !text.is_empty()
                && matches!(state, ThreadState::Finished | ThreadState::Sleeping) =>
        {
            let reply: String = text.chars().take(REPLY_MAX_CHARS).collect();
            save_reply(db, &message.message_id, &reply);
            ReplyState::Replied(reply)
        }
        _ => ReplyState::Working,
    }
}
