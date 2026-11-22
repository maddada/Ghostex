use serde::{Deserialize, Serialize};

use crate::agent::Agent;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub rate_percent: f64,
    #[serde(default)]
    pub cost: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub plan: String,
    /// The Hermes profile whose store holds the session (`default` for `HERMES_HOME` itself).
    #[serde(default)]
    pub profile: String,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    #[default]
    User,
    AgentAnswer,
}

impl RecordKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::AgentAnswer => "agent_answer",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::User => "Prompt",
            Self::AgentAnswer => "Answer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "user" | "prompt" | "prompts" => Some(Self::User),
            "agent_answer" | "agent-answer" | "answer" | "answers" | "agent" | "turn_final" => {
                Some(Self::AgentAnswer)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub agent: Agent,
    #[serde(default)]
    pub kind: RecordKind,
    #[serde(default)]
    pub interrupted: bool,
    #[serde(default)]
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub session: String,
    /// Session last-active unix seconds, 0 if unknown.
    pub ts: i64,
    #[serde(default)]
    pub meta: Meta,
}

impl Record {
    /// Title shown under a result: explicit title, else session id, else a placeholder.
    pub fn display_title(&self) -> &str {
        if !self.title.is_empty() {
            return &self.title;
        }
        if !self.session.is_empty() {
            return &self.session;
        }
        "Untitled session"
    }

    /// Argv that resumes this prompt's session.
    ///
    /// CDXC:Bots 2026-10-17 WHY:
    /// Hermes keeps each profile's sessions in its own store and plain `hermes` follows the sticky profile, so a Hermes resume names the profile of the store the prompt was read from, the way gxserver's resume plan does (`with_session_hermes_profile`).
    pub fn resume_argv(&self, accept_all: bool) -> Vec<String> {
        let mut argv = self.agent.resume_argv(&self.session, accept_all);
        if self.agent == Agent::Hermes && !self.meta.profile.is_empty() {
            argv.splice(1..1, ["-p".to_string(), self.meta.profile.clone()]);
        }
        argv
    }

    /// Last path segment of the project directory, or "".
    pub fn project_display_name(&self) -> &str {
        project_display_name(&self.project)
    }
}

pub fn project_display_name(project: &str) -> &str {
    if project.is_empty() {
        return "";
    }
    let trimmed = project.trim_end_matches(['/', '\\']);
    let trimmed = if trimmed.is_empty() { project } else { trimmed };
    match trimmed.rsplit(['/', '\\']).next() {
        Some(base) if !base.is_empty() => base,
        _ => trimmed,
    }
}
