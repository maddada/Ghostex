use super::*;

pub fn read_sidebar_hud(projects: &[Value], active_project_id: Option<&str>) -> Value {
    let agents = sidebar_agent_buttons_from_projects(projects);
    let commands = sidebar_command_buttons_from_projects(projects, active_project_id);
    let mut payload = Map::new();
    payload.insert("agents".to_string(), agents);
    payload.insert("commands".to_string(), commands);
    Value::Object(payload)
}

/*
CDXC:AgentLauncher 2026-07-12-00:00:
Mobile clients render quick-action rows for every visible project in one list,
so the CLI transport needs per-project command buttons in a single response.
Reuse the exact active-project command resolution per project id; parked
Recent Projects and hidden/system projects never contribute rows.
*/
pub fn read_sidebar_hud_commands_by_project(projects: &[Value]) -> Value {
    let mut commands_by_project = Map::new();
    for project in projects.iter().filter_map(Value::as_object) {
        if is_explicit_recent_project(project) {
            continue;
        }
        if trimmed_json_string_field(project, "visibility") == Some("hidden")
            || trimmed_json_string_field(project, "systemKind") == Some("remoteAttachCarrier")
        {
            continue;
        }
        let Some(project_id) = trimmed_json_string_field(project, "projectId") else {
            continue;
        };
        commands_by_project.insert(
            project_id.to_string(),
            sidebar_command_buttons_from_projects(projects, Some(project_id)),
        );
    }
    Value::Object(commands_by_project)
}

/*
CDXC:AgentLauncher 2026-08-01-16:00:
Global Actions are normalized through the same stored-command projection as
Project Actions, minus the defaults branch: there are no built-in global actions
to resurrect or tombstone, so the stored rows are the whole list. Rows arrive
from the repository already in sortOrder.
*/
pub fn read_sidebar_hud_global_commands(stored_definitions: &[Value]) -> Value {
    let stored_commands =
        normalized_stored_sidebar_commands(Some(&Value::Array(stored_definitions.to_vec())));
    Value::Array(
        stored_commands
            .iter()
            .map(sidebar_command_button_value)
            .collect(),
    )
}

pub(crate) fn sidebar_agent_buttons_from_projects(projects: &[Value]) -> Value {
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    sidebar_agent_buttons_from_state(&stored_agents, &stored_order)
}

pub(super) fn sidebar_agent_state_from_projects(
    projects: &[Value],
) -> (Vec<StoredSidebarAgent>, Vec<String>) {
    let source_project = projects.iter().find_map(|project| {
        let project = project.as_object()?;
        (json_array_field_is_nonempty(project, "customAgents")
            || json_array_field_is_nonempty(project, "customAgentOrder"))
        .then_some(project)
    });
    let stored_agents = normalized_stored_sidebar_agents(
        source_project.and_then(|project| project.get("customAgents")),
    );
    let stored_order =
        normalized_string_order(source_project.and_then(|project| project.get("customAgentOrder")));
    (stored_agents, stored_order)
}

pub(super) fn sidebar_agent_buttons_from_state(
    stored_agents: &[StoredSidebarAgent],
    stored_order: &[String],
) -> Value {
    let mut buttons = Vec::<(String, Value)>::new();
    for default_agent in DEFAULT_SIDEBAR_AGENTS {
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
                let name = default_sidebar_agent_name(default_agent.agent_id, &stored_agent.name);
                sidebar_agent_button_value(
                    Some(stored_agent),
                    stored_agent.agent_id.as_str(),
                    stored_agent.command.as_str(),
                    stored_agent.icon.as_deref().unwrap_or(default_agent.icon),
                    true,
                    &name,
                )
            }
            None => sidebar_agent_button_value(
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
        if is_default_sidebar_agent_id(&stored_agent.agent_id) || stored_agent.hidden {
            continue;
        }
        buttons.push((
            stored_agent.agent_id.clone(),
            sidebar_agent_button_value(
                Some(stored_agent),
                &stored_agent.agent_id,
                &stored_agent.command,
                stored_agent.icon.as_deref().unwrap_or(""),
                false,
                &stored_agent.name,
            ),
        ));
    }

    order_json_buttons(buttons, stored_order, "agentId")
}

fn sidebar_agent_button_value(
    stored_agent: Option<&StoredSidebarAgent>,
    agent_id: &str,
    command: &str,
    icon: &str,
    is_default: bool,
    name: &str,
) -> Value {
    let mut button = Map::new();
    if let Some(accept_all_mode) = stored_agent.and_then(|agent| agent.accept_all_mode.as_ref()) {
        button.insert(
            "acceptAllMode".to_string(),
            Value::String(accept_all_mode.clone()),
        );
    }
    button.insert("agentId".to_string(), Value::String(agent_id.to_string()));
    button.insert("command".to_string(), Value::String(command.to_string()));
    if !icon.is_empty() {
        button.insert("icon".to_string(), Value::String(icon.to_string()));
    }
    button.insert("isDefault".to_string(), Value::Bool(is_default));
    button.insert("name".to_string(), Value::String(name.to_string()));
    // Read by the launcher and the New Thread picker, which then offer this agent no account.
    if crate::accounts::launch::agent_uses_own_login(
        agent_id,
        (!icon.is_empty()).then_some(icon),
        command,
    ) {
        button.insert("usesOwnLogin".to_string(), Value::Bool(true));
    }
    Value::Object(button)
}

fn sidebar_command_buttons_from_projects(
    projects: &[Value],
    active_project_id: Option<&str>,
) -> Value {
    let active_project = if let Some(active_project_id) = active_project_id {
        normal_project_by_id(projects, active_project_id)
    } else {
        first_normal_project(projects)
    };
    let Some(active_project) = active_project else {
        return sidebar_command_buttons_from_state(&[], &[], &[]);
    };
    let Some(project_id) = trimmed_json_string_field(active_project, "projectId") else {
        return sidebar_command_buttons_from_state(&[], &[], &[]);
    };
    let owner_project_id = active_project
        .get("worktree")
        .and_then(Value::as_object)
        .and_then(|worktree| trimmed_json_string_field(worktree, "parentProjectId"))
        .unwrap_or(project_id);
    let source_project = normal_project_by_id(projects, owner_project_id).unwrap_or(active_project);
    let stored_commands = normalized_stored_sidebar_commands(source_project.get("customCommands"));
    let stored_order = normalized_string_order(source_project.get("customCommandOrder"));
    let deleted_default_command_ids =
        normalized_string_order(source_project.get("deletedDefaultCommandIds"));
    sidebar_command_buttons_from_state(
        &stored_commands,
        &stored_order,
        &deleted_default_command_ids,
    )
}

pub(super) fn sidebar_command_buttons_from_state(
    stored_commands: &[StoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> Value {
    let deleted_default_command_ids = deleted_default_command_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut buttons = Vec::<(String, Value)>::new();

    for default_command in DEFAULT_SIDEBAR_COMMANDS {
        if deleted_default_command_ids.contains(default_command.command_id) {
            continue;
        }
        let button = stored_commands
            .iter()
            .find(|command| command.command_id == default_command.command_id)
            .map(sidebar_command_button_value)
            .unwrap_or_else(|| default_sidebar_command_button_value(default_command));
        buttons.push((default_command.command_id.to_string(), button));
    }

    for stored_command in stored_commands {
        if is_default_sidebar_command_id(&stored_command.command_id) {
            continue;
        }
        buttons.push((
            stored_command.command_id.clone(),
            sidebar_command_button_value(stored_command),
        ));
    }

    order_json_buttons(buttons, stored_order, "commandId")
}

pub(super) struct SidebarCommandScope<'a> {
    hud_active_project_id: Option<String>,
    pub(super) owner_project: &'a Map<String, Value>,
    owner_project_id: String,
}

pub(super) struct SidebarCommandState {
    pub(super) commands: Vec<StoredSidebarCommand>,
    pub(super) deleted_default_command_ids: Vec<String>,
    pub(super) order: Vec<String>,
}

pub(super) fn sidebar_command_scope<'a>(
    projects: &'a [Value],
    params: &Map<String, Value>,
) -> Result<SidebarCommandScope<'a>, DomainStateError> {
    let requested_active_project_id = optional_trimmed_param(params, "activeProjectId");
    let active_project = match requested_active_project_id.as_deref() {
        Some(active_project_id) => normal_project_by_id(projects, active_project_id),
        None => first_normal_project(projects),
    }
    .ok_or_else(|| {
        DomainStateError::bad_request("No active project is available for sidebar action mutation.")
    })?;
    let active_project_id =
        trimmed_json_string_field(active_project, "projectId").ok_or_else(|| {
            DomainStateError::corrupt_state("Active project metadata is missing a project ID.")
        })?;
    let owner_project_id = active_project
        .get("worktree")
        .and_then(Value::as_object)
        .and_then(|worktree| trimmed_json_string_field(worktree, "parentProjectId"))
        .unwrap_or(active_project_id);
    let owner_project = normal_project_by_id(projects, owner_project_id).unwrap_or(active_project);
    let owner_project_id =
        trimmed_json_string_field(owner_project, "projectId").ok_or_else(|| {
            DomainStateError::corrupt_state("Sidebar action owner project is missing a project ID.")
        })?;
    Ok(SidebarCommandScope {
        hud_active_project_id: requested_active_project_id,
        owner_project,
        owner_project_id: owner_project_id.to_string(),
    })
}

pub(super) fn sidebar_command_state(project: &Map<String, Value>) -> SidebarCommandState {
    SidebarCommandState {
        commands: normalized_stored_sidebar_commands(project.get("customCommands")),
        deleted_default_command_ids: normalized_string_order(
            project.get("deletedDefaultCommandIds"),
        ),
        order: normalized_string_order(project.get("customCommandOrder")),
    }
}

pub(super) fn sidebar_command_project_mutation(
    command_scope: SidebarCommandScope<'_>,
    state: SidebarCommandState,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let mut update = Map::new();
    update.insert(
        "projectId".to_string(),
        Value::String(command_scope.owner_project_id.clone()),
    );
    update.insert(
        "customCommands".to_string(),
        stored_sidebar_commands_value(&state.commands),
    );
    update.insert(
        "customCommandOrder".to_string(),
        string_array_value(&state.order),
    );
    update.insert(
        "deletedDefaultCommandIds".to_string(),
        string_array_value(&state.deleted_default_command_ids),
    );
    Ok(SidebarHudSettingsMutation {
        global_command_update: None,
        hud_active_project_id: command_scope.hud_active_project_id,
        item_ids: None,
        updates: vec![SidebarHudProjectMutation {
            params: update,
            project_id: command_scope.owner_project_id,
        }],
    })
}

fn default_sidebar_command_button_value(command: &DefaultSidebarCommand) -> Value {
    let mut button = Map::new();
    button.insert(
        "actionType".to_string(),
        Value::String("terminal".to_string()),
    );
    button.insert("closeTerminalOnExit".to_string(), Value::Bool(false));
    button.insert(
        "commandId".to_string(),
        Value::String(command.command_id.to_string()),
    );
    button.insert("isDefault".to_string(), Value::Bool(true));
    button.insert("name".to_string(), Value::String(command.name.to_string()));
    button.insert("playCompletionSound".to_string(), Value::Bool(true));
    button.insert("showOnProjectRow".to_string(), Value::Bool(false));
    Value::Object(button)
}

pub(super) fn sidebar_command_button_value(command: &StoredSidebarCommand) -> Value {
    let mut button = Map::new();
    button.insert(
        "actionType".to_string(),
        Value::String(command.action_type.to_string()),
    );
    button.insert(
        "closeTerminalOnExit".to_string(),
        Value::Bool(command.close_terminal_on_exit),
    );
    if let Some(command_text) = command.command.as_ref() {
        button.insert("command".to_string(), Value::String(command_text.clone()));
    }
    button.insert(
        "commandId".to_string(),
        Value::String(command.command_id.clone()),
    );
    if let Some(icon) = command.icon.as_ref() {
        button.insert("icon".to_string(), Value::String(icon.clone()));
    }
    button.insert("isDefault".to_string(), Value::Bool(command.is_default));
    if !command.links.is_empty() {
        button.insert(
            "links".to_string(),
            sidebar_command_links_value(&command.links),
        );
    }
    button.insert("name".to_string(), Value::String(command.name.clone()));
    button.insert(
        "playCompletionSound".to_string(),
        Value::Bool(command.play_completion_sound),
    );
    button.insert(
        "showOnProjectRow".to_string(),
        Value::Bool(command.show_on_project_row),
    );
    if let Some(url) = command.url.as_ref() {
        button.insert("url".to_string(), Value::String(url.clone()));
    }
    Value::Object(button)
}

/// Settings > Agents' stored list (`customAgents` rows as a project holds them) and its order,
/// for settings sync (server/src/settings_sync/lists_bridge.rs).
pub(crate) fn synced_agent_roster(projects: &[Value]) -> (Vec<Value>, Vec<String>) {
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    let agents = match stored_sidebar_agents_value(&stored_agents) {
        Value::Array(agents) => agents,
        _ => Vec::new(),
    };
    (agents, stored_order)
}

/// The project updates that store `agents` (normalized as a Settings save would) and `order` on
/// every project, as an agent mutation writes them.
pub(crate) fn synced_agent_roster_updates(
    projects: &[Value],
    agents: &[Value],
    order: &[String],
) -> Vec<serde_json::Map<String, Value>> {
    let normalized = normalized_stored_sidebar_agents(Some(&Value::Array(agents.to_vec())));
    let custom_agents = stored_sidebar_agents_value(&normalized);
    let custom_agent_order = Value::Array(order.iter().cloned().map(Value::String).collect());
    projects
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|project| {
            let project_id = trimmed_json_string_field(project, "projectId")?;
            let mut update = serde_json::Map::new();
            update.insert(
                "projectId".to_string(),
                Value::String(project_id.to_string()),
            );
            update.insert("customAgents".to_string(), custom_agents.clone());
            update.insert("customAgentOrder".to_string(), custom_agent_order.clone());
            Some(update)
        })
        .collect()
}
