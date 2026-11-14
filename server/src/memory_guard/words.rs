//! What the guard says: the chat card, the notification and the note the agent gets.
//!
//! Card wording follows the user's card rule (AGENTS.md, 2026-11-03): a title of 2 to 6 words, at
//! most two plain sentences, no internal words; the processes and their command lines go behind
//! the card's Details toggle. Messages to the agent open with "Ghostex:" like every message Ghostex
//! types into a session.

use super::usage::{ChildGroup, SessionUsage};

/// "9.8 GB" (one decimal from 1 GB, whole megabytes below).
pub(crate) fn size_text(bytes: u64) -> String {
    let gib = bytes as f64 / (1u64 << 30) as f64;
    if gib >= 1.0 {
        format!("{gib:.1} GB")
    } else {
        format!("{} MB", bytes >> 20)
    }
}

/// The agent's name as the card says it ("Claude", "Codex"), or "The agent".
pub(crate) fn agent_name(agent_id: Option<&str>) -> String {
    let Some(agent) = agent_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return "The agent".to_string();
    };
    let base = agent
        .strip_prefix("custom-")
        .and_then(|rest| rest.split('-').next())
        .unwrap_or(agent);
    match base {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "opencode" => "OpenCode".to_string(),
        "gemini" => "Gemini".to_string(),
        other => {
            let mut chars = other.chars();
            chars.next().map_or_else(
                || "The agent".to_string(),
                |first| first.to_uppercase().chain(chars).collect(),
            )
        }
    }
}

/// The biggest commands of a stopped session, for the card ("next 8.1 GB, Chromium 4.9 GB").
fn biggest_parts(usage: &SessionUsage) -> String {
    usage
        .groups
        .iter()
        .take(2)
        .filter(|group| group.bytes >= 256 << 20)
        .map(|group| format!("{} {}", group.label, size_text(group.bytes)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn group_lines(groups: &[&ChildGroup]) -> String {
    groups
        .iter()
        .map(|group| format!("{}  {}", size_text(group.bytes), group.command))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A card: its title, its sentences and the Details text.
pub(crate) struct CardWords {
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) details: String,
}

pub(crate) fn stopped_command_card(group: &ChildGroup, agent: &str) -> CardWords {
    CardWords {
        title: format!("Stopped {} to free memory", group.label),
        detail: format!(
            "This computer was running out of memory and {} used {}. {agent} was told why.",
            group.label,
            size_text(group.bytes)
        ),
        details: group_lines(&[group]),
    }
}

pub(crate) fn stopped_command_message(group: &ChildGroup) -> String {
    format!(
        "Ghostex: this computer was running out of memory, so Ghostex stopped `{}` ({}), which you started. Don't run it again the same way: use fewer workers or a smaller run, or ask the user first. Then carry on.",
        group.label,
        size_text(group.bytes)
    )
}

pub(crate) fn restarted_agent_card(agent: &str, usage: &SessionUsage) -> CardWords {
    CardWords {
        title: format!("Restarted {agent} to free memory"),
        detail: format!(
            "{agent} itself used {} while this computer was running out of memory. It was restarted where it left off and asked to carry on.",
            size_text(usage.agent_bytes)
        ),
        details: String::new(),
    }
}

pub(crate) fn restarted_agent_message(usage: &SessionUsage) -> String {
    format!(
        "Ghostex: you were restarted because your own process used {} while this computer was running out of memory. Check where you were and carry on.",
        size_text(usage.agent_bytes)
    )
}

pub(crate) fn stopped_session_card(usage: &SessionUsage) -> CardWords {
    let parts = biggest_parts(usage);
    let used = if parts.is_empty() {
        size_text(usage.total)
    } else {
        format!("{} ({parts})", size_text(usage.total))
    };
    CardWords {
        title: "Stopped: out of memory".to_string(),
        detail: format!(
            "This computer was running out of memory and this chat's work used {used}, so Ghostex stopped all of it. Resume starts it again."
        ),
        details: group_lines(&usage.groups.iter().collect::<Vec<_>>()),
    }
}

/// What a resumed agent is told: what was running when its session was stopped.
pub(crate) fn resumed_message(parts: &str) -> String {
    if parts.is_empty() {
        "Ghostex: this session was stopped because the computer was running out of memory. Check where you were and carry on, without starting anything big again unless you need it.".to_string()
    } else {
        format!(
            "Ghostex: this session was stopped because the computer was running out of memory ({parts}). Don't start those again unless you need them, then carry on."
        )
    }
}

/// The parts named in a stopped session's card, kept for its Resume message.
pub(crate) fn stopped_session_parts(usage: &SessionUsage) -> String {
    biggest_parts(usage)
}
