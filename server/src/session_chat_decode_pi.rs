use serde_json::{Map, Value};

use crate::session_chat::*;

fn pi_inline_image_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
    let has_inline_bytes = record
        .get("data")
        .is_some_and(|data| data.as_str().is_some_and(|text| !text.trim().is_empty()));
    if !has_inline_bytes {
        return image_ref_block(record);
    }
    Some(SessionChatBlock::ImageRef {
        path: None,
        url: None,
        alt: Some(PASTED_IMAGE_ALT.to_string()),
    })
}

fn pi_message_content(content: Option<&Value>) -> (Vec<SessionChatBlock>, Vec<SessionChatBlock>) {
    let mut visible = Vec::new();
    let mut reasoning = Vec::new();
    let items: Vec<&Value> = match content {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    };
    for item in items {
        if let Value::String(text) = item {
            if !text.trim().is_empty() {
                visible.push(text_block(text.clone()));
            }
            continue;
        }
        let Some(record) = item.as_object() else {
            continue;
        };
        match record.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = extract_string(record.get("text")) {
                    visible.push(text_block(text));
                }
            }
            Some("thinking") => {
                if let Some(text) = extract_string(record.get("thinking")) {
                    reasoning.push(text_block(text));
                }
            }
            Some("toolCall") => {
                visible.push(SessionChatBlock::ToolCall {
                    name: extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string()),
                    input: record.get("arguments").cloned().unwrap_or(Value::Null),
                });
            }
            Some("image") => {
                if let Some(block) = pi_inline_image_block(record) {
                    visible.push(block);
                }
            }
            _ => {}
        }
    }
    (visible, reasoning)
}

pub fn decode_pi_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let record_type = record.get("type").and_then(Value::as_str)?;
    let id = extract_string(record.get("id")).unwrap_or_else(|| fallback_id.to_string());
    let record_timestamp = parse_timestamp(record.get("timestamp"));
    let transcript_message = |role, blocks, timestamp| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    };

    if record_type == "compaction" || record_type == "branch_summary" {
        let summary = extract_string(record.get("summary"))?;
        return Some(transcript_message(
            SessionChatRole::System,
            vec![text_block(summary)],
            record_timestamp,
        ));
    }
    if record_type == "custom_message" {
        if record.get("display") == Some(&Value::Bool(false)) {
            return None;
        }
        let (blocks, _) = pi_message_content(record.get("content"));
        if blocks.is_empty() {
            return None;
        }
        return Some(transcript_message(
            SessionChatRole::System,
            blocks,
            record_timestamp,
        ));
    }
    if record_type != "message" {
        return None;
    }

    let message = as_record(record.get("message"))?;
    let role = message.get("role").and_then(Value::as_str)?;
    let timestamp = parse_timestamp(message.get("timestamp")).or(record_timestamp);
    let (mut blocks, reasoning) = pi_message_content(message.get("content"));
    match role {
        "user" => {
            if blocks.is_empty() {
                return None;
            }
            Some(transcript_message(SessionChatRole::User, blocks, timestamp))
        }
        "assistant" => {
            if blocks.is_empty() {
                if let Some(error) = extract_string(message.get("errorMessage")) {
                    blocks.push(text_block(error));
                } else if !reasoning.is_empty() {
                    return Some(transcript_message(
                        SessionChatRole::Reasoning,
                        reasoning,
                        timestamp,
                    ));
                } else {
                    return None;
                }
            }
            Some(transcript_message(
                SessionChatRole::Assistant,
                blocks,
                timestamp,
            ))
        }
        "toolResult" => {
            let output = tool_result_output(message.get("content"));
            let is_error = message.get("isError") == Some(&Value::Bool(true));
            let mut tool_blocks = vec![SessionChatBlock::ToolResult {
                output,
                is_error: if is_error { Some(true) } else { None },
            }];
            tool_blocks.extend(
                blocks
                    .into_iter()
                    .filter(|block| matches!(block, SessionChatBlock::ImageRef { .. })),
            );
            Some(transcript_message(
                SessionChatRole::Tool,
                tool_blocks,
                timestamp,
            ))
        }
        "bashExecution" => {
            let command = extract_string(message.get("command")).unwrap_or_default();
            let output = extract_string(message.get("output")).unwrap_or_default();
            let text = match (command.is_empty(), output.is_empty()) {
                (true, true) => return None,
                (false, true) => format!("$ {command}"),
                (true, false) => output,
                (false, false) => format!("$ {command}\n{output}"),
            };
            Some(transcript_message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: text,
                    is_error: None,
                }],
                timestamp,
            ))
        }
        "custom" => {
            if message.get("display") == Some(&Value::Bool(false)) || blocks.is_empty() {
                return None;
            }
            Some(transcript_message(
                SessionChatRole::System,
                blocks,
                timestamp,
            ))
        }
        "branchSummary" | "compactionSummary" => {
            let summary = extract_string(message.get("summary"))?;
            Some(transcript_message(
                SessionChatRole::System,
                vec![text_block(summary)],
                timestamp,
            ))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Noise filter (upstream chat spec §9.1) — needed by the Claude lifecycle decoder.
// ---------------------------------------------------------------------------

