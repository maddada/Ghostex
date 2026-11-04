use super::*;

// ---------------------------------------------------------------------------
// Parsed model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PatchChangeKind {
    Added,
    Updated,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PatchFileChange {
    pub(super) path: String,
    pub(super) kind: PatchChangeKind,
    pub(super) added: usize,
    pub(super) removed: usize,
    /// New-side line range, only when the patch carries unified hunk headers.
    pub(super) start_line: Option<usize>,
    pub(super) end_line: Option<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct ExportEntry {
    pub(super) section: TranscriptExportSection,
    /// Message body, command text, tool output — whatever the section renders.
    pub(super) text: String,
    /// Tool name for call sections.
    pub(super) tool_name: Option<String>,
    /// Provider call id used to pair an output with its call.
    call_id: Option<String>,
    pub(super) patch: Vec<PatchFileChange>,
    pub(super) is_error: bool,
    /// Set on a Patch entry when its paired output reported a failure (plan Q4).
    pub(super) patch_failure: Option<String>,
}

impl ExportEntry {
    pub(super) fn new(section: TranscriptExportSection, text: impl Into<String>) -> Self {
        Self {
            section,
            text: text.into(),
            tool_name: None,
            call_id: None,
            patch: Vec::new(),
            is_error: false,
            patch_failure: None,
        }
    }

    pub(super) fn with_tool(mut self, name: impl Into<String>, call_id: Option<String>) -> Self {
        self.tool_name = Some(name.into());
        self.call_id = call_id;
        self
    }

    pub(super) fn with_patch(mut self, changes: Vec<PatchFileChange>) -> Self {
        self.patch = changes;
        self
    }

    fn with_error(mut self, is_error: bool) -> Self {
        self.is_error = is_error;
        self
    }
}

#[derive(Clone, Debug)]
pub(super) struct TranscriptMeta {
    pub(super) agent: SessionChatTranscriptAgent,
    pub(super) source_path: PathBuf,
    pub(super) agent_session_id: Option<String>,
    pub(super) title: Option<String>,
    pub(super) model: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) started_at: Option<String>,
}

impl TranscriptMeta {
    pub(super) fn new() -> Self {
        Self {
            agent: SessionChatTranscriptAgent::Claude,
            source_path: PathBuf::new(),
            agent_session_id: None,
            title: None,
            model: None,
            cwd: None,
            started_at: None,
        }
    }
}

pub(super) struct ParsedTranscript {
    pub(super) entries: Vec<ExportEntry>,
    pub(super) meta: TranscriptMeta,
}

/// Sequential parse state shared by all four agents: it holds the entry list,
/// the metadata being accumulated, and the call/output pairing bookkeeping.
pub(super) struct TranscriptBuilder {
    entries: Vec<ExportEntry>,
    pub(super) meta: TranscriptMeta,
    calls_by_id: HashMap<String, usize>,
    /// Calls still waiting for an output, oldest first — the order-based
    /// fallback for providers that omit call ids on one of the two records.
    pending_calls: Vec<usize>,
}

impl TranscriptBuilder {
    pub(super) fn new() -> Self {
        Self {
            entries: Vec::new(),
            meta: TranscriptMeta::new(),
            calls_by_id: HashMap::new(),
            pending_calls: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, entry: ExportEntry) {
        self.entries.push(entry);
    }

    /// Consecutive identical bodies in the same section are one turn recorded
    /// twice by two lanes of the same provider (Codex writes every visible
    /// message to both an event lane and a response lane).
    pub(super) fn push_dialog(&mut self, section: TranscriptExportSection, text: String) {
        if text.trim().is_empty() {
            return;
        }
        if let Some(last) = self.entries.last() {
            if last.section == section && last.text == text {
                return;
            }
        }
        self.push(ExportEntry::new(section, text));
    }

    pub(super) fn push_call(&mut self, mut entry: ExportEntry) {
        let index = self.entries.len();
        if let Some(call_id) = entry.call_id.clone() {
            self.calls_by_id.insert(call_id, index);
        }
        entry.tool_name.get_or_insert_with(|| "tool".to_string());
        self.entries.push(entry);
        self.pending_calls.push(index);
    }

    pub(super) fn push_output(&mut self, call_id: Option<String>, text: String, is_error: bool) {
        let matched = call_id
            .as_deref()
            .and_then(|call_id| self.calls_by_id.get(call_id).copied())
            .or_else(|| self.pending_calls.first().copied());
        let Some(call_index) = matched else {
            // An output whose call is not in this file (resumed transcripts
            // start mid-turn) is still real output; it inherits the generic
            // bucket rather than being dropped.
            self.push(
                ExportEntry::new(TranscriptExportSection::OtherToolOutput, text)
                    .with_error(is_error),
            );
            return;
        };
        self.pending_calls.retain(|index| *index != call_index);
        let call_section = self.entries[call_index].section;
        if call_section == TranscriptExportSection::Patch {
            self.apply_patch_output(call_index, &text, is_error);
        }
        let mut entry = ExportEntry::new(call_section.output_section(), text).with_error(is_error);
        entry.call_id = call_id;
        entry.tool_name = self.entries[call_index].tool_name.clone();
        self.push(entry);
    }

    /*
    A patch's own output is dropped from the export (the one-line patch summary
    already says what changed), with two exceptions that carry information the
    summary cannot: a failure, and the "file created" confirmation that is the
    only proof a whole-file write created rather than overwrote a file.
    */
    fn apply_patch_output(&mut self, call_index: usize, text: &str, is_error: bool) {
        if is_error || patch_output_failed(text) {
            self.entries[call_index].patch_failure = Some(short_failure_reason(text));
            return;
        }
        if text.contains("File created successfully") {
            for change in &mut self.entries[call_index].patch {
                if change.kind == PatchChangeKind::Updated && change.removed == 0 {
                    change.kind = PatchChangeKind::Added;
                }
            }
        }
    }

    pub(super) fn set_meta_title(&mut self, title: Option<String>) {
        if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
            self.meta.title = Some(title.trim().to_string());
        }
    }

    pub(super) fn set_meta_model(&mut self, model: Option<String>) {
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            self.meta.model = Some(model.trim().to_string());
        }
    }

    pub(super) fn set_meta_cwd(&mut self, cwd: Option<String>) {
        if self.meta.cwd.is_none() {
            if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
                self.meta.cwd = Some(cwd.trim().to_string());
            }
        }
    }

    pub(super) fn note_started_at(&mut self, timestamp: Option<String>) {
        if self.meta.started_at.is_none() {
            self.meta.started_at = timestamp.filter(|value| !value.trim().is_empty());
        }
    }

    fn finish(self) -> ParsedTranscript {
        ParsedTranscript {
            entries: self.entries,
            meta: self.meta,
        }
    }
}

pub(super) fn parse_transcript(
    agent: SessionChatTranscriptAgent,
    source_path: &Path,
    lines: &[String],
) -> ParsedTranscript {
    let mut builder = TranscriptBuilder::new();
    /*
    CDXC:SessionChat 2026-09-02:
    The export is read against the terminal, so it renders the same active
    branch chat does: a rewind leaves its abandoned turns in the file, and
    exporting them put turns in the document that the session no longer has.
    Claude only: every other agent's transcript is linear.
    */
    let off_branch = if agent == SessionChatTranscriptAgent::Claude {
        crate::session_chat_branch::claude_off_branch_line_indices(
            source_path,
            lines,
            crate::session_chat::decode_claude_transcript_line,
        )
    } else {
        std::collections::HashSet::new()
    };
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || off_branch.contains(&index) {
            continue;
        }
        let Some(record) = parse_json_line(line).and_then(|value| match value {
            Value::Object(record) => Some(record),
            _ => None,
        }) else {
            continue;
        };
        builder.note_started_at(text_field(&record, "timestamp"));
        match agent {
            SessionChatTranscriptAgent::Antigravity => {
                parse_antigravity_record(&mut builder, &record)
            }
            SessionChatTranscriptAgent::Claude => parse_claude_record(&mut builder, &record),
            SessionChatTranscriptAgent::Codex => parse_codex_record(&mut builder, &record),
            SessionChatTranscriptAgent::Copilot => parse_copilot_record(&mut builder, line),
            SessionChatTranscriptAgent::Cursor => parse_cursor_record(&mut builder, &record),
            SessionChatTranscriptAgent::Empryo => parse_empryo_record(&mut builder, line),
            SessionChatTranscriptAgent::Grok => parse_grok_record(&mut builder, &record),
            SessionChatTranscriptAgent::Hermes => parse_hermes_record(&mut builder, &record),
            SessionChatTranscriptAgent::Pi => parse_pi_record(&mut builder, &record),
            SessionChatTranscriptAgent::Zcode => parse_zcode_record(&mut builder, line),
            SessionChatTranscriptAgent::Freebuff => parse_freebuff_record(&mut builder, line),
            SessionChatTranscriptAgent::Dsh => parse_dsh_record(&mut builder, line),
            SessionChatTranscriptAgent::OpenCode => parse_opencode_record(&mut builder, line),
        }
    }
    builder.finish()
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

pub(super) fn record_of(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value?.as_object()
}

pub(super) fn text_field(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn bool_field(record: &Map<String, Value>, key: &str) -> bool {
    record.get(key) == Some(&Value::Bool(true))
}

pub(super) fn type_of(record: &Map<String, Value>) -> &str {
    record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Flattens the several shapes a provider uses for "the text of this result":
/// a string, a content array of `{text}` blocks, or a wrapper object.
pub(super) fn flatten_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| flatten_text(Some(item)))
                .filter(|part| !part.trim().is_empty())
                .collect();
            parts.join("\n")
        }
        Some(Value::Object(record)) => {
            for key in ["text", "content", "output", "message", "stdout"] {
                let nested = flatten_text(record.get(key));
                if !nested.trim().is_empty() {
                    return nested;
                }
            }
            String::new()
        }
        Some(other) => other.to_string(),
    }
}

/// Tool arguments arrive either as a JSON value or as a JSON string; both must
/// end up as a value before any field can be read out of them.
pub(super) fn as_arguments(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

pub(super) fn argument_text(arguments: &Value, keys: &[&str]) -> Option<String> {
    let record = arguments.as_object()?;
    for key in keys {
        if let Some(text) = record
            .get(*key)
            .map(|value| flatten_text(Some(value)))
            .filter(|text| !text.trim().is_empty())
        {
            return Some(text);
        }
    }
    None
}

pub(super) fn pretty_arguments(arguments: &Value) -> String {
    match arguments {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}
