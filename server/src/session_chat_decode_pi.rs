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
                let name = extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string());
                let mut input = record.get("arguments").cloned().unwrap_or(Value::Null);
                if name == "edit" {
                    with_omp_hashline_edit_file(&mut input);
                }
                visible.push(SessionChatBlock::ToolCall {
                    name,
                    input,
                    call_id: extract_string(record.get("id")),
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

/// CDXC:SessionChat 2026-10-17 WHY:
/// omp 18.11's `edit` takes one hashline script (`{"i":…,"input":"[omp-flow.txt#77F9]\nPUT 1.=1:\n+hello world"}`) instead of Pi's `{path, edits}`, so the chat drew the call as raw JSON and its file card kept only the file's first write. The file the script names (its first `[path#HASH]` line) becomes the call's `path` and its `+` lines its new text, so the call joins the file card, whose diff then comes from the result's `details.diff`.
fn with_omp_hashline_edit_file(input: &mut Value) {
    let Some(arguments) = input.as_object_mut() else {
        return;
    };
    if arguments.contains_key("path") || arguments.contains_key("edits") {
        return;
    }
    let Some(script) = arguments.get("input").and_then(Value::as_str) else {
        return;
    };
    let mut lines = script.lines();
    let Some(path) = lines
        .next()
        .and_then(|line| line.trim().strip_prefix('['))
        .and_then(|line| line.strip_suffix(']'))
        .and_then(|line| line.rsplit_once('#'))
        .map(|(path, _)| path.trim().to_string())
        .filter(|path| !path.is_empty())
    else {
        return;
    };
    let added = lines
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| line.strip_prefix('+'))
        .collect::<Vec<_>>()
        .join("\n");
    arguments.insert("path".to_string(), Value::String(path));
    arguments.insert("newText".to_string(), Value::String(added));
}

/// The one row the line-local readers (the send watchdog) see: an assistant entry's reply, without
/// the thinking `decode_pi_transcript_entry` splits out ahead of it.
pub fn decode_pi_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    decode_pi_transcript_entry(line, fallback_id).pop()
}

/// Every chat row one Pi session entry holds, in display order.
///
/// CDXC:SessionChat 2026-10-05 WHY:
/// Pi writes a whole assistant response as ONE entry whose content mixes `thinking`, `text` and `toolCall` blocks, where Claude and Codex write reasoning as rows of its own. Decoding the entry to a single row dropped the thinking of every Pi response that also said or did something, so the reasoning becomes its own Reasoning row ahead of the reply. It is stamped one millisecond earlier because both rows share the entry's byte offset, and the chat orders a tie by id.
pub fn decode_pi_transcript_entry(line: &str, fallback_id: &str) -> Vec<SessionChatMessage> {
    let Some(record) = parse_json_object(line) else {
        return Vec::new();
    };
    let is_assistant = record.get("type").and_then(Value::as_str) == Some("message")
        && as_record(record.get("message"))
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
            == Some("assistant");
    let Some(message) = decode_pi_record(&record, fallback_id) else {
        return Vec::new();
    };
    if !is_assistant || message.role != SessionChatRole::Assistant {
        return vec![message];
    }
    let reasoning = pi_message_content(
        as_record(record.get("message")).and_then(|message| message.get("content")),
    )
    .1;
    if reasoning.is_empty() {
        return vec![message];
    }
    let thinking = SessionChatMessage {
        id: format!("{}:thinking", message.id),
        role: SessionChatRole::Reasoning,
        blocks: reasoning,
        timestamp: message.timestamp.map(|timestamp| timestamp - 1),
        ..message.clone()
    };
    vec![thinking, message]
}

/// CDXC:SessionChat 2026-10-17 WHY:
/// Pi writes no turn markers: a user row opens a turn and the assistant row whose `stopReason` is not `toolUse` closes it (`aborted` is an Esc). Without this the queue, delayed sends and the send recovery saw a Pi turn only through hooks and Pi's own status report, and both go quiet while a provider runs its tools inside one response (Pi's `cursor:local` provider records the whole run when it ends): a queued message went into a running 40-second command and the send recovery restarted Pi, losing the turn (agent flow tests, 2026-10-17).
pub fn decode_pi_turn_lifecycle(line: &str, fallback_id: &str) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let message = as_record(record.get("message"))?;
    let state = match message.get("role").and_then(Value::as_str)? {
        "user" => SessionChatTurnLifecycleState::Working,
        "assistant" => match message.get("stopReason").and_then(Value::as_str)? {
            "toolUse" => return None,
            "aborted" => SessionChatTurnLifecycleState::Interrupted,
            _ => SessionChatTurnLifecycleState::Completed,
        },
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: extract_string(record.get("id")).unwrap_or_else(|| fallback_id.to_string()),
        timestamp: parse_timestamp(record.get("timestamp")),
    })
}

/// The part of an OMP custom message's `details` the chat draws, for the types it draws as their
/// own card or row; `None` for every other type, which keeps its marker line. A job's structured
/// output and the routing fields stay behind: the chat reads none of them.
/// SEE-ALSO: packages/gx-chat-core/src/transcript/background_rows.rs (`async-result`),
/// packages/gx-chat-core/src/transcript/system_cards.rs (`advisor`, `irc:incoming`).
fn omp_drawn_details(custom_type: &str, details: &Value) -> Option<Value> {
    let pick = |value: &Value, keys: &[&str]| -> Value {
        Value::Object(
            keys.iter()
                .filter_map(|key| Some((key.to_string(), value.get(*key)?.clone())))
                .collect(),
        )
    };
    let list = |key: &str, keys: &[&str]| -> Value {
        Value::Array(
            details
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|item| pick(item, keys))
                .collect(),
        )
    };
    let mut drawn = Map::new();
    match custom_type {
        "advisor" => {
            drawn.insert("notes".into(), list("notes", &["note", "severity"]));
        }
        "async-result" => {
            drawn.insert(
                "jobs".into(),
                list("jobs", &["jobId", "status", "label", "durationMs"]),
            );
        }
        "irc:incoming" => return Some(pick(details, &["from", "message"])),
        _ => return None,
    }
    Some(Value::Object(drawn))
}

/// CDXC:SessionChat 2026-10-30 WHY: an OMP `custom_message` reaches the chat as a system row whose text is written for the model (an `<advisory>`, `<irc>` or `<system-notice>` wrapper with instructions), so the chat drew advisor notes, background job results and IRC messages as raw muted lines. OMP's own terminal draws them from `customType` and `details`, so the row carries those beside its unchanged text: a chat that knows the type draws the card or row, and an older chat reads the same text as before.
fn omp_custom_message(record: &Map<String, Value>) -> Option<SessionChatCustomMessage> {
    let custom_type = record
        .get("customType")
        .and_then(Value::as_str)
        .filter(|custom_type| !custom_type.is_empty())?
        .to_string();
    let details = record
        .get("details")
        .filter(|details| details.is_object())
        .and_then(|details| omp_drawn_details(&custom_type, details));
    Some(SessionChatCustomMessage {
        custom_type,
        details,
    })
}

fn decode_pi_record(record: &Map<String, Value>, fallback_id: &str) -> Option<SessionChatMessage> {
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
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
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
        return Some(SessionChatMessage {
            custom_message: omp_custom_message(record),
            ..transcript_message(SessionChatRole::System, blocks, record_timestamp)
        });
    }
    if record_type != "message" {
        return None;
    }

    let message = as_record(record.get("message"))?;
    let role = message.get("role").and_then(Value::as_str)?;
    // CDXC:SessionChat 2026-10-17 WHY: an assistant message's own `timestamp` is when Pi began streaming it, a few milliseconds after the prompt, while its entry is written when the response ends. The chat times a turn from the prompt to its final reply, so a turn that was one long response read "Worked 1s" (agent flow tests, 2026-10-17); the entry's time is when the reply landed.
    let message_timestamp = parse_timestamp(message.get("timestamp"));
    let timestamp = if role == "assistant" {
        record_timestamp.or(message_timestamp)
    } else {
        message_timestamp.or(record_timestamp)
    };
    let (mut blocks, reasoning) = pi_message_content(message.get("content"));
    match role {
        "user" => {
            if blocks.is_empty() {
                return None;
            }
            Some(transcript_message(SessionChatRole::User, blocks, timestamp))
        }
        "assistant" => {
            let error = extract_string(message.get("errorMessage"));
            // A provider error after part of the reply streamed keeps that part, so the error that
            // ended the turn follows it rather than vanishing.
            if !blocks.is_empty()
                && message.get("stopReason").and_then(Value::as_str) == Some("error")
            {
                blocks.extend(error.map(text_block));
            } else if blocks.is_empty() {
                if let Some(error) = error {
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
            // Pi runs one response's tool calls in parallel, so a result names its call.
            // OMP records its edit's diff as `details.diff` (session_chat_file_patch.rs).
            let details = as_record(message.get("details"));
            let patch = details
                .and_then(|details| extract_string(details.get("diffString")))
                .and_then(|diff| crate::session_chat_file_patch::unified_diff_patch(&diff))
                .or_else(|| {
                    details
                        .and_then(|details| extract_string(details.get("diff")))
                        .and_then(|diff| crate::session_chat_file_patch::omp_edit_patch(&diff))
                });
            let mut tool_blocks = vec![SessionChatBlock::ToolResult {
                output,
                is_error: if is_error { Some(true) } else { None },
                call_id: extract_string(message.get("toolCallId")),
                patch,
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
                    call_id: None,
                    patch: None,
                }],
                timestamp,
            ))
        }
        "custom" => {
            if message.get("display") == Some(&Value::Bool(false)) || blocks.is_empty() {
                return None;
            }
            Some(SessionChatMessage {
                custom_message: omp_custom_message(message),
                ..transcript_message(SessionChatRole::System, blocks, timestamp)
            })
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
