// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the agent/command metadata-write builders and their mutation-params helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;

pub(crate) fn gpui_sidebar_agent_metadata_write_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Result<GpuiSidebarAgentMetadataWrite, String> {
    match command.get("type").and_then(serde_json::Value::as_str) {
        Some("saveSidebarAgent") => {
            let name = gpui_trimmed_json_string_field(command, "name")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            let command_text = gpui_trimmed_json_string_field(command, "command")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            let agent_id = command
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            let icon = command
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_strict_sidebar_agent_icon)
                .map(str::to_string);
            let accept_all_mode = match command.get("acceptAllMode") {
                None => GpuiAgentAcceptAllModeUpdate::Preserve,
                Some(value) => match value.as_str() {
                    Some("inherit") => GpuiAgentAcceptAllModeUpdate::Set(None),
                    Some("enabled") | Some("disabled") => {
                        GpuiAgentAcceptAllModeUpdate::Set(value.as_str().map(str::to_string))
                    }
                    _ => return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
                },
            };
            Ok(GpuiSidebarAgentMetadataWrite::Save {
                accept_all_mode,
                agent_id,
                command: command_text,
                icon,
                name,
            })
        }
        Some("deleteSidebarAgent") => {
            let agent_id = gpui_trimmed_json_string_field(command, "agentId")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            Ok(GpuiSidebarAgentMetadataWrite::Delete { agent_id })
        }
        Some("syncSidebarAgentOrder") => {
            let request_id = command
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let agent_ids = command
                .get("agentIds")
                .and_then(serde_json::Value::as_array)
                .map(|items| gpui_normalized_string_order_from_values(items))
                .unwrap_or_default();
            Ok(GpuiSidebarAgentMetadataWrite::SyncOrder {
                agent_ids,
                request_id,
            })
        }
        _ => Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
    }
}

pub(crate) fn gpui_sidebar_command_metadata_write_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
    active_project_id: Option<String>,
) -> Result<GpuiSidebarCommandMetadataWrite, String> {
    let active_project_id = active_project_id.unwrap_or_default();
    /*
    CDXC:AgentLauncher 2026-08-01-19:00:
    Global and Project Action writes carry the identical payload and differ only
    in which list they land in, so they share one parser and one set of
    validation rules. Parsing them separately is how the two lists would start
    accepting different action shapes.
    */
    let message_type = command.get("type").and_then(serde_json::Value::as_str);
    let scope = match message_type {
        Some("saveGlobalSidebarCommand")
        | Some("deleteGlobalSidebarCommand")
        | Some("syncGlobalSidebarCommandOrder") => GpuiSidebarCommandScope::Global,
        _ => GpuiSidebarCommandScope::Project,
    };
    match message_type {
        Some("saveSidebarCommand") | Some("saveGlobalSidebarCommand") => {
            let name = command
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let icon = command
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_sidebar_command_icon)
                .map(str::to_string);
            if name.is_empty() && icon.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            let action_type = match command
                .get("actionType")
                .and_then(serde_json::Value::as_str)
            {
                Some("browser") => "browser",
                Some("terminal") => "terminal",
                _ => return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
            };
            let terminal_command = command
                .get("command")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            let url = command
                .get("url")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            if action_type == "browser" && url.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            if action_type == "terminal" && terminal_command.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            Ok(GpuiSidebarCommandMetadataWrite::Save {
                action_type,
                active_project_id,
                scope,
                close_terminal_on_exit: action_type == "terminal"
                    && command
                        .get("closeTerminalOnExit")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                command: (action_type == "terminal")
                    .then_some(terminal_command)
                    .flatten(),
                command_id: command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                    .map(str::to_string),
                icon,
                name,
                play_completion_sound: action_type == "terminal"
                    && command
                        .get("playCompletionSound")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                show_on_project_row: command
                    .get("showOnProjectRow")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                url: (action_type == "browser").then_some(url).flatten(),
            })
        }
        Some("deleteSidebarCommand") | Some("deleteGlobalSidebarCommand") => {
            let command_id = gpui_trimmed_json_string_field(command, "commandId")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            Ok(GpuiSidebarCommandMetadataWrite::Delete {
                active_project_id,
                command_id,
                scope,
            })
        }
        Some("syncSidebarCommandOrder") | Some("syncGlobalSidebarCommandOrder") => {
            let request_id = command
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let command_ids = command
                .get("commandIds")
                .and_then(serde_json::Value::as_array)
                .map(|items| gpui_normalized_string_order_from_values(items))
                .unwrap_or_default();
            Ok(GpuiSidebarCommandMetadataWrite::SyncOrder {
                active_project_id,
                command_ids,
                request_id,
                scope,
            })
        }
        _ => Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
    }
}

pub(crate) fn gpui_sidebar_agent_mutation_params(
    write: &GpuiSidebarAgentMetadataWrite,
    active_project_id: Option<String>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "target".to_string(),
        serde_json::Value::String("agent".to_string()),
    );
    if let Some(active_project_id) = active_project_id
        .as_deref()
        .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
    {
        params.insert(
            "activeProjectId".to_string(),
            serde_json::Value::String(active_project_id.to_string()),
        );
    }
    match write {
        GpuiSidebarAgentMetadataWrite::Save {
            accept_all_mode,
            agent_id,
            command,
            icon,
            name,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("save".to_string()),
            );
            gpui_insert_optional_nonempty_string(&mut params, "agentId", agent_id.as_deref());
            params.insert(
                "command".to_string(),
                serde_json::Value::String(command.clone()),
            );
            gpui_insert_optional_nonempty_string(&mut params, "icon", icon.as_deref());
            params.insert("name".to_string(), serde_json::Value::String(name.clone()));
            match accept_all_mode {
                GpuiAgentAcceptAllModeUpdate::Preserve => {}
                GpuiAgentAcceptAllModeUpdate::Set(None) => {
                    params.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String("inherit".to_string()),
                    );
                }
                GpuiAgentAcceptAllModeUpdate::Set(Some(mode)) => {
                    params.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String(mode.clone()),
                    );
                }
            }
        }
        GpuiSidebarAgentMetadataWrite::Delete { agent_id } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("delete".to_string()),
            );
            params.insert(
                "agentId".to_string(),
                serde_json::Value::String(agent_id.clone()),
            );
        }
        GpuiSidebarAgentMetadataWrite::SyncOrder { agent_ids, .. } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("order".to_string()),
            );
            params.insert("agentIds".to_string(), gpui_string_array_value(agent_ids));
        }
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sidebar_command_mutation_params(
    write: &GpuiSidebarCommandMetadataWrite,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "target".to_string(),
        serde_json::Value::String(write.scope().mutation_target().to_string()),
    );
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            action_type,
            active_project_id,
            close_terminal_on_exit,
            command,
            command_id,
            scope: _,
            icon,
            name,
            play_completion_sound,
            show_on_project_row,
            url,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("save".to_string()),
            );
            params.insert(
                "actionType".to_string(),
                serde_json::Value::String((*action_type).to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "closeTerminalOnExit".to_string(),
                serde_json::Value::Bool(*close_terminal_on_exit),
            );
            gpui_insert_optional_nonempty_string(&mut params, "command", command.as_deref());
            gpui_insert_optional_nonempty_string(&mut params, "commandId", command_id.as_deref());
            gpui_insert_optional_nonempty_string(&mut params, "icon", icon.as_deref());
            params.insert("name".to_string(), serde_json::Value::String(name.clone()));
            params.insert(
                "playCompletionSound".to_string(),
                serde_json::Value::Bool(*play_completion_sound),
            );
            params.insert(
                "showOnProjectRow".to_string(),
                serde_json::Value::Bool(*show_on_project_row),
            );
            gpui_insert_optional_nonempty_string(&mut params, "url", url.as_deref());
        }
        GpuiSidebarCommandMetadataWrite::Delete {
            active_project_id,
            command_id,
            scope: _,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("delete".to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "commandId".to_string(),
                serde_json::Value::String(command_id.clone()),
            );
        }
        GpuiSidebarCommandMetadataWrite::SyncOrder {
            active_project_id,
            command_ids,
            ..
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("order".to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "commandIds".to_string(),
                gpui_string_array_value(command_ids),
            );
        }
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sidebar_metadata_mutation_item_ids(result: &serde_json::Value) -> Vec<String> {
    result
        .get("itemIds")
        .and_then(serde_json::Value::as_array)
        .map(|items| gpui_normalized_string_order_from_values(items))
        .unwrap_or_default()
}
