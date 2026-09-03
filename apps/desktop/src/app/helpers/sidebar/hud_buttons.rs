// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the sidebar HUD button state derivation for agents and commands, plus stored-agent/command normalization.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;

#[derive(Clone, Debug)]
pub(crate) struct GpuiSidebarHudButtons {
    pub(crate) agents: serde_json::Value,
    pub(crate) commands: serde_json::Value,
    /*
    CDXC:AgentLauncher 2026-08-01-19:00:
    The Settings app modal reads its Actions lists from this Rust-side HUD fetch,
    not from the sidebar TypeScript runtime, so Global Actions have to be carried
    here too or the new section renders empty no matter what is stored.
    */
    pub(crate) global_commands: serde_json::Value,
}

pub(crate) fn gpui_sidebar_hud_array_field(
    result: &serde_json::Value,
    key: &str,
) -> Result<serde_json::Value, String> {
    result
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .map(serde_json::Value::Array)
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_agent_state_from_domain_projects(
    domain_projects: &[serde_json::Value],
) -> (Vec<GpuiStoredSidebarAgent>, Vec<String>) {
    let source_project = domain_projects.iter().find(|project| {
        let Some(project) = project.as_object() else {
            return false;
        };
        gpui_json_array_field_is_nonempty(project, "customAgents")
            || gpui_json_array_field_is_nonempty(project, "customAgentOrder")
    });
    let source_project = source_project.and_then(serde_json::Value::as_object);
    let stored_agents = gpui_normalized_stored_sidebar_agents(
        source_project.and_then(|project| project.get("customAgents")),
    );
    let stored_order = gpui_normalized_string_order(
        source_project.and_then(|project| project.get("customAgentOrder")),
    );
    (stored_agents, stored_order)
}

pub(crate) fn gpui_sidebar_agent_buttons_from_state(
    stored_agents: &[GpuiStoredSidebarAgent],
    stored_order: &[String],
) -> serde_json::Value {
    let mut buttons = Vec::<(String, serde_json::Value)>::new();
    for default_agent in GPUI_DEFAULT_SIDEBAR_AGENTS {
        let stored_agent = stored_agents
            .iter()
            .find(|agent| agent.agent_id == default_agent.agent_id);
        if stored_agent.is_none() && default_agent.hidden_by_default {
            continue;
        }
        if stored_agent.map(|agent| agent.hidden).unwrap_or(false) {
            continue;
        }

        let button = match stored_agent {
            Some(stored_agent) => {
                let name =
                    gpui_default_sidebar_agent_name(default_agent.agent_id, &stored_agent.name);
                gpui_sidebar_agent_button_value(
                    Some(stored_agent),
                    stored_agent.agent_id.as_str(),
                    stored_agent.command.as_str(),
                    stored_agent.icon.as_deref().unwrap_or(default_agent.icon),
                    true,
                    &name,
                )
            }
            None => gpui_sidebar_agent_button_value(
                None,
                default_agent.agent_id,
                default_agent.command,
                default_agent.icon,
                true,
                default_agent.name,
            ),
        };
        buttons.push((default_agent.agent_id.to_string(), button));
    }

    for stored_agent in stored_agents {
        if gpui_is_default_sidebar_agent_id(&stored_agent.agent_id) || stored_agent.hidden {
            continue;
        }
        let icon = stored_agent.icon.as_deref();
        buttons.push((
            stored_agent.agent_id.clone(),
            gpui_sidebar_agent_button_value(
                Some(stored_agent),
                &stored_agent.agent_id,
                &stored_agent.command,
                icon.unwrap_or(""),
                false,
                &stored_agent.name,
            ),
        ));
    }

    gpui_order_json_buttons(buttons, stored_order, "agentId")
}

pub(crate) fn gpui_sidebar_agent_button_value(
    stored_agent: Option<&GpuiStoredSidebarAgent>,
    agent_id: &str,
    command: &str,
    icon: &str,
    is_default: bool,
    name: &str,
) -> serde_json::Value {
    let mut button = serde_json::Map::new();
    if let Some(accept_all_mode) = stored_agent.and_then(|agent| agent.accept_all_mode.as_ref()) {
        button.insert(
            "acceptAllMode".to_string(),
            serde_json::Value::String(accept_all_mode.clone()),
        );
    }
    button.insert(
        "agentId".to_string(),
        serde_json::Value::String(agent_id.to_string()),
    );
    button.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    if !icon.is_empty() {
        button.insert(
            "icon".to_string(),
            serde_json::Value::String(icon.to_string()),
        );
    }
    button.insert("isDefault".to_string(), serde_json::Value::Bool(is_default));
    button.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    serde_json::Value::Object(button)
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_command_state_for_active_project(
    domain_projects: &[serde_json::Value],
    active_project_id: &str,
) -> Result<
    (
        String,
        Vec<GpuiStoredSidebarCommand>,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let active_project = domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            gpui_trimmed_json_string_field(project, "projectId") == Some(active_project_id)
        })
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    let active_project_id = gpui_trimmed_json_string_field(active_project, "projectId")
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    let owner_project_id = active_project
        .get("worktree")
        .and_then(serde_json::Value::as_object)
        .and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId"))
        .unwrap_or(active_project_id);
    let owner_project = domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            gpui_trimmed_json_string_field(project, "projectId") == Some(owner_project_id)
        })
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    Ok((
        owner_project_id.to_string(),
        gpui_normalized_stored_sidebar_commands(owner_project.get("customCommands")),
        gpui_normalized_string_order(owner_project.get("customCommandOrder")),
        gpui_normalized_string_order(owner_project.get("deletedDefaultCommandIds")),
    ))
}

pub(crate) fn gpui_sidebar_command_buttons_from_state(
    stored_commands: &[GpuiStoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> serde_json::Value {
    let deleted_default_command_ids = deleted_default_command_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut buttons = Vec::<(String, serde_json::Value)>::new();

    for default_command in GPUI_DEFAULT_SIDEBAR_COMMANDS {
        if deleted_default_command_ids.contains(default_command.command_id) {
            continue;
        }
        let button = stored_commands
            .iter()
            .find(|command| command.command_id == default_command.command_id)
            .map(gpui_sidebar_command_button_value)
            .unwrap_or_else(|| gpui_default_sidebar_command_button_value(default_command));
        buttons.push((default_command.command_id.to_string(), button));
    }

    for stored_command in stored_commands {
        if gpui_is_default_sidebar_command_id(&stored_command.command_id) {
            continue;
        }
        buttons.push((
            stored_command.command_id.clone(),
            gpui_sidebar_command_button_value(stored_command),
        ));
    }

    gpui_order_json_buttons(buttons, stored_order, "commandId")
}

pub(crate) fn gpui_default_sidebar_command_button_value(
    command: &GpuiDefaultSidebarCommand,
) -> serde_json::Value {
    serde_json::json!({
        "actionType": "terminal",
        "closeTerminalOnExit": false,
        "commandId": command.command_id,
        "isDefault": true,
        "name": command.name,
        "playCompletionSound": true,
        "showOnProjectRow": false,
    })
}

pub(crate) fn gpui_sidebar_command_button_value(
    command: &GpuiStoredSidebarCommand,
) -> serde_json::Value {
    let mut button = serde_json::Map::new();
    button.insert(
        "actionType".to_string(),
        serde_json::Value::String(command.action_type.to_string()),
    );
    button.insert(
        "closeTerminalOnExit".to_string(),
        serde_json::Value::Bool(command.close_terminal_on_exit),
    );
    if let Some(command_text) = command.command.as_ref() {
        button.insert(
            "command".to_string(),
            serde_json::Value::String(command_text.clone()),
        );
    }
    button.insert(
        "commandId".to_string(),
        serde_json::Value::String(command.command_id.clone()),
    );
    if let Some(icon) = command.icon.as_ref() {
        button.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
    }
    button.insert(
        "isDefault".to_string(),
        serde_json::Value::Bool(command.is_default),
    );
    button.insert(
        "name".to_string(),
        serde_json::Value::String(command.name.clone()),
    );
    button.insert(
        "playCompletionSound".to_string(),
        serde_json::Value::Bool(command.play_completion_sound),
    );
    button.insert(
        "showOnProjectRow".to_string(),
        serde_json::Value::Bool(command.show_on_project_row),
    );
    if let Some(url) = command.url.as_ref() {
        button.insert("url".to_string(), serde_json::Value::String(url.clone()));
    }
    serde_json::Value::Object(button)
}

pub(crate) fn gpui_normalized_stored_sidebar_agents(
    candidate: Option<&serde_json::Value>,
) -> Vec<GpuiStoredSidebarAgent> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut agents = Vec::new();
    let mut seen_agent_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(agent_id) = gpui_trimmed_json_string_field(item, "agentId") else {
            continue;
        };
        if seen_agent_ids.contains(agent_id) {
            continue;
        }
        let Some(name) = gpui_trimmed_json_string_field(item, "name") else {
            continue;
        };
        let Some(command) = gpui_trimmed_json_string_field(item, "command") else {
            continue;
        };
        agents.push(GpuiStoredSidebarAgent {
            accept_all_mode: item
                .get("acceptAllMode")
                .and_then(serde_json::Value::as_str)
                .filter(|mode| matches!(*mode, "inherit" | "enabled" | "disabled"))
                .map(str::to_string),
            agent_id: agent_id.to_string(),
            command: command.to_string(),
            hidden: item
                .get("hidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            icon: item
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_strict_sidebar_agent_icon)
                .map(str::to_string),
            name: name.to_string(),
        });
        seen_agent_ids.insert(agent_id.to_string());
    }
    agents
}

pub(crate) fn gpui_normalized_stored_sidebar_commands(
    candidate: Option<&serde_json::Value>,
) -> Vec<GpuiStoredSidebarCommand> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    let mut seen_command_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(command_id) = gpui_trimmed_json_string_field(item, "commandId") else {
            continue;
        };
        if seen_command_ids.contains(command_id) {
            continue;
        }
        let url = item
            .get("url")
            .and_then(serde_json::Value::as_str)
            .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
            .map(str::to_string);
        let action_type = match item.get("actionType").and_then(serde_json::Value::as_str) {
            Some("browser") => "browser",
            Some("terminal") => "terminal",
            _ if url.is_some() => "browser",
            _ => "terminal",
        };
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let icon = item
            .get("icon")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sidebar_command_icon)
            .map(str::to_string);
        let is_default = item
            .get("isDefault")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || gpui_is_default_sidebar_command_id(command_id);

        let show_on_project_row = item
            .get("showOnProjectRow")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if action_type == "browser" {
            let Some(url) = url else {
                continue;
            };
            commands.push(GpuiStoredSidebarCommand {
                action_type,
                close_terminal_on_exit: false,
                command: None,
                command_id: command_id.to_string(),
                icon,
                is_default,
                name,
                play_completion_sound: false,
                show_on_project_row,
                url: Some(url),
            });
            seen_command_ids.insert(command_id.to_string());
            continue;
        }

        let Some(command_text) = item
            .get("command")
            .and_then(serde_json::Value::as_str)
            .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        else {
            continue;
        };
        commands.push(GpuiStoredSidebarCommand {
            action_type,
            close_terminal_on_exit: item
                .get("closeTerminalOnExit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            command: Some(command_text.to_string()),
            command_id: command_id.to_string(),
            icon,
            is_default,
            name,
            play_completion_sound: item
                .get("playCompletionSound")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            show_on_project_row,
            url: None,
        });
        seen_command_ids.insert(command_id.to_string());
    }
    commands
}

pub(crate) fn gpui_normalized_string_order(candidate: Option<&serde_json::Value>) -> Vec<String> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    gpui_normalized_string_order_from_values(items)
}

pub(crate) fn gpui_normalized_string_order_from_values(items: &[serde_json::Value]) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen_ids = HashSet::new();
    for item in items {
        let Some(item) = item
            .as_str()
            .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
        else {
            continue;
        };
        if seen_ids.insert(item.to_string()) {
            order.push(item.to_string());
        }
    }
    order
}
