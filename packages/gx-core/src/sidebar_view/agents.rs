//! The built-in agent catalog, as far as the sidebar rows need it: resolving a daemon's agent
//! name to an icon key, the display name of an icon, and the labels a row tooltip strips.
//!
//! SEE-ALSO: packages/shared/sidebar-agents.ts (`DEFAULT_SIDEBAR_AGENT_DEFINITIONS`) and
//! packages/core-ui/session-card-presentation.ts (`AGENT_SECONDARY_LABELS`); the three tables must
//! list the same agents.

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

/// `getDefaultSidebarAgentById(agentId)?.agentId`: the built-in agent with this id, trimmed and
/// lowercased first.
pub(crate) fn default_agent_id(agent_id: &str) -> Option<&'static str> {
    let agent_id = agent_id.trim().to_lowercase();
    AGENTS
        .iter()
        .find(|(id, _, _)| *id == agent_id)
        .map(|(id, _, _)| *id)
}

/// The icon of the browser rows; not an agent, but part of the same icon vocabulary.
pub(crate) const BROWSER_AGENT_ICON: &str = "browser";

/// `AGENT_SECONDARY_LABELS`: further names a tooltip line may start with.
fn secondary_labels(icon: &str) -> &'static [&'static str] {
    match icon {
        "amp-cli" => &["amp", "amp cli"],
        "antigravity-cli" => &["agy", "antigravity", "antigravity cli"],
        "browser" => &["browser"],
        "zcode" => &["zcode"],
        "claude" => &["claude", "claude code"],
        "codebuddy" => &["codebuddy", "code buddy"],
        "command-code" => &["command code", "commandcode"],
        "cursor-cli" => &["cursor", "cursor agent", "cursor cli", "cursor-agent"],
        "codex" => &["codex", "codex cli", "openai codex"],
        "copilot" => &["copilot", "github copilot"],
        "mastra" => &["mastra", "mastra code", "mastracode"],
        "devin" => &["devin"],
        "factory-droid" => &["droid", "factory droid"],
        "gemini" => &["gemini"],
        "grok-build" => &["grok", "grok build"],
        "hermes-agent" => &["hermes", "hermes agent"],
        "kimi" => &["kimi", "kimi code"],
        "kiro" => &["kiro", "kiro cli", "kiro-cli"],
        "omp" => &["omp"],
        "openclaude" => &["open claude", "openclaude"],
        "opencode" => &["open code", "opencode"],
        "pi" => &["pi", "π"],
        "qoder" => &["qoder", "qodercli"],
        "rovo-dev" => &["rovo", "rovo dev", "rovodev"],
        _ => &[],
    }
}

/// `resolveGpuiSidebarAgentIcon`: the icon key for a daemon's agent icon, name, or id. Matches an
/// agent id first, then (case-insensitively) an id, a display name, or an icon key.
pub(crate) fn resolve_agent_icon(agent: Option<&str>) -> Option<&'static str> {
    let normalized = super::text::js_trim(agent?).to_lowercase();
    if let Some((_, icon, _)) = AGENTS.iter().find(|(id, _, _)| *id == normalized) {
        return Some(icon);
    }
    if normalized.is_empty() {
        return None;
    }
    AGENTS
        .iter()
        .find(|(id, icon, name)| {
            *id == normalized || name.to_lowercase() == normalized || *icon == normalized
        })
        .map(|(_, icon, _)| *icon)
}

/// `getSidebarAgentNameByIcon`.
pub(crate) fn agent_name_by_icon(icon: &str) -> Option<&'static str> {
    if icon == BROWSER_AGENT_ICON {
        return Some("Browser");
    }
    AGENTS
        .iter()
        .find(|(_, candidate, _)| *candidate == icon)
        .map(|(_, _, name)| *name)
}

/// The labels a tooltip line is stripped of, longest first (stable for equal lengths, like the
/// JavaScript sort): the icon's display name, then its secondary labels, without duplicates.
pub(crate) fn tooltip_strip_labels(icon: &str) -> Vec<&'static str> {
    let mut labels: Vec<&'static str> = Vec::new();
    for label in agent_name_by_icon(icon)
        .into_iter()
        .chain(secondary_labels(icon).iter().copied())
    {
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let mut labels: Vec<&'static str> = labels
        .into_iter()
        .map(super::text::js_trim)
        .filter(|label| !label.is_empty())
        .collect();
    labels.sort_by_key(|label| std::cmp::Reverse(super::text::utf16_len(label)));
    labels
}
