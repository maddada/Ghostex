//! Bring back: what a side chat said, as one message for its main session.

use ghostex_gx_chat_core::composer::side_chat::{without_side_chat_note, SIDE_CHAT_NOTE_OPEN};
use serde_json::Value;

/// The most a Bring back message carries; the oldest exchanges go first.
const BRING_BACK_MAX_CHARS: usize = 8_000;
/// The most one message of the side chat carries.
const ITEM_MAX_CHARS: usize = 2_000;

fn message_text(message: &Value) -> String {
    message
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= ITEM_MAX_CHARS {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(ITEM_MAX_CHARS).collect();
    cut.push('…');
    cut
}

/// The side chat's own conversation, from its first question (the message carrying its note) on:
/// what the user asked and what the agent answered, oldest first. `None` when nothing was said.
pub(crate) fn bring_back_message(messages: &[Value], title: &str) -> Option<String> {
    let role = |message: &Value| {
        message
            .get("role")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
    };
    let start = messages.iter().position(|message| {
        role(message).as_deref() == Some("user")
            && message_text(message).contains(SIDE_CHAT_NOTE_OPEN)
    })?;
    let lines = messages[start..]
        .iter()
        .filter_map(|message| {
            let label = match role(message)?.as_str() {
                "user" => "User",
                "assistant" => "Side chat",
                _ => return None,
            };
            let text = message_text(message);
            let text = compact(without_side_chat_note(&text));
            (!text.is_empty()).then(|| format!("{label}: {text}"))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let header = format!(
        "Brought back from the side chat \"{title}\" (a fork of this session). This is what was said there, for context; it is not a new task unless it asks for one."
    );
    let trimmed_note = "(Earlier messages left out.)";
    let mut remaining = BRING_BACK_MAX_CHARS.saturating_sub(header.len() + trimmed_note.len() + 4);
    let mut kept = Vec::new();
    for line in lines.iter().rev() {
        if line.len() + 2 > remaining {
            break;
        }
        remaining -= line.len() + 2;
        kept.push(line.as_str());
    }
    kept.reverse();
    let mut parts = vec![header];
    if kept.len() < lines.len() {
        parts.push(trimmed_note.to_string());
    }
    parts.extend(kept.into_iter().map(str::to_string));
    Some(parts.join("\n\n"))
}
