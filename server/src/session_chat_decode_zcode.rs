use crate::session_chat::*;
use serde_json::Value;

pub fn decode_zcode_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record: Value = serde_json::from_str(line).ok()?;
    if record.get("kind")?.as_str()? == "lifecycle" {
        let error = record.pointer("/info/error")?;
        if error.pointer("/data/turnResult").and_then(Value::as_str) == Some("cancelled") {
            return None;
        }
        let message = error
            .pointer("/data/message")
            .or_else(|| error.get("message"))
            .and_then(Value::as_str)?;
        return Some(SessionChatMessage {
            id: format!(
                "{}:error",
                record
                    .get("messageId")
                    .and_then(Value::as_str)
                    .unwrap_or(fallback_id)
            ),
            role: SessionChatRole::System,
            blocks: vec![text_block(message)],
            timestamp: record.get("timestamp").and_then(Value::as_i64),
            source: SessionChatSource::Transcript,
            turn_id: record
                .pointer("/info/anchor/turnId")
                .and_then(Value::as_str)
                .map(str::to_string),
            byte_offset: None,
            async_questions: None,
            queued: false,
        });
    }
    if record.get("kind")?.as_str()? != "part" {
        return None;
    }
    let part = record.get("part")?;
    if part.get("ignored").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let mut role = match record.get("role")?.as_str()? {
        "user" => SessionChatRole::User,
        "assistant" => SessionChatRole::Assistant,
        "system" => SessionChatRole::System,
        _ => return None,
    };
    let blocks = match part.get("type")?.as_str()? {
        "text" | "reasoning" => {
            if part.get("type")?.as_str()? == "reasoning" {
                role = SessionChatRole::Reasoning;
            }
            let text = part.get("text")?.as_str()?;
            if text.trim().is_empty() {
                return None;
            }
            vec![text_block(text)]
        }
        "tool" => {
            role = SessionChatRole::Tool;
            let state = part.get("state")?;
            let mut blocks = vec![SessionChatBlock::ToolCall {
                name: part
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string(),
                input: state.get("input").cloned().unwrap_or(Value::Null),
                call_id: None,
            }];
            let status = state.get("status").and_then(Value::as_str);
            if matches!(status, Some("completed" | "error")) {
                let output = state
                    .get("output")
                    .or_else(|| state.get("error"))
                    .unwrap_or(&Value::Null);
                let output = output
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| output.to_string());
                blocks.push(SessionChatBlock::ToolResult {
                    output: bounded_tool_payload(output),
                    is_error: (status == Some("error")).then_some(true),
                    call_id: None,
                });
            }
            blocks
        }
        "file" => {
            let url = part.get("url").and_then(Value::as_str)?;
            let name = part
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or("Attachment");
            if part
                .get("mime")
                .and_then(Value::as_str)
                .is_some_and(|mime| mime.starts_with("image/"))
            {
                vec![SessionChatBlock::ImageRef {
                    path: url.strip_prefix("file://").map(str::to_string),
                    url: (!url.starts_with("file://")).then(|| url.to_string()),
                    alt: Some(name.to_string()),
                }]
            } else {
                vec![text_block(format!("{name}: {url}"))]
            }
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
        turn_id: record
            .get("turnId")
            .and_then(Value::as_str)
            .map(str::to_string),
        byte_offset: None,
        async_questions: None,
        queued: false,
    })
}

pub fn decode_zcode_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record: Value = serde_json::from_str(line).ok()?;
    if record.get("kind")?.as_str()? != "lifecycle" {
        return None;
    }
    let info = record.get("info")?;
    let state = match info.get("role")?.as_str()? {
        "user" => SessionChatTurnLifecycleState::Working,
        "assistant" if info.get("error").is_some_and(|error| !error.is_null()) => {
            SessionChatTurnLifecycleState::Interrupted
        }
        "assistant" => {
            let finish = info.get("finish").and_then(Value::as_str)?;
            if matches!(finish, "tool-calls" | "tool_calls" | "unknown")
                || info.pointer("/time/completed").is_none()
            {
                return None;
            }
            SessionChatTurnLifecycleState::Completed
        }
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: info
            .pointer("/anchor/turnId")
            .or_else(|| record.get("messageId"))
            .and_then(Value::as_str)
            .unwrap_or(fallback_id)
            .to_string(),
        timestamp: info
            .pointer("/time/completed")
            .or_else(|| record.get("timestamp"))
            .and_then(Value::as_i64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_text_thinking_tools_and_turn_boundaries() {
        let row = |part| {
            json!({"kind":"part","id":"part_test","role":"assistant","timestamp":1000,"turnId":"turn_test","part":part}).to_string()
        };
        let thinking = decode_zcode_transcript_line(
            &row(json!({"type":"reasoning","text":"Thinking"})),
            "fallback",
        )
        .unwrap();
        assert_eq!(thinking.role, SessionChatRole::Reasoning);
        assert_eq!(thinking.id, "part_test");
        let tool = decode_zcode_transcript_line(&row(json!({"type":"tool","tool":"Bash","state":{"status":"error","input":{"command":"false"},"error":"Exit 1"}})), "fallback").unwrap();
        assert!(
            matches!(&tool.blocks[1], SessionChatBlock::ToolResult { is_error: Some(true), output, call_id: None } if output == "Exit 1")
        );
        assert!(decode_zcode_transcript_line(
            &row(json!({"type":"text","text":"hidden","ignored":true})),
            "fallback"
        )
        .is_none());
        let mut lifecycle = json!({"kind":"lifecycle","info":{"role":"assistant","finish":"tool-calls","time":{"completed":1000},"anchor":{"turnId":"turn_test"}}});
        assert!(decode_zcode_turn_lifecycle(&lifecycle.to_string(), "fallback").is_none());
        lifecycle["info"]["finish"] = json!("stop");
        let completed = decode_zcode_turn_lifecycle(&lifecycle.to_string(), "fallback").unwrap();
        assert_eq!(completed.state, SessionChatTurnLifecycleState::Completed);
        assert_eq!(completed.turn_id, "turn_test");
        lifecycle["info"]["error"] =
            json!({"data":{"message":"Authentication required","code":"authentication_error"}});
        let error = decode_zcode_transcript_line(&lifecycle.to_string(), "fallback").unwrap();
        assert_eq!(error.role, SessionChatRole::System);
        assert_eq!(error.blocks, vec![text_block("Authentication required")]);
        assert_eq!(
            decode_zcode_turn_lifecycle(&lifecycle.to_string(), "fallback")
                .unwrap()
                .state,
            SessionChatTurnLifecycleState::Interrupted
        );
        lifecycle["info"]["error"]["data"]["turnResult"] = json!("cancelled");
        assert!(decode_zcode_transcript_line(&lifecycle.to_string(), "fallback").is_none());
    }
}
