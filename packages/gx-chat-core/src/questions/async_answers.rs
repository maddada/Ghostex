//! Which questions a working agent is still waiting on, and how an answer to one is framed.
//!
//! Port of `packages/shared/session-chat-presentation/async-questions.ts`. These are the
//! questions Codex asks without stopping: they arrive on an assistant row and are retired by the
//! user turn that answers them, not by any tool result.

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatRole, ChatSource};

pub use crate::document::PendingAsyncQuestion;

/// How many UTF-8 bytes of the question title an answer quotes back.
const ANSWER_PREFIX_BYTE_LIMIT: usize = 512;

/// Matches Codex's `AnsweredQuestion` framing, including its 512-byte UTF-8 bound.
pub fn async_answer_prefix(title: &str) -> String {
    let mut bounded = String::new();
    let mut bytes = 0usize;
    for character in title.chars() {
        bytes += character.len_utf8();
        if bytes > ANSWER_PREFIX_BYTE_LIMIT {
            break;
        }
        bounded.push(character);
    }
    let flattened: String = bounded
        .chars()
        .map(|character| {
            if character == '\r' || character == '\n' {
                ' '
            } else {
                character
            }
        })
        .collect();
    format!("> {flattened}\n\n")
}

/// The questions still open, oldest first.
///
/// Only an answer to that question retires it; unrelated messages and tool results do not.
pub fn pending_async_questions(messages: &[ChatRow]) -> Vec<PendingAsyncQuestion> {
    let mut pending: Vec<PendingAsyncQuestion> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for message in messages {
        if message.role == ChatRole::Assistant {
            for (index, question) in message.async_questions.iter().flatten().enumerate() {
                let key = format!("{}:{index}", message.id);
                if seen.iter().any(|entry| entry == &key) {
                    continue;
                }
                seen.push(key.clone());
                pending.push(PendingAsyncQuestion {
                    title: question.title.clone(),
                    options: question.options.clone(),
                    key,
                });
            }
        } else if message.role == ChatRole::User
            && message.source == ChatSource::Transcript
            // With no question waiting, a user row has nothing to answer, and its text is not
            // read: this runs on every document, over the whole transcript.
            && !pending.is_empty()
        {
            let text = message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ChatBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut matching = pending
                .iter()
                .enumerate()
                .filter(|(_, question)| text.starts_with(&async_answer_prefix(&question.title)));
            // CDXC:SessionChat 2026-10-04 WHY:
            // Match before filtering retired IDs. Equal prefixes are ambiguous; the confirmed card submission retires its exact key separately, including after a transcript refresh.
            if let Some((at, _)) = matching.next() {
                if matching.next().is_none() {
                    pending.remove(at);
                }
            }
        }
    }
    pending
}
