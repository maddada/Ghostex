use serde_json::Value;

use crate::session_chat::*;

const CURSOR_REDACTED_REASONING: &str = "[REDACTED]";

fn strip_cursor_metadata_block<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let body = text.strip_prefix(open)?;
    let close_at = body.find(close)?;
    Some(&body[close_at + close.len()..])
}

fn is_cursor_user_query_envelope_prefix(mut prefix: &str) -> bool {
    let mut found_metadata = false;
    loop {
        prefix = prefix.trim_start();
        if let Some(rest) = prefix.strip_prefix("[Image]") {
            if rest.chars().next().is_none_or(char::is_whitespace) {
                prefix = rest;
                found_metadata = true;
                continue;
            }
        }
        if let Some(rest) = strip_cursor_metadata_block(prefix, "<image_files>", "</image_files>") {
            prefix = rest;
            found_metadata = true;
            continue;
        }
        if let Some(rest) = strip_cursor_metadata_block(prefix, "<timestamp>", "</timestamp>") {
            prefix = rest;
            found_metadata = true;
            continue;
        }
        return found_metadata && prefix.trim().is_empty();
    }
}

fn cursor_user_query(text: &str) -> String {
    let Some(open) = text.find("<user_query>") else {
        return text.to_string();
    };
    let Some(close) = text.rfind("</user_query>") else {
        return text.to_string();
    };
    let after_close = close + "</user_query>".len();
    if close < open + "<user_query>".len()
        || !is_cursor_user_query_envelope_prefix(&text[..open])
        || !text[after_close..].trim().is_empty()
    {
        return text.to_string();
    }
    text[open + "<user_query>".len()..close]
        .trim_matches(['\r', '\n'])
        .to_string()
}

fn cursor_visible_text(text: &str) -> Option<String> {
    let visible = text
        .lines()
        .filter(|line| line.trim() != CURSOR_REDACTED_REASONING)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (!visible.is_empty()).then_some(visible)
}

/// Blocks plus whether every one of them came from a `thinking` block, which
/// is what turns the message into a reasoning turn. Thinking blocks are not in
/// Cursor's raw jsonl; the chat mirror splices them in from the session store
/// (`session_chat_cursor_mirror`).
fn cursor_message_blocks(role: &str, content: Option<&Value>) -> (Vec<SessionChatBlock>, bool) {
    let Some(items) = content.and_then(Value::as_array) else {
        return (Vec::new(), false);
    };
    let mut blocks = Vec::new();
    let mut thinking_blocks = 0usize;
    for item in items {
        let Some(record) = item.as_object() else {
            continue;
        };
        match record.get("type").and_then(Value::as_str) {
            Some("thinking") => {
                let Some(text) = extract_string(record.get("text")) else {
                    continue;
                };
                if !text.trim().is_empty() {
                    blocks.push(text_block(text));
                    thinking_blocks += 1;
                }
            }
            Some("text") => {
                let Some(text) = record.get("text").and_then(Value::as_str) else {
                    continue;
                };
                let text = if role == "user" {
                    Some(cursor_user_query(text))
                } else {
                    cursor_visible_text(text)
                };
                if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                    blocks.push(text_block(text));
                }
            }
            Some("tool_use") => blocks.push(SessionChatBlock::ToolCall {
                name: extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string()),
                input: record.get("input").cloned().unwrap_or(Value::Null),
            }),
            _ => {}
        }
    }
    let reasoning_only = thinking_blocks > 0 && thinking_blocks == blocks.len();
    (blocks, reasoning_only)
}

pub fn decode_cursor_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) == Some("turn_ended") {
        if record.get("status").and_then(Value::as_str) == Some("success") {
            return None;
        }
        let text = extract_string(record.get("error"))
            .unwrap_or_else(|| INTERRUPTED_STATUS_TEXT.to_string());
        return Some(SessionChatMessage {
            id: fallback_id.to_string(),
            role: SessionChatRole::System,
            blocks: vec![text_block(text)],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: Some(fallback_id.to_string()),
            byte_offset: None,
            queued: false,
        });
    }

    let role = record.get("role").and_then(Value::as_str)?;
    let message = as_record(record.get("message"))?;
    let (blocks, reasoning_only) = cursor_message_blocks(role, message.get("content"));
    if blocks.is_empty() {
        return None;
    }
    let role = match role {
        "user" => SessionChatRole::User,
        "assistant" if reasoning_only => SessionChatRole::Reasoning,
        "assistant" => SessionChatRole::Assistant,
        _ => return None,
    };
    Some(SessionChatMessage {
        id: fallback_id.to_string(),
        role,
        blocks,
        timestamp: None,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    })
}

pub fn decode_cursor_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) != Some("turn_ended") {
        return None;
    }
    let state = match record.get("status").and_then(Value::as_str) {
        Some("success") => SessionChatTurnLifecycleState::Completed,
        Some("error" | "aborted") => SessionChatTurnLifecycleState::Interrupted,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: fallback_id.to_string(),
        timestamp: None,
    })
}
