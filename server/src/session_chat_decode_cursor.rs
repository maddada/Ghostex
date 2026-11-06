use serde_json::Value;

use crate::session_chat::*;

const CURSOR_REDACTED_REASONING: &str = "[REDACTED]";

fn strip_cursor_metadata_block<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let body = text.strip_prefix(open)?;
    let close_at = body.find(close)?;
    Some(&body[close_at + close.len()..])
}

fn is_cursor_user_query_envelope_prefix(mut prefix: &str) -> bool {
    let mut found_metadata = false;
    loop {
        prefix = prefix.trim_start();
        if let Some(rest) = prefix.strip_prefix("[Image]") {
            if rest.chars().next().is_none_or(char::is_whitespace) {
                prefix = rest;
                found_metadata = true;
                continue;
            }
        }
        if let Some(rest) = strip_cursor_metadata_block(prefix, "<image_files>", "</image_files>") {
            prefix = rest;
            found_metadata = true;
            continue;
        }
        if let Some(rest) = strip_cursor_metadata_block(prefix, "<timestamp>", "</timestamp>") {
            prefix = rest;
            found_metadata = true;
            continue;
        }
        return found_metadata && prefix.trim().is_empty();
    }
}

fn cursor_user_query(text: &str) -> String {
    let Some(open) = text.find("<user_query>") else {
        return text.to_string();
    };
    let Some(close) = text.rfind("</user_query>") else {
        return text.to_string();
    };
    let after_close = close + "</user_query>".len();
    if close < open + "<user_query>".len()
        || !is_cursor_user_query_envelope_prefix(&text[..open])
        || !text[after_close..].trim().is_empty()
    {
        return text.to_string();
    }
    text[open + "<user_query>".len()..close]
        .trim_matches(['\r', '\n'])
        .to_string()
}

fn cursor_visible_text(text: &str) -> Option<String> {
    let visible = text
        .lines()
        .filter(|line| line.trim() != CURSOR_REDACTED_REASONING)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (!visible.is_empty()).then_some(visible)
}

/// CDXC:SessionChat 2026-09-16 WHY:
/// Cursor appends a user-role tool catalog during compaction without a user_query envelope; rendering it leaked internal metadata into chat and counting it as a prompt shifted the mirror's reasoning alignment.
/// Match the complete metadata envelope so actual user queries quoting these tags remain visible.
pub(crate) fn is_cursor_context_metadata(record: &serde_json::Map<String, Value>) -> bool {
    if record.get("role").and_then(Value::as_str) != Some("user") {
        return false;
    }
    let Some(items) = record
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    if items.len() != 1 || items[0].get("type").and_then(Value::as_str) != Some("text") {
        return false;
    }
    let Some(mut text) = items[0].get("text").and_then(Value::as_str) else {
        return false;
    };
    for (open, close) in [
        ("<available_subagent_types>", "</available_subagent_types>"),
        (
            "<available_subagent_models>",
            "</available_subagent_models>",
        ),
        ("<dynamic_tools>", "</dynamic_tools>"),
    ] {
        let Some(rest) = strip_cursor_metadata_block(text.trim_start(), open, close) else {
            return false;
        };
        text = rest;
    }
    text.trim().is_empty()
}

/// Blocks plus whether every one of them came from a `thinking` block, which
/// is what turns the message into a reasoning turn. Thinking blocks are not in
/// Cursor's raw jsonl; the chat mirror splices them in from the session store
/// (`session_chat_cursor_mirror`).
fn cursor_message_blocks(role: &str, content: Option<&Value>) -> (Vec<SessionChatBlock>, bool) {
    let Some(items) = content.and_then(Value::as_array) else {
        return (Vec::new(), false);
    };
    let mut blocks = Vec::new();
    let mut thinking_blocks = 0usize;
    for item in items {
        let Some(record) = item.as_object() else {
            continue;
        };
        match record.get("type").and_then(Value::as_str) {
            Some("thinking") => {
                let Some(text) = extract_string(record.get("text")) else {
                    continue;
                };
                if !text.trim().is_empty() {
                    blocks.push(text_block(text));
                    thinking_blocks += 1;
                }
            }
            Some("text") => {
                let Some(text) = record.get("text").and_then(Value::as_str) else {
                    continue;
                };
                let text = if role == "user" {
                    Some(cursor_user_query(text))
                } else {
                    cursor_visible_text(text)
                };
                if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
                    blocks.push(text_block(text));
                }
            }
            Some("tool_use") => {
                let name = extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string());
                let input = record.get("input").cloned().unwrap_or(Value::Null);
                // CDXC:SessionChat 2026-10-07 WHY: Cursor 2026.10 records the tools it loads on demand (its to-do list among them) as `CallDynamicTool {namespace, toolName, arguments}`, which the chat showed as one opaque "CallDynamicTool" row; the call is shown as the tool it runs, so TodoWrite reads as Claude's does.
                let (name, input) =
                    match (name.as_str(), input.get("toolName").and_then(Value::as_str)) {
                        ("CallDynamicTool", Some(tool)) if !tool.trim().is_empty() => (
                            tool.to_string(),
                            input.get("arguments").cloned().unwrap_or(Value::Null),
                        ),
                        _ => (name, input),
                    };
                let plan = (name == "CreatePlan")
                    .then(|| cursor_plan_markdown(&input))
                    .flatten();
                let (name, input) = canonical_cursor_tool(name, input);
                blocks.push(SessionChatBlock::ToolCall {
                    name,
                    input,
                    // The chat mirror names each call from Cursor's store (`session_chat_cursor_mirror`).
                    call_id: extract_string(record.get("id")),
                });
                blocks.extend(plan.map(text_block));
            }
            // A result the chat mirror read from Cursor's store.
            Some("tool_result") => blocks.push(SessionChatBlock::ToolResult {
                output: extract_string(record.get("content")).unwrap_or_default(),
                is_error: record.get("is_error").and_then(Value::as_bool),
                call_id: extract_string(record.get("tool_use_id")),
                patch: record
                    .get("diff")
                    .and_then(Value::as_str)
                    .and_then(crate::session_chat_file_patch::unified_diff_patch),
            }),
            _ => {}
        }
    }
    let reasoning_only = thinking_blocks > 0 && thinking_blocks == blocks.len();
    (blocks, reasoning_only)
}

/// CDXC:SessionChat 2026-10-17 WHY: Cursor names its tools `Shell`, `StrReplace`, `Read {path}`, `Write {path, contents}` and `Glob {glob_pattern, target_directory}`, none of which the chat knows, so a command showed only its description, an edit expanded to raw JSON with no diff card and a search read `Glob: {"glob_pattern":…}` (agent flow tests, Windows, 2026-10-17). They are shown as the Claude tools they are, the way Antigravity's and Copilot's are; `Shell` must not stay `Shell`, which is the chat's own name for a command the user ran with `!`.
fn canonical_cursor_tool(name: String, input: Value) -> (String, Value) {
    let Value::Object(mut fields) = input else {
        return (name, input);
    };
    let mut rename = |from: &str, to: &str| {
        if let Some(value) = fields.remove(from) {
            fields.entry(to.to_string()).or_insert(value);
        }
    };
    let canonical = match name.as_str() {
        "Shell" => "Bash",
        "StrReplace" => {
            rename("path", "file_path");
            "Edit"
        }
        "Read" => {
            rename("path", "file_path");
            "Read"
        }
        "Write" => {
            rename("path", "file_path");
            rename("contents", "content");
            "Write"
        }
        "Glob" => {
            rename("glob_pattern", "pattern");
            rename("target_directory", "path");
            "Glob"
        }
        _ => return (name, Value::Object(fields)),
    };
    (canonical.to_string(), Value::Object(fields))
}

/// CDXC:SessionChat 2026-10-07 WHY: Cursor's Plan mode writes its plan only as the `plan` argument of a `CreatePlan` tool call, which the chat folded away with the other tool rows (its open row showed the argument as escaped JSON), while the terminal shows the plan above its "Ready to build?" panel. The plan follows the call as Markdown so it reads in the chat before the build decision; Cursor drops the leading `<!-- … -->` marker the same way.
fn cursor_plan_markdown(input: &Value) -> Option<String> {
    let plan = input.get("plan").and_then(Value::as_str)?.trim_start();
    let plan = match plan
        .strip_prefix("<!--")
        .and_then(|rest| rest.split_once("-->"))
    {
        Some((_, rest)) => rest.trim_start(),
        None => plan,
    };
    (!plan.trim().is_empty()).then(|| plan.trim_end().to_string())
}

/// CDXC:SessionChat 2026-10-07 WHY: choosing Build on Cursor's "Ready to build?" panel makes Cursor itself send a user query (the plan's title, then "Implement the plan as specified, it is attached for your reference. …"), which the chat drew as a message the user had typed. It is shown as a status row naming the plan instead.
fn cursor_plan_build_title(blocks: &[SessionChatBlock]) -> Option<String> {
    let [SessionChatBlock::Text { text }] = blocks else {
        return None;
    };
    let (title, rest) = text.trim().split_once("\n\n")?;
    let title = title.trim();
    (rest
        .trim_start()
        .starts_with("Implement the plan as specified, it is attached for your reference.")
        && !title.is_empty()
        && !title.contains('\n'))
    .then(|| title.to_string())
}

/// The time the chat mirror stamps on each line (`session_chat_cursor_mirror::merge_cursor_transcript`).
fn cursor_record_timestamp(record: &serde_json::Map<String, Value>) -> Option<i64> {
    record.get("ghostexTimestamp").and_then(Value::as_i64)
}

fn cursor_record_id(record: &serde_json::Map<String, Value>, fallback_id: &str) -> String {
    match (
        record.get("ghostexId").and_then(Value::as_str),
        fallback_id.rsplit_once(':'),
    ) {
        (Some(id), Some((path, _))) => format!("{path}:{id}"),
        _ => fallback_id.to_string(),
    }
}

pub fn decode_cursor_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    if is_cursor_context_metadata(&record) {
        return None;
    }
    if record.get("type").and_then(Value::as_str) == Some("turn_ended") {
        if record.get("status").and_then(Value::as_str) == Some("success") {
            return None;
        }
        let text = extract_string(record.get("error"))
            .unwrap_or_else(|| INTERRUPTED_STATUS_TEXT.to_string());
        return Some(SessionChatMessage {
            id: cursor_record_id(&record, fallback_id),
            role: SessionChatRole::System,
            blocks: vec![text_block(text)],
            timestamp: cursor_record_timestamp(&record),
            source: SessionChatSource::Transcript,
            turn_id: Some(cursor_record_id(&record, fallback_id)),
            byte_offset: None,
            async_questions: None,
            queued: false,
            api_error: None,
            custom_message: None,
        });
    }

    let role = record.get("role").and_then(Value::as_str)?;
    let message = as_record(record.get("message"))?;
    let (blocks, reasoning_only) = cursor_message_blocks(role, message.get("content"));
    if blocks.is_empty() {
        return None;
    }
    let (role, blocks) = match role {
        "user" => match cursor_plan_build_title(&blocks) {
            Some(title) => (
                SessionChatRole::System,
                vec![text_block(format!("Building the plan: {title}"))],
            ),
            None => (SessionChatRole::User, blocks),
        },
        "assistant" if reasoning_only => (SessionChatRole::Reasoning, blocks),
        "assistant" => (SessionChatRole::Assistant, blocks),
        "tool" => (SessionChatRole::Tool, blocks),
        _ => return None,
    };
    Some(SessionChatMessage {
        id: cursor_record_id(&record, fallback_id),
        role,
        blocks,
        timestamp: cursor_record_timestamp(&record),
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    })
}

pub fn decode_cursor_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) != Some("turn_ended") {
        return None;
    }
    let state = match record.get("status").and_then(Value::as_str) {
        Some("success") => SessionChatTurnLifecycleState::Completed,
        Some("error" | "aborted") => SessionChatTurnLifecycleState::Interrupted,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: cursor_record_id(&record, fallback_id),
        timestamp: cursor_record_timestamp(&record),
    })
}
