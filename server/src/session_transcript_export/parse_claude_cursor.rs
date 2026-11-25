use super::*;

// ---------------------------------------------------------------------------
// Claude parser
// ---------------------------------------------------------------------------

pub(super) fn parse_claude_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    if bool_field(record, "isSidechain") {
        return;
    }
    if let Some(session_id) = text_field(record, "sessionId") {
        builder.meta.agent_session_id = Some(session_id);
    }
    builder.set_meta_cwd(text_field(record, "cwd"));

    match type_of(record) {
        "custom-title" => builder.set_meta_title(text_field(record, "customTitle")),
        "ai-title" => builder.set_meta_title(text_field(record, "title")),
        "summary" => {
            if let Some(summary) = text_field(record, "summary") {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::SessionEvent,
                    format!("Summary: {summary}"),
                ));
            }
        }
        "system" => {
            let text = flatten_text(record.get("content"));
            if !text.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::SystemMessage,
                    text,
                ));
            }
        }
        "file-history-snapshot" | "file-history-delta" => {
            builder.push(ExportEntry::new(
                TranscriptExportSection::GitSnapshot,
                "File history snapshot",
            ));
        }
        "attachment" => parse_claude_attachment(builder, record),
        "queue-operation" => {
            // Queue bookkeeping is transient state, not conversation: the
            // prompt itself is written again as a user or queued_command row.
        }
        "user" => parse_claude_user(builder, record),
        "assistant" => parse_claude_assistant(builder, record),
        _ => {}
    }
}

fn parse_claude_attachment(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let Some(attachment) = record_of(record.get("attachment")) else {
        return;
    };
    if type_of(attachment) != "queued_command" {
        return;
    }
    // A prompt typed mid-turn is delivered as this attachment and exists
    // nowhere else in the file (see session_chat.rs). Terminal mode writes a string;
    // Native (`stream-json`) writes an array of content blocks.
    if let Some(prompt) = text_field(attachment, "prompt") {
        builder.push_dialog(TranscriptExportSection::UserMessage, prompt);
    } else if let Some(prompt_value) = attachment.get("prompt") {
        let mut spoken = String::new();
        for block in claude_blocks(prompt_value) {
            if matches!(type_of(&block), "text" | "input_text" | "") {
                append_paragraph(&mut spoken, text_field(&block, "text"));
            }
        }
        if !spoken.trim().is_empty() {
            builder.push_dialog(TranscriptExportSection::UserMessage, spoken);
        }
    }
}

fn parse_claude_user(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    if text_field(record, "interruptedMessageId").is_some() {
        builder.push(ExportEntry::new(
            TranscriptExportSection::SessionEvent,
            "Conversation interrupted",
        ));
        return;
    }
    let injected = bool_field(record, "isMeta")
        || bool_field(record, "isSynthetic")
        || bool_field(record, "isCompactSummary");
    let message = record_of(record.get("message"));
    let Some(content) = message.and_then(|message| message.get("content")) else {
        return;
    };
    // Harness-injected turns are the transcript's system channel, not the
    // user's voice, so they are classified apart from real prompts.
    let section = if injected {
        TranscriptExportSection::SystemMessage
    } else {
        TranscriptExportSection::UserMessage
    };
    let mut spoken = String::new();
    for block in claude_blocks(content) {
        match type_of(&block) {
            "text" | "input_text" => append_paragraph(&mut spoken, text_field(&block, "text")),
            "tool_result" => {
                flush_claude_user_dialog(builder, section, &mut spoken);
                builder.push_output(
                    text_field(&block, "tool_use_id"),
                    flatten_text(block.get("content")),
                    bool_field(&block, "is_error"),
                );
            }
            _ => {}
        }
    }
    flush_claude_user_dialog(builder, section, &mut spoken);
}

/// A user turn's own body has the last word on where it belongs: the record
/// flags say nothing about slash-command plumbing, which arrives as a perfectly
/// ordinary external user record.
fn flush_claude_user_dialog(
    builder: &mut TranscriptBuilder,
    section: TranscriptExportSection,
    spoken: &mut String,
) {
    let text = std::mem::take(spoken);
    if text.trim().is_empty() {
        return;
    }
    match harness_user_turn_section(&text) {
        Some(harness_section) => builder.push(ExportEntry::new(harness_section, text)),
        None => builder.push_dialog(section, text),
    }
}

/// Text blocks accumulate into one message, but a tool call or a reasoning
/// block ends it: emitting the text last would reorder the turn so the agent
/// appears to answer before it ran anything.
pub(super) fn flush_dialog(
    builder: &mut TranscriptBuilder,
    section: TranscriptExportSection,
    spoken: &mut String,
) {
    let text = std::mem::take(spoken);
    if !text.trim().is_empty() {
        builder.push_dialog(section, text);
    }
}

pub(super) fn append_paragraph(spoken: &mut String, text: Option<String>) {
    let Some(text) = text else {
        return;
    };
    if !spoken.is_empty() {
        spoken.push_str("\n\n");
    }
    spoken.push_str(&text);
}

fn parse_claude_assistant(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let Some(message) = record_of(record.get("message")) else {
        return;
    };
    builder.set_meta_model(text_field(message, "model"));
    let Some(content) = message.get("content") else {
        return;
    };
    let mut spoken = String::new();
    for block in claude_blocks(content) {
        match type_of(&block) {
            "text" | "output_text" => append_paragraph(&mut spoken, text_field(&block, "text")),
            "thinking" => {
                flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                if let Some(text) =
                    text_field(&block, "thinking").or_else(|| text_field(&block, "text"))
                {
                    builder.push(ExportEntry::new(
                        TranscriptExportSection::AgentReasoning,
                        text,
                    ));
                }
            }
            "tool_use" => {
                flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                parse_claude_tool_use(builder, &block);
            }
            _ => {}
        }
    }
    flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
}

fn claude_blocks(content: &Value) -> Vec<Map<String, Value>> {
    match content {
        Value::String(text) => {
            let mut block = Map::new();
            block.insert("type".to_string(), Value::String("text".to_string()));
            block.insert("text".to_string(), Value::String(text.clone()));
            vec![block]
        }
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_object().cloned())
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_claude_tool_use(builder: &mut TranscriptBuilder, block: &Map<String, Value>) {
    let name = text_field(block, "name").unwrap_or_else(|| "tool".to_string());
    let call_id = text_field(block, "id");
    let arguments = block.get("input").cloned().unwrap_or(Value::Null);
    let mut section = classify_tool(&name);
    let command = argument_text(&arguments, &["command", "cmd"]);
    if section == TranscriptExportSection::TerminalCmd
        && command.as_deref().is_some_and(contains_patch_envelope)
    {
        // `Bash` running an `apply_patch` heredoc is a patch, not a command.
        section = TranscriptExportSection::Patch;
    }
    match section {
        TranscriptExportSection::TerminalCmd => {
            let text = command.unwrap_or_else(|| pretty_arguments(&arguments));
            builder.push_call(
                ExportEntry::new(TranscriptExportSection::TerminalCmd, text)
                    .with_tool(name, call_id),
            );
        }
        TranscriptExportSection::Patch => {
            let changes = claude_patch_changes(&name, &arguments, command.as_deref());
            builder.push_call(
                ExportEntry::new(TranscriptExportSection::Patch, String::new())
                    .with_tool(name, call_id)
                    .with_patch(changes),
            );
        }
        other => {
            builder.push_call(
                ExportEntry::new(other, pretty_arguments(&arguments)).with_tool(name, call_id),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Cursor Agent parser
// ---------------------------------------------------------------------------

pub(super) fn parse_cursor_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    if type_of(record) == "turn_ended" {
        if text_field(record, "status").as_deref() != Some("success") {
            builder.push(ExportEntry::new(
                TranscriptExportSection::SessionEvent,
                text_field(record, "error")
                    .unwrap_or_else(|| "Conversation interrupted".to_string()),
            ));
        }
        return;
    }
    let Some(role) = text_field(record, "role") else {
        return;
    };
    let Some(message) = record_of(record.get("message")) else {
        return;
    };
    let Some(content) = message.get("content") else {
        return;
    };
    let mut spoken = String::new();
    for block in claude_blocks(content) {
        match type_of(&block) {
            "text" if role == "user" => {
                append_paragraph(
                    &mut spoken,
                    text_field(&block, "text").map(|text| strip_grok_user_query(&text)),
                );
            }
            "text" if role == "assistant" => {
                let visible = text_field(&block, "text")
                    .map(|text| {
                        text.lines()
                            .filter(|line| line.trim() != "[REDACTED]")
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .filter(|text| !text.trim().is_empty());
                append_paragraph(&mut spoken, visible);
            }
            "thinking" if role == "assistant" => {
                flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                if let Some(text) = text_field(&block, "text") {
                    builder.push(ExportEntry::new(
                        TranscriptExportSection::AgentReasoning,
                        text,
                    ));
                }
            }
            "tool_use" if role == "assistant" => {
                flush_dialog(builder, TranscriptExportSection::AgentMessage, &mut spoken);
                parse_claude_tool_use(builder, &block);
            }
            _ => {}
        }
    }
    let section = if role == "user" {
        TranscriptExportSection::UserMessage
    } else {
        TranscriptExportSection::AgentMessage
    };
    flush_dialog(builder, section, &mut spoken);
}

/// Claude has no diff format: `Edit`/`MultiEdit` carry the replaced strings and
/// `Write` the whole file, so changed-line counts come from those payloads.
pub(super) fn claude_patch_changes(
    name: &str,
    arguments: &Value,
    command: Option<&str>,
) -> Vec<PatchFileChange> {
    if let Some(command) = command.filter(|text| contains_patch_envelope(text)) {
        return parse_patch_envelope(command);
    }
    let Some(record) = arguments.as_object() else {
        return Vec::new();
    };
    let path = text_field(record, "file_path")
        .or_else(|| text_field(record, "notebook_path"))
        .or_else(|| text_field(record, "path"))
        .unwrap_or_default();
    if path.is_empty() {
        return Vec::new();
    }
    let mut change = new_patch_change(&path, PatchChangeKind::Updated);
    match name.to_ascii_lowercase().as_str() {
        "write" => {
            change.added = line_count(&flatten_text(record.get("content")));
        }
        "notebookedit" | "notebook_edit" => {
            change.added = line_count(&flatten_text(record.get("new_source")));
        }
        "multiedit" | "multi_edit" => {
            if let Some(Value::Array(edits)) = record.get("edits") {
                for edit in edits {
                    let Some(edit) = edit.as_object() else {
                        continue;
                    };
                    change.removed += line_count(&flatten_text(edit.get("old_string")));
                    change.added += line_count(&flatten_text(edit.get("new_string")));
                }
            }
        }
        _ => {
            change.removed = line_count(&flatten_text(record.get("old_string")));
            change.added = line_count(&flatten_text(record.get("new_string")));
        }
    }
    if change.removed == 0 && change.added > 0 && name.eq_ignore_ascii_case("edit") {
        change.kind = PatchChangeKind::Added;
    }
    vec![change]
}
