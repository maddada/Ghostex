//! The agents Chat View can open: gxserver reads their transcripts, and every client offers the
//! chat toggle, Export Transcript and Switch Agent only for these.

/// One agent family Chat View reads, with the labels (agent ids, icon ids and spoken names, all
/// trimmed and lower-cased) that resolve to it and the names it is called by in messages.
pub struct SessionChatAgent {
    pub family: &'static str,
    pub labels: &'static [&'static str],
    pub names: &'static [&'static str],
}

/// Every agent family Chat View reads, in the order messages name them.
///
/// CDXC:SessionChat 2026-10-21 WHY:
/// gxserver, gx-core, gx-chat-core and the desktop each kept their own copy of this list, and OpenCode 2's chat (10.25.0) reached only gxserver's, so its Chat View toggle stayed dimmed (#267). Every Rust layer reads this table now; add a chat agent here and nowhere else in Rust.
/// SEE-ALSO: packages/shared/session-chat-agents.ts and its generated phone copy (apps/mobile/app/src/chat/session-chat-agents.generated.ts), apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts `transcriptAgent` (the phone cannot link this crate).
pub const SESSION_CHAT_AGENTS: [SessionChatAgent; 13] = [
    SessionChatAgent {
        family: "claude",
        labels: &["claude", "openclaude"],
        names: &["Claude"],
    },
    SessionChatAgent {
        family: "codex",
        labels: &["codex"],
        names: &["Codex"],
    },
    SessionChatAgent {
        family: "opencode",
        labels: &["opencode"],
        names: &["OpenCode"],
    },
    SessionChatAgent {
        family: "cursor",
        labels: &["cursor", "cursor-agent", "cursor-cli", "cursor cli"],
        names: &["Cursor"],
    },
    SessionChatAgent {
        family: "grok",
        labels: &["grok", "grok-build"],
        names: &["Grok"],
    },
    SessionChatAgent {
        family: "copilot",
        labels: &["copilot", "github copilot", "github-copilot"],
        names: &["Copilot"],
    },
    SessionChatAgent {
        family: "antigravity",
        labels: &["antigravity", "antigravity-cli", "antigravity cli", "agy"],
        names: &["Antigravity"],
    },
    SessionChatAgent {
        family: "pi",
        labels: &["pi", "omp"],
        names: &["Pi", "OMP"],
    },
    SessionChatAgent {
        family: "hermes",
        labels: &["hermes", "hermes-agent", "hermes agent"],
        names: &["Hermes"],
    },
    SessionChatAgent {
        family: "zcode",
        labels: &["zcode", "zcode-cli"],
        names: &["ZCode"],
    },
    SessionChatAgent {
        family: "empryo",
        labels: &["empryo"],
        names: &["Empryo"],
    },
    SessionChatAgent {
        family: "freebuff",
        labels: &["freebuff"],
        names: &["Freebuff"],
    },
    SessionChatAgent {
        family: "dsh",
        labels: &["dsh", "dsh-tui", "deepseek", "deepseek harness"],
        names: &["DeepSeek Harness"],
    },
];

/// The transcript family a trimmed, lower-cased agent label belongs to, or `None` when Chat View
/// cannot read that agent. OMP resolves to `pi` because it writes Pi's transcript format.
pub fn session_chat_transcript_family(label: &str) -> Option<&'static str> {
    SESSION_CHAT_AGENTS
        .iter()
        .find(|agent| agent.labels.contains(&label))
        .map(|agent| agent.family)
}

/// "Claude, Codex, … and DeepSeek Harness": the agents Chat View supports, for messages that say so.
pub fn session_chat_agent_names_text() -> String {
    let names: Vec<&str> = SESSION_CHAT_AGENTS
        .iter()
        .flat_map(|agent| agent.names.iter().copied())
        .collect();
    match names.split_last() {
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}
