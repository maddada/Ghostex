/*
CDXC:SessionTranscriptExport 2026-08-20:
"Export transcript" turns a running agent session's conversation into one
markdown file so a NEW agent conversation can be seeded with it (plan
`plans/015-export-transcript.md`).

The parsers here read the provider's RAW transcript JSONL rather than the
normalized `SessionChatMessage` stream that powers chat view. That is
deliberate: chat view drops every record it cannot render (token counts, turn
context, git snapshots, developer prompts, session events), and it flattens a
tool call and its output into role-tagged blocks that no longer say WHICH tool
ran. Export classifies every record into the full section taxonomy below, so a
caller-supplied selection can later enable any of them without a second parser.

Only transcript RESOLUTION is shared with chat view
(`resolve_session_chat_transcript_path` + Claude successor adoption), because
that logic is load-bearing and already correct for all four agents.
*/

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::Local;
use serde_json::{Map, Value};

use crate::resume_lookup::{parse_json_line, read_lines_lossy};
use crate::session_chat::{
    find_claude_successor_transcript, last_substantive_transcript_timestamp_ms,
    resolve_session_chat_transcript_agent, resolve_session_chat_transcript_path,
    SessionChatSuccessorOutcome, SessionChatTranscriptAgent,
};

// ---------------------------------------------------------------------------
// Section taxonomy
// ---------------------------------------------------------------------------

/// Every kind of record an agent transcript can carry. The default selection
/// renders a subset (see `DEFAULT_EXPORT_SECTIONS`); the rest are parsed and
/// kept so a configurable selection costs nothing but a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TranscriptExportSection {
    UserMessage,
    AgentMessage,
    AgentReasoning,
    InternalReasoning,
    TerminalCmd,
    TerminalOutput,
    McpCall,
    McpOutput,
    Patch,
    PatchOutput,
    OtherTool,
    OtherToolOutput,
    WebSearch,
    TokenCount,
    TurnContext,
    TaskEvent,
    SystemMessage,
    GitSnapshot,
    SessionEvent,
    SessionMeta,
}

impl TranscriptExportSection {
    pub fn as_str(self) -> &'static str {
        match self {
            TranscriptExportSection::UserMessage => "user_message",
            TranscriptExportSection::AgentMessage => "agent_message",
            TranscriptExportSection::AgentReasoning => "agent_reasoning",
            TranscriptExportSection::InternalReasoning => "internal_reasoning",
            TranscriptExportSection::TerminalCmd => "terminal_cmd",
            TranscriptExportSection::TerminalOutput => "terminal_output",
            TranscriptExportSection::McpCall => "mcp_call",
            TranscriptExportSection::McpOutput => "mcp_output",
            TranscriptExportSection::Patch => "patch",
            TranscriptExportSection::PatchOutput => "patch_output",
            TranscriptExportSection::OtherTool => "other_tool",
            TranscriptExportSection::OtherToolOutput => "other_tool_output",
            TranscriptExportSection::WebSearch => "web_search",
            TranscriptExportSection::TokenCount => "token_count",
            TranscriptExportSection::TurnContext => "turn_context",
            TranscriptExportSection::TaskEvent => "task_event",
            TranscriptExportSection::SystemMessage => "system_message",
            TranscriptExportSection::GitSnapshot => "git_snapshot",
            TranscriptExportSection::SessionEvent => "session_event",
            TranscriptExportSection::SessionMeta => "session_meta",
        }
    }

    /// The output section a tool call's paired result belongs to. Outputs
    /// always inherit their call's category, so a `mcp__*` result never lands
    /// in Terminal Outputs.
    fn output_section(self) -> TranscriptExportSection {
        match self {
            TranscriptExportSection::TerminalCmd => TranscriptExportSection::TerminalOutput,
            TranscriptExportSection::McpCall => TranscriptExportSection::McpOutput,
            TranscriptExportSection::Patch => TranscriptExportSection::PatchOutput,
            _ => TranscriptExportSection::OtherToolOutput,
        }
    }
}

/// Fixed default selection (plan Q4/Q5): conversation, what the agent ran, and
/// what it changed. Reasoning, MCP traffic, other tools, token counts, turn
/// context, task/session events, system prompts and git snapshots are parsed
/// but not rendered.
pub const DEFAULT_EXPORT_SECTIONS: &[TranscriptExportSection] = &[
    TranscriptExportSection::SessionMeta,
    TranscriptExportSection::UserMessage,
    TranscriptExportSection::AgentMessage,
    TranscriptExportSection::TerminalCmd,
    TranscriptExportSection::TerminalOutput,
    TranscriptExportSection::Patch,
];

/// Terminal output keeps its LAST N lines with a trim marker above (plan Q5).
pub const DEFAULT_TERMINAL_OUTPUT_TAIL_LINES: usize = 8;

const PATCH_FAILURE_REASON_MAX_CHARS: usize = 160;
const EXPORT_FILE_NAME_SLUG_MAX_CHARS: usize = 60;
const EXPORT_SESSION_ID_PREFIX_CHARS: usize = 8;
const UNIQUE_EXPORT_PATH_ATTEMPTS: usize = 200;

/// What the renderer emits. Kept as data rather than hardcoded in the renderer
/// so the future "configurable selection" endpoint only has to build one of
/// these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTranscriptExportSelection {
    pub sections: Vec<TranscriptExportSection>,
    pub terminal_output_tail_lines: usize,
}

impl Default for SessionTranscriptExportSelection {
    fn default() -> Self {
        Self {
            sections: DEFAULT_EXPORT_SECTIONS.to_vec(),
            terminal_output_tail_lines: DEFAULT_TERMINAL_OUTPUT_TAIL_LINES,
        }
    }
}

impl SessionTranscriptExportSelection {
    pub fn includes(&self, section: TranscriptExportSection) -> bool {
        self.sections.contains(&section)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Everything the export engine needs. Field names mirror what a server
/// handler already has in hand: the agent name from
/// `session_chat_agent_for_session`, `agentSessionId`/`agentSessionPath` from
/// `runtimeSettings`, and the Ghostex session id/title from the session record.
#[derive(Clone, Copy, Debug)]
pub struct SessionTranscriptExportRequest<'a> {
    /// Provider name as stored on the session (`claude`, `openclaude`,
    /// `codex`, `grok`, `pi`, `omp`).
    pub agent: Option<&'a str>,
    pub agent_session_id: Option<&'a str>,
    pub agent_session_path: Option<&'a str>,
    /// Ghostex session id — only used to name the file.
    pub session_id: &'a str,
    pub session_title: Option<&'a str>,
    /// Exports directory (the caller owns the app-data lookup, so the engine
    /// stays independent of `AppState`).
    pub exports_dir: &'a Path,
    /// `None` uses `SessionTranscriptExportSelection::default()`.
    pub selection: Option<&'a SessionTranscriptExportSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTranscriptExportOutcome {
    /// Absolute path of the written markdown file.
    pub path: PathBuf,
    pub bytes: usize,
    /// Transcript the export was parsed from (successor-adopted for Claude).
    pub source_path: PathBuf,
    pub agent: SessionChatTranscriptAgent,
    /// Records that survived the selection, i.e. what the file actually shows.
    pub rendered_entries: usize,
    /// Records parsed from the transcript, whether rendered or not.
    pub parsed_entries: usize,
}

/// Distinct, non-degrading failures. There is deliberately no "export what we
/// could" path: a half-transcript handed to the next agent is worse than a
/// clear error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionTranscriptExportError {
    /// The session's agent has no transcript format we can parse.
    UnsupportedAgent {
        agent: Option<String>,
    },
    /// Neither `agentSessionId` nor a usable `agentSessionPath` was supplied.
    MissingSessionReference,
    /// The agent is supported but no transcript file exists for the session.
    TranscriptNotFound {
        agent: SessionChatTranscriptAgent,
        agent_session_id: Option<String>,
    },
    TranscriptUnreadable {
        path: PathBuf,
    },
    /// The transcript exists but holds nothing the export can render.
    EmptyTranscript {
        path: PathBuf,
    },
    ExportsDirectoryUnwritable {
        path: PathBuf,
    },
    WriteFailed {
        path: PathBuf,
    },
}

impl SessionTranscriptExportError {
    /// Matches the `DomainStateError` codes the RPC layer already speaks.
    pub fn code(&self) -> &'static str {
        match self {
            SessionTranscriptExportError::UnsupportedAgent { .. } => "unsupportedAgent",
            SessionTranscriptExportError::MissingSessionReference => "invalidParams",
            SessionTranscriptExportError::TranscriptNotFound { .. } => "transcriptNotFound",
            SessionTranscriptExportError::TranscriptUnreadable { .. } => "transcriptUnreadable",
            SessionTranscriptExportError::EmptyTranscript { .. } => "transcriptEmpty",
            SessionTranscriptExportError::ExportsDirectoryUnwritable { .. }
            | SessionTranscriptExportError::WriteFailed { .. } => "internalError",
        }
    }
}

impl std::fmt::Display for SessionTranscriptExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionTranscriptExportError::UnsupportedAgent { agent } => match agent {
                Some(agent) => write!(
                    formatter,
                    "Exporting a transcript is not supported for {agent} sessions."
                ),
                None => write!(
                    formatter,
                    "This session has no agent, so there is no transcript to export."
                ),
            },
            SessionTranscriptExportError::MissingSessionReference => write!(
                formatter,
                "This session has not reported an agent session id yet."
            ),
            SessionTranscriptExportError::TranscriptNotFound {
                agent,
                agent_session_id,
            } => match agent_session_id {
                Some(agent_session_id) => write!(
                    formatter,
                    "No {} transcript file was found for session {agent_session_id}.",
                    agent_display_name(*agent)
                ),
                None => write!(
                    formatter,
                    "No {} transcript file was found for this session.",
                    agent_display_name(*agent)
                ),
            },
            SessionTranscriptExportError::TranscriptUnreadable { path } => {
                write!(
                    formatter,
                    "Could not read the transcript at {}.",
                    path.display()
                )
            }
            SessionTranscriptExportError::EmptyTranscript { path } => write!(
                formatter,
                "The transcript at {} has no conversation to export yet.",
                path.display()
            ),
            SessionTranscriptExportError::ExportsDirectoryUnwritable { path } => write!(
                formatter,
                "Could not create the exports directory at {}.",
                path.display()
            ),
            SessionTranscriptExportError::WriteFailed { path } => {
                write!(
                    formatter,
                    "Could not write the export to {}.",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for SessionTranscriptExportError {}

fn agent_display_name(agent: SessionChatTranscriptAgent) -> &'static str {
    match agent {
        SessionChatTranscriptAgent::Claude => "Claude",
        SessionChatTranscriptAgent::Codex => "Codex",
        SessionChatTranscriptAgent::Grok => "Grok",
        SessionChatTranscriptAgent::Pi => "Pi",
    }
}

/// Parse the session's transcript and write the markdown export.
pub fn export_session_transcript(
    request: &SessionTranscriptExportRequest<'_>,
) -> Result<SessionTranscriptExportOutcome, SessionTranscriptExportError> {
    let agent = resolve_session_chat_transcript_agent(request.agent).ok_or_else(|| {
        SessionTranscriptExportError::UnsupportedAgent {
            agent: request
                .agent
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    })?;
    let agent_session_id = request
        .agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_session_path = request
        .agent_session_path
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if agent_session_id.is_none() && agent_session_path.is_none() {
        return Err(SessionTranscriptExportError::MissingSessionReference);
    }

    let source_path = resolve_export_transcript_path(agent, agent_session_id, agent_session_path)
        .ok_or_else(|| SessionTranscriptExportError::TranscriptNotFound {
        agent,
        agent_session_id: agent_session_id.map(str::to_string),
    })?;

    let lines = read_lines_lossy(&source_path).ok_or_else(|| {
        SessionTranscriptExportError::TranscriptUnreadable {
            path: source_path.clone(),
        }
    })?;
    let mut transcript = parse_transcript(agent, &lines);
    transcript.meta.agent = agent;
    transcript.meta.source_path = source_path.clone();
    if transcript.meta.agent_session_id.is_none() {
        transcript.meta.agent_session_id = agent_session_id.map(str::to_string);
    }

    let default_selection = SessionTranscriptExportSelection::default();
    let selection = request.selection.unwrap_or(&default_selection);
    let rendered_entries = transcript
        .entries
        .iter()
        .filter(|entry| selection.includes(entry.section))
        .count();
    /*
    A transcript that exists but has produced nothing the selection renders is
    reported as empty rather than exported: a file holding only the metadata
    header would look like a successful export while transferring no context at
    all.
    */
    let has_conversation = transcript.entries.iter().any(|entry| {
        entry.section != TranscriptExportSection::SessionMeta && selection.includes(entry.section)
    });
    if !has_conversation {
        return Err(SessionTranscriptExportError::EmptyTranscript { path: source_path });
    }

    let title = export_title(request.session_title, &transcript);
    let markdown = render_markdown(&title, &transcript, selection);

    let path = unique_export_path(request.exports_dir, &title, request.session_id)?;
    let bytes = markdown.len();
    fs::write(&path, markdown.as_bytes())
        .map_err(|_| SessionTranscriptExportError::WriteFailed { path: path.clone() })?;

    Ok(SessionTranscriptExportOutcome {
        path,
        bytes,
        source_path,
        agent,
        rendered_entries,
        parsed_entries: transcript.entries.len(),
    })
}

/*
Claude resumes and compactions continue a conversation in a NEW file that
carries the stale session id in its head records; chat view adopts the
successor, so export has to as well or a resumed session exports the abandoned
half. `owned_session_ids` is empty here on purpose: export is a one-shot read
with no follower registry to consult, and reading a transcript another live
session is bound to is harmless.
*/
fn resolve_export_transcript_path(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
    let path = resolve_session_chat_transcript_path(agent, agent_session_id, agent_session_path)?;
    if agent != SessionChatTranscriptAgent::Claude {
        return Some(path);
    }
    let Some(session_id) = agent_session_id else {
        return Some(path);
    };
    let last_substantive_ms = last_substantive_transcript_timestamp_ms(&path).unwrap_or_default();
    match find_claude_successor_transcript(session_id, &path, last_substantive_ms, &[]) {
        SessionChatSuccessorOutcome::Found(successor) => Some(successor.path),
        _ => Some(path),
    }
}

// ---------------------------------------------------------------------------
// Parsed model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatchChangeKind {
    Added,
    Updated,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PatchFileChange {
    path: String,
    kind: PatchChangeKind,
    added: usize,
    removed: usize,
    /// New-side line range, only when the patch carries unified hunk headers.
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[derive(Clone, Debug)]
struct ExportEntry {
    section: TranscriptExportSection,
    /// Message body, command text, tool output — whatever the section renders.
    text: String,
    /// Tool name for call sections.
    tool_name: Option<String>,
    /// Provider call id used to pair an output with its call.
    call_id: Option<String>,
    patch: Vec<PatchFileChange>,
    is_error: bool,
    /// Set on a Patch entry when its paired output reported a failure (plan Q4).
    patch_failure: Option<String>,
}

impl ExportEntry {
    fn new(section: TranscriptExportSection, text: impl Into<String>) -> Self {
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

    fn with_tool(mut self, name: impl Into<String>, call_id: Option<String>) -> Self {
        self.tool_name = Some(name.into());
        self.call_id = call_id;
        self
    }

    fn with_patch(mut self, changes: Vec<PatchFileChange>) -> Self {
        self.patch = changes;
        self
    }

    fn with_error(mut self, is_error: bool) -> Self {
        self.is_error = is_error;
        self
    }
}

#[derive(Clone, Debug)]
struct TranscriptMeta {
    agent: SessionChatTranscriptAgent,
    source_path: PathBuf,
    agent_session_id: Option<String>,
    title: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    started_at: Option<String>,
}

impl TranscriptMeta {
    fn new() -> Self {
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

struct ParsedTranscript {
    entries: Vec<ExportEntry>,
    meta: TranscriptMeta,
}

/// Sequential parse state shared by all four agents: it holds the entry list,
/// the metadata being accumulated, and the call/output pairing bookkeeping.
struct TranscriptBuilder {
    entries: Vec<ExportEntry>,
    meta: TranscriptMeta,
    calls_by_id: HashMap<String, usize>,
    /// Calls still waiting for an output, oldest first — the order-based
    /// fallback for providers that omit call ids on one of the two records.
    pending_calls: Vec<usize>,
}

impl TranscriptBuilder {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            meta: TranscriptMeta::new(),
            calls_by_id: HashMap::new(),
            pending_calls: Vec::new(),
        }
    }

    fn push(&mut self, entry: ExportEntry) {
        self.entries.push(entry);
    }

    /// Consecutive identical bodies in the same section are one turn recorded
    /// twice by two lanes of the same provider (Codex writes every visible
    /// message to both an event lane and a response lane).
    fn push_dialog(&mut self, section: TranscriptExportSection, text: String) {
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

    fn push_call(&mut self, mut entry: ExportEntry) {
        let index = self.entries.len();
        if let Some(call_id) = entry.call_id.clone() {
            self.calls_by_id.insert(call_id, index);
        }
        entry.tool_name.get_or_insert_with(|| "tool".to_string());
        self.entries.push(entry);
        self.pending_calls.push(index);
    }

    fn push_output(&mut self, call_id: Option<String>, text: String, is_error: bool) {
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

    fn set_meta_title(&mut self, title: Option<String>) {
        if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
            self.meta.title = Some(title.trim().to_string());
        }
    }

    fn set_meta_model(&mut self, model: Option<String>) {
        if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
            self.meta.model = Some(model.trim().to_string());
        }
    }

    fn set_meta_cwd(&mut self, cwd: Option<String>) {
        if self.meta.cwd.is_none() {
            if let Some(cwd) = cwd.filter(|value| !value.trim().is_empty()) {
                self.meta.cwd = Some(cwd.trim().to_string());
            }
        }
    }

    fn note_started_at(&mut self, timestamp: Option<String>) {
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

fn parse_transcript(agent: SessionChatTranscriptAgent, lines: &[String]) -> ParsedTranscript {
    let mut builder = TranscriptBuilder::new();
    for line in lines {
        if line.trim().is_empty() {
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
            SessionChatTranscriptAgent::Claude => parse_claude_record(&mut builder, &record),
            SessionChatTranscriptAgent::Codex => parse_codex_record(&mut builder, &record),
            SessionChatTranscriptAgent::Grok => parse_grok_record(&mut builder, &record),
            SessionChatTranscriptAgent::Pi => parse_pi_record(&mut builder, &record),
        }
    }
    builder.finish()
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

fn record_of(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value?.as_object()
}

fn text_field(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn bool_field(record: &Map<String, Value>, key: &str) -> bool {
    record.get(key) == Some(&Value::Bool(true))
}

fn type_of(record: &Map<String, Value>) -> &str {
    record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Flattens the several shapes a provider uses for "the text of this result":
/// a string, a content array of `{text}` blocks, or a wrapper object.
fn flatten_text(value: Option<&Value>) -> String {
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
fn as_arguments(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

fn argument_text(arguments: &Value, keys: &[&str]) -> Option<String> {
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

fn pretty_arguments(arguments: &Value) -> String {
    match arguments {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tool classification
// ---------------------------------------------------------------------------

const MCP_TOOL_PREFIX: &str = "mcp__";

const TERMINAL_TOOL_NAMES: &[&str] = &[
    "bash",
    "bashexecution",
    "bashoutput",
    "exec",
    "exec_command",
    "execute_command",
    "killshell",
    "kill_shell",
    "local_shell",
    "local_shell_call",
    "run_command",
    "run_terminal_cmd",
    "send_input",
    "shell",
    "shell_command",
    "terminal",
    "write_stdin",
];

const PATCH_TOOL_NAMES: &[&str] = &[
    "apply_patch",
    "create_file",
    "edit",
    "edit_file",
    "multiedit",
    "multi_edit",
    "notebookedit",
    "notebook_edit",
    "str_replace",
    "str_replace_based_edit_tool",
    "str_replace_editor",
    "update_file",
    "write",
    "write_file",
];

const WEB_SEARCH_TOOL_NAMES: &[&str] = &["browser_search", "web_search", "websearch"];

/// Name-only classification, shared by all four agents. Content-based
/// overrides (a shell command that is really an `apply_patch` heredoc) are
/// applied by the per-agent parsers on top of this.
fn classify_tool(name: &str) -> TranscriptExportSection {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.starts_with(MCP_TOOL_PREFIX) || normalized == "mcp" {
        return TranscriptExportSection::McpCall;
    }
    if TERMINAL_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::TerminalCmd;
    }
    if PATCH_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::Patch;
    }
    if WEB_SEARCH_TOOL_NAMES.contains(&normalized.as_str()) {
        return TranscriptExportSection::WebSearch;
    }
    TranscriptExportSection::OtherTool
}

const TOOL_USE_ERROR_OPEN_TAG: &str = "<tool_use_error>";
const TOOL_USE_ERROR_CLOSE_TAG: &str = "</tool_use_error>";

fn patch_output_failed(text: &str) -> bool {
    let normalized = text.trim().to_ascii_lowercase();
    normalized.starts_with(TOOL_USE_ERROR_OPEN_TAG)
        || normalized.starts_with("error")
        || normalized.contains("failed to apply")
        || normalized.contains("patch failed")
        || normalized.contains("could not apply")
        || normalized.contains("no such file")
}

/*
A failed patch renders as exactly ONE line (plan Q4), and a raw provider error
is neither one line nor readable: Claude wraps tool errors in
`<tool_use_error>…</tool_use_error>` and follows the sentence that names the
failure ("String to replace not found in file.") with the entire rejected
payload. The wrapper is stripped, only the first meaningful line is kept, and
the result is clamped — the next agent needs to know an edit did not land, not
to re-read the edit.
*/
fn short_failure_reason(text: &str) -> String {
    let unwrapped = strip_tool_use_error_wrapper(text);
    let reason = unwrapped
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(one_line)
        .unwrap_or_default();
    if reason.is_empty() {
        return "the agent reported an error".to_string();
    }
    if reason.chars().count() <= PATCH_FAILURE_REASON_MAX_CHARS {
        return reason;
    }
    let clipped: String = reason
        .chars()
        .take(PATCH_FAILURE_REASON_MAX_CHARS)
        .collect();
    format!("{clipped}…")
}

fn strip_tool_use_error_wrapper(text: &str) -> String {
    let trimmed = text.trim();
    let Some(body) = trimmed.strip_prefix(TOOL_USE_ERROR_OPEN_TAG) else {
        return trimmed.to_string();
    };
    body.strip_suffix(TOOL_USE_ERROR_CLOSE_TAG)
        .unwrap_or(body)
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Patch parsing
// ---------------------------------------------------------------------------

const PATCH_ENVELOPE_START: &str = "*** Begin Patch";
const PATCH_ENVELOPE_END: &str = "*** End Patch";

/*
The `*** Begin Patch` envelope reaches us in two spellings: with real newlines
(Codex `function_call` arguments, Pi `apply_patch` arguments) and with escaped
`\n`, because current Codex sends patches inside a freeform JavaScript `exec`
snippet where the whole envelope is one string literal.

Which spelling is in play is decided by the text as a whole, never per
character: a real-newline patch whose body happens to contain the two
characters `\` `n` (any source file that writes a newline escape does) must not
be split there, and the escaped form has no real newlines to be confused by.
*/
fn split_patch_lines(text: &str) -> Vec<String> {
    if text.contains('\n') {
        return text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.peek() {
                Some('n') => {
                    characters.next();
                    lines.push(std::mem::take(&mut current));
                }
                Some('\\') => {
                    characters.next();
                    current.push('\\');
                }
                _ => current.push('\\'),
            },
            other => current.push(other),
        }
    }
    lines.push(current);
    lines
}

fn contains_patch_envelope(text: &str) -> bool {
    text.contains(PATCH_ENVELOPE_START)
}

/// Reads an `*** Begin Patch` envelope into one summary per touched file.
fn parse_patch_envelope(text: &str) -> Vec<PatchFileChange> {
    let Some(start) = text.find(PATCH_ENVELOPE_START) else {
        return Vec::new();
    };
    let body = &text[start..];
    let body = match body.find(PATCH_ENVELOPE_END) {
        Some(end) => &body[..end],
        None => body,
    };
    let mut changes: Vec<PatchFileChange> = Vec::new();
    for line in split_patch_lines(body) {
        let trimmed = line.trim_end();
        if let Some(path) = trimmed.strip_prefix("*** Add File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Added));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Update File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Updated));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Delete File:") {
            changes.push(new_patch_change(path, PatchChangeKind::Deleted));
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("*** Move to:") {
            if let Some(change) = changes.last_mut() {
                change.path = format!("{} → {}", change.path, path.trim());
            }
            continue;
        }
        let Some(change) = changes.last_mut() else {
            continue;
        };
        if trimmed.starts_with("@@") {
            if let Some((first, last)) = unified_hunk_range(trimmed) {
                change.start_line = Some(change.start_line.map_or(first, |value| value.min(first)));
                change.end_line = Some(change.end_line.map_or(last, |value| value.max(last)));
            }
            continue;
        }
        if line.starts_with('+') {
            change.added += 1;
        } else if line.starts_with('-') {
            change.removed += 1;
        }
    }
    changes
}

fn new_patch_change(path: &str, kind: PatchChangeKind) -> PatchFileChange {
    PatchFileChange {
        path: path.trim().to_string(),
        kind,
        added: 0,
        removed: 0,
        start_line: None,
        end_line: None,
    }
}

/// `@@ -12,7 +40,9 @@` → the new-side range `40..=48`. Codex's own hunk
/// headers carry a context line instead of numbers, which is why the patch
/// summary falls back to `+added/-removed` counts.
fn unified_hunk_range(header: &str) -> Option<(usize, usize)> {
    let after_plus = header.split('+').nth(1)?;
    let numbers = after_plus
        .split_whitespace()
        .next()?
        .trim_end_matches("@@")
        .trim_end_matches(',');
    let mut parts = numbers.split(',');
    let start: usize = parts.next()?.parse().ok()?;
    let count: usize = parts.next().map_or(Some(1), |value| value.parse().ok())?;
    if count == 0 {
        return Some((start, start));
    }
    Some((start, start + count - 1))
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    text.lines().count()
}

// ---------------------------------------------------------------------------
// Harness plumbing (mirrors the chat-view noise filter)
// ---------------------------------------------------------------------------

/*
Claude writes slash commands, their output and every harness injection as
ordinary `user` records whose body is an XML envelope
(`<command-name>/model</command-name>`, `<local-command-stdout>…`). Those are UI
bookkeeping, not the user's voice, so they are filed under the event/system
buckets instead of being quoted verbatim as `👤 User` blocks.

The tag vocabulary is a hand-kept mirror of `KNOWN_HARNESS_TAG_NAMES` /
`HARNESS_INJECTED_TURN_PREFIXES` in `session_chat.rs`. It is copied rather than
called because that module's filter takes an already-normalized
`SessionChatMessage`, which is exactly the representation export does not build.
Only a LEADING tag counts, same as chat view: a real prompt that happens to
carry an appended `<system-reminder>` is still the user speaking.
*/
const HARNESS_COMMAND_TAG_NAMES: &[&str] = &[
    "bash-input",
    "bash-stderr",
    "bash-stdout",
    "command-args",
    "command-message",
    "command-name",
    "local-command-caveat",
    "local-command-stderr",
    "local-command-stdout",
];

const HARNESS_SYSTEM_TAG_NAMES: &[&str] = &[
    "agent-message",
    "cross-session-message",
    "fork-boilerplate",
    "mcp-polling-update",
    "mcp-resource-update",
    "system-reminder",
    "task-notification",
    "teammate-message",
    "user-memory-input",
    "user-prompt-submit-hook",
];

const HARNESS_INJECTED_TURN_PREFIXES: &[&str] = &[
    "<channel source=",
    "[request interrupted",
    "a message arrived from ",
    "another claude session sent a message",
    "no response requested.",
    "caveat: the messages below were generated by the user while running local commands",
    "this session is being continued from a previous conversation",
];

fn leading_tag_name(normalized: &str) -> Option<&str> {
    let rest = normalized.strip_prefix('<')?;
    let first = rest.chars().next()?;
    if !first.is_ascii_lowercase() {
        return None;
    }
    let end = rest
        .find(|character: char| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        })
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    match rest[end..].chars().next() {
        None => Some(&rest[..end]),
        Some(character) if character.is_whitespace() || character == '>' => Some(&rest[..end]),
        Some(_) => None,
    }
}

/// The section a user-role record belongs to when its body is harness plumbing.
/// `None` means the record really is the user speaking.
fn harness_user_turn_section(text: &str) -> Option<TranscriptExportSection> {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if let Some(tag) = leading_tag_name(&normalized) {
        if HARNESS_COMMAND_TAG_NAMES.contains(&tag) {
            return Some(TranscriptExportSection::SessionEvent);
        }
        if HARNESS_SYSTEM_TAG_NAMES.contains(&tag) {
            return Some(TranscriptExportSection::SystemMessage);
        }
    }
    HARNESS_INJECTED_TURN_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
        .then_some(TranscriptExportSection::SystemMessage)
}

// ---------------------------------------------------------------------------
// Claude parser
// ---------------------------------------------------------------------------

fn parse_claude_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
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
    // nowhere else in the file (see session_chat.rs).
    if let Some(prompt) = text_field(attachment, "prompt") {
        builder.push_dialog(TranscriptExportSection::UserMessage, prompt);
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
fn flush_dialog(
    builder: &mut TranscriptBuilder,
    section: TranscriptExportSection,
    spoken: &mut String,
) {
    let text = std::mem::take(spoken);
    if !text.trim().is_empty() {
        builder.push_dialog(section, text);
    }
}

fn append_paragraph(spoken: &mut String, text: Option<String>) {
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

/// Claude has no diff format: `Edit`/`MultiEdit` carry the replaced strings and
/// `Write` the whole file, so changed-line counts come from those payloads.
fn claude_patch_changes(
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

// ---------------------------------------------------------------------------
// Codex parser
// ---------------------------------------------------------------------------

fn parse_codex_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let Some(payload) = record_of(record.get("payload")) else {
        return;
    };
    match type_of(record) {
        "session_meta" => {
            builder.meta.agent_session_id = text_field(payload, "session_id")
                .or_else(|| text_field(payload, "id"))
                .or(builder.meta.agent_session_id.take());
            builder.set_meta_cwd(text_field(payload, "cwd"));
            builder.push(ExportEntry::new(
                TranscriptExportSection::SessionMeta,
                "Session started",
            ));
        }
        "turn_context" => {
            builder.set_meta_model(text_field(payload, "model"));
            builder.set_meta_cwd(text_field(payload, "cwd"));
            let model = text_field(payload, "model").unwrap_or_else(|| "?".to_string());
            let effort = text_field(payload, "effort").unwrap_or_else(|| "?".to_string());
            builder.push(ExportEntry::new(
                TranscriptExportSection::TurnContext,
                format!("model={model}, effort={effort}"),
            ));
        }
        "compacted" => builder.push(ExportEntry::new(
            TranscriptExportSection::SessionEvent,
            "Context compacted",
        )),
        "event_msg" => parse_codex_event(builder, payload),
        "response_item" => parse_codex_response_item(builder, payload),
        _ => {}
    }
}

fn parse_codex_event(builder: &mut TranscriptBuilder, payload: &Map<String, Value>) {
    match type_of(payload) {
        "user_message" => {
            builder.push_dialog(
                TranscriptExportSection::UserMessage,
                flatten_text(payload.get("message")),
            );
        }
        "agent_message" => {
            builder.push_dialog(
                TranscriptExportSection::AgentMessage,
                flatten_text(payload.get("message")),
            );
        }
        "agent_reasoning" => {
            let text = flatten_text(payload.get("text"));
            if !text.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::AgentReasoning,
                    text,
                ));
            }
        }
        "token_count" => builder.push(ExportEntry::new(
            TranscriptExportSection::TokenCount,
            codex_token_summary(payload),
        )),
        "task_started" | "task_complete" => builder.push(ExportEntry::new(
            TranscriptExportSection::TaskEvent,
            type_of(payload).replace('_', " "),
        )),
        "context_compacted" | "thread_rolled_back" | "turn_aborted" => {
            builder.push(ExportEntry::new(
                TranscriptExportSection::SessionEvent,
                type_of(payload).replace('_', " "),
            ));
        }
        /*
        Newer Codex builds stopped writing the `user_message`/`agent_message`
        event lane and emit `item_completed` items instead; the two lanes are
        mutually exclusive per file. Tool/reasoning items are skipped here
        because their `response_item` twins are the sole source for those.
        */
        "item_completed" => {
            let Some(item) = record_of(payload.get("item")) else {
                return;
            };
            let section = match type_of(item) {
                "UserMessage" => TranscriptExportSection::UserMessage,
                "AgentMessage" => TranscriptExportSection::AgentMessage,
                _ => return,
            };
            builder.push_dialog(section, flatten_text(item.get("content")));
        }
        _ => {}
    }
}

fn codex_token_summary(payload: &Map<String, Value>) -> String {
    let Some(info) = record_of(payload.get("info")) else {
        return "token count".to_string();
    };
    let usage = record_of(info.get("last_token_usage"))
        .or_else(|| record_of(info.get("total_token_usage")))
        .unwrap_or(info);
    let input = usage
        .get("input_tokens")
        .map(Value::to_string)
        .unwrap_or_else(|| "?".to_string());
    let output = usage
        .get("output_tokens")
        .map(Value::to_string)
        .unwrap_or_else(|| "?".to_string());
    format!("in={input}, out={output}")
}

fn parse_codex_response_item(builder: &mut TranscriptBuilder, payload: &Map<String, Value>) {
    let call_id = text_field(payload, "call_id").or_else(|| text_field(payload, "id"));
    match type_of(payload) {
        "reasoning" => {
            let text = flatten_text(payload.get("text"));
            let text = if text.trim().is_empty() {
                flatten_text(payload.get("summary"))
            } else {
                text
            };
            builder.push(ExportEntry::new(
                TranscriptExportSection::InternalReasoning,
                text,
            ));
        }
        "function_call" | "local_shell_call" | "custom_tool_call" => {
            parse_codex_tool_call(builder, payload, call_id);
        }
        "function_call_output" | "custom_tool_call_output" => {
            let output = payload.get("output");
            let is_error = record_of(output).is_some_and(|record| {
                record.get("success") == Some(&Value::Bool(false)) || bool_field(record, "is_error")
            });
            builder.push_output(
                text_field(payload, "call_id"),
                flatten_text(output),
                is_error,
            );
        }
        "web_search_call" => builder.push_call(
            ExportEntry::new(TranscriptExportSection::WebSearch, String::new())
                .with_tool("web_search", call_id),
        ),
        "tool_search_call" => builder.push_call(
            ExportEntry::new(TranscriptExportSection::OtherTool, String::new())
                .with_tool("tool_search", call_id),
        ),
        "tool_search_output" => {
            builder.push_output(call_id, flatten_text(payload.get("tools")), false);
        }
        "ghost_snapshot" => {
            let commit = record_of(payload.get("ghost_commit"))
                .and_then(|commit| text_field(commit, "id"))
                .unwrap_or_else(|| "?".to_string());
            builder.push(ExportEntry::new(
                TranscriptExportSection::GitSnapshot,
                format!("commit {commit}"),
            ));
        }
        /*
        `message` items are the duplicate twin of the event lane: every visible
        turn is written to both, and the response lane additionally carries the
        harness-injected envelopes (AGENTS.md, environment context, developer
        prompts). Only the developer/system role is taken from here; user and
        assistant text comes from the event lane so no turn is doubled.
        */
        "message" => {
            if matches!(
                payload.get("role").and_then(Value::as_str),
                Some("developer" | "system")
            ) {
                let text = flatten_text(payload.get("content"));
                if !text.trim().is_empty() {
                    builder.push(ExportEntry::new(
                        TranscriptExportSection::SystemMessage,
                        text,
                    ));
                }
            }
        }
        _ => {}
    }
}

fn parse_codex_tool_call(
    builder: &mut TranscriptBuilder,
    payload: &Map<String, Value>,
    call_id: Option<String>,
) {
    let name = text_field(payload, "name").unwrap_or_else(|| "tool".to_string());
    let raw_input = payload
        .get("input")
        .filter(|value| !value.is_null())
        .or_else(|| payload.get("arguments").filter(|value| !value.is_null()))
        .or_else(|| payload.get("action").filter(|value| !value.is_null()));
    let input_text = flatten_text(raw_input);
    let arguments = as_arguments(raw_input);

    /*
    Current Codex funnels every tool through one freeform `exec` call whose
    input is a JavaScript snippet (`await tools.exec_command({cmd:…})`,
    `tools.apply_patch({patch})`, …). Classifying on the call name alone would
    file every patch under Terminal Commands, so the snippet's own content
    decides.
    */
    let mut section = classify_tool(&name);
    if contains_patch_envelope(&input_text) || input_text.contains("tools.apply_patch(") {
        section = TranscriptExportSection::Patch;
    } else if input_text.contains("tools.exec_command(")
        || input_text.contains("tools.write_stdin(")
    {
        section = TranscriptExportSection::TerminalCmd;
    }

    match section {
        TranscriptExportSection::Patch => {
            builder.push_call(
                ExportEntry::new(TranscriptExportSection::Patch, String::new())
                    .with_tool(name, call_id)
                    .with_patch(parse_patch_envelope(&input_text)),
            );
        }
        TranscriptExportSection::TerminalCmd => {
            let command = codex_command_text(&arguments, &input_text);
            builder.push_call(
                ExportEntry::new(TranscriptExportSection::TerminalCmd, command)
                    .with_tool(name, call_id),
            );
        }
        other => {
            let text = if matches!(arguments, Value::Null) {
                input_text
            } else {
                pretty_arguments(&arguments)
            };
            builder.push_call(ExportEntry::new(other, text).with_tool(name, call_id));
        }
    }
}

/// Pulls the shell command out of the three shapes Codex uses: a JSON
/// `{command:[…]}` array, a JSON `{cmd:"…"}` object, and the `cmd:"…"` field of
/// a freeform JavaScript `exec` snippet.
fn codex_command_text(arguments: &Value, input_text: &str) -> String {
    if let Some(record) = arguments.as_object() {
        if let Some(Value::Array(command)) = record.get("command") {
            let parts: Vec<String> = command
                .iter()
                .map(|part| flatten_text(Some(part)))
                .filter(|part| !part.is_empty())
                .collect();
            if !parts.is_empty() {
                return parts.join(" ");
            }
        }
        if let Some(command) = argument_text(arguments, &["cmd", "command", "chars"]) {
            return command;
        }
    }
    javascript_string_field(input_text, "cmd")
        .or_else(|| javascript_string_field(input_text, "chars"))
        .unwrap_or_else(|| input_text.trim().to_string())
}

/// Reads `<field>:"…"` out of a JavaScript snippet, honoring backslash escapes
/// so an embedded quote does not end the value early.
fn javascript_string_field(snippet: &str, field: &str) -> Option<String> {
    let needle = format!("{field}:");
    let start = snippet.find(&needle)? + needle.len();
    let rest = snippet[start..].trim_start();
    let mut characters = rest.chars();
    if characters.next()? != '"' {
        return None;
    }
    let mut value = String::new();
    while let Some(character) = characters.next() {
        match character {
            '"' => return Some(unescape_javascript(&value)),
            '\\' => {
                let Some(escaped) = characters.next() else {
                    break;
                };
                value.push('\\');
                value.push(escaped);
            }
            other => value.push(other),
        }
    }
    None
}

fn unescape_javascript(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => {}
            Some(other) => text.push(other),
            None => break,
        }
    }
    text
}

// ---------------------------------------------------------------------------
// Grok parser
// ---------------------------------------------------------------------------

fn parse_grok_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    match type_of(record) {
        "user" => {
            // A synthetic reason marks a harness-injected turn, and the
            // `<user_info>` bootstrap block is the session's system context.
            let text = strip_grok_user_query(&flatten_text(record.get("content")));
            let section = if text_field(record, "synthetic_reason").is_some()
                || text
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("<user_info>")
            {
                TranscriptExportSection::SystemMessage
            } else {
                TranscriptExportSection::UserMessage
            };
            builder.push_dialog(section, text);
        }
        "assistant" => {
            builder.set_meta_model(text_field(record, "model"));
            builder.push_dialog(
                TranscriptExportSection::AgentMessage,
                flatten_text(record.get("content")),
            );
            if let Some(Value::Array(tool_calls)) = record.get("tool_calls") {
                for tool_call in tool_calls {
                    let Some(tool_call) = tool_call.as_object() else {
                        continue;
                    };
                    parse_grok_tool_call(builder, tool_call);
                }
            }
        }
        "reasoning" => {
            let text = flatten_text(record.get("text"));
            let text = if text.trim().is_empty() {
                flatten_text(record.get("summary"))
            } else {
                text
            };
            if !text.trim().is_empty() {
                builder.push(ExportEntry::new(
                    TranscriptExportSection::AgentReasoning,
                    text,
                ));
            }
        }
        "backend_tool_call" | "tool_call" => parse_grok_tool_call(builder, record),
        "tool_result" => {
            let output = record
                .get("content")
                .filter(|value| !value.is_null())
                .or_else(|| record.get("output").filter(|value| !value.is_null()))
                .or_else(|| record.get("result").filter(|value| !value.is_null()));
            builder.push_output(
                text_field(record, "tool_call_id").or_else(|| text_field(record, "id")),
                flatten_text(output),
                bool_field(record, "is_error") || bool_field(record, "isError"),
            );
        }
        "session" | "session_start" => {
            builder.set_meta_cwd(text_field(record, "cwd"));
            builder.set_meta_title(text_field(record, "title"));
            builder.push(ExportEntry::new(
                TranscriptExportSection::SessionMeta,
                "Session started",
            ));
        }
        _ => {}
    }
}

fn parse_grok_tool_call(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
    let name = record_of(record.get("kind"))
        .and_then(|kind| text_field(kind, "tool_type"))
        .or_else(|| text_field(record, "name"))
        .or_else(|| text_field(record, "tool"))
        .unwrap_or_else(|| "tool".to_string());
    let call_id = text_field(record, "tool_call_id").or_else(|| text_field(record, "id"));
    let arguments = as_arguments(
        record
            .get("arguments")
            .filter(|value| !value.is_null())
            .or_else(|| record.get("input").filter(|value| !value.is_null()))
            .or_else(|| record.get("args").filter(|value| !value.is_null()))
            .or_else(|| record.get("kind").filter(|value| !value.is_null())),
    );
    let command = argument_text(&arguments, &["command", "cmd", "script"]);
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
            let changes = grok_patch_changes(&arguments, command.as_deref());
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

fn grok_patch_changes(arguments: &Value, command: Option<&str>) -> Vec<PatchFileChange> {
    if let Some(command) = command.filter(|text| contains_patch_envelope(text)) {
        return parse_patch_envelope(command);
    }
    let patch = argument_text(arguments, &["patch", "input", "diff"]);
    if let Some(patch) = patch.filter(|text| contains_patch_envelope(text)) {
        return parse_patch_envelope(&patch);
    }
    let Some(record) = arguments.as_object() else {
        return Vec::new();
    };
    let path = text_field(record, "path")
        .or_else(|| text_field(record, "file_path"))
        .or_else(|| text_field(record, "filename"))
        .unwrap_or_default();
    if path.is_empty() {
        return Vec::new();
    }
    let mut change = new_patch_change(&path, PatchChangeKind::Updated);
    change.removed = line_count(&flatten_text(
        record.get("old_str").or_else(|| record.get("old_string")),
    ));
    change.added = line_count(&flatten_text(
        record
            .get("new_str")
            .or_else(|| record.get("new_string"))
            .or_else(|| record.get("content")),
    ));
    if change.removed == 0 && change.added > 0 {
        change.kind = PatchChangeKind::Added;
    }
    vec![change]
}

fn strip_grok_user_query(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let Some(start) = lower.find("<user_query>") else {
        return text.to_string();
    };
    let body_start = start + "<user_query>".len();
    match lower[body_start..].find("</user_query>") {
        Some(end) => text[body_start..body_start + end].trim().to_string(),
        None => text[body_start..].trim().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pi-family parser
// ---------------------------------------------------------------------------

fn parse_pi_record(builder: &mut TranscriptBuilder, record: &Map<String, Value>) {
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
        "model_change" => builder.set_meta_model(text_field(record, "model")),
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

// ---------------------------------------------------------------------------
// Markdown rendering
// ---------------------------------------------------------------------------

const UNTITLED_EXPORT_TITLE: &str = "Untitled session";
const TITLE_MAX_CHARS: usize = 80;

fn export_title(session_title: Option<&str>, transcript: &ParsedTranscript) -> String {
    if let Some(title) = session_title
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return clamp_title(title);
    }
    if let Some(title) = transcript.meta.title.as_deref() {
        return clamp_title(title);
    }
    let first_prompt = transcript
        .entries
        .iter()
        .find(|entry| entry.section == TranscriptExportSection::UserMessage)
        .map(|entry| entry.text.as_str())
        .unwrap_or_default();
    let first_line = first_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first_line.is_empty() {
        return UNTITLED_EXPORT_TITLE.to_string();
    }
    clamp_title(first_line)
}

fn clamp_title(title: &str) -> String {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= TITLE_MAX_CHARS {
        return normalized;
    }
    let clipped: String = normalized.chars().take(TITLE_MAX_CHARS).collect();
    format!("{clipped}…")
}

/*
Message bodies are emitted verbatim, headings and all, so agent prose that
contains its own `##` sits at the same outline level as the structural
`## 👤 User` / `## 🤖 Agent` headings. That is deliberate: demoting either side
means rewriting message text, and a leading `#` inside a fenced block is a shell
comment, not a heading, so the rewrite would corrupt content to tidy an outline
no one navigates — this file is read by the next agent, not skimmed in a TOC.
The reference exporter (`codex-md.py`) makes the same call.
*/
fn render_markdown(
    title: &str,
    transcript: &ParsedTranscript,
    selection: &SessionTranscriptExportSelection,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    if selection.includes(TranscriptExportSection::SessionMeta) {
        out.push_str(&render_session_meta(title, &transcript.meta));
    }
    /*
    Patch lines are buffered instead of written per entry: a turn that edits one
    file seven times in a row produced seven near-identical `🔧 path` lines, and
    the next agent has to read past all of them to reach the next real step.
    Consecutive changes to the same file merge into one line with summed counts;
    the first entry that renders anything else — a failure note included — flushes
    the run, so a failure always stays attached to the patch that failed.
    */
    let mut patch_run: Vec<PatchFileChange> = Vec::new();
    for entry in &transcript.entries {
        if !selection.includes(entry.section) {
            continue;
        }
        if !matches!(
            entry.section,
            TranscriptExportSection::Patch | TranscriptExportSection::PatchOutput
        ) {
            flush_patch_run(&mut out, &mut patch_run, transcript.meta.cwd.as_deref());
        }
        match entry.section {
            TranscriptExportSection::SessionMeta => {}
            TranscriptExportSection::UserMessage => {
                out.push_str(&format!("## 👤 User\n\n{}\n\n", entry.text.trim_end()));
            }
            TranscriptExportSection::AgentMessage => {
                out.push_str(&format!("## 🤖 Agent\n\n{}\n\n", entry.text.trim_end()));
            }
            TranscriptExportSection::AgentReasoning => {
                out.push_str(&format!(
                    "> 🧠 **Reasoning:** {}\n\n",
                    one_line(&entry.text)
                ));
            }
            TranscriptExportSection::InternalReasoning => {
                out.push_str(&format!(
                    "> 🔒 **Internal reasoning:** {}\n\n",
                    one_line(&entry.text)
                ));
            }
            TranscriptExportSection::TerminalCmd => {
                out.push_str(&render_tool_call("💻", entry, "bash"));
            }
            TranscriptExportSection::McpCall => {
                out.push_str(&render_tool_call("🔌", entry, "json"));
            }
            TranscriptExportSection::OtherTool => {
                out.push_str(&render_tool_call("🧩", entry, "json"));
            }
            TranscriptExportSection::TerminalOutput
            | TranscriptExportSection::McpOutput
            | TranscriptExportSection::OtherToolOutput => {
                out.push_str(&render_output(entry, selection.terminal_output_tail_lines));
            }
            TranscriptExportSection::Patch => {
                if entry.patch.is_empty() || entry.patch_failure.is_some() {
                    // A failed (or file-less) patch prints on its own, so the
                    // run in front of it has to land first or the summaries
                    // would appear after the patch that follows them.
                    flush_patch_run(&mut out, &mut patch_run, transcript.meta.cwd.as_deref());
                    out.push_str(&render_patch(entry, transcript.meta.cwd.as_deref()));
                } else {
                    for change in &entry.patch {
                        merge_patch_change(&mut patch_run, change);
                    }
                }
            }
            TranscriptExportSection::PatchOutput => {
                // Patch results are folded into the patch summary (plan Q4).
            }
            TranscriptExportSection::WebSearch => out.push_str("> 🔍 **Web search**\n\n"),
            TranscriptExportSection::TokenCount => {
                out.push_str(&format!("> 📊 **Tokens:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::TurnContext => {
                out.push_str(&format!("> 🔄 **Turn:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::TaskEvent => {
                out.push_str(&format!("> 📌 **{}**\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::SystemMessage => {
                out.push_str(&format!(
                    "### ⚙️ System message\n\n{}\n\n",
                    fenced_block(&entry.text, "text")
                ));
            }
            TranscriptExportSection::GitSnapshot => {
                out.push_str(&format!("> 📸 **Snapshot:** {}\n\n", one_line(&entry.text)));
            }
            TranscriptExportSection::SessionEvent => {
                out.push_str(&format!("> 🔔 **{}**\n\n", one_line(&entry.text)));
            }
        }
    }
    flush_patch_run(&mut out, &mut patch_run, transcript.meta.cwd.as_deref());
    while out.ends_with("\n\n\n") {
        out.pop();
    }
    out
}

fn flush_patch_run(out: &mut String, run: &mut Vec<PatchFileChange>, cwd: Option<&str>) {
    for change in run.drain(..) {
        out.push_str(&format!("🔧 {}\n\n", patch_change_line(&change, cwd)));
    }
}

/// Folds a change into the open run when it touches the same file as the line
/// currently being built. Only the LAST line is a merge candidate: alternating
/// edits to two files stay in the order the agent made them.
fn merge_patch_change(run: &mut Vec<PatchFileChange>, change: &PatchFileChange) {
    let Some(open) = run.last_mut().filter(|open| open.path == change.path) else {
        run.push(change.clone());
        return;
    };
    open.added += change.added;
    open.removed += change.removed;
    open.start_line = min_line(open.start_line, change.start_line);
    open.end_line = max_line(open.end_line, change.end_line);
    open.kind = merged_patch_kind(open.kind, change.kind);
}

/// A file created and then edited is still a new file; a file whose last change
/// deletes it is deleted, whatever happened to it before.
fn merged_patch_kind(open: PatchChangeKind, next: PatchChangeKind) -> PatchChangeKind {
    match (open, next) {
        (_, PatchChangeKind::Deleted) => PatchChangeKind::Deleted,
        (PatchChangeKind::Added, _) | (_, PatchChangeKind::Added) => PatchChangeKind::Added,
        _ => PatchChangeKind::Updated,
    }
}

fn min_line(open: Option<usize>, next: Option<usize>) -> Option<usize> {
    match (open, next) {
        (Some(open), Some(next)) => Some(open.min(next)),
        (value, None) | (None, value) => value,
    }
}

fn max_line(open: Option<usize>, next: Option<usize>) -> Option<usize> {
    match (open, next) {
        (Some(open), Some(next)) => Some(open.max(next)),
        (value, None) | (None, value) => value,
    }
}

fn render_session_meta(title: &str, meta: &TranscriptMeta) -> String {
    let mut lines: Vec<String> = vec![
        format!("Title: {title}"),
        format!("Agent: {}", agent_display_name(meta.agent)),
    ];
    if let Some(model) = &meta.model {
        lines.push(format!("Model: {model}"));
    }
    if let Some(session_id) = &meta.agent_session_id {
        lines.push(format!("Session: {session_id}"));
    }
    if let Some(cwd) = &meta.cwd {
        lines.push(format!("CWD: {cwd}"));
    }
    if let Some(started_at) = &meta.started_at {
        lines.push(format!("Date: {started_at}"));
    }
    lines.push(format!("Source: {}", meta.source_path.display()));
    format!("```yaml\n{}\n```\n\n", lines.join("\n"))
}

fn render_tool_call(emoji: &str, entry: &ExportEntry, language: &str) -> String {
    let name = entry.tool_name.as_deref().unwrap_or("tool");
    if entry.text.trim().is_empty() {
        return format!("### {emoji} `{name}`\n\n");
    }
    format!(
        "### {emoji} `{name}`\n\n{}\n\n",
        fenced_block(entry.text.trim_end(), language)
    )
}

fn render_output(entry: &ExportEntry, tail_lines: usize) -> String {
    let text = tail_capped(entry.text.trim_end(), tail_lines);
    if text.trim().is_empty() {
        return String::new();
    }
    let label = if entry.is_error { "Error" } else { "Output" };
    format!("**{label}:**\n\n{}\n\n", fenced_block(&text, "text"))
}

/// Keeps the LAST `tail_lines` lines of a block, announcing what was dropped.
fn tail_capped(text: &str, tail_lines: usize) -> String {
    if tail_lines == 0 {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() <= tail_lines {
        return text.to_string();
    }
    let trimmed = lines.len() - tail_lines;
    let kept = lines[lines.len() - tail_lines..].join("\n");
    format!("... ({trimmed} lines trimmed) ...\n{kept}")
}

fn render_patch(entry: &ExportEntry, cwd: Option<&str>) -> String {
    let mut out = String::new();
    if entry.patch.is_empty() {
        let name = entry.tool_name.as_deref().unwrap_or("patch");
        out.push_str(&format!("🔧 `{name}` (no file changes recorded)\n\n"));
    }
    for change in &entry.patch {
        out.push_str(&format!("🔧 {}\n\n", patch_change_line(change, cwd)));
    }
    if let Some(failure) = &entry.patch_failure {
        out.push_str(&format!("⚠ patch failed: {failure}\n\n"));
    }
    out
}

/// Absolute paths inside the session's own working directory are shown
/// relative to it — the next agent runs there, and the full prefix repeated on
/// every patch line buries the part that matters.
fn project_relative_path(path: &str, cwd: Option<&str>) -> String {
    let Some(cwd) = cwd
        .map(|cwd| cwd.trim_end_matches('/'))
        .filter(|cwd| !cwd.is_empty())
    else {
        return path.to_string();
    };
    match path.strip_prefix(&format!("{cwd}/")) {
        Some(relative) if !relative.is_empty() => relative.to_string(),
        _ => path.to_string(),
    }
}

fn patch_change_line(change: &PatchFileChange, cwd: Option<&str>) -> String {
    let mut line = project_relative_path(&change.path, cwd);
    if let (Some(start), Some(end)) = (change.start_line, change.end_line) {
        if start == end {
            line.push_str(&format!(":{start}"));
        } else {
            line.push_str(&format!(":{start}-{end}"));
        }
    }
    match change.kind {
        PatchChangeKind::Added => line.push_str(" (new file)"),
        PatchChangeKind::Deleted => line.push_str(" (deleted)"),
        PatchChangeKind::Updated => {
            line.push_str(&format!(" (+{}/-{})", change.added, change.removed));
        }
    }
    line
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Picks a fence longer than any backtick run inside the body, so transcript
/// content that itself contains fenced code cannot break out of the block.
fn fenced_block(text: &str, language: &str) -> String {
    let mut longest_run = 0usize;
    let mut current_run = 0usize;
    for character in text.chars() {
        if character == '`' {
            current_run += 1;
            longest_run = longest_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}{language}\n{text}\n{fence}")
}

// ---------------------------------------------------------------------------
// Output file
// ---------------------------------------------------------------------------

/// `<exports dir>/<title-slug>-<sessionId8>-<yyyyMMdd-HHmmss>.md`, suffixed on
/// collision the same way saved chat attachments are.
fn unique_export_path(
    exports_dir: &Path,
    title: &str,
    session_id: &str,
) -> Result<PathBuf, SessionTranscriptExportError> {
    fs::create_dir_all(exports_dir).map_err(|_| {
        SessionTranscriptExportError::ExportsDirectoryUnwritable {
            path: exports_dir.to_path_buf(),
        }
    })?;
    let slug = export_slug(title);
    let session_prefix = export_session_prefix(session_id);
    let stamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let base = if session_prefix.is_empty() {
        format!("{slug}-{stamp}")
    } else {
        format!("{slug}-{session_prefix}-{stamp}")
    };
    let first = exports_dir.join(format!("{base}.md"));
    if !first.exists() {
        return Ok(first);
    }
    for index in 2..UNIQUE_EXPORT_PATH_ATTEMPTS {
        let candidate = exports_dir.join(format!("{base}-{index}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Ok(exports_dir.join(format!("{base}-{}.md", std::process::id())))
}

fn export_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        return "session".to_string();
    }
    trimmed
        .chars()
        .take(EXPORT_FILE_NAME_SLUG_MAX_CHARS)
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn export_session_prefix(session_id: &str) -> String {
    session_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(EXPORT_SESSION_ID_PREFIX_CHARS)
        .collect::<String>()
        .to_ascii_lowercase()
}
