//! Decodes the DeepSeek Harness session events `session_chat_dsh.rs` mirrors from DSH's session log
//! (format version 4: `{"type":"<event>","seq":…,"time":…,"data":{…}}`).

use crate::session_chat::*;
use serde_json::Value;

fn text_parts(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// CDXC:SessionChat 2026-10-28 WHY:
/// DSH's system prompt has the model open each step with an action label, `⏵ Running the tests`, which dsh-tui draws as a step marker. The chat's fonts have no `⏵`, so every reply opened with a missing-glyph box (seen in the GPUI web chat, 2026-10-28); the label stays and the glyph goes.
fn without_step_marker(text: &str) -> &str {
    text.strip_prefix('\u{23f5}')
        .map(str::trim_start)
        .unwrap_or(text)
}

fn message(
    record: &Value,
    id: Option<&Value>,
    fallback_id: &str,
    role: SessionChatRole,
    blocks: Vec<SessionChatBlock>,
) -> SessionChatMessage {
    SessionChatMessage {
        id: id
            .and_then(Value::as_str)
            .unwrap_or(fallback_id)
            .to_string(),
        role,
        blocks,
        timestamp: record.get("time").and_then(Value::as_i64),
        source: SessionChatSource::Transcript,
        turn_id: turn_id(record),
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
    }
}

fn turn_id(record: &Value) -> Option<String> {
    record
        .pointer("/data/turn")
        .and_then(Value::as_i64)
        .map(|turn| format!("turn-{turn}"))
}

/// The model's raw `arguments` string as a JSON value when it parses, else the string itself.
fn tool_input(arguments: Option<&Value>) -> Value {
    match arguments {
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

/// The reason a turn that did not complete gives, worded for the chat.
fn turn_end_notice(reason: &Value) -> Option<String> {
    match reason.get("kind")?.as_str()? {
        "error" => {
            let error = reason.get("error");
            let text = error
                .and_then(|error| error.get("message").or_else(|| error.get("reason")))
                .and_then(Value::as_str)
                .or_else(|| {
                    error
                        .and_then(|error| error.get("code"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("The model request failed.");
            Some(text.to_string())
        }
        "max-tokens" => Some("The reply reached the model's output limit.".to_string()),
        _ => None,
    }
}

pub fn decode_dsh_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record: Value = serde_json::from_str(line).ok()?;
    let data = record.get("data")?;
    match record.get("type")?.as_str()? {
        "user/message" => {
            if data.pointer("/source/kind").and_then(Value::as_str) != Some("user") {
                return None;
            }
            let text = text_parts(data.get("content"));
            if text.trim().is_empty() {
                return None;
            }
            Some(message(
                &record,
                data.get("id"),
                fallback_id,
                SessionChatRole::User,
                vec![text_block(text)],
            ))
        }
        "assistant/message" => {
            let content = data.get("message")?;
            let mut blocks = Vec::new();
            let mut reasoning = Vec::new();
            for part in content
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = part
                            .get("text")
                            .and_then(Value::as_str)
                            .map(without_step_marker)
                            .filter(|text| !text.trim().is_empty())
                        {
                            blocks.push(text_block(text));
                        }
                    }
                    Some("reasoning") => {
                        if let Some(text) = part
                            .get("text")
                            .and_then(Value::as_str)
                            .filter(|text| !text.trim().is_empty())
                        {
                            reasoning.push(text_block(text));
                        }
                    }
                    Some("tool-call") => blocks.push(SessionChatBlock::ToolCall {
                        name: part
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        input: tool_input(part.get("arguments")),
                        call_id: part.get("id").and_then(Value::as_str).map(str::to_string),
                    }),
                    _ => {}
                }
            }
            // As for Pi: a step that only thought shows its reasoning; otherwise the reply and its
            // tool calls are the message.
            let (role, blocks) = if blocks.is_empty() {
                if reasoning.is_empty() {
                    return None;
                }
                (SessionChatRole::Reasoning, reasoning)
            } else {
                (SessionChatRole::Assistant, blocks)
            };
            Some(message(
                &record,
                content.get("id"),
                fallback_id,
                role,
                blocks,
            ))
        }
        // The thinking `session_chat_dsh.rs` splits off a step that also replied or ran a tool.
        "dsh/reasoning" => {
            let thinking = data.get("message")?;
            let blocks: Vec<SessionChatBlock> = thinking
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .filter(|text| !text.trim().is_empty())
                .map(text_block)
                .collect();
            if blocks.is_empty() {
                return None;
            }
            Some(message(
                &record,
                thinking.get("id"),
                fallback_id,
                SessionChatRole::Reasoning,
                blocks,
            ))
        }
        "tool/result" => {
            let result = data.get("message")?;
            let mut output = text_parts(result.get("content"));
            if output.is_empty() {
                if let Some(reason) = data.pointer("/error/reason").and_then(Value::as_str) {
                    output = reason.to_string();
                }
            }
            let is_error = result.get("isError").and_then(Value::as_bool) == Some(true);
            Some(message(
                &record,
                result.get("id"),
                fallback_id,
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: bounded_tool_payload(output),
                    is_error: is_error.then_some(true),
                    call_id: result
                        .get("toolCallId")
                        .or_else(|| result.pointer("/source/callId"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    patch: None,
                }],
            ))
        }
        "turn/end" => {
            let notice = turn_end_notice(data.get("reason")?)?;
            Some(message(
                &record,
                None,
                &format!("{fallback_id}:end"),
                SessionChatRole::System,
                vec![text_block(notice)],
            ))
        }
        _ => None,
    }
}

pub fn decode_dsh_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record: Value = serde_json::from_str(line).ok()?;
    let state = match record.get("type")?.as_str()? {
        "turn/start" => SessionChatTurnLifecycleState::Working,
        "turn/end" => match record.pointer("/data/reason/kind").and_then(Value::as_str) {
            Some("completed") => SessionChatTurnLifecycleState::Completed,
            _ => SessionChatTurnLifecycleState::Interrupted,
        },
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: turn_id(&record).unwrap_or_else(|| fallback_id.to_string()),
        timestamp: record.get("time").and_then(Value::as_i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORDED_LOG: &str = include_str!("../testdata/dsh/fresh-round-trip.v4.jsonl");

    fn decoded() -> Vec<SessionChatMessage> {
        crate::session_chat_dsh::dsh_mirror_records(RECORDED_LOG)
            .lines()
            .enumerate()
            .filter_map(|(index, line)| decode_dsh_transcript_line(line, &format!("line-{index}")))
            .collect()
    }

    #[test]
    fn a_recorded_session_reads_as_prompt_tool_call_result_and_reply() {
        let messages = decoded();
        let roles: Vec<SessionChatRole> = messages.iter().map(|message| message.role).collect();
        assert_eq!(
            roles,
            [
                SessionChatRole::User,
                SessionChatRole::Reasoning,
                SessionChatRole::Assistant,
                SessionChatRole::Tool,
                SessionChatRole::Reasoning,
                SessionChatRole::Assistant
            ]
        );
        assert_eq!(
            messages[1].blocks,
            vec![text_block(
                "The user wants me to run a simple bash command and reply with \"DONE\"."
            )]
        );
        let messages: Vec<SessionChatMessage> = messages
            .into_iter()
            .filter(|message| message.role != SessionChatRole::Reasoning)
            .collect();
        assert_eq!(
            messages[0].blocks,
            vec![text_block(
                "Use the bash tool to run exactly: echo WEB_E2E_OK. Then reply with the single word DONE and stop."
            )]
        );
        let SessionChatBlock::ToolCall {
            name,
            input,
            call_id,
        } = &messages[1].blocks[0]
        else {
            panic!("tool call expected: {:?}", messages[1].blocks);
        };
        assert_eq!(name, "bash");
        assert_eq!(input["command"], "echo WEB_E2E_OK");
        assert_eq!(call_id.as_deref(), Some("call_00_BYXlxjFaalMg95YVqEeF2495"));
        assert_eq!(
            messages[2].blocks,
            vec![SessionChatBlock::ToolResult {
                output: "WEB_E2E_OK\n".to_string(),
                is_error: None,
                call_id: Some("call_00_BYXlxjFaalMg95YVqEeF2495".to_string()),
                patch: None,
            }]
        );
        assert_eq!(messages[3].blocks, vec![text_block("DONE")]);
        assert!(messages
            .iter()
            .skip(1)
            .all(|message| message.turn_id.as_deref() == Some("turn-1")));
    }

    #[test]
    fn the_recorded_turn_starts_working_and_ends_completed() {
        let lifecycle: Vec<SessionChatTurnLifecycle> = RECORDED_LOG
            .lines()
            .filter_map(|line| decode_dsh_turn_lifecycle(line, "x"))
            .collect();
        assert_eq!(lifecycle.len(), 2);
        assert_eq!(lifecycle[0].state, SessionChatTurnLifecycleState::Working);
        assert_eq!(lifecycle[1].state, SessionChatTurnLifecycleState::Completed);
        assert_eq!(lifecycle[0].turn_id, "turn-1");
        assert_eq!(lifecycle[1].turn_id, "turn-1");
    }

    #[test]
    fn runtime_context_is_not_a_prompt_and_time_becomes_the_timestamp() {
        let context = r#"{"type":"user/message","seq":9,"time":1788240667000,"data":{"content":[{"type":"text","text":"Current runtime context."}],"source":{"kind":"runtime-context"},"role":"user","id":"m2"},"surfaceOp":"append"}"#;
        assert!(decode_dsh_transcript_line(context, "x").is_none());
        let prompt = r#"{"type":"user/message","seq":8,"time":1788240666900,"data":{"content":[{"type":"text","text":"hi"}],"source":{"kind":"user"},"role":"user","id":"m1"},"surfaceOp":"append"}"#;
        let message = decode_dsh_transcript_line(prompt, "x").unwrap();
        assert_eq!(message.id, "m1");
        assert_eq!(message.timestamp, Some(1_788_240_666_900));
    }

    #[test]
    fn the_step_marker_glyph_is_dropped_and_its_label_kept() {
        let line = r#"{"type":"assistant/message","data":{"turn":1,"step":2,"message":{"role":"assistant","content":[{"type":"text","text":"\u23f5 Reporting the folder status\nIt is empty."}],"id":"a2"}}}"#;
        assert_eq!(
            decode_dsh_transcript_line(line, "x").unwrap().blocks,
            vec![text_block("Reporting the folder status\nIt is empty.")]
        );
    }

    #[test]
    fn a_reasoning_only_step_and_a_failed_turn_show() {
        let thinking = r#"{"type":"assistant/message","data":{"turn":2,"step":1,"message":{"role":"assistant","content":[{"type":"reasoning","text":"Let me think."}],"id":"a1"}}}"#;
        assert_eq!(
            decode_dsh_transcript_line(thinking, "x").unwrap().role,
            SessionChatRole::Reasoning
        );
        let failed = r#"{"type":"turn/end","data":{"turn":2,"reason":{"kind":"error","error":{"code":"ACCOUNT_SIGN_IN_REQUIRED","message":"Sign in to your DeepSeek account."}}}}"#;
        let notice = decode_dsh_transcript_line(failed, "x").unwrap();
        assert_eq!(notice.role, SessionChatRole::System);
        assert_eq!(
            notice.blocks,
            vec![text_block("Sign in to your DeepSeek account.")]
        );
        assert_eq!(
            decode_dsh_turn_lifecycle(failed, "x").unwrap().state,
            SessionChatTurnLifecycleState::Interrupted
        );
        let completed = r#"{"type":"turn/end","data":{"turn":2,"reason":{"kind":"completed"}}}"#;
        assert!(decode_dsh_transcript_line(completed, "x").is_none());
    }
}
