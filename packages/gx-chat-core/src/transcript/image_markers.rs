//! The two image markers a transcript-decoded user turn can carry.
//!
//! Ported from `packages/core-ui/chat/session-chat-image-transcript-markers.ts`.

use crate::state::ChatRow;
use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::transcript::jsstr::{is_js_space, js_trim};

/// `^\[Image:\s*source:\s*(.+?)\]\s*$`.
pub fn image_source_path_from_text(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("[Image:")?;
    let rest = &rest[leading_space(rest)..];
    let rest = rest.strip_prefix("source:")?;
    let rest = &rest[leading_space(rest)..];
    // `(.+?)\]\s*$` is lazy, so the first `]` that leaves only whitespace behind closes it.
    let mut cursor = 0;
    while let Some(at) = rest[cursor..].find(']') {
        let close = cursor + at;
        if close > 0 && js_trim(&rest[close + 1..]).is_empty() {
            return Some(js_trim(&rest[..close]));
        }
        cursor = close + 1;
    }
    None
}

fn leading_space(value: &str) -> usize {
    value
        .chars()
        .take_while(|character| is_js_space(*character))
        .map(char::len_utf8)
        .sum()
}

/// `^(?:\[Image #\d+\](?:\s+|$|(?=\[Image #)))+`: every marker in front of the prompt, one per
/// picture (Codex writes `[Image #1] [Image #2] text` for two, #509).
fn image_prompt_marker_len(text: &str) -> Option<usize> {
    let mut length = 0;
    while let Some(marker) = single_image_prompt_marker_len(&text[length..]) {
        length += marker;
    }
    (length > 0).then_some(length)
}

/// `^\[Image #\d+\](?:\s+|$|(?=\[Image #))`.
fn single_image_prompt_marker_len(text: &str) -> Option<usize> {
    let rest = text.strip_prefix("[Image #")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || !rest[digits..].starts_with(']') {
        return None;
    }
    let after = "[Image #".len() + digits + 1;
    let space = leading_space(&text[after..]);
    if space == 0 && after != text.len() && !text[after..].starts_with("[Image #") {
        return None;
    }
    Some(after + space)
}

pub fn strip_image_prompt_marker(text: &str) -> String {
    match image_prompt_marker_len(text) {
        Some(length) => text[length..].to_string(),
        None => text.to_string(),
    }
}

fn sole_text(message: &ChatMessage) -> &str {
    match (message.blocks.len(), message.blocks.first()) {
        (1, Some(ChatBlock::Text { text })) => text,
        _ => "",
    }
}

fn strip_first_image_prompt_marker(blocks: &[ChatBlock]) -> Vec<ChatBlock> {
    let mut stripped = false;
    let mut next = Vec::with_capacity(blocks.len());
    for block in blocks {
        if !stripped {
            if let ChatBlock::Text { text } = block {
                stripped = true;
                let text = strip_image_prompt_marker(text);
                if !js_trim(&text).is_empty() {
                    next.push(ChatBlock::Text { text });
                }
                continue;
            }
        }
        next.push(block.clone());
    }
    next
}

pub fn image_prompt_marker_starts_message(message: &ChatMessage) -> bool {
    message
        .blocks
        .iter()
        .find_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .is_some_and(|text| image_prompt_marker_len(text).is_some())
}

/// Folds a `[Image: source: …]` marker row onto the prompt that follows it, so a picture the reader
/// attached renders as an image block rather than as two lines of bookkeeping.
pub fn normalize_image_transcript_messages(messages: &[ChatRow]) -> Vec<ChatRow> {
    let mut normalized: Vec<ChatRow> = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        if message.role != ChatRole::User || message.source != ChatSource::Transcript {
            normalized.push(message.clone());
            index += 1;
            continue;
        }

        let image_path = image_source_path_from_text(sole_text(message)).map(str::to_string);
        let next = messages.get(index + 1);
        let merges = image_path.is_some()
            && next.is_some_and(|next| {
                next.role == ChatRole::User
                    && next.source == message.source
                    && image_prompt_marker_starts_message(next)
            });
        if merges {
            let next = next.expect("a merge implies a following message");
            let mut folded = next.clone();
            let mut blocks = vec![ChatBlock::ImageRef {
                path: image_path,
                url: None,
                alt: None,
            }];
            blocks.extend(strip_first_image_prompt_marker(&next.blocks));
            std::sync::Arc::make_mut(&mut folded).blocks = blocks;
            normalized.push(folded);
            index += 2;
            continue;
        }

        let mut row = message.clone();
        std::sync::Arc::make_mut(&mut row).blocks = match image_path {
            Some(path) => vec![ChatBlock::ImageRef {
                path: Some(path),
                url: None,
                alt: None,
            }],
            None => strip_first_image_prompt_marker(&message.blocks),
        };
        normalized.push(row);
        index += 1;
    }
    normalized
}
