/*
Antigravity CLI's chat is read through the Ghostex-owned mirror that
`session_chat_antigravity_mirror.rs` derives from the CLI's per-conversation
step log. One mirror row per rendered message, in this shape:

    {"stepIndex": 7, "part": "reasoning", "createdAt": "2026-…", "text": "…"}
    {"stepIndex": 7, "part": "assistant", "createdAt": "…", "text": "pong",
     "toolCalls": [{"name": "run_command", "args": {"CommandLine": "echo pong"}}]}
    {"stepIndex": 0, "part": "user", "createdAt": "…", "text": "…"}
    {"stepIndex": 2, "part": "tool", "createdAt": "…", "toolType": "GENERIC",
     "text": "The command exited with code 0.\nOutput:\npong", "isError": true}

`stepIndex` is the CLI's own trajectory index, so ids are stable across mirror
rewrites; a planner step's reasoning and message rows share it and differ by
part.
*/

use serde_json::{Map, Value};

use crate::session_chat::*;

fn antigravity_row_id(record: &Map<String, Value>, part: &str, fallback_id: &str) -> String {
    match record.get("stepIndex").and_then(Value::as_u64) {
        Some(step_index) if part == "reasoning" => {
            format!("antigravity-step-{step_index}-reasoning")
        }
        Some(step_index) => format!("antigravity-step-{step_index}"),
        None => fallback_id.to_string(),
    }
}

fn antigravity_tool_call_blocks(record: &Map<String, Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(tool_calls)) = record.get("toolCalls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            Some(SessionChatBlock::ToolCall {
                name: extract_string(tool_call.get("name")).unwrap_or_else(|| "tool".to_string()),
                input: tool_call.get("args").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub fn decode_antigravity_transcript_line(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let part = record.get("part").and_then(Value::as_str)?;
    let id = antigravity_row_id(&record, part, fallback_id);
    let timestamp = parse_timestamp(record.get("createdAt"));
    let text = extract_string(record.get("text"));
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

    match part {
        "user" => Some(message(SessionChatRole::User, vec![text_block(text?)])),
        "reasoning" => Some(message(SessionChatRole::Reasoning, vec![text_block(text?)])),
        "assistant" => {
            let mut blocks = Vec::new();
            if let Some(text) = text {
                blocks.push(text_block(text));
            }
            blocks.extend(antigravity_tool_call_blocks(&record));
            if blocks.is_empty() {
                return None;
            }
            Some(message(SessionChatRole::Assistant, blocks))
        }
        "tool" => {
            let is_error = (record.get("isError") == Some(&Value::Bool(true))).then_some(true);
            Some(message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: text.unwrap_or_default(),
                    is_error,
                }],
            ))
        }
        _ => None,
    }
}

/*
The step log writes no explicit turn markers: a user row opens a turn, and a
planner step that answers in prose WITHOUT asking for tools is the turn's last
step (a step with tool calls is always followed by the tool's result and
another planner step). Interrupts write no closing row, so ready-state
recovery for them rides the agent-hook activity signal like Pi's.
*/
pub fn decode_antigravity_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let part = record.get("part").and_then(Value::as_str)?;
    let turn_id = antigravity_row_id(&record, part, fallback_id);
    let timestamp = parse_timestamp(record.get("createdAt"));
    match part {
        "user" => Some(SessionChatTurnLifecycle {
            state: SessionChatTurnLifecycleState::Working,
            turn_id,
            timestamp,
        }),
        "assistant" => {
            let asks_for_tools = record
                .get("toolCalls")
                .and_then(Value::as_array)
                .is_some_and(|tool_calls| !tool_calls.is_empty());
            if asks_for_tools {
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
