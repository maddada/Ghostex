// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds button ordering/JSON-array helpers, default-id checks, sidebar icon lookups, and command-pane session indicator derivation.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_order_json_buttons(
    buttons: Vec<(String, serde_json::Value)>,
    stored_order: &[String],
    id_key: &str,
) -> serde_json::Value {
    let mut ordered_buttons = Vec::new();
    let mut used_ids = HashSet::new();
    for item_id in stored_order {
        if let Some((_, button)) = buttons.iter().find(|(button_id, _)| button_id == item_id) {
            ordered_buttons.push(button.clone());
            used_ids.insert(item_id.clone());
        }
    }
    for (button_id, button) in buttons {
        let actual_id = button
            .get(id_key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(button_id.as_str());
        if used_ids.insert(actual_id.to_string()) {
            ordered_buttons.push(button);
        }
    }
    serde_json::Value::Array(ordered_buttons)
}

pub(crate) fn gpui_json_array_field_is_nonempty(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> bool {
    object
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_default_sidebar_agent_id(agent_id: &str) -> bool {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.agent_id == agent_id)
}

pub(crate) fn gpui_default_sidebar_agent_name(agent_id: &str, stored_name: &str) -> String {
    let default_name = GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.name);
    let Some(default_name) = default_name else {
        return stored_name.to_string();
    };
    let normalized = stored_name.trim().to_ascii_lowercase();
    if (agent_id == "codex" && normalized == "codex cli")
        || (agent_id == "claude" && normalized == "claude code")
        || (agent_id == "cursor" && normalized == "cursor")
        || (agent_id == "pi" && normalized == "pi")
    {
        default_name.to_string()
    } else {
        stored_name.to_string()
    }
}

pub(crate) fn gpui_is_default_sidebar_command_id(command_id: &str) -> bool {
    GPUI_DEFAULT_SIDEBAR_COMMANDS
        .iter()
        .any(|command| command.command_id == command_id)
}

pub(crate) fn gpui_strict_sidebar_agent_icon(candidate: &str) -> Option<&str> {
    if candidate == "browser" {
        return Some(candidate);
    }
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.icon == candidate)
        .then_some(candidate)
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
Action icon slugs are camelCase ids shared with the TypeScript surfaces, while
the bundled assets are kebab-case files under `titlebar/`. Convert rather than
hand-maintaining a 59-entry table that would silently rot whenever an icon is
added on the shared side. `terminal` is the one id whose asset is not its
kebab-case name — the bundle ships `terminal-2.svg`, which the tab strip's own
Terminal View button already points at.

Actions saved without an icon fall back to the same default the sidebar uses, so
an icon-less Global Action still renders a button rather than an empty slot.
*/
pub(crate) fn gpui_sidebar_command_icon_asset_path(icon: Option<&str>) -> gpui::SharedString {
    let icon = icon.unwrap_or(GPUI_DEFAULT_SIDEBAR_COMMAND_ICON);
    if icon == "terminal" {
        return gpui::SharedString::new_static("titlebar/terminal-2.svg");
    }
    let mut asset = String::with_capacity(icon.len() + 1);
    for character in icon.chars() {
        if character.is_ascii_uppercase() {
            asset.push('-');
            asset.push(character.to_ascii_lowercase());
        } else {
            asset.push(character);
        }
    }
    gpui::SharedString::from(format!("titlebar/{asset}.svg"))
}

pub(crate) fn gpui_sidebar_command_icon(candidate: &str) -> Option<&str> {
    GPUI_SIDEBAR_COMMAND_ICON_IDS
        .iter()
        .any(|icon| *icon == candidate)
        .then_some(candidate)
}

pub(crate) fn gpui_sidebar_command_session_indicators_from_command_pane_sources(
    commands: &serde_json::Value,
    command_pane_sessions: &serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:GPUICommandSessionHud 2026-06-27-08:45:
    Command-session HUD matching consumes the same external bridge shape emitted by command-pane sources: `sessionId` must be a string accepted by the canonical `G{u64}` parser and `status` must be a known lifecycle value before commandId/title matching runs. Stale legacy numeric, lowercase, malformed, non-string, or invalid-status rows must be invisible to matching so they cannot mask a live canonical row, and emitted indicators must continue to omit command text, paths, URLs, run ids, status files, terminal output, and unknown raw fields.
    */
    let Some(commands) = commands.as_array() else {
        return serde_json::Value::Array(Vec::new());
    };
    let sessions = command_pane_sessions
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    serde_json::Value::Array(
        commands
            .iter()
            .filter_map(|command| {
                if command
                    .get("actionType")
                    .and_then(serde_json::Value::as_str)
                    != Some("terminal")
                {
                    return None;
                }
                let command_id = command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_command_pane_sidebar_indicator_text)?;
                let command_title = gpui_sidebar_command_indicator_title_from_command(command)?;
                let command_title_key = gpui_command_pane_sidebar_indicator_key(&command_title);
                if command_title_key.is_empty() {
                    return None;
                }
                let mapped_session = sessions.iter().find(|session| {
                    gpui_sidebar_command_indicator_matchable_session_source(session)
                        && session
                            .get("commandId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|candidate| candidate == command_id.as_str())
                        && gpui_command_pane_sidebar_indicator_key(
                            session
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default(),
                        ) == command_title_key
                });
                let session = mapped_session.or_else(|| {
                    sessions.iter().find(|session| {
                        gpui_sidebar_command_indicator_matchable_session_source(session)
                            && gpui_command_pane_sidebar_indicator_key(
                                session
                                    .get("title")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default(),
                            ) == command_title_key
                    })
                })?;
                let session_id = gpui_sidebar_command_indicator_session_id(session)?;
                let status = gpui_sidebar_command_indicator_status(session)?;
                let title = session
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_command_pane_sidebar_indicator_text);
                let mut indicator = serde_json::json!({
                    "commandId": command_id,
                    "isActive": session
                        .get("isActive")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    "sessionId": session_id,
                    "status": status,
                });
                if let Some(title) = title {
                    indicator["title"] = serde_json::json!(title);
                }
                for key in [
                    "delayedSendDeadlineAt",
                    "delayedSendRemainingLabel",
                    "closeAfterDoneDeadlineAt",
                    "closeAfterDoneRemainingLabel",
                ] {
                    if let Some(value) = session
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .and_then(gpui_command_pane_sidebar_indicator_text)
                    {
                        indicator[key] = serde_json::json!(value);
                    }
                }
                for key in ["delayedSendRemainingMs", "closeAfterDoneRemainingMs"] {
                    if let Some(value) = session.get(key).and_then(serde_json::Value::as_u64) {
                        indicator[key] = serde_json::json!(value);
                    }
                }
                if session
                    .get("closeAfterDone")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    indicator["closeAfterDone"] = serde_json::json!(true);
                }
                Some(indicator)
            })
            .collect(),
    )
}

pub(crate) fn gpui_sidebar_command_indicator_matchable_session_source(session: &serde_json::Value) -> bool {
    gpui_sidebar_command_indicator_session_id(session).is_some()
        && gpui_sidebar_command_indicator_status(session).is_some()
}

pub(crate) fn gpui_sidebar_command_indicator_session_id(session: &serde_json::Value) -> Option<&str> {
    let session_id = session.get("sessionId")?.as_str()?;
    gpui_command_session_id_from_external_id(session_id)?;
    Some(session_id)
}

pub(crate) fn gpui_sidebar_command_indicator_status(session: &serde_json::Value) -> Option<&'static str> {
    match session.get("status").and_then(serde_json::Value::as_str) {
        Some("idle") => Some("idle"),
        Some("running") => Some("running"),
        Some("error") => Some("error"),
        _ => None,
    }
}

pub(crate) fn gpui_sidebar_command_indicator_title_from_command(
    command: &serde_json::Value,
) -> Option<String> {
    if let Some(name) = command
        .get("name")
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_command_pane_sidebar_indicator_text)
    {
        return Some(name);
    }
    let command_text = command
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(20)
        .collect::<String>();
    gpui_command_pane_sidebar_indicator_text(&command_text)
}

pub(crate) fn gpui_command_pane_sidebar_indicator_key(value: &str) -> String {
    gpui_command_pane_sidebar_indicator_text(value)
        .map(|text| text.to_lowercase())
        .unwrap_or_default()
}

