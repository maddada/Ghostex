//! The "Theme changed" card: an agent changed the look with `ghostex theme set`, and its chat shows
//! it in one card naming the agent, listing what changed and anything it made hard to read, with
//! Undo. It rides the "GBot added" card's pipeline (gbot_cards.rs): the same card line, the same
//! per-change state, the same Undo; its state and Undo come from gxserver's theme history
//! (`/api/theme` `change` and `undo`), so a reload, another computer or the phone show the same.
//!
//! CDXC:Theming 2026-11-14 DECISION:
//! User (Q3118 and Q3117, agent theming page): any agent may change the theme, "with the Theme changed card naming it", and a change that makes text hard to read is applied with a warning on the card instead of being refused. After Undo the card reads "Theme change undone".
//! SEE-ALSO: server/src/theme/ (the card line, history and Undo), apps/desktop/src/app/native_chat/theme_card.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/GbotCard.tsx (phone).

use ghostex_gx_protocol::chat_card::THEME_CHANGED;
use serde_json::{json, Value};

use crate::transcript::agent_message_card::agent_display_name;
use crate::transcript::gbot_cards::GbotCardEntry;

/// Theme change ids start with this (server/src/theme/endpoint.rs), which is how a card's state
/// and Undo go to `/api/theme` rather than `/api/gbots`.
pub const THEME_CHANGE_PREFIX: &str = "theme-";

pub fn is_theme_change(change_id: &str) -> bool {
    change_id.starts_with(THEME_CHANGE_PREFIX)
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The card's fixed words, from the card line `ghostex theme set` printed.
pub fn theme_card(card: &Value) -> Value {
    let named = agent_display_name(str_of(card, "agentId"), "");
    let by = if named.trim().is_empty() {
        str_of(card, "by").trim().to_string()
    } else {
        named
    };
    let title = if by.is_empty() || by == "You" || by == "An agent" {
        "Theme changed".to_string()
    } else {
        format!("Theme changed by {by}")
    };
    let lines: Vec<Value> = card
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| {
            json!(format!(
                "{}: {} to {}",
                str_of(item, "title"),
                str_of(item, "before"),
                str_of(item, "after")
            ))
        })
        .collect();
    let warnings: Vec<Value> = card
        .get("warnings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|warning| warning.as_str().is_some_and(|text| !text.trim().is_empty()))
        .cloned()
        .collect();
    json!({
        "kind": THEME_CHANGED,
        "changeId": str_of(card, "changeId"),
        "icon": "palette",
        "title": title,
        "removedTitle": "Theme change undone",
        "body": "",
        "lines": lines,
        "warnings": warnings,
    })
}

/// What `gbotCardStates` says of a theme card: `{ loading, undone, note, undoLabel, undoEnabled,
/// settingsEnabled, error }`, as for a "GBot added" card.
pub fn card_state(entry: &GbotCardEntry) -> Value {
    let change = entry.change.as_ref();
    let undone = change.is_some_and(|change| change.get("undoneAt").is_some_and(Value::is_string));
    let undoable = change.is_some_and(|change| change.get("undoable") == Some(&Value::Bool(true)));
    let note = if undone {
        let by = change
            .and_then(|change| change.get("undoneBy"))
            .map(|by| str_of(by, "name"))
            .unwrap_or_default();
        if by.is_empty() || by == "You" {
            "Undone. The look is back to how it was.".to_string()
        } else {
            format!("{by} undid it. The look is back to how it was.")
        }
    } else if change.is_some() && !undoable {
        change
            .and_then(|change| change.get("undoProblem"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };
    json!({
        "loading": entry.loading,
        "undone": undone,
        "note": note,
        "undoLabel": if entry.undoing { "Undoing…" } else { "Undo" },
        "undoEnabled": undoable && !entry.undoing && !entry.loading,
        "settingsEnabled": false,
        "error": entry.error,
    })
}

/// The refusal a theme card shows when gxserver's answer has no words of its own.
pub fn fallback_error(undoing: bool) -> &'static str {
    if undoing {
        "Undo didn't go through. Try again."
    } else {
        "Couldn't read this theme change."
    }
}
