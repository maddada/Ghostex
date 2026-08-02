use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

use crate::domain::DomainStateError;

#[derive(Clone, Copy)]
struct DefaultSidebarAgent {
    agent_id: &'static str,
    command: &'static str,
    hidden_by_default: bool,
    icon: &'static str,
    name: &'static str,
}

#[derive(Clone, Debug)]
struct StoredSidebarAgent {
    accept_all_mode: Option<String>,
    agent_id: String,
    command: String,
    hidden: bool,
    icon: Option<String>,
    name: String,
}

#[derive(Clone, Copy)]
struct DefaultSidebarCommand {
    command_id: &'static str,
    name: &'static str,
}

#[derive(Clone, Debug)]
struct StoredSidebarCommand {
    action_type: &'static str,
    close_terminal_on_exit: bool,
    command: Option<String>,
    command_id: String,
    icon: Option<String>,
    is_default: bool,
    links: Vec<StoredSidebarCommandLink>,
    name: String,
    play_completion_sound: bool,
    show_on_project_row: bool,
    url: Option<String>,
}

/// Terminal actions can carry saved links that open alongside the command run,
/// each targeting the integrated browser pane or the user's system browser.
#[derive(Clone, Debug)]
struct StoredSidebarCommandLink {
    target: &'static str,
    url: String,
}

#[derive(Clone, Debug)]
pub struct SidebarHudProjectMutation {
    pub params: Map<String, Value>,
    pub project_id: String,
}

#[derive(Clone, Debug)]
pub struct SidebarHudSettingsMutation {
    /*
    CDXC:GlobalActions 2026-08-01-16:00:
    Global Actions belong to the daemon, not to a project row, so a settings
    mutation is no longer always a project write. A global mutation carries this
    field and leaves `updates` empty; a project mutation is unchanged and leaves
    this None. The two never both apply in one request because the Settings UI
    edits one list at a time.
    */
    pub global_command_update: Option<GlobalSidebarCommandUpdate>,
    pub hud_active_project_id: Option<String>,
    pub item_ids: Option<Vec<String>>,
    pub updates: Vec<SidebarHudProjectMutation>,
}

/// A single write against the daemon-owned Global Actions list. The repository
/// owns ordering and timestamps; this only describes the intent.
#[derive(Clone, Debug)]
pub enum GlobalSidebarCommandUpdate {
    Delete {
        command_id: String,
    },
    Order {
        command_ids: Vec<String>,
    },
    Save {
        command_id: String,
        definition: Value,
    },
}

#[derive(Clone, Debug)]
enum SidebarAgentAcceptAllModeUpdate {
    Preserve,
    Set(Option<String>),
}

/*
CDXC:SidebarHudContract 2026-06-24-20:34:
GPUI sidebar and app-modal clients consume normalized launcher/action HUD rows from gxserver instead of hand-mirroring the shared TypeScript read projection in each Rust host. Keep this platform-neutral and project-metadata-only: default rows, hidden built-ins, custom validation, icon allowlists, display order, deleted default actions, and active-project command ownership are resolved here without logging paths, names, commands, URLs, prompts, tokens, stdout/stderr, or daemon bodies.

CDXC:SidebarHudSettingsMutation 2026-06-24-20:54:
Settings save/delete/order mutations for custom agents and actions are gxserver-owned. Accept only narrow semantic mutation payloads, resolve hidden built-in agents, deleted default actions, display order, icon allowlists, active-project command scoping, and worktree parent ownership here, then persist only normalized project metadata fields through the existing project store.

CDXC:GPUIRecentProjects 2026-06-25-21:36:
Sidebar HUD action commands must resolve from normal project rows only: explicit boolean isRecentProject true rows are parked Recent Projects metadata and cannot hydrate command buttons or own action mutations. False, missing, or non-boolean flags stay normal so older metadata remains eligible.

CDXC:GPUIRecentProjects 2026-06-25-21:36:
When an explicit active project ID resolves only to a parked recent row, the HUD must show default actions and action mutations must fail as no-normal-project behavior instead of borrowing parked commands. Worktree parent ownership also skips parked rows and falls back to the active normal project.
*/
const DEFAULT_SIDEBAR_AGENTS: &[DefaultSidebarAgent] = &[
    DefaultSidebarAgent {
        agent_id: "t3",
        command: "npx --yes t3",
        hidden_by_default: false,
        icon: "t3",
        name: "T3 Code",
    },
    DefaultSidebarAgent {
        agent_id: "codex",
        command: "codex",
        hidden_by_default: false,
        icon: "codex",
        name: "Codex",
    },
    DefaultSidebarAgent {
        agent_id: "claude",
        command: "claude",
        hidden_by_default: false,
        icon: "claude",
        name: "Claude",
    },
    DefaultSidebarAgent {
        agent_id: "cursor",
        command: "cursor-agent",
        hidden_by_default: false,
        icon: "cursor-cli",
        name: "Cursor CLI",
    },
    DefaultSidebarAgent {
        agent_id: "pi",
        command: "pi",
        hidden_by_default: false,
        icon: "pi",
        name: "Pi Agent",
    },
    DefaultSidebarAgent {
        agent_id: "opencode",
        command: "opencode",
        hidden_by_default: false,
        icon: "opencode",
        name: "OpenCode",
    },
    DefaultSidebarAgent {
        agent_id: "gemini",
        command: "gemini",
        hidden_by_default: false,
        icon: "gemini",
        name: "Gemini",
    },
    DefaultSidebarAgent {
        agent_id: "copilot",
        command: "copilot",
        hidden_by_default: false,
        icon: "copilot",
        name: "Copilot",
    },
    DefaultSidebarAgent {
        agent_id: "droid",
        command: "droid",
        hidden_by_default: false,
        icon: "factory-droid",
        name: "Factory Droid",
    },
    DefaultSidebarAgent {
        agent_id: "grok",
        command: "grok",
        hidden_by_default: false,
        icon: "grok-build",
        name: "Grok Build",
    },
    DefaultSidebarAgent {
        agent_id: "antigravity",
        command: "agy",
        hidden_by_default: false,
        icon: "antigravity-cli",
        name: "Antigravity CLI",
    },
    DefaultSidebarAgent {
        agent_id: "amp",
        command: "amp",
        hidden_by_default: false,
        icon: "amp-cli",
        name: "Amp CLI",
    },
    DefaultSidebarAgent {
        agent_id: "hermes-agent",
        command: "hermes",
        hidden_by_default: false,
        icon: "hermes-agent",
        name: "Hermes Agent",
    },
    DefaultSidebarAgent {
        agent_id: "rovodev",
        command: "acli rovodev run",
        hidden_by_default: true,
        icon: "rovo-dev",
        name: "Rovo Dev",
    },
    DefaultSidebarAgent {
        agent_id: "codebuddy",
        command: "codebuddy",
        hidden_by_default: true,
        icon: "codebuddy",
        name: "CodeBuddy",
    },
    DefaultSidebarAgent {
        agent_id: "qoder",
        command: "qodercli",
        hidden_by_default: true,
        icon: "qoder",
        name: "Qoder",
    },
    DefaultSidebarAgent {
        agent_id: "kiro",
        command: "kiro-cli chat --agent ghostex",
        hidden_by_default: true,
        icon: "kiro",
        name: "Kiro CLI",
    },
    DefaultSidebarAgent {
        agent_id: "omp",
        command: "omp",
        hidden_by_default: true,
        icon: "omp",
        name: "OMP",
    },
];

const DEFAULT_SIDEBAR_COMMANDS: &[DefaultSidebarCommand] = &[
    DefaultSidebarCommand {
        command_id: "dev",
        name: "Dev",
    },
    DefaultSidebarCommand {
        command_id: "build",
        name: "Build",
    },
    DefaultSidebarCommand {
        command_id: "test",
        name: "Test",
    },
    DefaultSidebarCommand {
        command_id: "setup",
        name: "Setup",
    },
];

const SIDEBAR_COMMAND_ICON_IDS: &[&str] = &[
    "playerPlay",
    "api",
    "archive",
    "bell",
    "bolt",
    "book",
    "brain",
    "braces",
    "brandDocker",
    "brandGithub",
    "brandPython",
    "brandReact",
    "brandVscode",
    "bug",
    "chartBar",
    "cloud",
    "checklist",
    "clock",
    "code",
    "command",
    "cpu",
    "database",
    "deviceDesktop",
    "deviceLaptop",
    "download",
    "fileCode",
    "fileDiff",
    "fileSearch",
    "fileText",
    "flask",
    "folder",
    "folderOpen",
    "gitBranch",
    "gitCommit",
    "gitMerge",
    "gitPullRequest",
    "key",
    "layoutDashboard",
    "link",
    "lock",
    "messageCircle",
    "package",
    "pencilCode",
    "refresh",
    "robot",
    "route",
    "rocket",
    "search",
    "server",
    "settings",
    "shieldSearch",
    "sparkles",
    "stack",
    "terminal",
    "testPipe",
    "tool",
    "upload",
    "wand",
    "world",
];

pub fn read_sidebar_hud(projects: &[Value], active_project_id: Option<&str>) -> Value {
    let agents = sidebar_agent_buttons_from_projects(projects);
    let commands = sidebar_command_buttons_from_projects(projects, active_project_id);
    let mut payload = Map::new();
    payload.insert("agents".to_string(), agents);
    payload.insert("commands".to_string(), commands);
    Value::Object(payload)
}

/*
CDXC:MobileSidebarHud 2026-07-12-00:00:
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

pub fn create_sidebar_hud_settings_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let target = required_trimmed_param(params, "target")?;
    let operation = required_trimmed_param(params, "operation")?;
    match (target.as_str(), operation.as_str()) {
        ("agent", "save") => sidebar_agent_save_mutation(projects, params),
        ("agent", "delete") => sidebar_agent_delete_mutation(projects, params),
        ("agent", "order") => sidebar_agent_order_mutation(projects, params),
        ("command", "save") => sidebar_command_save_mutation(projects, params),
        ("command", "delete") => sidebar_command_delete_mutation(projects, params),
        ("command", "order") => sidebar_command_order_mutation(projects, params),
        ("globalCommand", "save") => global_sidebar_command_save_mutation(params),
        ("globalCommand", "delete") => global_sidebar_command_delete_mutation(params),
        ("globalCommand", "order") => global_sidebar_command_order_mutation(params),
        _ => Err(DomainStateError::bad_request(
            "Unsupported sidebar Settings mutation.",
        )),
    }
}

fn sidebar_agent_save_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let name = required_trimmed_param(params, "name")?;
    let command = required_trimmed_param(params, "command")?;
    let requested_agent_id = optional_trimmed_param(params, "agentId");
    let requested_icon = params
        .get("icon")
        .and_then(Value::as_str)
        .and_then(strict_sidebar_agent_icon)
        .map(str::to_string);
    let accept_all_mode = sidebar_agent_accept_all_mode_update(params)?;
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    let current_agent_ids = sidebar_button_ids(
        &sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
        "agentId",
    );
    let selected_default_agent_id = requested_icon
        .as_deref()
        .and_then(default_sidebar_agent_by_icon)
        .map(|agent| agent.agent_id);
    let should_restore_hidden_default = requested_agent_id.is_none()
        && selected_default_agent_id
            .map(|agent_id| !is_sidebar_agent_visible(&stored_agents, agent_id))
            .unwrap_or(false);
    let agent_id = requested_agent_id
        .or_else(|| {
            should_restore_hidden_default
                .then_some(selected_default_agent_id)
                .flatten()
                .map(str::to_string)
        })
        .unwrap_or_else(|| create_custom_sidebar_agent_id(&name));
    let existing_index = stored_agents
        .iter()
        .position(|agent| agent.agent_id == agent_id);
    let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
    let default_agent = default_sidebar_agent_by_id(&agent_id);
    let next_agent = StoredSidebarAgent {
        accept_all_mode: match accept_all_mode {
            SidebarAgentAcceptAllModeUpdate::Preserve => previous_agent
                .and_then(|agent| agent.accept_all_mode.as_ref())
                .cloned(),
            SidebarAgentAcceptAllModeUpdate::Set(mode) => mode,
        },
        agent_id: agent_id.clone(),
        command,
        hidden: false,
        icon: requested_icon
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
        || stored_order.iter().any(|candidate| candidate == &agent_id)
        || is_default_sidebar_agent_id(&agent_id)
    {
        stored_order
    } else {
        let mut next_order = current_agent_ids;
        next_order.push(agent_id);
        next_order
    };
    sidebar_agent_projects_mutation(projects, next_agents, next_order, params)
}

fn sidebar_agent_delete_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let agent_id = required_trimmed_param(params, "agentId")?;
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    if !is_default_sidebar_agent_id(&agent_id) {
        let next_agents = stored_agents
            .into_iter()
            .filter(|agent| agent.agent_id != agent_id)
            .collect::<Vec<_>>();
        let next_order = stored_order
            .into_iter()
            .filter(|candidate| candidate != &agent_id)
            .collect::<Vec<_>>();
        return sidebar_agent_projects_mutation(projects, next_agents, next_order, params);
    }
    let Some(default_agent) = default_sidebar_agent_by_id(&agent_id) else {
        return sidebar_agent_projects_mutation(projects, stored_agents, stored_order, params);
    };
    let existing_index = stored_agents
        .iter()
        .position(|agent| agent.agent_id == agent_id);
    let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
    let next_agent = StoredSidebarAgent {
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
    sidebar_agent_projects_mutation(projects, next_agents, next_order, params)
}

fn sidebar_agent_order_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let agent_ids = normalized_string_order(params.get("agentIds"));
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    let current_agent_ids = sidebar_button_ids(
        &sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
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
    let item_ids = sidebar_button_ids(
        &sidebar_agent_buttons_from_state(&stored_agents, &next_order),
        "agentId",
    );
    let mut mutation =
        sidebar_agent_projects_mutation(projects, stored_agents, next_order, params)?;
    mutation.item_ids = Some(item_ids);
    Ok(mutation)
}

fn sidebar_agent_projects_mutation(
    projects: &[Value],
    agents: Vec<StoredSidebarAgent>,
    order: Vec<String>,
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let custom_agents = stored_sidebar_agents_value(&agents);
    let custom_agent_order = string_array_value(&order);
    let mut updates = Vec::new();
    for project in projects.iter().filter_map(Value::as_object) {
        let Some(project_id) = trimmed_json_string_field(project, "projectId") else {
            continue;
        };
        let mut update = Map::new();
        update.insert(
            "projectId".to_string(),
            Value::String(project_id.to_string()),
        );
        update.insert("customAgents".to_string(), custom_agents.clone());
        update.insert("customAgentOrder".to_string(), custom_agent_order.clone());
        updates.push(SidebarHudProjectMutation {
            params: update,
            project_id: project_id.to_string(),
        });
    }
    if updates.is_empty() {
        return Err(DomainStateError::bad_request(
            "No project metadata is available for sidebar agent mutation.",
        ));
    }
    Ok(SidebarHudSettingsMutation {
        global_command_update: None,
        hud_active_project_id: optional_trimmed_param(params, "activeProjectId"),
        item_ids: None,
        updates,
    })
}

fn sidebar_command_save_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let command_scope = sidebar_command_scope(projects, params)?;
    let mut state = sidebar_command_state(command_scope.owner_project);
    let current_command_ids = sidebar_button_ids(
        &sidebar_command_buttons_from_state(
            &state.commands,
            &state.order,
            &state.deleted_default_command_ids,
        ),
        "commandId",
    );
    let command_id = optional_trimmed_param(params, "commandId")
        .unwrap_or_else(create_custom_sidebar_command_id);
    let next_command = stored_sidebar_command_from_save_params(params, command_id.clone())?;
    reject_duplicate_sidebar_command_title(
        &next_command,
        &state.commands,
        &state.order,
        &state.deleted_default_command_ids,
    )?;
    let existing_index = state
        .commands
        .iter()
        .position(|command| command.command_id == command_id);
    if let Some(existing_index) = existing_index {
        state.commands[existing_index] = next_command;
    } else {
        state.commands.push(next_command);
    }
    state.order = if existing_index.is_some()
        || state.order.iter().any(|candidate| candidate == &command_id)
        || is_default_sidebar_command_id(&command_id)
    {
        state.order
    } else if current_command_ids
        .iter()
        .any(|candidate| candidate == &command_id)
    {
        current_command_ids
    } else {
        let mut next_order = current_command_ids;
        next_order.push(command_id.clone());
        next_order
    };
    if is_default_sidebar_command_id(&command_id) {
        state
            .deleted_default_command_ids
            .retain(|candidate| candidate != &command_id);
    }
    sidebar_command_project_mutation(command_scope, state)
}

fn sidebar_command_delete_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let command_scope = sidebar_command_scope(projects, params)?;
    let command_id = required_trimmed_param(params, "commandId")?;
    let mut state = sidebar_command_state(command_scope.owner_project);
    state
        .commands
        .retain(|command| command.command_id != command_id);
    state.order.retain(|candidate| candidate != &command_id);
    if is_default_sidebar_command_id(&command_id)
        && !state
            .deleted_default_command_ids
            .iter()
            .any(|candidate| candidate == &command_id)
    {
        state.deleted_default_command_ids.push(command_id);
    }
    sidebar_command_project_mutation(command_scope, state)
}

fn sidebar_command_order_mutation(
    projects: &[Value],
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let command_scope = sidebar_command_scope(projects, params)?;
    let mut state = sidebar_command_state(command_scope.owner_project);
    let current_command_ids = sidebar_button_ids(
        &sidebar_command_buttons_from_state(
            &state.commands,
            &state.order,
            &state.deleted_default_command_ids,
        ),
        "commandId",
    );
    let mut next_order = normalized_string_order(params.get("commandIds"))
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
    state.order = next_order;
    let item_ids = sidebar_button_ids(
        &sidebar_command_buttons_from_state(
            &state.commands,
            &state.order,
            &state.deleted_default_command_ids,
        ),
        "commandId",
    );
    let mut mutation = sidebar_command_project_mutation(command_scope, state)?;
    mutation.item_ids = Some(item_ids);
    Ok(mutation)
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
Project and Global Actions accept the exact same action definition from
Settings — only ownership differs — so both saves validate through one path.
Splitting the validation would let the two lists drift into accepting different
action shapes, which is how a Global Action that mobile cannot run gets saved.
*/
fn stored_sidebar_command_from_save_params(
    params: &Map<String, Value>,
    command_id: String,
) -> Result<StoredSidebarCommand, DomainStateError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let icon = params
        .get("icon")
        .and_then(Value::as_str)
        .and_then(sidebar_command_icon)
        .map(str::to_string);
    if name.is_empty() && icon.is_none() {
        return Err(DomainStateError::bad_request(
            "Sidebar action mutations require a name or icon.",
        ));
    }
    let action_type = match params.get("actionType").and_then(Value::as_str) {
        Some("browser") => "browser",
        Some("terminal") => "terminal",
        _ => {
            return Err(DomainStateError::bad_request(
                "Unsupported sidebar action type.",
            ));
        }
    };
    let command_text = optional_trimmed_param(params, "command");
    let url = optional_trimmed_param(params, "url");
    if action_type == "browser" && url.is_none() {
        return Err(DomainStateError::bad_request(
            "Browser sidebar actions require a URL.",
        ));
    }
    if action_type == "terminal" && command_text.is_none() {
        return Err(DomainStateError::bad_request(
            "Terminal sidebar actions require a command.",
        ));
    }
    Ok(StoredSidebarCommand {
        action_type,
        close_terminal_on_exit: action_type == "terminal"
            && params
                .get("closeTerminalOnExit")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        command: (action_type == "terminal")
            .then_some(command_text)
            .flatten(),
        is_default: is_default_sidebar_command_id(&command_id),
        command_id,
        icon,
        links: if action_type == "terminal" {
            normalized_sidebar_command_links(params.get("links"))
        } else {
            Vec::new()
        },
        name,
        play_completion_sound: action_type == "terminal"
            && params
                .get("playCompletionSound")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        show_on_project_row: params
            .get("showOnProjectRow")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        url: (action_type == "browser").then_some(url).flatten(),
    })
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
Global Action mutations never read or write project rows, so they do not resolve
a command scope, an owner project, or a worktree parent the way Project Action
mutations must. The repository owns ordering and the stored list; these arms only
validate the intent and hand it over.
*/
fn global_sidebar_command_save_mutation(
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    let command_id = optional_trimmed_param(params, "commandId")
        .unwrap_or_else(create_custom_sidebar_command_id);
    /*
    The four default actions (dev/build/test/setup) are project-scoped, so their
    ids are reserved and a Global Action may never claim one. Rejecting at save
    is what makes that hold on BOTH paths: the read projection recomputes
    is_default from the id itself, so a stored Global Action called "dev" would
    come back marked as a default however it was written. Rejecting here also
    keeps global and project ids from colliding on the reserved names, which a
    run-by-id selector could not otherwise tell apart.
    */
    if is_default_sidebar_command_id(&command_id) {
        return Err(DomainStateError::bad_request(
            "Global actions cannot use a built-in action id.",
        ));
    }
    let mut next_command = stored_sidebar_command_from_save_params(params, command_id.clone())?;
    next_command.is_default = false;
    Ok(SidebarHudSettingsMutation {
        global_command_update: Some(GlobalSidebarCommandUpdate::Save {
            command_id,
            definition: sidebar_command_button_value(&next_command),
        }),
        hud_active_project_id: optional_trimmed_param(params, "activeProjectId"),
        item_ids: None,
        updates: Vec::new(),
    })
}

fn global_sidebar_command_delete_mutation(
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    Ok(SidebarHudSettingsMutation {
        global_command_update: Some(GlobalSidebarCommandUpdate::Delete {
            command_id: required_trimmed_param(params, "commandId")?,
        }),
        hud_active_project_id: optional_trimmed_param(params, "activeProjectId"),
        item_ids: None,
        updates: Vec::new(),
    })
}

fn global_sidebar_command_order_mutation(
    params: &Map<String, Value>,
) -> Result<SidebarHudSettingsMutation, DomainStateError> {
    Ok(SidebarHudSettingsMutation {
        global_command_update: Some(GlobalSidebarCommandUpdate::Order {
            command_ids: normalized_string_order(params.get("commandIds")),
        }),
        hud_active_project_id: optional_trimmed_param(params, "activeProjectId"),
        item_ids: None,
        updates: Vec::new(),
    })
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
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

fn sidebar_agent_buttons_from_projects(projects: &[Value]) -> Value {
    let (stored_agents, stored_order) = sidebar_agent_state_from_projects(projects);
    sidebar_agent_buttons_from_state(&stored_agents, &stored_order)
}

fn sidebar_agent_state_from_projects(projects: &[Value]) -> (Vec<StoredSidebarAgent>, Vec<String>) {
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

fn sidebar_agent_buttons_from_state(
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

fn sidebar_command_buttons_from_state(
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

struct SidebarCommandScope<'a> {
    hud_active_project_id: Option<String>,
    owner_project: &'a Map<String, Value>,
    owner_project_id: String,
}

struct SidebarCommandState {
    commands: Vec<StoredSidebarCommand>,
    deleted_default_command_ids: Vec<String>,
    order: Vec<String>,
}

fn sidebar_command_scope<'a>(
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

fn sidebar_command_state(project: &Map<String, Value>) -> SidebarCommandState {
    SidebarCommandState {
        commands: normalized_stored_sidebar_commands(project.get("customCommands")),
        deleted_default_command_ids: normalized_string_order(
            project.get("deletedDefaultCommandIds"),
        ),
        order: normalized_string_order(project.get("customCommandOrder")),
    }
}

fn sidebar_command_project_mutation(
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

fn sidebar_command_button_value(command: &StoredSidebarCommand) -> Value {
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
        button.insert("links".to_string(), sidebar_command_links_value(&command.links));
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

fn normalized_stored_sidebar_agents(candidate: Option<&Value>) -> Vec<StoredSidebarAgent> {
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
        if seen_agent_ids.contains(agent_id) {
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

fn normalized_stored_sidebar_commands(candidate: Option<&Value>) -> Vec<StoredSidebarCommand> {
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

fn normalized_sidebar_command_links(candidate: Option<&Value>) -> Vec<StoredSidebarCommandLink> {
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

fn sidebar_command_links_value(links: &[StoredSidebarCommandLink]) -> Value {
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

fn normalized_string_order(candidate: Option<&Value>) -> Vec<String> {
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

fn order_json_buttons(
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

fn sidebar_button_ids(buttons: &Value, id_key: &str) -> Vec<String> {
    buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|button| trimmed_json_string_field(button, id_key))
        .map(str::to_string)
        .collect()
}

fn required_trimmed_param(
    params: &Map<String, Value>,
    key: &str,
) -> Result<String, DomainStateError> {
    optional_trimmed_param(params, key)
        .ok_or_else(|| DomainStateError::bad_request("Invalid sidebar Settings mutation payload."))
}

fn optional_trimmed_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| trimmed_nonempty_str(Some(value)))
        .map(str::to_string)
}

fn sidebar_agent_accept_all_mode_update(
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

fn stored_sidebar_agents_value(agents: &[StoredSidebarAgent]) -> Value {
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

fn stored_sidebar_commands_value(commands: &[StoredSidebarCommand]) -> Value {
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

fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn json_array_field_is_nonempty(object: &Map<String, Value>, key: &str) -> bool {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

fn first_normal_project(projects: &[Value]) -> Option<&Map<String, Value>> {
    projects
        .iter()
        .filter_map(Value::as_object)
        .find(|project| !is_explicit_recent_project(project))
}

fn normal_project_by_id<'a>(
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

fn is_explicit_recent_project(project: &Map<String, Value>) -> bool {
    project
        .get("isRecentProject")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn trimmed_json_string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    trimmed_nonempty_str(object.get(key).and_then(Value::as_str))
}

fn trimmed_nonempty_str(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

fn is_default_sidebar_agent_id(agent_id: &str) -> bool {
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.agent_id == agent_id)
}

fn default_sidebar_agent_by_id(agent_id: &str) -> Option<&'static DefaultSidebarAgent> {
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
}

fn default_sidebar_agent_by_icon(icon: &str) -> Option<&'static DefaultSidebarAgent> {
    if icon == "browser" {
        return None;
    }
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == icon)
}

fn is_sidebar_agent_visible(agents: &[StoredSidebarAgent], agent_id: &str) -> bool {
    agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| !agent.hidden)
        .unwrap_or(true)
}

fn default_sidebar_agent_name(agent_id: &str, stored_name: &str) -> String {
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

fn is_default_sidebar_command_id(command_id: &str) -> bool {
    DEFAULT_SIDEBAR_COMMANDS
        .iter()
        .any(|command| command.command_id == command_id)
}

fn strict_sidebar_agent_icon(candidate: &str) -> Option<&str> {
    if candidate == "browser" {
        return Some(candidate);
    }
    DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.icon == candidate)
        .then_some(candidate)
}

fn sidebar_command_icon(candidate: &str) -> Option<&str> {
    SIDEBAR_COMMAND_ICON_IDS
        .iter()
        .any(|icon| *icon == candidate)
        .then_some(candidate)
}

fn reject_duplicate_sidebar_command_title(
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

fn create_custom_sidebar_agent_id(name: &str) -> String {
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

fn create_custom_sidebar_command_id() -> String {
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{create_sidebar_hud_settings_mutation, read_sidebar_hud};

    fn command_ids(hud: &Value) -> Vec<&str> {
        hud.get("commands")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
            .collect::<Vec<_>>()
    }

    #[test]
    fn hides_hidden_default_agents_until_stored() {
        let hud = read_sidebar_hud(&[], None);
        let agents = hud
            .get("agents")
            .and_then(|value| value.as_array())
            .unwrap();
        assert!(agents
            .iter()
            .all(|agent| agent.get("agentId").and_then(|value| value.as_str()) != Some("rovodev")));

        let hud = read_sidebar_hud(
            &[json!({
                "customAgents": [
                    {
                        "agentId": "rovodev",
                        "command": "acli rovodev run",
                        "isDefault": true,
                        "name": "Rovo Dev"
                    }
                ],
                "customAgentOrder": ["rovodev", "codex"],
                "projectId": "P1a"
            })],
            None,
        );
        let agents = hud
            .get("agents")
            .and_then(|value| value.as_array())
            .unwrap();
        assert_eq!(
            agents
                .first()
                .and_then(|agent| agent.get("agentId"))
                .and_then(|value| value.as_str()),
            Some("rovodev")
        );
    }

    #[test]
    fn validates_custom_command_runnable_fields_and_deleted_defaults() {
        let hud = read_sidebar_hud(
            &[json!({
                "customCommandOrder": ["browser-good", "terminal-good", "build"],
                "customCommands": [
                    {
                        "actionType": "browser",
                        "commandId": "browser-bad",
                        "name": "Bad Browser"
                    },
                    {
                        "actionType": "browser",
                        "commandId": "browser-good",
                        "name": "Browser",
                        "url": "https://example.test"
                    },
                    {
                        "actionType": "terminal",
                        "commandId": "terminal-bad",
                        "name": "Bad Terminal"
                    },
                    {
                        "actionType": "terminal",
                        "command": "bun run dev",
                        "commandId": "terminal-good",
                        "name": "Dev"
                    }
                ],
                "deletedDefaultCommandIds": ["build"],
                "projectId": "P1a"
            })],
            None,
        );
        let commands = hud
            .get("commands")
            .and_then(|value| value.as_array())
            .unwrap();
        let command_ids = commands
            .iter()
            .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(command_ids[0], "browser-good");
        assert_eq!(command_ids[1], "terminal-good");
        assert!(!command_ids.contains(&"browser-bad"));
        assert!(!command_ids.contains(&"terminal-bad"));
        assert!(!command_ids.contains(&"build"));
    }

    #[test]
    fn scopes_commands_to_active_project_owner() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo test",
                        "commandId": "parent-test",
                        "name": "Parent Test"
                    }
                ],
                "customCommandOrder": ["parent-test"],
                "projectId": "Pparent"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "bun test",
                        "commandId": "worktree-test",
                        "name": "Worktree Test"
                    }
                ],
                "projectId": "Pworktree",
                "worktree": { "parentProjectId": "Pparent" }
            }),
        ];
        let hud = read_sidebar_hud(&projects, Some("Pworktree"));
        let command_ids = hud
            .get("commands")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(command_ids.first().copied(), Some("parent-test"));

        let hud = read_sidebar_hud(&projects, Some("Pmissing"));
        let command_ids = hud
            .get("commands")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .filter_map(|command| command.get("commandId").and_then(|value| value.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(command_ids, vec!["dev", "build", "test", "setup"]);
    }

    #[test]
    fn skips_explicit_recent_project_for_no_active_command_source() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo check",
                        "commandId": "parked-check",
                        "name": "Parked Check"
                    }
                ],
                "customCommandOrder": ["parked-check"],
                "isRecentProject": true,
                "projectId": "Precent"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo test",
                        "commandId": "normal-test",
                        "name": "Normal Test"
                    }
                ],
                "customCommandOrder": ["normal-test"],
                "projectId": "Pnormal"
            }),
        ];

        let hud = read_sidebar_hud(&projects, None);
        let command_ids = command_ids(&hud);
        assert_eq!(command_ids.first().copied(), Some("normal-test"));
        assert!(!command_ids.contains(&"parked-check"));
    }

    #[test]
    fn explicit_active_recent_project_does_not_expose_custom_commands() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo check",
                        "commandId": "parked-check",
                        "name": "Parked Check"
                    }
                ],
                "customCommandOrder": ["parked-check"],
                "isRecentProject": true,
                "projectId": "Precent"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo test",
                        "commandId": "normal-test",
                        "name": "Normal Test"
                    }
                ],
                "customCommandOrder": ["normal-test"],
                "projectId": "Pnormal"
            }),
        ];

        let hud = read_sidebar_hud(&projects, Some("Precent"));
        assert_eq!(command_ids(&hud), vec!["dev", "build", "test", "setup"]);
    }

    #[test]
    fn non_true_recent_flags_remain_normal_command_sources() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo check",
                        "commandId": "false-flag-check",
                        "name": "False Flag Check"
                    }
                ],
                "customCommandOrder": ["false-flag-check"],
                "isRecentProject": false,
                "projectId": "Pfalse"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo test",
                        "commandId": "missing-flag-test",
                        "name": "Missing Flag Test"
                    }
                ],
                "customCommandOrder": ["missing-flag-test"],
                "projectId": "Pmissing"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo build",
                        "commandId": "non-boolean-flag-build",
                        "name": "Non Boolean Flag Build"
                    }
                ],
                "customCommandOrder": ["non-boolean-flag-build"],
                "isRecentProject": "true",
                "projectId": "PnonBoolean"
            }),
        ];

        let hud = read_sidebar_hud(&projects, None);
        assert_eq!(command_ids(&hud).first().copied(), Some("false-flag-check"));

        let hud = read_sidebar_hud(&projects, Some("Pmissing"));
        assert_eq!(
            command_ids(&hud).first().copied(),
            Some("missing-flag-test")
        );

        let hud = read_sidebar_hud(&projects, Some("PnonBoolean"));
        assert_eq!(
            command_ids(&hud).first().copied(),
            Some("non-boolean-flag-build")
        );
    }

    #[test]
    fn worktree_owner_resolution_skips_explicit_recent_project_rows() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo check",
                        "commandId": "parked-parent-check",
                        "name": "Parked Parent Check"
                    }
                ],
                "customCommandOrder": ["parked-parent-check"],
                "isRecentProject": true,
                "projectId": "Pparent"
            }),
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo test",
                        "commandId": "worktree-test",
                        "name": "Worktree Test"
                    }
                ],
                "customCommandOrder": ["worktree-test"],
                "projectId": "Pworktree",
                "worktree": { "parentProjectId": "Pparent" }
            }),
        ];

        let hud = read_sidebar_hud(&projects, Some("Pworktree"));
        let command_ids = command_ids(&hud);
        assert_eq!(command_ids.first().copied(), Some("worktree-test"));
        assert!(!command_ids.contains(&"parked-parent-check"));
    }

    #[test]
    fn command_mutation_scope_skips_explicit_recent_project_rows() {
        let projects = vec![
            json!({
                "customCommands": [
                    {
                        "actionType": "terminal",
                        "command": "cargo check",
                        "commandId": "parked-parent-check",
                        "name": "Parked Parent Check"
                    }
                ],
                "isRecentProject": true,
                "projectId": "Pparent"
            }),
            json!({
                "projectId": "Pworktree",
                "worktree": { "parentProjectId": "Pparent" }
            }),
        ];
        let params = json!({
            "activeProjectId": "Pworktree",
            "actionType": "terminal",
            "command": "cargo test",
            "name": "Workspace Verify",
            "operation": "save",
            "target": "command"
        })
        .as_object()
        .unwrap()
        .clone();

        let mutation = create_sidebar_hud_settings_mutation(&projects, &params).unwrap();
        assert_eq!(mutation.updates.len(), 1);
        assert_eq!(mutation.updates[0].project_id, "Pworktree");

        let params = json!({
            "activeProjectId": "Pparent",
            "actionType": "terminal",
            "command": "cargo test",
            "name": "Workspace Verify",
            "operation": "save",
            "target": "command"
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(create_sidebar_hud_settings_mutation(&projects, &params).is_err());
    }

    /*
    CDXC:GlobalActions 2026-08-01-19:00:
    A Global Action may not claim a reserved built-in id. The read projection
    recomputes isDefault from the id, so a stored global "dev" would come back
    marked as a default however it was written, and a run-by-id selector could
    not tell it apart from the project action of the same name.
    */
    #[test]
    fn global_command_save_rejects_reserved_default_ids() {
        let projects = Vec::new();
        for command_id in ["dev", "build", "test", "setup"] {
            let params = json!({
                "actionType": "terminal",
                "command": "echo hi",
                "commandId": command_id,
                "name": "Reserved",
                "operation": "save",
                "target": "globalCommand"
            })
            .as_object()
            .unwrap()
            .clone();
            assert!(
                create_sidebar_hud_settings_mutation(&projects, &params).is_err(),
                "expected reserved global command id {command_id} to be rejected"
            );
        }
    }

    #[test]
    fn global_command_save_needs_no_project() {
        let projects = Vec::new();
        let params = json!({
            "actionType": "terminal",
            "command": "gh pr list",
            "name": "PRs",
            "operation": "save",
            "target": "globalCommand"
        })
        .as_object()
        .unwrap()
        .clone();
        let mutation = create_sidebar_hud_settings_mutation(&projects, &params)
            .expect("global saves must not require an active project");
        assert!(mutation.updates.is_empty());
        assert!(mutation.global_command_update.is_some());
    }
}
