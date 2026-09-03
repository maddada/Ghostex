use std::fmt;
use std::path::PathBuf;

/*
CDXC:PromptSearch 2026-06-25-19:49:
Keep the shared model limited to the transcript blocks requested for v1: user prompts, agent thinking, simple tool-call lines without tool output, and final agent response text.

CDXC:PromptSearch 2026-06-25-21:54:
ghostex-history now remains a transcript-first browser while also allowing explicit Ctrl+R resume into the selected agent session.
Keep session identity and project cwd in the shared model because resume commands need the same agent/session/project tuple that Zehn uses.
*/

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Agent {
    Claude,
    Codex,
    Cursor,
    Grok,
    Pi,
}

impl Agent {
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Cursor => "cursor",
            Agent::Grok => "grok",
            Agent::Pi => "pi",
        }
    }

    pub fn from_filter(filter: &str) -> Option<Self> {
        match filter.to_ascii_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            "grok" => Some(Self::Grok),
            "pi" => Some(Self::Pi),
            _ => None,
        }
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub agent: Agent,
    pub id: String,
    pub title: String,
    pub project: String,
    pub path: PathBuf,
    pub created_at: Option<i64>,
    pub updated_at: i64,
    pub preview: String,
    pub blocks: Vec<TranscriptBlock>,
    pub transcript_loaded: bool,
}

impl Session {
    pub fn display_title(&self) -> &str {
        if self.title.trim().is_empty() {
            &self.preview
        } else {
            &self.title
        }
    }

    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.to_ascii_lowercase();
        self.agent.label().contains(&query)
            || self.id.to_ascii_lowercase().contains(&query)
            || self.title.to_ascii_lowercase().contains(&query)
            || self.project.to_ascii_lowercase().contains(&query)
            || self.preview.to_ascii_lowercase().contains(&query)
            || self
                .path
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains(&query)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TranscriptKind {
    Agent,
    Thinking,
    Tool,
    User,
}

impl TranscriptKind {
    pub fn label(self) -> &'static str {
        match self {
            TranscriptKind::Agent => "agent",
            TranscriptKind::Thinking => "thinking",
            TranscriptKind::Tool => "tool",
            TranscriptKind::User => "user",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptBlock {
    pub kind: TranscriptKind,
    pub text: String,
}

impl TranscriptBlock {
    pub fn new(kind: TranscriptKind, text: impl Into<String>, _ts: Option<i64>) -> Option<Self> {
        let text = text.into();
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(Self { kind, text })
    }
}
