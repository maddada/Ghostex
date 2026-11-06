//! Decodes the rows `session_chat_freebuff.rs` mirrors from Freebuff's `chat-messages.json`.

use crate::session_chat::*;
use serde_json::Value;

fn turn_id(record: &Value) -> Option<String> {
    record
        .get("turnId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn decode_freebuff_transcript_line(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatMessage> {
    let record: Value = serde_json::from_str(line).ok()?;
    let (role, blocks) = match record.get("kind")?.as_str()? {
        "message" => {
            let role = match record.get("role")?.as_str()? {
                "user" => SessionChatRole::User,
                "assistant" => SessionChatRole::Assistant,
                "reasoning" => SessionChatRole::Reasoning,
                "system" => SessionChatRole::System,
                _ => return None,
            };
            let text = record.get("text")?.as_str()?;
            if text.trim().is_empty() {
                return None;
            }
            (role, vec![text_block(text)])
        }
        "tool" => {
            let call_id = record.get("id").and_then(Value::as_str).map(str::to_string);
            let mut blocks = vec![SessionChatBlock::ToolCall {
                name: record
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                input: record.get("input").cloned().unwrap_or(Value::Null),
                call_id: call_id.clone(),
            }];
            if let Some(output) = record.get("output").filter(|output| !output.is_null()) {
                let output = output
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| output.to_string());
                blocks.push(SessionChatBlock::ToolResult {
                    output: bounded_tool_payload(output),
                    is_error: (record.get("isError").and_then(Value::as_bool) == Some(true))
                        .then_some(true),
                    call_id,
                    patch: None,
                });
            }
            (SessionChatRole::Tool, blocks)
        }
        _ => return None,
    };
    Some(SessionChatMessage {
        id: record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(fallback_id)
            .to_string(),
        role,
        blocks,
        timestamp: record.get("timestamp").and_then(Value::as_i64),
        source: SessionChatSource::Transcript,
        turn_id: turn_id(&record),
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    })
}

pub fn decode_freebuff_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record: Value = serde_json::from_str(line).ok()?;
    if record.get("kind")?.as_str()? != "lifecycle" {
        return None;
    }
    let state = match record.get("state")?.as_str()? {
        "working" => SessionChatTurnLifecycleState::Working,
        "completed" => SessionChatTurnLifecycleState::Completed,
        "interrupted" => SessionChatTurnLifecycleState::Interrupted,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: turn_id(&record).unwrap_or_else(|| fallback_id.to_string()),
        timestamp: record.get("timestamp").and_then(Value::as_i64),
    })
}
