//! The per-agent `/` catalogs and the picker's filter.
//!
//! Port of `packages/core-ui/chat/session-chat-slash-commands.ts`. The six catalogs themselves are
//! in `slash_catalogs.rs`; this file is the filter and the headings.

use serde::{Deserialize, Serialize};

use crate::composer::slash_catalogs::{CLAUDE_CODE, CODEX, CURSOR, FALLBACK, GROK, HERMES};

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
        Some("antigravity") => "Antigravity CLI",
        Some("claude") => "Claude Code",
        Some("codex") => "Codex",
        Some("cursor") => "Cursor CLI",
        Some("grok") => "Grok",
        Some("hermes") => "Hermes Agent",
        Some("openclaude") => "OpenClaude",
        _ => "Commands",
    }
}

/// The catalog for an agent, or the conservative fallback.
pub fn slash_commands_for_agent(agent: Option<&str>) -> &'static [SlashCommand] {
    match agent {
        Some("claude") | Some("openclaude") => CLAUDE_CODE,
        Some("codex") => CODEX,
        Some("cursor") => CURSOR,
        Some("grok") => GROK,
        Some("hermes") => HERMES,
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
