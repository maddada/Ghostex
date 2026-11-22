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
    pub(super) fn output_section(self) -> TranscriptExportSection {
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

/// How many of the left-out turns' messages from the user a limited handoff still lists, newest
/// kept, and how long each line may be.
pub(super) const EARLIER_PROMPTS_MAX: usize = 100;
pub(super) const EARLIER_PROMPT_MAX_CHARS: usize = 200;

pub(super) const PATCH_FAILURE_REASON_MAX_CHARS: usize = 160;
pub(super) const EXPORT_FILE_NAME_SLUG_MAX_CHARS: usize = 60;
pub(super) const EXPORT_SESSION_ID_PREFIX_CHARS: usize = 8;
pub(super) const UNIQUE_EXPORT_PATH_ATTEMPTS: usize = 200;

/// What the renderer emits. Kept as data rather than hardcoded in the renderer
/// so the future "configurable selection" endpoint only has to build one of
/// these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionTranscriptExportSelection {
    pub sections: Vec<TranscriptExportSection>,
    pub terminal_output_tail_lines: usize,
    /// Only the conversation's last turns, counted from the end; `None` is all of it. A turn is
    /// one of your messages and everything up to your next one (`render::RenderedParts`).
    pub last_turns: Option<usize>,
    /// When true, keep only the final non-empty `AgentMessage` of each turn (dropping mid-turn
    /// commentary while preserving every user prompt and closing answer).
    pub final_answer_only: bool,
    /// Number of leading turns already transferred in an earlier handover (used for delta
    /// handovers into an existing session).
    pub skip_turns: usize,
}

impl Default for SessionTranscriptExportSelection {
    fn default() -> Self {
        Self {
            sections: DEFAULT_EXPORT_SECTIONS.to_vec(),
            terminal_output_tail_lines: DEFAULT_TERMINAL_OUTPUT_TAIL_LINES,
            last_turns: None,
            final_answer_only: false,
            skip_turns: 0,
        }
    }
}

impl SessionTranscriptExportSelection {
    pub fn includes(&self, section: TranscriptExportSection) -> bool {
        self.sections.contains(&section)
    }

    /// Compact handover preset: session metadata, every user prompt, and only the closing answer
    /// of each agent turn.
    pub fn prompts_and_answers() -> Self {
        Self {
            sections: vec![
                TranscriptExportSection::SessionMeta,
                TranscriptExportSection::UserMessage,
                TranscriptExportSection::AgentMessage,
            ],
            terminal_output_tail_lines: DEFAULT_TERMINAL_OUTPUT_TAIL_LINES,
            last_turns: None,
            final_answer_only: true,
            skip_turns: 0,
        }
    }

    /// Builds the selection the export dialog's include-toggles describe. The
    /// conversation itself (session meta, user and agent messages) is never
    /// optional; the toggles only govern the three record families users
    /// actually ask to drop or add.
    pub fn from_include_toggles(
        include_commands: bool,
        include_patches: bool,
        include_reasoning: bool,
    ) -> Self {
        let mut sections = vec![
            TranscriptExportSection::SessionMeta,
            TranscriptExportSection::UserMessage,
            TranscriptExportSection::AgentMessage,
        ];
        if include_commands {
            sections.push(TranscriptExportSection::TerminalCmd);
            sections.push(TranscriptExportSection::TerminalOutput);
        }
        if include_patches {
            sections.push(TranscriptExportSection::Patch);
        }
        if include_reasoning {
            sections.push(TranscriptExportSection::AgentReasoning);
        }
        Self {
            sections,
            terminal_output_tail_lines: DEFAULT_TERMINAL_OUTPUT_TAIL_LINES,
            last_turns: None,
            final_answer_only: false,
            skip_turns: 0,
        }
    }

    /// The same selection, limited to the conversation's last `turns` (`None`: all of it).
    pub fn with_last_turns(mut self, turns: Option<usize>) -> Self {
        self.last_turns = turns.filter(|turns| *turns > 0);
        self
    }

    /// Keep only the turn-ending closing agent answer in each turn.
    pub fn with_final_answer_only(mut self, final_answer_only: bool) -> Self {
        self.final_answer_only = final_answer_only;
        self
    }

    /// Skip the first `skip_turns` turns (already handed off in an earlier delta handover).
    pub fn with_skip_turns(mut self, skip_turns: usize) -> Self {
        self.skip_turns = skip_turns;
        self
    }
}
