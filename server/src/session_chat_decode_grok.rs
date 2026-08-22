use serde_json::{Map, Value};

use crate::session_chat::*;

/// The one text block an ACP `content` value carries, empty ones rejected.
fn grok_update_text(content: Option<&Value>) -> Option<String> {
    let text = match content? {
        Value::String(text) => text.clone(),
        value => extract_string(as_record(Some(value))?.get("text"))?,
    };
    (!text.trim().is_empty()).then_some(text)
}

fn grok_tool_meta(update: &Map<String, Value>) -> Option<&Map<String, Value>> {
    as_record(as_record(update.get("_meta"))?.get("x.ai/tool"))
}

/*
The display label ("Write", "Read", "Edit") rather than the wire name
("write", "read_file", "search_replace"): it is what grok's own UI shows, and
the shared edit-tool table that renders a diff from the call's input is keyed by
exactly those labels.
*/
fn grok_tool_name(update: &Map<String, Value>) -> String {
    grok_tool_meta(update)
        .and_then(|meta| {
            extract_string(meta.get("label")).or_else(|| extract_string(meta.get("name")))
        })
        .or_else(|| extract_string(update.get("title")))
        .unwrap_or_else(|| "tool".to_string())
}

/*
A completed call carries its output as ACP content entries: `content` for tool
text (a file read, command output) and `diff` for an edit. `rawOutput` is the
raw tool struct, used only when neither is present — its
`tool_output_for_prompt` is the one-line summary the model itself was given.
*/
fn grok_tool_result_output(update: &Map<String, Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(Value::Array(items)) = update.get("content") {
        for item in items {
            let Some(record) = item.as_object() else {
                continue;
            };
            match record.get("type").and_then(Value::as_str) {
                Some("content") => {
                    if let Some(text) = grok_update_text(record.get("content")) {
                        parts.push(text);
                    }
                }
                Some("diff") => {
                    let path = extract_string(record.get("path")).unwrap_or_default();
                    let new_text = extract_string(record.get("newText")).unwrap_or_default();
                    let old_text = extract_string(record.get("oldText")).unwrap_or_default();
                    let verb = if old_text.trim().is_empty() {
                        "Wrote"
                    } else {
                        "Edited"
                    };
                    let lines = new_text.lines().count();
                    parts.push(format!("{verb} {path} ({lines} lines)"));
                }
                _ => {}
            }
        }
    }
    if !parts.is_empty() {
        return parts.join("\n\n");
    }
    let Some(raw_output) = update.get("rawOutput").filter(|value| !value.is_null()) else {
        return String::new();
    };
    as_record(Some(raw_output))
        .and_then(|record| {
            record
                .values()
                .filter_map(|value| as_record(Some(value)))
                .find_map(|inner| extract_string(inner.get("tool_output_for_prompt")))
        })
        .unwrap_or_else(|| tool_result_output(Some(raw_output)))
}

/// The `params.update` payload of one `session/update` line.
fn grok_session_update(record: &Map<String, Value>) -> Option<&Map<String, Value>> {
    as_record(as_record(record.get("params"))?.get("update"))
}

pub fn decode_grok_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let update = grok_session_update(&record)?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let transcript_message = |role, blocks| SessionChatMessage {
        id: fallback_id.to_string(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
    };

    match update.get("sessionUpdate").and_then(Value::as_str)? {
        "user_message_chunk" => {
            // Ghostex writes a pasted image into the prompt as its absolute
            // path, so the same split the persisted rows needed still applies.
            let blocks = normalize_grok_user_query_block(text_block(grok_update_text(
                update.get("content"),
            )?));
            (!blocks.is_empty()).then(|| transcript_message(SessionChatRole::User, blocks))
        }
        "agent_thought_chunk" => Some(transcript_message(
            SessionChatRole::Reasoning,
            vec![text_block(grok_update_text(update.get("content"))?)],
        )),
        "agent_message_chunk" => Some(transcript_message(
            SessionChatRole::Assistant,
            vec![text_block(grok_update_text(update.get("content"))?)],
        )),
        "tool_call" => Some(transcript_message(
            SessionChatRole::Assistant,
            vec![SessionChatBlock::ToolCall {
                name: grok_tool_name(update),
                input: update
                    .get("rawInput")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .unwrap_or(Value::Null),
            }],
        )),
        /*
        Only the SETTLED update is a result row. The others re-describe a call
        that is still running (a resolved title, a partial diff); publishing
        them would put a second, contentless row under every tool call.
        */
        "tool_call_update" => {
            let is_error = match update.get("status").and_then(Value::as_str)? {
                "completed" => None,
                "failed" => Some(true),
                _ => return None,
            };
            Some(transcript_message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: grok_tool_result_output(update),
                    is_error,
                }],
            ))
        }
        _ => None,
    }
}

/*
Grok's turn boundaries, which the persisted history never exposed: a prompt
opens a turn and `turn_completed` closes it, with `stop_reason` naming a
cancellation. Chat settles its Working marker on this, and the prompt queue
uses it to tell a real stop from the pause between two tool calls.
*/
pub fn decode_grok_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let update = grok_session_update(&record)?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let state = match update.get("sessionUpdate").and_then(Value::as_str)? {
        "user_message_chunk" => SessionChatTurnLifecycleState::Working,
        "turn_completed" => match update.get("stop_reason").and_then(Value::as_str) {
            Some("cancelled" | "canceled" | "interrupted" | "aborted") => {
                SessionChatTurnLifecycleState::Interrupted
            }
            _ => SessionChatTurnLifecycleState::Completed,
        },
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: extract_string(update.get("prompt_id")).unwrap_or_else(|| fallback_id.to_string()),
        timestamp,
    })
}

fn normalize_grok_user_query_block(block: SessionChatBlock) -> Vec<SessionChatBlock> {
    let SessionChatBlock::Text { text } = &block else {
        return vec![block];
    };
    let stripped = strip_grok_user_query_envelope(text);
    if stripped.trim().is_empty() {
        return Vec::new();
    }
    match split_grok_pasted_image_query(&stripped) {
        None => {
            if stripped == *text {
                vec![block]
            } else {
                vec![text_block(stripped)]
            }
        }
        Some((path, query)) => {
            let mut blocks = vec![SessionChatBlock::ImageRef {
                path: Some(path),
                url: None,
                alt: None,
            }];
            if !query.is_empty() {
                blocks.push(text_block(query));
            }
            blocks
        }
    }
}

fn strip_grok_user_query_envelope(text: &str) -> String {
    let opener = "<user_query>";
    let closer = "</user_query>";
    let lower = text.to_lowercase();
    let Some(start) = lower.find(opener) else {
        return text.to_string();
    };
    let body_start = start + opener.len();
    match lower[body_start..].find(closer) {
        None => text[body_start..].trim().to_string(),
        Some(relative_end) => text[body_start..body_start + relative_end]
            .trim()
            .to_string(),
    }
}

/*
Manual port of the upstream chat spec's pasted-image regex (no regex crate here):
^((win-drive|/|UNC)(.*?[\\/])?ghostex-paste-[^\\/\r\n]+?\.png)([\s\S]*)$ with
case-insensitive matching. The token must sit directly after a path separator
(every prefix alternative ends in one), the file name may not cross a
separator or newline, and the path portion before the token may not contain a
newline.
*/
fn split_grok_pasted_image_query(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    let valid_start = text.starts_with('/')
        || text.starts_with('\\')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'));
    if !valid_start {
        return None;
    }
    let lower = text.to_lowercase();
    let mut search_from = 0usize;
    while let Some(found) = lower[search_from..].find(GROK_PASTED_IMAGE_TOKEN) {
        let token_start = search_from + found;
        let preceded_by_separator =
            token_start > 0 && matches!(bytes[token_start - 1], b'/' | b'\\');
        if preceded_by_separator && !text[..token_start].contains(['\r', '\n']) {
            let name_start = token_start + GROK_PASTED_IMAGE_TOKEN.len();
            if let Some(png_relative) = lower[name_start..].find(".png") {
                let name_segment = &text[name_start..name_start + png_relative];
                if !name_segment.is_empty() && !name_segment.contains(['/', '\\', '\r', '\n']) {
                    let end = name_start + png_relative + ".png".len();
                    return Some((text[..end].to_string(), text[end..].trim().to_string()));
                }
            }
        }
        search_from = token_start + 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Pi-family decoder
// ---------------------------------------------------------------------------

