use crate::session_chat::*;
use serde_json::{json, Value};

fn row(
    id: String,
    role: SessionChatRole,
    blocks: Vec<SessionChatBlock>,
    time: Option<i64>,
    turn: Option<&str>,
) -> SessionChatMessage {
    SessionChatMessage {
        id,
        role,
        blocks,
        timestamp: time,
        source: SessionChatSource::Transcript,
        turn_id: turn.map(str::to_string),
        byte_offset: None,
        queued: false,
        async_questions: None,
    }
}

fn text_block(text: &str) -> Vec<SessionChatBlock> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![SessionChatBlock::Text { text: text.into() }]
    }
}

pub(crate) fn decode_messages(messages: &[Value]) -> Vec<SessionChatMessage> {
    let mut result = Vec::new();
    let mut turn: Option<String> = None;
    for message in messages {
        let Some(id) = message["id"].as_str() else {
            continue;
        };
        let timestamp = message["time"]["created"].as_i64();
        match message["type"].as_str().unwrap_or("") {
            "user" => {
                turn = Some(id.into());
                let text = message["text"].as_str().unwrap_or("");
                let references = super::attachments::image_references(text);
                let mut blocks = text_block(text);
                for file in message["files"].as_array().into_iter().flatten() {
                    let mime = file["mime"].as_str().unwrap_or("");
                    if mime.starts_with("image/") {
                        // Numbered composer links already render the uploaded picture in place.
                        // OpenCode retains the same bytes separately for the model's vision input.
                        if file["name"].as_str().is_some_and(|name| {
                            references
                                .iter()
                                .any(|path| path.rsplit(['/', '\\']).next() == Some(name))
                        }) {
                            continue;
                        }
                        if let Some(data) = file["data"].as_str() {
                            blocks.push(SessionChatBlock::ImageRef {
                                path: None,
                                url: Some(format!("data:{mime};base64,{data}")),
                                alt: file["name"].as_str().map(str::to_string),
                            });
                        }
                    }
                }
                result.push(row(
                    id.into(),
                    SessionChatRole::User,
                    blocks,
                    timestamp,
                    turn.as_deref(),
                ));
            }
            "assistant" => {
                for (index, part) in message["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let part_id = format!("{id}:part:{index}");
                    match part["type"].as_str().unwrap_or("") {
                        "text" | "reasoning" => {
                            let blocks = text_block(part["text"].as_str().unwrap_or(""));
                            if !blocks.is_empty() {
                                result.push(row(
                                    part_id,
                                    if part["type"] == "reasoning" {
                                        SessionChatRole::Reasoning
                                    } else {
                                        SessionChatRole::Assistant
                                    },
                                    blocks,
                                    timestamp,
                                    turn.as_deref(),
                                ));
                            }
                        }
                        "tool" => {
                            let state = &part["state"];
                            let name = part["name"].as_str().unwrap_or("tool").to_string();
                            let input = state["input"]
                                .as_str()
                                .and_then(|text| serde_json::from_str(text).ok())
                                .unwrap_or_else(|| state["input"].clone());
                            result.push(row(
                                part_id.clone(),
                                SessionChatRole::Assistant,
                                vec![SessionChatBlock::ToolCall {
                                    name,
                                    input,
                                    call_id: None,
                                }],
                                timestamp,
                                turn.as_deref(),
                            ));
                            if matches!(state["status"].as_str(), Some("completed" | "error")) {
                                let mut output = Vec::new();
                                for content in state["content"].as_array().into_iter().flatten() {
                                    if let Some(text) = content["text"].as_str() {
                                        output.push(text.to_string());
                                    }
                                }
                                if state["status"] == "error" {
                                    output.push(
                                        state["error"]["message"]
                                            .as_str()
                                            .map(str::to_string)
                                            .unwrap_or_else(|| state["error"].to_string()),
                                    );
                                }
                                result.push(row(
                                    format!("{part_id}:result"),
                                    SessionChatRole::Tool,
                                    vec![SessionChatBlock::ToolResult {
                                        output: output.join("\n"),
                                        is_error: Some(state["status"] == "error"),
                                        call_id: None,
                                    }],
                                    timestamp,
                                    turn.as_deref(),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                if !message["error"].is_null() {
                    let text = message["error"]["message"]
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| message["error"].to_string());
                    result.push(row(
                        format!("{id}:error"),
                        SessionChatRole::System,
                        text_block(&text),
                        timestamp,
                        turn.as_deref(),
                    ));
                }
            }
            "compaction" if message["status"] == "completed" => result.push(row(
                id.into(),
                SessionChatRole::System,
                text_block(CONTEXT_COMPACTED_STATUS_TEXT),
                timestamp,
                turn.as_deref(),
            )),
            "system" => result.push(row(
                id.into(),
                SessionChatRole::System,
                text_block(
                    message["description"]
                        .as_str()
                        .or(message["text"].as_str())
                        .unwrap_or(""),
                ),
                timestamp,
                turn.as_deref(),
            )),
            "shell" => {
                result.push(row(
                    id.into(),
                    SessionChatRole::Assistant,
                    vec![SessionChatBlock::ToolCall {
                        name: "bash".into(),
                        input: json!({"command":message["command"]}),
                        call_id: None,
                    }],
                    timestamp,
                    turn.as_deref(),
                ));
                if let Some(output) = message["output"]["output"].as_str() {
                    result.push(row(
                        format!("{id}:result"),
                        SessionChatRole::Tool,
                        vec![SessionChatBlock::ToolResult {
                            output: output.into(),
                            is_error: Some(message["exit"].as_i64().is_some_and(|code| code != 0)),
                            call_id: None,
                        }],
                        timestamp,
                        turn.as_deref(),
                    ));
                }
            }
            _ => {}
        }
    }
    result
}

pub(crate) fn decode_line(line: &str, _: &str) -> Option<SessionChatMessage> {
    serde_json::from_str(line).ok()
}

pub(crate) fn decode_lifecycle(line: &str, _: &str) -> Option<SessionChatTurnLifecycle> {
    serde_json::from_value(serde_json::from_str::<Value>(line).ok()?["opencodeLifecycle"].clone())
        .ok()
}
