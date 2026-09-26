//! The synthetic streaming bubble, and how a send is classified.
//!
//! Ported from `packages/core-ui/chat/session-chat-streaming.ts` and
//! `session-chat-send-classification.ts`.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::session::constants::STREAMING_ID;
use crate::session::text::is_js_space;

fn assistant_text(message: Option<&ChatMessage>) -> String {
    let Some(message) = message.filter(|message| matches!(message.role, ChatRole::Assistant))
    else {
        return String::new();
    };
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>()
        .trim_matches(is_js_space)
        .to_string()
}

/// Show the hook's assistant preview as a synthetic bubble only while it LEADS the transcript
/// (strictly longer and not a substring of the last assistant turn), and only while working. A
/// stale preview from a finished turn never shows.
pub fn derive_streaming_text(
    messages: &[ChatMessage],
    preview_text: Option<&str>,
    working: bool,
) -> Option<String> {
    if !working {
        return None;
    }
    let text = preview_text?.trim_matches(is_js_space);
    if text.is_empty() {
        return None;
    }
    let last = assistant_text(messages.last());
    if last.contains(text) || text.chars().count() <= last.chars().count() {
        return None;
    }
    Some(text.to_string())
}

fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    line.len() - trimmed.len() <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
}

/// An ATX heading. It ends the paragraph above it even with no blank line between them.
fn is_atx_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    line.len() - trimmed.len() <= 3
        && (1..=6).contains(&hashes)
        && trimmed[hashes..]
            .chars()
            .next()
            .is_none_or(|ch| ch == ' ' || ch == '\t')
}

/// A line of only bold text, which models often use as a heading. Right under a paragraph line it
/// continues that paragraph instead.
fn is_bold_title(line: &str) -> bool {
    let trimmed = line.trim_matches(is_js_space);
    let bold = trimmed.strip_suffix(':').unwrap_or(trimmed);
    bold.len() > 4
        && bold.starts_with("**")
        && bold.ends_with("**")
        && !bold[2..bold.len() - 2].contains("**")
}

/// The streaming bubble's text with the section titles at its end held back.
///
/// CDXC:SessionChat 2026-09-26 DECISION:
/// User: a streamed section title must not land alone and sit above a block that is still streaming; it waits for the text under it and ships with its paragraph, its first list item, or its table or code block. Only the GPUI chat view on the Rust chat core does this, and a title is recognised from its Markdown only: gxserver writes the rows Claude paints fully bold as `**Title**` so the stream carries it. A run of titles (`# Plan` then `## Setup`) is held together, and a title inside an open code fence is code. This supersedes the 2026-09-25 version that also guessed titles from short unpunctuated lines.
/// SEE-ALSO: `render_agent_stream` in server/src/session_chat_terminal_activity.rs.
pub fn without_trailing_section_titles(text: &str) -> &str {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut titles = vec![false; lines.len()];
    let mut in_fence = false;
    for (index, line) in lines.iter().enumerate() {
        if is_fence_line(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // A stream whose top scrolled off starts with `… `.
        let line = if index == 0 {
            line.trim_start_matches("… ")
        } else {
            line
        };
        let alone = index == 0 || lines[index - 1].trim_matches(is_js_space).is_empty();
        titles[index] = is_atx_heading(line) || (alone && is_bold_title(line));
    }
    let mut keep = lines.len();
    while keep > 0 && (titles[keep - 1] || lines[keep - 1].trim_matches(is_js_space).is_empty()) {
        keep -= 1;
    }
    if keep == lines.len() {
        return text;
    }
    let end = lines[..keep]
        .iter()
        .map(|line| line.len() + 1)
        .sum::<usize>();
    text[..end.saturating_sub(1)].trim_end_matches(is_js_space)
}

/// The bubble itself.
pub fn streaming_message(text: &str) -> ChatMessage {
    ChatMessage {
        id: STREAMING_ID.to_string(),
        role: ChatRole::Assistant,
        blocks: vec![ChatBlock::Text {
            text: text.to_string(),
        }],
        async_questions: None,
        timestamp: None,
        source: ChatSource::Hook,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
    }
}

/// How a composer send is treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendClassification {
    /// Ordinary prose: it gets an optimistic echo.
    Chat,
    /// A catalog slash command: it gets a "Ran /x" marker and no echo.
    Command,
    /// A `!` shell line.
    LocalCommand,
    /// A `/token` or `$token` the catalog does not know: sent as is, no echo, no marker.
    UnknownToken,
}

/// `classifySessionChatSend`.
///
/// The first token is NOT trimmed: a leading space means prose, because the agent TUIs only treat
/// LINE-LEADING tokens as commands.
pub fn classify_send(
    draft: &str,
    catalog_command_names: &[String],
    skill_prefix: Option<&str>,
) -> SendClassification {
    let first_token = draft.split(is_js_space).next().unwrap_or_default();
    if catalog_command_names
        .iter()
        .any(|name| first_token == format!("/{name}"))
    {
        return SendClassification::Command;
    }
    if first_token.starts_with('/') {
        return SendClassification::UnknownToken;
    }
    if first_token.starts_with('!') {
        return SendClassification::LocalCommand;
    }
    if skill_prefix == Some("$") && first_token.starts_with('$') {
        // `$` is Codex grammar only; elsewhere `$PATH` is prose.
        return SendClassification::UnknownToken;
    }
    SendClassification::Chat
}
