// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds command-title dedupe/key helpers, sidebar button-id/order helpers, default-agent lookups, and stored agent/command JSON value builders.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::{SystemTime, UNIX_EPOCH};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::Result;

use crate::app::helpers::*;

pub(crate) fn gpui_reject_duplicate_sidebar_command_title(
    next_command: &GpuiStoredSidebarCommand,
    stored_commands: &[GpuiStoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> Result<(), String> {
    let next_title_key = gpui_sidebar_command_title_key(
        &next_command.name,
        next_command.command.as_deref(),
        next_command.url.as_deref(),
    );
    let buttons = gpui_sidebar_command_buttons_from_state(
        stored_commands,
        stored_order,
        deleted_default_command_ids,
    );
    let duplicate = buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .any(|candidate| {
            gpui_trimmed_json_string_field(candidate, "commandId")
                .map(|command_id| command_id != next_command.command_id)
                .unwrap_or(false)
                && gpui_sidebar_command_title_key(
                    candidate
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    candidate.get("command").and_then(serde_json::Value::as_str),
                    candidate.get("url").and_then(serde_json::Value::as_str),
                ) == next_title_key
        });
    if duplicate {
        Err(GPUI_SIDEBAR_DUPLICATE_ACTION_TITLE_ERROR.to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn gpui_sidebar_command_title_key(name: &str, command: Option<&str>, url: Option<&str>) -> String {
    let title = gpui_normalized_sidebar_command_title(Some(name))
        .or_else(|| {
            gpui_normalized_sidebar_command_title(command.or(url))
                .map(gpui_sidebar_command_short_title)
        })
        .unwrap_or_default();
    title.to_lowercase()
}

pub(crate) fn gpui_sidebar_command_short_title(value: String) -> String {
    value.chars().take(20).collect()
}

pub(crate) fn gpui_normalized_sidebar_command_title(value: Option<&str>) -> Option<String> {
    let normalized = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn gpui_sidebar_button_ids(buttons: &serde_json::Value, id_key: &str) -> Vec<String> {
    buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .filter_map(|button| gpui_trimmed_json_string_field(button, id_key))
        .map(str::to_string)
        .collect()
}

pub(crate) fn gpui_sidebar_order_sync_result_message(
    kind: &'static str,
    request_id: &str,
    status: &'static str,
    item_ids: Vec<String>,
) -> serde_json::Value {
    serde_json::json!({
        "itemIds": item_ids,
        "kind": kind,
        "requestId": request_id,
        "status": status,
        "type": "sidebarOrderSyncResult",
    })
}

pub(crate) fn gpui_is_sidebar_agent_visible(agents: &[GpuiStoredSidebarAgent], agent_id: &str) -> bool {
    agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| !agent.hidden)
        .unwrap_or(true)
}

pub(crate) fn gpui_default_sidebar_agent_by_id(agent_id: &str) -> Option<&'static GpuiDefaultSidebarAgent> {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
}

pub(crate) fn gpui_default_sidebar_agent_by_icon(icon: &str) -> Option<&'static GpuiDefaultSidebarAgent> {
    if icon == "browser" {
        return None;
    }
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == icon)
}

pub(crate) fn gpui_create_custom_sidebar_agent_id(name: &str) -> String {
    let slug = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "agent".to_string()
    } else {
        slug.chars().take(24).collect::<String>()
    };
    format!("custom-{slug}-{}", gpui_generated_sidebar_metadata_suffix())
}

pub(crate) fn gpui_create_custom_sidebar_command_id() -> String {
    format!("custom-{}", gpui_generated_sidebar_metadata_suffix())
}

pub(crate) fn gpui_generated_sidebar_metadata_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "{}-{}",
        gpui_base36(nanos),
        gpui_base36(std::process::id() as u128)
    )
}

pub(crate) fn gpui_base36(mut value: u128) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        let digit = (value % 36) as usize;
        output.push(DIGITS[digit] as char);
        value /= 36;
    }
    output.iter().rev().collect()
}

pub(crate) fn gpui_stored_sidebar_agents_value(agents: &[GpuiStoredSidebarAgent]) -> serde_json::Value {
    serde_json::Value::Array(
        agents
            .iter()
            .map(|agent| {
                let mut item = serde_json::Map::new();
                if let Some(accept_all_mode) = agent.accept_all_mode.as_ref() {
                    item.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String(accept_all_mode.clone()),
                    );
                }
                item.insert(
                    "agentId".to_string(),
                    serde_json::Value::String(agent.agent_id.clone()),
                );
                item.insert(
                    "command".to_string(),
                    serde_json::Value::String(agent.command.clone()),
                );
                item.insert("hidden".to_string(), serde_json::Value::Bool(agent.hidden));
                if let Some(icon) = agent.icon.as_ref() {
                    item.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    serde_json::Value::Bool(gpui_is_default_sidebar_agent_id(&agent.agent_id)),
                );
                item.insert(
                    "name".to_string(),
                    serde_json::Value::String(agent.name.clone()),
                );
                serde_json::Value::Object(item)
            })
            .collect(),
    )
}

pub(crate) fn gpui_stored_sidebar_commands_value(commands: &[GpuiStoredSidebarCommand]) -> serde_json::Value {
    serde_json::Value::Array(
        commands
            .iter()
            .map(|command| {
                let mut item = serde_json::Map::new();
                item.insert(
                    "actionType".to_string(),
                    serde_json::Value::String(command.action_type.to_string()),
                );
                item.insert(
                    "closeTerminalOnExit".to_string(),
                    serde_json::Value::Bool(command.close_terminal_on_exit),
                );
                if let Some(command_text) = command.command.as_ref() {
                    item.insert(
                        "command".to_string(),
                        serde_json::Value::String(command_text.clone()),
                    );
                }
                item.insert(
                    "commandId".to_string(),
                    serde_json::Value::String(command.command_id.clone()),
                );
                if let Some(icon) = command.icon.as_ref() {
                    item.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    serde_json::Value::Bool(command.is_default),
                );
                item.insert(
                    "name".to_string(),
                    serde_json::Value::String(command.name.clone()),
                );
                item.insert(
                    "playCompletionSound".to_string(),
                    serde_json::Value::Bool(command.play_completion_sound),
                );
                item.insert(
                    "showOnProjectRow".to_string(),
                    serde_json::Value::Bool(command.show_on_project_row),
                );
                if let Some(url) = command.url.as_ref() {
                    item.insert("url".to_string(), serde_json::Value::String(url.clone()));
                }
                serde_json::Value::Object(item)
            })
            .collect(),
    )
}

pub(crate) fn gpui_string_array_value(items: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|item| serde_json::Value::String(item.clone()))
            .collect(),
    )
}

