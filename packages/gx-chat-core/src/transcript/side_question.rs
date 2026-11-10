//! Claude's `/btw` side questions as transcript rows.
//!
//! gxserver archives a `/btw` like any slash command the user sent, with the side answer as its
//! output (server/src/session_chat_app_command.rs, `attach_side_answer`). The chat shows the pair as
//! one folded row instead of a "Slash command" marker and a "Local command output" marker.

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::foreign::{decode_escaped_markup, parse_command_envelope};
use crate::transcript::line_breaks::{agent_line_breaks, AgentLineBreaks};
use crate::transcript::native_markdown::native_markdown;

pub const SIDE_QUESTION_ID_PREFIX: &str = "side-question:";
pub const SIDE_QUESTION_LABEL: &str = "Side question";

const STDOUT_OPEN: &str = "<local-command-stdout";
const STDOUT_CLOSE: &str = "</local-command-stdout>";

fn text_of(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The question a `/btw` command row asked, or `None` for any other row.
fn side_question(message: &ChatMessage) -> Option<String> {
    if message.role != ChatRole::User {
        return None;
    }
    let envelope = parse_command_envelope(&text_of(message))?;
    (envelope.name.eq_ignore_ascii_case("/btw") && !envelope.args.trim().is_empty())
        .then(|| envelope.args.trim().to_string())
}

/// Whether a row is a `/btw` command row asking a question.
pub fn is_side_question_command(message: &ChatMessage) -> bool {
    side_question(message).is_some()
}

/// The body of an output row's `<local-command-stdout>` marker.
fn stdout_body(message: &ChatMessage) -> Option<String> {
    let text = text_of(message);
    let start = text.find(STDOUT_OPEN)?;
    let open_end = text[start..].find('>')? + start;
    let attributes = &text[start + STDOUT_OPEN.len()..open_end];
    let end = text.rfind(STDOUT_CLOSE)?;
    let body = text.get(open_end + 1..end)?.trim();
    Some(if attributes.contains("data-ghostex-escaped") {
        decode_escaped_markup(body)
    } else {
        body.to_string()
    })
}

/// Folds every answered `/btw` command row and its output row into one side-question row, placed
/// where the question was asked. The folded row keeps the command row's time and source; its
/// blocks are the question and the answer.
pub fn fold_side_questions(messages: &[ChatRow]) -> Vec<ChatRow> {
    let questions: Vec<(String, String)> = messages
        .iter()
        .filter_map(|message| side_question(message).map(|question| (message.id.clone(), question)))
        .collect();
    if questions.is_empty() {
        return messages.to_vec();
    }
    let output_ids: Vec<String> = questions
        .iter()
        .map(|(id, _)| format!("{id}:output"))
        .collect();
    let mut folded = Vec::with_capacity(messages.len());
    for message in messages {
        if output_ids.contains(&message.id) {
            continue;
        }
        let Some((_, question)) = questions.iter().find(|(id, _)| *id == message.id) else {
            folded.push(message.clone());
            continue;
        };
        let answer = messages
            .iter()
            .find(|row| row.id == format!("{}:output", message.id))
            .and_then(|message| stdout_body(message))
            .filter(|answer| !answer.is_empty());
        // Until gxserver has read the answer, the card above the composer is the side question;
        // a question closed before Claude answered leaves nothing to keep.
        let Some(answer) = answer else {
            continue;
        };
        let mut row = message.clone();
        let edit = std::sync::Arc::make_mut(&mut row);
        edit.id = format!("{SIDE_QUESTION_ID_PREFIX}{}", message.id);
        edit.blocks = vec![
            ChatBlock::Text {
                text: question.clone(),
            },
            ChatBlock::Text { text: answer },
        ];
        folded.push(row);
    }
    folded
}

pub fn is_side_question_message(message: &ChatMessage) -> bool {
    message.id.starts_with(SIDE_QUESTION_ID_PREFIX)
}

/// `{question, answer, answerMarkdown}` for a folded row; the renderers draw it folded by default.
pub fn side_question_presentation(message: &ChatMessage) -> Value {
    if !is_side_question_message(message) {
        return Value::Null;
    }
    let texts: Vec<&str> = message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let answer = texts.get(1).copied();
    let markdown = answer
        .map(|answer| native_markdown(&agent_line_breaks(answer, AgentLineBreaks::Every), false));
    json!({
        "question": texts.first().copied().unwrap_or_default(),
        "answer": answer,
        "answerReferences": markdown.as_deref().map(crate::transcript::markdown_links::markdown_references),
        "answerMarkdown": markdown,
    })
}

/// The question of the side-question card open above the composer, when it is on screen.
pub fn live_side_question(state: &crate::state::ChatState) -> Option<String> {
    if !crate::questions::gates::notice_visible(state) {
        return None;
    }
    let notice = state.session.terminal_notice.as_ref()?;
    notice
        .get("dialog")?
        .get("sideQuestion")?
        .get("question")?
        .as_str()
        .map(str::to_string)
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: a sent side question showed twice, as the card above the composer and as the row in the transcript; only the card shows until the user clicks Close, then the row takes its place. The newest row asking the card's question is left out while the card is open. A question Claude cut short with `…` matches the full text it stands for.
pub fn hide_live_side_question(messages: &mut Vec<ChatRow>, question: &str) {
    hide_newest_side_question(messages, question);
}

/// [`hide_live_side_question`], saying whether `messages` held a row asking it.
pub fn hide_newest_side_question(messages: &mut Vec<ChatRow>, question: &str) -> bool {
    match messages
        .iter()
        .rposition(|message| asks_side_question(message, question))
    {
        Some(at) => {
            messages.remove(at);
            true
        }
        None => false,
    }
}

/// Whether a folded side-question row asks the card's question. A question Claude cut short with
/// `…` matches the full text it stands for.
pub fn asks_side_question(message: &ChatMessage, question: &str) -> bool {
    if !is_side_question_message(message) {
        return false;
    }
    let Some(ChatBlock::Text { text }) = message.blocks.first() else {
        return false;
    };
    match question.strip_suffix('\u{2026}') {
        Some(prefix) => text.starts_with(prefix.trim_end()),
        None => text == question,
    }
}
