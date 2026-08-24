// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds the agent/command metadata-write apply and next-state computation functions.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;

pub(crate) fn gpui_apply_sidebar_agent_metadata_write(
    write: GpuiSidebarAgentMetadataWrite,
    active_project_id: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    let params = gpui_sidebar_agent_mutation_params(&write, active_project_id);
    let result = gpui_gxserver_rpc_result(
        "/api/mutateSidebarHudSettings",
        &params,
        Duration::from_secs(10),
    )?;
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarAgentMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("agent", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_next_sidebar_agent_metadata_state(
    stored_agents: Vec<GpuiStoredSidebarAgent>,
    stored_order: Vec<String>,
    write: GpuiSidebarAgentMetadataWrite,
) -> Result<
    (
        Vec<GpuiStoredSidebarAgent>,
        Vec<String>,
        Option<serde_json::Value>,
    ),
    String,
> {
    match write {
        GpuiSidebarAgentMetadataWrite::Save {
            accept_all_mode,
            agent_id,
            command,
            icon,
            name,
        } => {
            let current_agent_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
                "agentId",
            );
            let selected_default_agent_id = icon
                .as_deref()
                .and_then(gpui_default_sidebar_agent_by_icon)
                .map(|agent| agent.agent_id);
            let should_restore_hidden_default = agent_id.is_none()
                && selected_default_agent_id
                    .map(|agent_id| !gpui_is_sidebar_agent_visible(&stored_agents, agent_id))
                    .unwrap_or(false);
            let next_agent_id = agent_id
                .or_else(|| {
                    should_restore_hidden_default
                        .then_some(selected_default_agent_id)
                        .flatten()
                        .map(str::to_string)
                })
                .unwrap_or_else(|| gpui_create_custom_sidebar_agent_id(&name));
            let existing_index = stored_agents
                .iter()
                .position(|agent| agent.agent_id == next_agent_id);
            let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
            let default_agent = gpui_default_sidebar_agent_by_id(&next_agent_id);
            let next_agent = GpuiStoredSidebarAgent {
                accept_all_mode: match accept_all_mode {
                    GpuiAgentAcceptAllModeUpdate::Preserve => previous_agent
                        .and_then(|agent| agent.accept_all_mode.as_ref())
                        .cloned(),
                    GpuiAgentAcceptAllModeUpdate::Set(mode) => mode,
                },
                agent_id: next_agent_id.clone(),
                command,
                hidden: false,
                icon: icon
                    .or_else(|| {
                        previous_agent
                            .and_then(|agent| agent.icon.as_ref())
                            .cloned()
                    })
                    .or_else(|| default_agent.map(|agent| agent.icon.to_string())),
                name,
            };
            let mut next_agents = stored_agents.clone();
            if let Some(existing_index) = existing_index {
                next_agents[existing_index] = next_agent;
            } else {
                next_agents.push(next_agent);
            }
            let next_order = if existing_index.is_some()
                || stored_order
                    .iter()
                    .any(|candidate| candidate == &next_agent_id)
                || gpui_is_default_sidebar_agent_id(&next_agent_id)
            {
                stored_order
            } else {
                let mut next_order = current_agent_ids;
                next_order.push(next_agent_id);
                next_order
            };
            Ok((next_agents, next_order, None))
        }
        GpuiSidebarAgentMetadataWrite::Delete { agent_id } => {
            if !gpui_is_default_sidebar_agent_id(&agent_id) {
                let next_agents = stored_agents
                    .into_iter()
                    .filter(|agent| agent.agent_id != agent_id)
                    .collect::<Vec<_>>();
                let next_order = stored_order
                    .into_iter()
                    .filter(|candidate| candidate != &agent_id)
                    .collect::<Vec<_>>();
                return Ok((next_agents, next_order, None));
            }
            let Some(default_agent) = gpui_default_sidebar_agent_by_id(&agent_id) else {
                return Ok((stored_agents, stored_order, None));
            };
            let existing_index = stored_agents
                .iter()
                .position(|agent| agent.agent_id == agent_id);
            let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
            let next_agent = GpuiStoredSidebarAgent {
                accept_all_mode: None,
                agent_id: default_agent.agent_id.to_string(),
                command: previous_agent
                    .map(|agent| agent.command.clone())
                    .unwrap_or_else(|| default_agent.command.to_string()),
                hidden: true,
                icon: previous_agent
                    .and_then(|agent| agent.icon.as_ref())
                    .cloned()
                    .or_else(|| Some(default_agent.icon.to_string())),
                name: previous_agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| default_agent.name.to_string()),
            };
            let mut next_agents = stored_agents.clone();
            if let Some(existing_index) = existing_index {
                next_agents[existing_index] = next_agent;
            } else {
                next_agents.push(next_agent);
            }
            let next_order = stored_order
                .into_iter()
                .filter(|candidate| candidate != &agent_id)
                .collect::<Vec<_>>();
            Ok((next_agents, next_order, None))
        }
        GpuiSidebarAgentMetadataWrite::SyncOrder {
            agent_ids,
            request_id,
        } => {
            let current_agent_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
                "agentId",
            );
            let mut next_order = agent_ids
                .into_iter()
                .filter(|agent_id| {
                    current_agent_ids
                        .iter()
                        .any(|candidate| candidate == agent_id)
                })
                .collect::<Vec<_>>();
            for agent_id in current_agent_ids {
                if !next_order.iter().any(|candidate| candidate == &agent_id) {
                    next_order.push(agent_id);
                }
            }
            let item_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &next_order),
                "agentId",
            );
            Ok((
                stored_agents,
                next_order,
                Some(gpui_sidebar_order_sync_result_message(
                    "agent",
                    &request_id,
                    "success",
                    item_ids,
                )),
            ))
        }
    }
}

pub(crate) fn gpui_apply_sidebar_command_metadata_write(
    write: GpuiSidebarCommandMetadataWrite,
) -> Result<Option<serde_json::Value>, String> {
    let params = gpui_sidebar_command_mutation_params(&write);
    let result = gpui_gxserver_rpc_result(
        "/api/mutateSidebarHudSettings",
        &params,
        Duration::from_secs(10),
    )?;
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarCommandMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("command", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_command_write_active_project_id(
    write: &GpuiSidebarCommandMetadataWrite,
) -> &str {
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::Delete {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::SyncOrder {
            active_project_id, ..
        } => active_project_id,
    }
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_next_sidebar_command_metadata_state(
    stored_commands: Vec<GpuiStoredSidebarCommand>,
    stored_order: Vec<String>,
    deleted_default_command_ids: Vec<String>,
    write: GpuiSidebarCommandMetadataWrite,
) -> Result<
    (
        Vec<GpuiStoredSidebarCommand>,
        Vec<String>,
        Vec<String>,
        Option<serde_json::Value>,
    ),
    String,
> {
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            action_type,
            close_terminal_on_exit,
            command,
            command_id,
            icon,
            name,
            play_completion_sound,
            show_on_project_row,
            url,
            ..
        } => {
            let current_command_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &stored_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            let next_command_id = command_id.unwrap_or_else(gpui_create_custom_sidebar_command_id);
            let next_command = GpuiStoredSidebarCommand {
                action_type,
                close_terminal_on_exit: action_type == "terminal" && close_terminal_on_exit,
                command: (action_type == "terminal").then_some(command).flatten(),
                command_id: next_command_id.clone(),
                icon,
                is_default: gpui_is_default_sidebar_command_id(&next_command_id),
                name,
                play_completion_sound: action_type == "terminal" && play_completion_sound,
                show_on_project_row,
                url: (action_type == "browser").then_some(url).flatten(),
            };
            gpui_reject_duplicate_sidebar_command_title(
                &next_command,
                &stored_commands,
                &stored_order,
                &deleted_default_command_ids,
            )?;
            let existing_index = stored_commands
                .iter()
                .position(|command| command.command_id == next_command_id);
            let mut next_commands = stored_commands.clone();
            if let Some(existing_index) = existing_index {
                next_commands[existing_index] = next_command;
            } else {
                next_commands.push(next_command);
            }
            let next_order = if existing_index.is_some()
                || stored_order
                    .iter()
                    .any(|candidate| candidate == &next_command_id)
                || gpui_is_default_sidebar_command_id(&next_command_id)
            {
                stored_order
            } else if current_command_ids
                .iter()
                .any(|candidate| candidate == &next_command_id)
            {
                current_command_ids
            } else {
                let mut next_order = current_command_ids;
                next_order.push(next_command_id.clone());
                next_order
            };
            let next_deleted_default_ids = if gpui_is_default_sidebar_command_id(&next_command_id) {
                deleted_default_command_ids
                    .into_iter()
                    .filter(|candidate| candidate != &next_command_id)
                    .collect::<Vec<_>>()
            } else {
                deleted_default_command_ids
            };
            Ok((next_commands, next_order, next_deleted_default_ids, None))
        }
        GpuiSidebarCommandMetadataWrite::Delete { command_id, .. } => {
            let next_commands = stored_commands
                .into_iter()
                .filter(|command| command.command_id != command_id)
                .collect::<Vec<_>>();
            let next_order = stored_order
                .into_iter()
                .filter(|candidate| candidate != &command_id)
                .collect::<Vec<_>>();
            let mut next_deleted_default_ids = deleted_default_command_ids;
            if gpui_is_default_sidebar_command_id(&command_id)
                && !next_deleted_default_ids
                    .iter()
                    .any(|candidate| candidate == &command_id)
            {
                next_deleted_default_ids.push(command_id);
            }
            Ok((next_commands, next_order, next_deleted_default_ids, None))
        }
        GpuiSidebarCommandMetadataWrite::SyncOrder {
            command_ids,
            request_id,
            ..
        } => {
            let current_command_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &stored_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            let mut next_order = command_ids
                .into_iter()
                .filter(|command_id| {
                    current_command_ids
                        .iter()
                        .any(|candidate| candidate == command_id)
                })
                .collect::<Vec<_>>();
            for command_id in current_command_ids {
                if !next_order.iter().any(|candidate| candidate == &command_id) {
                    next_order.push(command_id);
                }
            }
            let item_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &next_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            Ok((
                stored_commands,
                next_order,
                deleted_default_command_ids,
                Some(gpui_sidebar_order_sync_result_message(
                    "command",
                    &request_id,
                    "success",
                    item_ids,
                )),
            ))
        }
    }
}
