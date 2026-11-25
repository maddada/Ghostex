//! The per-agent `/` catalogs and the picker's filter.
//!
//! Port of `packages/core-ui/chat/session-chat-slash-commands.ts`. The catalogs themselves are
//! in `slash_catalogs.rs`; this file is the filter and the headings.

use serde::{Deserialize, Serialize};

use crate::composer::slash_catalogs::{
    ANTIGRAVITY_NATIVE, CLAUDE_CODE, CLAUDE_NATIVE, CODEX, CODEX_NATIVE, COPILOT, CURSOR, FALLBACK,
    GROK, HERMES, OMP, OPENCODE_NATIVE, PI,
};

/// One offered `/` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    /// Command name without the leading slash.
    pub name: &'static str,
    pub description: &'static str,
    /// Completes a composer-native action instead of sending a CLI command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<&'static str>,
}

/// The picker's section heading: the agent's display name, or `Commands`.
pub fn slash_heading_for_agent(agent: Option<&str>) -> &'static str {
    match agent {
        Some("antigravity") => "Antigravity",
        Some("claude") => "Claude Code",
        Some("codex") => "Codex",
        Some("copilot") => "Copilot",
        Some("cursor") => "Cursor CLI",
        Some("grok") => "Grok",
        Some("hermes") => "Hermes Agent",
        Some("openclaude") => "OpenClaude",
        Some("opencode") => "OpenCode",
        Some("pi") => "Pi Agent",
        Some("omp") => "OMP",
        _ => "Commands",
    }
}

/// The catalog for a chat: a Native Codex, Antigravity or Claude chat has its own (`CODEX_NATIVE`,
/// `ANTIGRAVITY_NATIVE`, `CLAUDE_NATIVE`), every other chat the agent's.
pub fn slash_commands_for_session(agent: Option<&str>, non_cli: bool) -> &'static [SlashCommand] {
    match agent {
        Some("codex") if non_cli => CODEX_NATIVE,
        Some("antigravity") if non_cli => ANTIGRAVITY_NATIVE,
        Some("claude") if non_cli => CLAUDE_NATIVE.as_slice(),
        Some("opencode") if non_cli => OPENCODE_NATIVE,
        _ => slash_commands_for_agent(agent),
    }
}

/// The catalog for an agent, or the conservative fallback.
pub fn slash_commands_for_agent(agent: Option<&str>) -> &'static [SlashCommand] {
    match agent {
        Some("claude") | Some("openclaude") => CLAUDE_CODE,
        Some("codex") => CODEX,
        Some("copilot") => COPILOT,
        Some("cursor") => CURSOR,
        Some("grok") => GROK,
        Some("hermes") => HERMES,
        Some("pi") => PI,
        Some("omp") => OMP,
        _ => FALLBACK,
    }
}

/// The token being completed: the whole draft when it is a single line-leading `/word` with no
/// whitespace yet, else `None` (picker closed).
///
/// `/^\/[^\s/]*$/`.
pub fn slash_query(draft: &str) -> Option<&str> {
    let rest = draft.strip_prefix('/')?;
    if rest.chars().any(|character| {
        character == '/' || crate::composer::references::is_js_whitespace(character)
    }) {
        return None;
    }
    Some(rest)
}

/// Prefix matches first, then substring matches, each in catalog order.
pub fn filter_slash_commands(
    commands: &'static [SlashCommand],
    query: &str,
) -> Vec<&'static SlashCommand> {
    if query.is_empty() {
        return commands.iter().collect();
    }
    let lower = query.to_lowercase();
    let mut matches: Vec<&SlashCommand> = commands
        .iter()
        .filter(|command| command.name.starts_with(&lower))
        .collect();
    matches.extend(
        commands
            .iter()
            .filter(|command| !command.name.starts_with(&lower) && command.name.contains(&lower)),
    );
    matches
}
