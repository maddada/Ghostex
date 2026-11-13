use super::*;

pub(super) fn normalized_stored_sidebar_agents(
    candidate: Option<&Value>,
) -> Vec<StoredSidebarAgent> {
    let Some(items) = candidate.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut agents = Vec::new();
    let mut seen_agent_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(agent_id) = trimmed_json_string_field(item, "agentId") else {
            continue;
        };
        // `gemini`: Gemini CLI was dropped on 2026-11-13 (storage migration 0078, which says why);
        // a row synced from an older computer stays dropped too.
        if agent_id == "campfire" || agent_id == "gemini" || seen_agent_ids.contains(agent_id) {
            continue;
        }
        let Some(name) = trimmed_json_string_field(item, "name") else {
            continue;
        };
        let Some(command) = trimmed_json_string_field(item, "command") else {
            continue;
        };
        agents.push(StoredSidebarAgent {
            accept_all_mode: item
                .get("acceptAllMode")
                .and_then(Value::as_str)
                .filter(|mode| matches!(*mode, "inherit" | "enabled" | "disabled"))
                .map(str::to_string),
            agent_id: agent_id.to_string(),
            command: command.to_string(),
            hidden: item.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            icon: item
                .get("icon")
                .and_then(Value::as_str)
                .and_then(strict_sidebar_agent_icon)
                .map(str::to_string),
            name: name.to_string(),
        });
        seen_agent_ids.insert(agent_id.to_string());
    }
    agents
}

pub(super) fn normalized_stored_sidebar_commands(
    candidate: Option<&Value>,
) -> Vec<StoredSidebarCommand> {
    let Some(items) = candidate.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    let mut seen_command_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(command_id) = trimmed_json_string_field(item, "commandId") else {
            continue;
        };
        if seen_command_ids.contains(command_id) {
            continue;
        }
        let url = item
            .get("url")
            .and_then(Value::as_str)
            .and_then(|url| trimmed_nonempty_str(Some(url)))
            .map(str::to_string);
        let action_type = match item.get("actionType").and_then(Value::as_str) {
            Some("browser") => "browser",
            Some("terminal") => "terminal",
            _ if url.is_some() => "browser",
            _ => "terminal",
        };
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let icon = item
            .get("icon")
            .and_then(Value::as_str)
            .and_then(sidebar_command_icon)
            .map(str::to_string);
        let is_default = item
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || is_default_sidebar_command_id(command_id);

        let show_on_project_row = item
            .get("showOnProjectRow")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        if action_type == "browser" {
            let Some(url) = url else {
                continue;
            };
            commands.push(StoredSidebarCommand {
                action_type,
                close_terminal_on_exit: false,
                command: None,
                command_id: command_id.to_string(),
                icon,
                is_default,
                links: Vec::new(),
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
            .and_then(Value::as_str)
            .and_then(|command| trimmed_nonempty_str(Some(command)))
        else {
            continue;
        };
        commands.push(StoredSidebarCommand {
            action_type,
            close_terminal_on_exit: item
                .get("closeTerminalOnExit")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            command: Some(command_text.to_string()),
            command_id: command_id.to_string(),
            icon,
            is_default,
            links: normalized_sidebar_command_links(item.get("links")),
            name,
            play_completion_sound: item
                .get("playCompletionSound")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            show_on_project_row,
            url: None,
        });
        seen_command_ids.insert(command_id.to_string());
    }
    commands
}

pub(super) fn normalized_sidebar_command_links(
    candidate: Option<&Value>,
) -> Vec<StoredSidebarCommandLink> {
    let Some(items) = candidate.and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|item| {
            let url = item
                .get("url")
                .and_then(Value::as_str)
                .and_then(|url| trimmed_nonempty_str(Some(url)))?;
            Some(StoredSidebarCommandLink {
                target: match item.get("target").and_then(Value::as_str) {
                    Some("external") => "external",
                    _ => "integrated",
                },
                url: url.to_string(),
            })
        })
        .collect()
}

pub(super) fn sidebar_command_links_value(links: &[StoredSidebarCommandLink]) -> Value {
    Value::Array(
        links
            .iter()
            .map(|link| {
                let mut item = Map::new();
                item.insert("target".to_string(), Value::String(link.target.to_string()));
                item.insert("url".to_string(), Value::String(link.url.clone()));
                Value::Object(item)
            })
            .collect(),
    )
}

pub(super) fn normalized_string_order(candidate: Option<&Value>) -> Vec<String> {
    let Some(items) = candidate.and_then(Value::as_array) else {
        return Vec::new();
    };
    normalized_string_order_from_values(items)
}

fn normalized_string_order_from_values(items: &[Value]) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen_ids = HashSet::new();
    for item in items {
        let Some(item) = item
            .as_str()
            .and_then(|value| trimmed_nonempty_str(Some(value)))
        else {
            continue;
        };
        if seen_ids.insert(item.to_string()) {
            order.push(item.to_string());
        }
    }
    order
}

pub(super) fn order_json_buttons(
    buttons: Vec<(String, Value)>,
    stored_order: &[String],
    id_key: &str,
) -> Value {
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
            .and_then(Value::as_str)
            .unwrap_or(button_id.as_str());
        if used_ids.insert(actual_id.to_string()) {
            ordered_buttons.push(button);
        }
    }
    Value::Array(ordered_buttons)
}

pub(super) fn sidebar_button_ids(buttons: &Value, id_key: &str) -> Vec<String> {
    buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|button| trimmed_json_string_field(button, id_key))
        .map(str::to_string)
        .collect()
}

pub(super) fn required_trimmed_param(
    params: &Map<String, Value>,
    key: &str,
) -> Result<String, DomainStateError> {
    optional_trimmed_param(params, key)
        .ok_or_else(|| DomainStateError::bad_request("Invalid sidebar Settings mutation payload."))
}

pub(super) fn optional_trimmed_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| trimmed_nonempty_str(Some(value)))
        .map(str::to_string)
}

pub(super) fn sidebar_agent_accept_all_mode_update(
    params: &Map<String, Value>,
) -> Result<SidebarAgentAcceptAllModeUpdate, DomainStateError> {
    match params.get("acceptAllMode") {
        None => Ok(SidebarAgentAcceptAllModeUpdate::Preserve),
        Some(Value::String(value)) if value == "inherit" => {
            Ok(SidebarAgentAcceptAllModeUpdate::Set(None))
        }
        Some(Value::String(value)) if value == "enabled" || value == "disabled" => Ok(
            SidebarAgentAcceptAllModeUpdate::Set(Some(value.to_string())),
        ),
        _ => Err(DomainStateError::bad_request(
            "Unsupported sidebar agent accept-all mode.",
        )),
    }
}

pub(super) fn stored_sidebar_agents_value(agents: &[StoredSidebarAgent]) -> Value {
    Value::Array(
        agents
            .iter()
            .map(|agent| {
                let mut item = Map::new();
                if let Some(accept_all_mode) = agent.accept_all_mode.as_ref() {
                    item.insert(
                        "acceptAllMode".to_string(),
                        Value::String(accept_all_mode.clone()),
                    );
                }
                item.insert("agentId".to_string(), Value::String(agent.agent_id.clone()));
                item.insert("command".to_string(), Value::String(agent.command.clone()));
                item.insert("hidden".to_string(), Value::Bool(agent.hidden));
                if let Some(icon) = agent.icon.as_ref() {
                    item.insert("icon".to_string(), Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    Value::Bool(is_default_sidebar_agent_id(&agent.agent_id)),
                );
                item.insert("name".to_string(), Value::String(agent.name.clone()));
                Value::Object(item)
            })
            .collect(),
    )
}

pub(super) fn stored_sidebar_commands_value(commands: &[StoredSidebarCommand]) -> Value {
    Value::Array(
        commands
            .iter()
            .map(|command| {
                let mut item = Map::new();
                item.insert(
                    "actionType".to_string(),
                    Value::String(command.action_type.to_string()),
                );
                item.insert(
                    "closeTerminalOnExit".to_string(),
                    Value::Bool(command.close_terminal_on_exit),
                );
                if let Some(command_text) = command.command.as_ref() {
                    item.insert("command".to_string(), Value::String(command_text.clone()));
                }
                item.insert(
                    "commandId".to_string(),
                    Value::String(command.command_id.clone()),
                );
                if let Some(icon) = command.icon.as_ref() {
                    item.insert("icon".to_string(), Value::String(icon.clone()));
                }
                item.insert("isDefault".to_string(), Value::Bool(command.is_default));
                if !command.links.is_empty() {
                    item.insert(
                        "links".to_string(),
                        sidebar_command_links_value(&command.links),
                    );
                }
                item.insert("name".to_string(), Value::String(command.name.clone()));
                item.insert(
                    "playCompletionSound".to_string(),
                    Value::Bool(command.play_completion_sound),
                );
                item.insert(
                    "showOnProjectRow".to_string(),
                    Value::Bool(command.show_on_project_row),
                );
                if let Some(url) = command.url.as_ref() {
                    item.insert("url".to_string(), Value::String(url.clone()));
                }
                Value::Object(item)
            })
            .collect(),
    )
}

pub(super) fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

pub(super) fn json_array_field_is_nonempty(object: &Map<String, Value>, key: &str) -> bool {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

pub(super) fn first_normal_project(projects: &[Value]) -> Option<&Map<String, Value>> {
    projects
        .iter()
        .filter_map(Value::as_object)
        .find(|project| !is_explicit_recent_project(project))
}

pub(super) fn normal_project_by_id<'a>(
    projects: &'a [Value],
    project_id: &str,
) -> Option<&'a Map<String, Value>> {
    projects
        .iter()
        .filter_map(Value::as_object)
        .find(|project| {
            !is_explicit_recent_project(project)
                && trimmed_json_string_field(project, "projectId") == Some(project_id)
        })
}

pub(super) fn is_explicit_recent_project(project: &Map<String, Value>) -> bool {
    project
        .get("isRecentProject")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn trimmed_json_string_field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    trimmed_nonempty_str(object.get(key).and_then(Value::as_str))
}

fn trimmed_nonempty_str(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

pub(super) fn is_default_sidebar_agent_id(agent_id: &str) -> bool {
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.agent_id == agent_id)
}

/// The built-in name and icon of a default agent family, for surfaces that
/// must label a family row the project never customised (Switch Account's
/// "Claude" row on a session that currently runs a custom Claude configuration).
pub(crate) fn default_sidebar_agent_identity(
    agent_id: &str,
) -> Option<(&'static str, &'static str)> {
    default_sidebar_agent_by_id(agent_id).map(|agent| (agent.name, agent.icon))
}

pub(super) fn default_sidebar_agent_by_id(agent_id: &str) -> Option<&'static DefaultSidebarAgent> {
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
}

pub(super) fn default_sidebar_agent_by_icon(icon: &str) -> Option<&'static DefaultSidebarAgent> {
    if icon == "browser" {
        return None;
    }
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == icon)
}

/// Whether an agent is on: its stored `hidden` flag, else (a built-in the user never touched)
/// whether it starts visible.
pub(super) fn is_sidebar_agent_enabled(agents: &[StoredSidebarAgent], agent_id: &str) -> bool {
    match agents.iter().find(|agent| agent.agent_id == agent_id) {
        Some(agent) => !agent.hidden,
        None => default_sidebar_agent_by_id(agent_id)
            .map(|agent| !agent.hidden_by_default)
            .unwrap_or(false),
    }
}

pub(super) fn default_sidebar_agent_name(agent_id: &str, stored_name: &str) -> String {
    let default_name = DEFAULT_SIDEBAR_AGENTS
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

/// The ids of the Dev, Build, Test and Setup placeholders every project starts with.
pub(crate) fn default_sidebar_command_ids() -> impl Iterator<Item = &'static str> {
    DEFAULT_SIDEBAR_COMMANDS
        .iter()
        .map(|command| command.command_id)
}

pub(super) fn is_default_sidebar_command_id(command_id: &str) -> bool {
    DEFAULT_SIDEBAR_COMMANDS
        .iter()
        .any(|command| command.command_id == command_id)
}

pub(super) fn strict_sidebar_agent_icon(candidate: &str) -> Option<&str> {
    if candidate == "browser" {
        return Some(candidate);
    }
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.icon == candidate)
        .then_some(candidate)
}

pub(super) fn sidebar_command_icon(candidate: &str) -> Option<&str> {
    SIDEBAR_COMMAND_ICON_IDS
        .contains(&candidate)
        .then_some(candidate)
}

pub(super) fn reject_duplicate_sidebar_command_title(
    next_command: &StoredSidebarCommand,
    stored_commands: &[StoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> Result<(), DomainStateError> {
    let next_title_key = sidebar_command_title_key(
        &next_command.name,
        next_command.command.as_deref(),
        next_command.url.as_deref(),
    );
    let duplicate = sidebar_command_buttons_from_state(
        stored_commands,
        stored_order,
        deleted_default_command_ids,
    )
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_object)
    .any(|candidate| {
        trimmed_json_string_field(candidate, "commandId")
            .map(|command_id| command_id != next_command.command_id.as_str())
            .unwrap_or(false)
            && sidebar_command_title_key(
                candidate
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                candidate.get("command").and_then(Value::as_str),
                candidate.get("url").and_then(Value::as_str),
            ) == next_title_key
    });
    if duplicate {
        Err(DomainStateError::bad_request("duplicate action title"))
    } else {
        Ok(())
    }
}

fn sidebar_command_title_key(name: &str, command: Option<&str>, url: Option<&str>) -> String {
    normalized_sidebar_command_title(Some(name))
        .or_else(|| {
            normalized_sidebar_command_title(command.or(url))
                .map(|value| value.chars().take(20).collect::<String>())
        })
        .unwrap_or_default()
        .to_lowercase()
}

fn normalized_sidebar_command_title(value: Option<&str>) -> Option<String> {
    let normalized = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(super) fn create_custom_sidebar_agent_id(name: &str) -> String {
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
    format!("custom-{slug}-{}", generated_sidebar_metadata_suffix())
}

pub(super) fn create_custom_sidebar_command_id() -> String {
    format!("custom-{}", generated_sidebar_metadata_suffix())
}

fn generated_sidebar_metadata_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{}", base36(nanos), base36(std::process::id() as u128))
}

fn base36(mut value: u128) -> String {
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
