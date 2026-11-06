/*
Empryo's chat is read through the Ghostex-owned mirror that `session_chat_empryo_mirror.rs`
derives from Empryo's `session.jsonl`. One mirror row per rendered message, plus turn rows:

    {"row": "user", "id": "empryo:<key>", "turn": "<key>", "ts": 1791…, "text": "…",
     "images": ["screenshot.png"]}
    {"row": "reasoning", "id": "empryo:<key>:reasoning:2", "turn": "<key>", "ts": …, "text": "…"}
    {"row": "assistant", "id": "empryo:<key>:text:4", "turn": "<key>", "ts": …, "text": "…"}
    {"row": "assistant", "id": "empryo:<key>:calls:<callId>", "turn": "<key>", "ts": …,
     "toolCalls": [{"callId": "…", "name": "Bash", "input": {"command": "…"}}]}
    {"row": "tool", "id": "empryo:<key>:result:<callId>", "turn": "<key>", "ts": …,
     "callId": "…", "output": "…", "isError": false}
    {"row": "turn", "turn": "<key>", "state": "working" | "completed" | "interrupted", "ts": …}
    {"row": "notice", "id": "empryo:note:<tab>:3", "turn": "<key>" | null, "ts": …,
     "text": "Set effort level to high"}

`<key>` is the Empryo user record's own id, so row ids are stable across the mirror rewrites a
checkpoint causes.
*/

use serde_json::{Map, Value};

use crate::session_chat::*;

fn empryo_message(
    record: &Map<String, Value>,
    fallback_id: &str,
    role: SessionChatRole,
    blocks: Vec<SessionChatBlock>,
) -> SessionChatMessage {
    SessionChatMessage {
        id: extract_string(record.get("id")).unwrap_or_else(|| fallback_id.to_string()),
        role,
        blocks,
        async_questions: None,
        timestamp: parse_timestamp(record.get("ts")),
        source: SessionChatSource::Transcript,
        // The chat core keys rows by `turnId`, so the rows of one turn must not share one.
        turn_id: None,
        byte_offset: None,
        queued: false,
        api_error: None,
        custom_message: None,
    }
}

pub fn decode_empryo_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let text = extract_string(record.get("text"));
    match record.get("row").and_then(Value::as_str)? {
        "user" => {
            let mut blocks = Vec::new();
            if let Some(text) = text {
                blocks.push(text_block(text));
            }
            for image in record
                .get("images")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                blocks.push(SessionChatBlock::ImageRef {
                    path: None,
                    url: None,
                    alt: Some(
                        image
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| PASTED_IMAGE_ALT.to_string()),
                    ),
                });
            }
            (!blocks.is_empty())
                .then(|| empryo_message(&record, fallback_id, SessionChatRole::User, blocks))
        }
        "reasoning" => Some(empryo_message(
            &record,
            fallback_id,
            SessionChatRole::Reasoning,
            vec![text_block(text?)],
        )),
        "assistant" => {
            let mut blocks = Vec::new();
            if let Some(text) = text {
                blocks.push(text_block(text));
            }
            for call in record
                .get("toolCalls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
            {
                blocks.push(SessionChatBlock::ToolCall {
                    name: extract_string(call.get("name")).unwrap_or_else(|| "tool".into()),
                    input: bounded_tool_call_input(call.get("input").cloned().unwrap_or_default()),
                    call_id: extract_string(call.get("callId")),
                });
            }
            (!blocks.is_empty())
                .then(|| empryo_message(&record, fallback_id, SessionChatRole::Assistant, blocks))
        }
        // An event, a model or effort change, or one of Empryo's own system rows
        // (session_chat_empryo_notes.rs); the chat core draws the changes as status rows.
        "notice" => Some(empryo_message(
            &record,
            fallback_id,
            SessionChatRole::System,
            vec![text_block(text?)],
        )),
        "tool" => Some(empryo_message(
            &record,
            fallback_id,
            SessionChatRole::Tool,
            vec![SessionChatBlock::ToolResult {
                output: tool_result_output(record.get("output")),
                is_error: (record.get("isError") == Some(&Value::Bool(true))).then_some(true),
                call_id: extract_string(record.get("callId")),
                patch: None,
            }],
        )),
        _ => None,
    }
}

pub fn decode_empryo_turn_lifecycle(
    line: &str,
    _fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    if record.get("row").and_then(Value::as_str)? != "turn" {
        return None;
    }
    let state = match record.get("state").and_then(Value::as_str)? {
        "working" => SessionChatTurnLifecycleState::Working,
        "completed" => SessionChatTurnLifecycleState::Completed,
        "interrupted" => SessionChatTurnLifecycleState::Interrupted,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: extract_string(record.get("turn"))?,
        timestamp: parse_timestamp(record.get("ts")),
    })
}
