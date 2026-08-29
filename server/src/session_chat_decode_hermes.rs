/*
Hermes Agent persists conversations in `~/.hermes/state.db` (SQLite `messages`
rows), not a jsonl transcript. `session_chat_hermes.rs` mirrors each session's
active rows into a per-session jsonl file — one JSON object per row, in the
shape decoded here — so the rest of the chat pipeline can treat Hermes like
every other agent. The record shape is Ghostex-owned:

    {"rowId": 15, "role": "tool", "content": "…", "toolCalls": [...],
     "toolName": "terminal", "toolCallId": "call_…", "timestamp": 1787…,
     "finishReason": "stop", "reasoning": "…", "reasoningContent": "…"}

`toolCalls` is the OpenAI-style array Hermes stores on assistant rows
(`function.name` + `function.arguments` as a JSON string). Tool rows store the
tool's return value serialized as JSON, usually `{"output": …, "exit_code": …,
"error": …}` for terminal-family tools.
*/

use serde_json::{Map, Value};

use crate::session_chat::*;

fn hermes_row_id(record: &Map<String, Value>, fallback_id: &str) -> String {
    record
        .get("rowId")
        .and_then(Value::as_i64)
        .map(|row_id| format!("hermes-row-{row_id}"))
        .unwrap_or_else(|| fallback_id.to_string())
}

fn hermes_tool_call_blocks(record: &Map<String, Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(tool_calls)) = record.get("toolCalls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            let function = as_record(tool_call.get("function"));
            let name = function
                .and_then(|function| extract_string(function.get("name")))
                .or_else(|| extract_string(tool_call.get("name")))
                .unwrap_or_else(|| "tool".to_string());
            // Arguments arrive as a JSON string; parse so the UI renders
            // structured input instead of an escaped blob.
            let input = function
                .and_then(|function| function.get("arguments"))
                .map(|arguments| match arguments {
                    Value::String(text) => {
                        serde_json::from_str::<Value>(text).unwrap_or(arguments.clone())
                    }
                    other => other.clone(),
                })
                .unwrap_or(Value::Null);
            Some(SessionChatBlock::ToolCall { name, input })
        })
        .collect()
}

/// Terminal-family tools return `{"output": …, "exit_code": …, "error": …}`;
/// other tools return arbitrary JSON. Prefer the human-readable `output`
/// field, and fall back to the raw payload.
fn hermes_tool_result(content: &str) -> (String, Option<bool>) {
    let Ok(Value::Object(record)) = serde_json::from_str::<Value>(content) else {
        return (content.to_string(), None);
    };
    let is_error = record
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
        || record.get("success") == Some(&Value::Bool(false))
        || record
            .get("exit_code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0);
    let output = match record.get("output").or_else(|| record.get("result")) {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(value) if !value.is_null() => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| content.to_string())
        }
        _ => match record.get("error").and_then(Value::as_str) {
            Some(error) if !error.trim().is_empty() => error.to_string(),
            _ => serde_json::to_string_pretty(&Value::Object(record.clone()))
                .unwrap_or_else(|_| content.to_string()),
        },
    };
    (output, if is_error { Some(true) } else { None })
}

pub fn decode_hermes_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let role = record.get("role").and_then(Value::as_str)?;
    let id = hermes_row_id(&record, fallback_id);
    let timestamp = parse_timestamp(record.get("timestamp"));
    let content = extract_string(record.get("content"));
    let message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    };

    match role {
        "user" => {
            let text = content?;
            Some(message(SessionChatRole::User, vec![text_block(text)]))
        }
        "assistant" => {
            let mut blocks = Vec::new();
            if let Some(text) = content {
                blocks.push(text_block(text));
            }
            blocks.extend(hermes_tool_call_blocks(&record));
            if blocks.is_empty() {
                let reasoning = extract_string(record.get("reasoning"))
                    .or_else(|| extract_string(record.get("reasoningContent")))?;
                return Some(message(
                    SessionChatRole::Reasoning,
                    vec![text_block(reasoning)],
                ));
            }
            Some(message(SessionChatRole::Assistant, blocks))
        }
        "tool" => {
            let (output, is_error) = hermes_tool_result(&content?);
            Some(message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult { output, is_error }],
            ))
        }
        _ => None,
    }
}

/*
Hermes writes no explicit turn markers: a user row opens a turn, and the
closing signal is the final assistant row's `finish_reason`. `tool_calls`
means the turn is still running (the model asked for tools and will be called
again); every other recorded finish reason (`stop`, `length`,
`content_filter`, `incomplete`) ends the turn. Interrupts write no closing row
at all, so ready-state recovery for them rides the agent-hook activity signal
like Pi's.
*/
pub fn decode_hermes_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let turn_id = hermes_row_id(&record, fallback_id);
    let timestamp = parse_timestamp(record.get("timestamp"));
    match record.get("role").and_then(Value::as_str)? {
        "user" => Some(SessionChatTurnLifecycle {
            state: SessionChatTurnLifecycleState::Working,
            turn_id,
            timestamp,
        }),
        "assistant" => {
            let finish_reason = record.get("finishReason").and_then(Value::as_str)?;
            if finish_reason.eq_ignore_ascii_case("tool_calls") {
                return None;
            }
            Some(SessionChatTurnLifecycle {
                state: SessionChatTurnLifecycleState::Completed,
                turn_id,
                timestamp,
            })
        }
        _ => None,
    }
}
