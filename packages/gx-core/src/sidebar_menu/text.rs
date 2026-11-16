//! The text helpers the menus share.

pub(crate) use crate::sidebar_view::text::js_trim;

/// `resolveSessionChatTranscriptAgent(agentId, agentIcon)`: which transcript family an agent
/// belongs to, or `None` when it has none. Both candidates are tried in order, trimmed and
/// lowercased, against the one chat agent table (`SESSION_CHAT_AGENTS` in gx-protocol).
pub(crate) fn transcript_agent(
    agent_id: Option<&str>,
    agent_icon: Option<&str>,
) -> Option<&'static str> {
    [agent_id, agent_icon]
        .into_iter()
        .flatten()
        .find_map(|candidate| {
            ghostex_gx_protocol::session_chat_transcript_family(&js_trim(candidate).to_lowercase())
        })
}

/// `formatIdentifier` from the Copy Details text: a known agent's proper name, else the id split
/// on `-` and `_` with each part capitalized.
pub(crate) fn format_identifier(value: &str) -> String {
    let known = match value {
        "browser" => Some("Browser"),
        "claude" => Some("Claude"),
        "codex" => Some("Codex"),
        "copilot" => Some("Copilot"),
        "cursor-cli" => Some("Cursor CLI"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi"),
        "terminal" => Some("Terminal"),
        _ => None,
    };
    if let Some(known) = known {
        return known.to_string();
    }
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            // `charAt(0).toUpperCase() + slice(1)` works on UTF-16 units; the first code unit of
            // an astral character has no uppercase form either way, so taking the first character
            // gives the same answer for every input that is not a lone surrogate.
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
