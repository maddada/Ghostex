//! The built-in agent catalog, as far as the new-session welcome needs it.
//!
//! Family f needs two answers from it: the display name of an agent id, and whether a wire-supplied
//! icon is one the app ships artwork for. `packages/gx-core/src/sidebar_view/agents.rs` holds the
//! same table for the sidebar but keeps it crate-private, so this is a private copy until the two
//! crates share one (noted in `docs/2026-09-21/rust-chat/PROGRESS.md`).
//!
//! SEE-ALSO: packages/shared/sidebar-agents.ts (`DEFAULT_SIDEBAR_AGENT_DEFINITIONS`) and
//! packages/gx-core/src/sidebar_view/agents.rs; the three tables must list the same agents.

/// `(agentId, icon, name)` in catalog order.
const AGENTS: &[(&str, &str, &str)] = &[
    ("codex", "codex", "Codex"),
    ("claude", "claude", "Claude"),
    ("cursor", "cursor-cli", "Cursor CLI"),
    ("pi", "pi", "Pi Agent"),
    ("opencode", "opencode", "OpenCode"),
    ("gemini", "gemini", "Gemini"),
    ("copilot", "copilot", "Copilot"),
    ("droid", "factory-droid", "Factory Droid"),
    ("grok", "grok-build", "Grok Build"),
    ("antigravity", "antigravity-cli", "Antigravity CLI"),
    ("amp", "amp-cli", "Amp CLI"),
    ("hermes-agent", "hermes-agent", "Hermes Agent"),
    ("rovodev", "rovo-dev", "Rovo Dev"),
    ("codebuddy", "codebuddy", "CodeBuddy"),
    ("qoder", "qoder", "Qoder"),
    ("kiro", "kiro", "Kiro CLI"),
    ("omp", "omp", "OMP"),
    ("kimi", "kimi", "Kimi Code"),
    ("openclaude", "openclaude", "OpenClaude"),
    ("command-code", "command-code", "Command Code"),
    ("devin", "devin", "Devin"),
    ("mastra", "mastra", "Mastra Code"),
    ("zcode", "zcode", "ZCode"),
];

/// The icon of the browser rows; not an agent, but part of the same icon vocabulary.
const BROWSER_ICON: &str = "browser";

/// `getDefaultSidebarAgentById`: the catalog row for a trimmed, lower-cased agent id.
pub fn default_agent_by_id(agent_id: Option<&str>) -> Option<(&'static str, &'static str)> {
    let normalized = crate::extras::agent_tasks::js_trim(agent_id?).to_lowercase();
    AGENTS
        .iter()
        .find(|(id, _, _)| *id == normalized)
        .map(|(_, icon, name)| (*icon, *name))
}

/// `isSidebarAgentIcon`: narrows a wire-supplied icon string to the union the app ships artwork
/// for.
pub fn is_sidebar_agent_icon(candidate: Option<&str>) -> bool {
    match candidate {
        Some(BROWSER_ICON) => true,
        Some(candidate) => AGENTS.iter().any(|(_, icon, _)| *icon == candidate),
        None => false,
    }
}

/// `resolveSessionChatTranscriptAgent`: the transcript family an agent label belongs to.
pub fn transcript_agent(candidates: [Option<&str>; 2]) -> Option<&'static str> {
    for candidate in candidates.into_iter().flatten() {
        let normalized = crate::extras::agent_tasks::js_trim(candidate).to_lowercase();
        let resolved = match normalized.as_str() {
            "antigravity" | "antigravity-cli" | "antigravity cli" | "agy" => Some("antigravity"),
            "claude" | "openclaude" => Some("claude"),
            "codex" => Some("codex"),
            "cursor" | "cursor-agent" | "cursor cli" => Some("cursor"),
            "grok" | "grok-build" => Some("grok"),
            "hermes" | "hermes-agent" | "hermes agent" => Some("hermes"),
            "pi" | "omp" => Some("pi"),
            "zcode" | "zcode-cli" => Some("zcode"),
            _ => None,
        };
        if resolved.is_some() {
            return resolved;
        }
    }
    None
}

/// `resolveSessionChatDisplayAgent`: OMP transcripts use Pi's format, but OMP stays its own product
/// name and logo everywhere the session is presented.
fn display_agent(agent_label: Option<&str>) -> Option<&'static str> {
    if let Some(label) = agent_label {
        if crate::extras::agent_tasks::js_trim(label).to_lowercase() == "omp" {
            return Some("omp");
        }
    }
    transcript_agent([agent_label, None])
}

/// `sessionChatAgentIconId`: the sidebar artwork id for a chat agent label.
///
/// Read-state labels are transcript family ids, and two of those differ from the sidebar agent id
/// that owns the brand artwork (`hermes` to `hermes-agent`, `grok` to `grok-build`); the rest match
/// their sidebar id as is.
pub fn agent_icon_id(agent_label: Option<&str>) -> Option<&'static str> {
    Some(match display_agent(agent_label)? {
        "antigravity" => "antigravity-cli",
        "hermes" => "hermes-agent",
        "grok" => "grok-build",
        other => other,
    })
}
