use serde::{Deserialize, Serialize};

/// The agents whose local prompt history zehn indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Pi,
    Opencode,
    Cursor,
    Grok,
}

pub const ALL_AGENTS: [Agent; 6] = [
    Agent::Claude,
    Agent::Codex,
    Agent::Pi,
    Agent::Opencode,
    Agent::Cursor,
    Agent::Grok,
];

impl Agent {
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "cursor",
            Agent::Grok => "grok",
        }
    }

    pub fn parse(name: &str) -> Option<Agent> {
        ALL_AGENTS.into_iter().find(|agent| agent.label() == name)
    }

    pub fn bit(self) -> u8 {
        match self {
            Agent::Claude => 1 << 0,
            Agent::Codex => 1 << 1,
            Agent::Pi => 1 << 2,
            Agent::Opencode => 1 << 3,
            Agent::Cursor => 1 << 4,
            Agent::Grok => 1 << 5,
        }
    }

    /// Official brand colors as 24-bit truecolor escapes, used by the TUI.
    pub fn ansi_color(self) -> &'static str {
        match self {
            Agent::Claude => "\x1b[38;2;218;119;86m", // #DA7756 Anthropic terra cotta
            Agent::Codex => "\x1b[38;2;16;163;127m",  // #10A37F OpenAI green
            Agent::Opencode => "\x1b[38;2;207;206;205m", // #CFCECD opencode logo gray
            Agent::Pi => "\x1b[38;2;136;192;208m",    // #88C0D0 pi Nord frost
            Agent::Cursor => "\x1b[38;2;74;144;226m", // Cursor blue
            Agent::Grok => "\x1b[38;2;180;160;255m",  // xAI/Grok purple accent
        }
    }

    /// The same brand colors as CSS hex, used by the Find GUI.
    pub fn hex_color(self) -> &'static str {
        match self {
            Agent::Claude => "#DA7756",
            Agent::Codex => "#10A37F",
            Agent::Opencode => "#CFCECD",
            Agent::Pi => "#88C0D0",
            Agent::Cursor => "#4A90E2",
            Agent::Grok => "#B4A0FF",
        }
    }

    /// Argv that resumes an existing session in this agent. `accept_all` adds
    /// the same permission-bypass flags Ghostex uses elsewhere; pi has none.
    pub fn resume_argv(self, session: &str, accept_all: bool) -> Vec<String> {
        let parts: Vec<&str> = if accept_all {
            match self {
                Agent::Claude => vec![
                    "claude",
                    "--dangerously-skip-permissions",
                    "--resume",
                    session,
                ],
                Agent::Codex => vec!["codex", "--yolo", "resume", session],
                Agent::Pi => vec!["pi", "--session", session],
                Agent::Opencode => {
                    vec![
                        "opencode",
                        "--dangerously-skip-permissions",
                        "--session",
                        session,
                    ]
                }
                Agent::Cursor => vec!["cursor-agent", "--yolo", "--resume", session],
                Agent::Grok => vec![
                    "grok",
                    "--permission-mode",
                    "bypassPermissions",
                    "--resume",
                    session,
                ],
            }
        } else {
            match self {
                Agent::Claude => vec!["claude", "--resume", session],
                Agent::Codex => vec!["codex", "resume", session],
                Agent::Pi => vec!["pi", "--session", session],
                Agent::Opencode => vec!["opencode", "--session", session],
                Agent::Cursor => vec!["cursor-agent", "--resume", session],
                Agent::Grok => vec!["grok", "--resume", session],
            }
        };
        parts.into_iter().map(str::to_string).collect()
    }

    /// Argv that starts a brand-new session seeded with `prompt`. Unlike resume
    /// this needs no session id, so a prompt can be branched into any agent.
    pub fn fresh_session_argv(self, prompt: &str) -> Vec<String> {
        let binary = match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "cursor-agent",
            Agent::Grok => "grok",
        };
        vec![binary.to_string(), prompt.to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_filter_parsing_accepts_known_names_only() {
        assert_eq!(Agent::parse("claude"), Some(Agent::Claude));
        assert_eq!(Agent::parse("opencode"), Some(Agent::Opencode));
        assert_eq!(Agent::parse("grok"), Some(Agent::Grok));
        assert_eq!(Agent::parse("antigravity"), None);
    }

    #[test]
    fn resume_argv_optionally_applies_accept_all_flags() {
        assert_eq!(
            Agent::Codex.resume_argv("s", false),
            vec!["codex", "resume", "s"]
        );
        assert_eq!(
            Agent::Codex.resume_argv("s", true),
            vec!["codex", "--yolo", "resume", "s"]
        );
        assert_eq!(
            Agent::Pi.resume_argv("s", true),
            vec!["pi", "--session", "s"]
        );
        assert_eq!(
            Agent::Grok.resume_argv("s", true),
            vec![
                "grok",
                "--permission-mode",
                "bypassPermissions",
                "--resume",
                "s"
            ]
        );
    }

    #[test]
    fn fresh_session_argv_carries_the_prompt_as_one_arg() {
        assert_eq!(
            Agent::Cursor.fresh_session_argv("make it faster"),
            vec!["cursor-agent", "make it faster"]
        );
    }
}
