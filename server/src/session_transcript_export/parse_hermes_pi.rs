use super::*;

// ---------------------------------------------------------------------------
// Hermes parser — reads the mirrored row records described in
// `session_chat_decode_hermes.rs` (role + content + OpenAI-style `toolCalls`).
// ---------------------------------------------------------------------------

pub(super) fn parse_zcode_record(builder: &mut TranscriptBuilder, line: &str) {
    let Some(message) = crate::session_chat::decode_zcode_transcript_line(line, "zcode") else {
        return;
    };
    parse_normalized_record(builder, message);
}

pub(super) fn parse_freebuff_record(builder: &mut TranscriptBuilder, line: &str) {
    if let Some(message) = crate::session_chat::decode_freebuff_transcript_line(line, "freebuff") {
        parse_normalized_record(builder, message);
    }
}

/// DeepSeek Harness mirror rows (`session_chat_decode_dsh.rs`).
pub(super) fn parse_dsh_record(builder: &mut TranscriptBuilder, line: &str) {
    if let Some(message) = crate::session_chat::decode_dsh_transcript_line(line, "dsh") {
        parse_normalized_record(builder, message);
    }
}

/// Copilot's `events.jsonl`, read through the chat's own decoder.
pub(super) fn parse_copilot_record(builder: &mut TranscriptBuilder, line: &str) {
    if let Some(message) = crate::session_chat::decode_copilot_transcript_line(line, "copilot") {
        parse_normalized_record(builder, message);
    }
}

/// Empryo mirror rows (`session_chat_decode_empryo.rs`), whose calls and results name their
/// own call ids because one row can carry a whole group of parallel calls.
pub(super) fn parse_empryo_record(builder: &mut TranscriptBuilder, line: &str) {
    use crate::session_chat::SessionChatBlock;
    let Some(mut rest) = crate::session_chat::decode_empryo_transcript_line(line, "empryo") else {
        return;
    };
    for block in std::mem::take(&mut rest.blocks) {
        match block {
            SessionChatBlock::ToolCall {
                name,
                input,
                call_id,
            } => {
                // `ast_edit` is Empryo's preferred TS/JS edit: a patch on `path` that adds `newCode`.
                let is_ast_edit = name == "ast_edit";
                let section = if is_ast_edit {
                    TranscriptExportSection::Patch
                } else {
                    classify_tool(&name)
                };
                let entry = match section {
                    TranscriptExportSection::TerminalCmd => ExportEntry::new(
                        section,
                        argument_text(&input, &["command"])
                            .unwrap_or_else(|| pretty_arguments(&input)),
                    ),
                    // The mirror hands edits over in Claude's `Edit` shape.
                    TranscriptExportSection::Patch => {
                        let mut changes = claude_patch_changes(&name, &input, None);
                        if is_ast_edit {
                            for change in &mut changes {
                                change.added = line_count(&flatten_text(input.get("newCode")));
                            }
                        }
                        ExportEntry::new(section, String::new()).with_patch(changes)
                    }
                    _ => ExportEntry::new(section, pretty_arguments(&input)),
                };
                builder.push_call(entry.with_tool(name, call_id));
            }
            SessionChatBlock::ToolResult {
                output,
                is_error,
                call_id,
                ..
            } => builder.push_output(call_id, output, is_error.unwrap_or(false)),
            other => rest.blocks.push(other),
        }
    }
    parse_normalized_record(builder, rest);
}

pub(super) fn parse_opencode_record(builder: &mut TranscriptBuilder, line: &str) {
    if let Some(message) = crate::session_chat_opencode::decode_line(line, "opencode") {
        parse_normalized_record(builder, message);
    }
}

fn parse_normalized_record(
    builder: &mut TranscriptBuilder,
    message: crate::session_chat::SessionChatMessage,
) {
    use crate::session_chat::{SessionChatBlock, SessionChatRole};
    for block in message.blocks {
        match block {
            SessionChatBlock::Text { text } => {
                let section = match message.role {
                    SessionChatRole::User => TranscriptExportSection::UserMessage,
                    SessionChatRole::Reasoning => TranscriptExportSection::AgentReasoning,
                    SessionChatRole::System => TranscriptExportSection::SystemMessage,
                    _ => TranscriptExportSection::AgentMessage,
                };
                builder.push_dialog(section, text);
            }
            SessionChatBlock::ToolCall { name, input, .. } => {
                let section = classify_tool(&name);
                builder.push_call(
                    ExportEntry::new(section, pretty_arguments(&input))
                        .with_tool(name, Some(message.id.clone())),
                );
            }
            SessionChatBlock::ToolResult {
                output, is_error, ..
            } => builder.push_output(
                Some(message.id.trim_end_matches(":result").to_string()),
                output,
                is_error.unwrap_or(false),
            ),
            SessionChatBlock::ImageRef { path, url, alt } => builder.push_dialog(
                TranscriptExportSection::UserMessage,
                format!(
                    "{}: {}",
                    alt.unwrap_or_else(|| "Image".into()),
                    path.or(url).unwrap_or_default()
                ),
            ),
        }
    }
}

pub(super) fn parse_hermes_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    match text_field(record, "role").as_deref() {
        Some("user") => {
            builder.push_dialog(
                TranscriptExportSection::UserMessage,
                flatten_text(record.get("content")),
            );
        }
        Some("assistant") => {
            let reasoning = flatten_text(
                record
                    .get("reasoning")
                    .filter(|value| !value.is_null())
                    .or_else(|| record.get("reasoningContent")),
            );
            if !reasoning.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::AgentReasoning,
                    reasoning,
                ));
            }
            builder.push_dialog(
                TranscriptExportSection::AgentMessage,
                flatten_text(record.get("content")),
            );
            if let Some(Value::Array(tool_calls)) = record.get("toolCalls") {
                for tool_call in tool_calls {
                    let Some(tool_call) = tool_call.as_object() else {
                        continue;
                    };
                    parse_hermes_tool_call(builder, tool_call);
                }
            }
        }
        Some("tool") => {
            let content = flatten_text(record.get("content"));
            let (output, is_error) = match parse_json_line(&content) {
                Some(Value::Object(result)) => {
                    let is_error = result
                        .get("error")
                        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
                        || result.get("success") == Some(&Value::Bool(false))
                        || result
                            .get("exit_code")
                            .and_then(Value::as_i64)
                            .is_some_and(|code| code != 0);
                    let output = match result.get("output").or_else(|| result.get("result")) {
                        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
                        _ => content.clone(),
                    };
                    (output, is_error)
                }
                _ => (content, false),
            };
            builder.push_output(text_field(record, "toolCallId"), output, is_error);
        }
        _ => {}
    }
}

/// Antigravity mirror rows (`session_chat_decode_antigravity.rs` documents
/// the shape): one row per rendered message, keyed by `part`.
pub(super) fn parse_antigravity_record(
    builder: &mut TranscriptBuilder,
    record: &Map<String, Value>,
) {
    builder.note_started_at(text_field(record, "createdAt"));
    match text_field(record, "part").as_deref() {
        Some("user") => {
            builder.push_dialog(
                TranscriptExportSection::UserMessage,
                flatten_text(record.get("text")),
            );
        }
        Some("reasoning") => {
            let reasoning = flatten_text(record.get("text"));
            if !reasoning.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::AgentReasoning,
                    reasoning,
                ));
            }
        }
        Some("assistant") => {
            let text = flatten_text(record.get("text"));
            if !text.trim().is_empty() {
                builder.push_dialog(TranscriptExportSection::AgentMessage, text);
            }
            if let Some(Value::Array(tool_calls)) = record.get("toolCalls") {
                for tool_call in tool_calls {
                    let Some(tool_call) = tool_call.as_object() else {
                        continue;
                    };
                    let name = text_field(tool_call, "name").unwrap_or_else(|| "tool".to_string());
                    let arguments = as_arguments(tool_call.get("args"));
                    let command = argument_text(&arguments, &["CommandLine", "command"]);
                    let section = classify_tool(&name);
                    match section {
                        TranscriptExportSection::TerminalCmd => builder.push_call(
                            ExportEntry::new(
                                TranscriptExportSection::TerminalCmd,
                                command.unwrap_or_else(|| pretty_arguments(&arguments)),
                            )
                            .with_tool(name, None),
                        ),
                        other => builder.push_call(
                            ExportEntry::new(other, pretty_arguments(&arguments))
                                .with_tool(name, None),
                        ),
                    }
                }
            }
        }
        Some("tool") => {
            let is_error = record.get("isError") == Some(&Value::Bool(true));
            builder.push_output(None, flatten_text(record.get("text")), is_error);
        }
        _ => {}
    }
}

fn parse_hermes_tool_call(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let function = record_of(record.get("function"));
    let name = function
        .and_then(|function| text_field(function, "name"))
        .or_else(|| text_field(record, "name"))
        .unwrap_or_else(|| "tool".to_string());
    let call_id = text_field(record, "call_id").or_else(|| text_field(record, "id"));
    // `function.arguments` is a JSON string; decode it so command extraction
    // and pretty-printing see structured input.
    let arguments = as_arguments(
        function
            .and_then(|function| function.get("arguments"))
            .filter(|value| !value.is_null()),
    );
    let command = argument_text(&arguments, &["command", "cmd", "script"]);
    let section = classify_tool(&name);
    match section {
        TranscriptExportSection::TerminalCmd => builder.push_call(
            ExportEntry::new(
                TranscriptExportSection::TerminalCmd,
                command.unwrap_or_else(|| pretty_arguments(&arguments)),
            )
            .with_tool(name, call_id),
        ),
        other => builder.push_call(
            ExportEntry::new(other, pretty_arguments(&arguments)).with_tool(name, call_id),
        ),
    }
}

// ---------------------------------------------------------------------------
// Pi-family parser
// ---------------------------------------------------------------------------

pub(super) fn parse_pi_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    match type_of(record) {
        "session" => {
            builder.meta.agent_session_id = text_field(record, "id");
            builder.set_meta_cwd(text_field(record, "cwd"));
            builder.set_meta_title(text_field(record, "title"));
            builder.push(ExportEntry::new(
                TranscriptExportSection::SessionMeta,
                "Session started",
            ));
        }
        "title" | "title_change" => builder.set_meta_title(text_field(record, "title")),
        // `/name` and `--name` write the session's display name as `session_info`.
        "session_info" => builder.set_meta_title(text_field(record, "name")),
        // Pi writes `provider` and `modelId`; OMP writes `model` already joined.
        "model_change" => builder.set_meta_model(
            match (
                text_field(record, "provider"),
                text_field(record, "modelId"),
            ) {
                (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
                _ => text_field(record, "model"),
            },
        ),
        "thinking_level_change" => {
            if let Some(level) = text_field(record, "thinkingLevel") {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::TurnContext,
                    format!("thinking={level}"),
                ));
            }
        }
        "compaction" | "branch_summary" => {
            if let Some(summary) = text_field(record, "summary") {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::SessionEvent,
                    summary,
                ));
            }
        }
        "custom" => {
            // `tool_execution_start` and friends duplicate the toolCall block
            // that the assistant message already carries.
        }
        "message" => parse_pi_message(builder, record),
        _ => {}
    }
}

fn parse_pi_message(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let Some(message) = record_of(record.get("message")) else {
        return;
    };
    builder.set_meta_model(text_field(message, "model"));
    match message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "user" => builder.push_dialog(
            TranscriptExportSection::UserMessage,
            pi_message_text(message.get("content")),
        ),
        "assistant" => {
            let mut spoken = String::new();
            for block in pi_content_blocks(message.get("content")) {
                match type_of(&block) {
                    "text" => append_paragraph(&mut spoken, text_field(&block, "text")),
                    "thinking" => {
                        flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                        if let Some(text) = text_field(&block, "thinking") {
                            builder.push(ExportEntry::new(
                                TranscriptExportSection::AgentReasoning,
                                text,
                            ));
                        }
                    }
                    "toolCall" => {
                        flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                        parse_pi_tool_call(builder, &block);
                    }
                    _ => {}
                }
            }
            flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
        }
        "toolResult" => builder.push_output(
            text_field(message, "toolCallId"),
            flatten_text(message.get("content")),
            bool_field(message, "isError"),
        ),
        /*
        `bashExecution` is a user-run shell command Pi records with its output
        inline — one record that is both the call and its result.
        */
        "bashExecution" => {
            let command = text_field(message, "command").unwrap_or_default();
            let output = text_field(message, "output").unwrap_or_default();
            if !command.is_empty() {
                builder.push_call(
                    ExportEntry::new(TranscriptExportSection::TerminalCmd, command)
                        .with_tool("bashExecution", None),
                );
            }
            if !output.is_empty() {
                builder.push_output(None, output, false);
            }
        }
        "custom" => {
            let spoken = pi_message_text(message.get("content"));
            if !spoken.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::SystemMessage,
                    spoken,
                ));
            }
        }
        "branchSummary" | "compactionSummary" => {
            if let Some(summary) = text_field(message, "summary") {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::SessionEvent,
                    summary,
                ));
            }
        }
        _ => {}
    }
}

/// Pi content is a block array, but plain strings appear too; they are lifted
/// into text blocks so one loop reads either shape.
fn pi_content_blocks(content: Option<&Value>) -> Vec<Map<String, Value>> {
    let items: Vec<&Value> = match content {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::String(text) => {
                let mut block = Map::new();
                block.insert("type".to_string(), Value::String("text".to_string()));
                block.insert("text".to_string(), Value::String(text.clone()));
                Some(block)
            }
            Value::Object(block) => Some(block.clone()),
            _ => None,
        })
        .collect()
}

/// Visible text of a Pi message, ignoring reasoning and tool blocks.
fn pi_message_text(content: Option<&Value>) -> String {
    let mut spoken = String::new();
    for block in pi_content_blocks(content) {
        if type_of(&block) == "text" {
            append_paragraph(&mut spoken, text_field(&block, "text"));
        }
    }
    spoken
}

fn parse_pi_tool_call(builder: &mut TranscriptBuilder, block: &Map<String, Value>) {
    let name = text_field(block, "name").unwrap_or_else(|| "tool".to_string());
    let call_id = text_field(block, "id");
    let arguments = as_arguments(block.get("arguments"));
    let command = argument_text(&arguments, &["cmd", "command", "chars"]);
    let mut section = classify_tool(&name);
    if section == TranscriptExportSection::TerminalCmd
        && command.as_deref().is_some_and(contains_patch_envelope)
    {
        section = TranscriptExportSection::Patch;
    }
    match section {
        TranscriptExportSection::TerminalCmd => builder.push_call(
            ExportEntry::new(
                TranscriptExportSection::TerminalCmd,
                command.unwrap_or_else(|| pretty_arguments(&arguments)),
            )
            .with_tool(name, call_id),
        ),
        TranscriptExportSection::Patch => {
            let changes = pi_patch_changes(&name, &arguments, command.as_deref());
            builder.push_call(
                ExportEntry::new(TranscriptExportSection::Patch, String::new())
                    .with_tool(name, call_id)
                    .with_patch(changes),
            );
        }
        other => builder.push_call(
            ExportEntry::new(other, pretty_arguments(&arguments)).with_tool(name, call_id),
        ),
    }
}

fn pi_patch_changes(name: &str, arguments: &Value, command: Option<&str>) -> Vec<PatchFileChange> {
    if let Some(command) = command.filter(|text| contains_patch_envelope(text)) {
        return parse_patch_envelope(command);
    }
    if let Some(patch) = argument_text(arguments, &["input", "patch", "diff"])
        .filter(|text| contains_patch_envelope(text))
    {
        return parse_patch_envelope(&patch);
    }
    let Some(record) = arguments.as_object() else {
        return Vec::new();
    };
    let path = text_field(record, "path")
        .or_else(|| text_field(record, "file_path"))
        .unwrap_or_default();
    if path.is_empty() {
        return Vec::new();
    }
    let mut change = new_patch_change(&path, PatchChangeKind::Updated);
    if name.eq_ignore_ascii_case("write") {
        change.added = line_count(&flatten_text(record.get("content")));
    } else if let Some(Value::Array(edits)) = record.get("edits") {
        for edit in edits {
            let Some(edit) = edit.as_object() else {
                continue;
            };
            change.removed += line_count(&flatten_text(
                edit.get("oldText").or_else(|| edit.get("old_string")),
            ));
            change.added += line_count(&flatten_text(
                edit.get("newText").or_else(|| edit.get("new_string")),
            ));
        }
    }
    vec![change]
}
