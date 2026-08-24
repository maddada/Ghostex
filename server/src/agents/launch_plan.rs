use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;
use crate::session_status::normalize_agent_activity_value;
use rusqlite::Connection;

pub(crate) fn build_project_agent_launch_plan(
    project: &Value,
    agent_id: &str,
    agent_session_id: Option<String>,
    settings: &Map<String, Value>,
) -> Value {
    let agent_config = resolve_project_agent_config(project, agent_id, None);
    build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: read_text_from_map(&agent_config, "acceptAllMode"),
        agent_id: agent_id.to_string(),
        agent_session_id,
        command: read_text_from_map(&agent_config, "command"),
        delayed_send_deadline_at: None,
        first_user_message: None,
        global_accept_all_enabled: settings
            .get("agentAcceptAllEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        icon: read_text_from_map(&agent_config, "icon"),
    })
}

/*
CDXC:GxserverCrudParity 2026-06-22-05:39:
`createAgentSession` is a CRUD endpoint, but its durable row is shaped by the same project agent config and persisted agent settings as TypeScript gxserver. Build the launch plan before repository insertion so listSessions/readProjectStatus return the same launchSettings and runtimeSettings immediately after creation.
*/
pub(crate) fn create_agent_session_params_for_project(
    db: &Connection,
    project: &Value,
    params: &Map<String, Value>,
) -> Result<Map<String, Value>, DomainStateError> {
    let settings = read_agent_settings(db)?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::corrupt_state("Project missing projectId."))?;
    let agent_id =
        read_text(params, "agentId").unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string());
    let mut launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut runtime_settings = params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let agent_config = resolve_project_agent_config(project, &agent_id, Some(&launch_settings));
    let agent_icon = read_text_from_map(&agent_config, "icon")
        .or_else(|| read_text_from_map(&launch_settings, "icon"));
    let launch_plan = build_agent_launch_plan(AgentLaunchInput {
        accept_all_mode: read_text_from_map(&agent_config, "acceptAllMode")
            .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode")),
        agent_id: agent_id.clone(),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        command: read_text_from_map(&agent_config, "command")
            .or_else(|| read_text_from_map(&launch_settings, "agentCommand")),
        delayed_send_deadline_at: read_text_from_map(&launch_settings, "delayedSendDeadlineAt"),
        first_user_message: read_text_from_map(&runtime_settings, "firstUserMessage"),
        global_accept_all_enabled: settings
            .get("agentAcceptAllEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        icon: agent_icon.clone(),
    });
    let launch_plan_object = launch_plan.as_object().cloned().unwrap_or_default();
    let has_launch_command = launch_plan_object
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if params.get("requireLaunchCommand").and_then(Value::as_bool) == Some(true)
        && !has_launch_command
    {
        /*
        CDXC:GPUIRemoteSessions 2026-06-24-17:19:
        Remote GPUI starts send only the selected agent id and require gxserver to resolve the command from remote project metadata or built-in defaults. Reject commandless launches so unknown custom agent ids do not create inert sessions that look successful.
        */
        return Err(DomainStateError::bad_request(
            "Agent command is required to create this session.",
        ));
    }
    let has_launch_startup_text = launch_plan_object
        .get("startupText")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let agent_activity = if runtime_settings.get("agentActivity").is_some() {
        normalize_agent_activity_value(runtime_settings.get("agentActivity"), "idle")
    } else {
        default_activity(
            Some(&agent_id),
            has_launch_startup_text.then_some("working"),
        )
    };
    runtime_settings.insert("agentActivity".to_string(), agent_activity);
    runtime_settings.insert(
        "agentCommand".to_string(),
        Value::String(
            launch_plan_object
                .get("agentCommand")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    runtime_settings.insert("launchAgentId".to_string(), Value::String(agent_id.clone()));
    if let Some(first_user_message) = launch_plan_object
        .get("firstUserMessage")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        runtime_settings.insert(
            "firstUserMessage".to_string(),
            Value::String(first_user_message.to_string()),
        );
    }
    /*
    CDXC:GxserverFirstUserInputDraft 2026-08-20:
    Arm the draft here, at creation, so only sessions that were created with one
    can ever consume it. A draft that is missing or blank leaves no marker and
    no key behind.
    */
    match runtime_settings
        .get(FIRST_USER_INPUT_DRAFT_KEY)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
    {
        Some(draft) => {
            runtime_settings.insert(FIRST_USER_INPUT_DRAFT_KEY.to_string(), Value::String(draft));
            runtime_settings.insert(
                FIRST_USER_INPUT_DRAFT_STATUS_KEY.to_string(),
                json!("pending"),
            );
        }
        None => {
            runtime_settings.remove(FIRST_USER_INPUT_DRAFT_KEY);
            runtime_settings.remove(FIRST_USER_INPUT_DRAFT_STATUS_KEY);
        }
    }

    let mut runtime_relevant = Map::new();
    if let Some(deadline_at) = launch_plan_object
        .get("delayedSend")
        .and_then(Value::as_object)
        .and_then(|delayed| delayed.get("deadlineAt"))
        .and_then(Value::as_str)
    {
        runtime_relevant.insert(
            "delayedSendDeadlineAt".to_string(),
            Value::String(deadline_at.to_string()),
        );
    }
    runtime_relevant.insert(
        "queueProviderStartupText".to_string(),
        Value::Bool(
            launch_plan_object
                .get("startupTextDisposition")
                .and_then(Value::as_str)
                == Some("queueAfterTerminalReady"),
        ),
    );
    if let Some(agent_icon) = agent_icon {
        launch_settings.insert("icon".to_string(), Value::String(agent_icon));
    }
    launch_settings.insert("agentLaunchPlan".to_string(), launch_plan);
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        Value::Object(runtime_relevant),
    );

    let mut normalized = params.clone();
    normalized.insert("agentId".to_string(), Value::String(agent_id));
    normalized.insert("kind".to_string(), Value::String("agent".to_string()));
    normalized.insert("launchSettings".to_string(), Value::Object(launch_settings));
    normalized.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    normalized
        .entry("lifecycleState".to_string())
        .or_insert_with(|| Value::String("running".to_string()));
    normalized.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if read_text(&normalized, "title").is_none() {
        normalized.insert(
            "title".to_string(),
            Value::String(create_agent_session_default_title(
                read_text_from_map(&agent_config, "name").as_deref(),
                normalized.get("agentId").and_then(Value::as_str),
            )),
        );
    }
    Ok(normalized)
}

pub(crate) fn create_agent_session_default_title(
    agent_name: Option<&str>,
    agent_id: Option<&str>,
) -> String {
    let title_name = normalize_agent_session_title_name(agent_name)
        .or_else(|| {
            default_agent_session_title_name(agent_id.unwrap_or_default()).map(str::to_string)
        })
        .or_else(|| normalize_agent_session_title_name(agent_id));
    title_name
        .map(|name| format!("{name} Session"))
        .unwrap_or_else(|| "Terminal Session".to_string())
}

pub(crate) fn normalize_agent_session_title_name(value: Option<&str>) -> Option<String> {
    let normalized = value?
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn default_agent_session_title_name(agent_id: &str) -> Option<&'static str> {
    match agent_id.trim().to_ascii_lowercase().as_str() {
        "amp" => Some("Amp CLI"),
        "antigravity" => Some("Antigravity CLI"),
        "claude" => Some("Claude"),
        "codebuddy" => Some("CodeBuddy"),
        "codex" => Some("Codex"),
        "copilot" => Some("Copilot"),
        "cursor" => Some("Cursor CLI"),
        "droid" => Some("Factory Droid"),
        "gemini" => Some("Gemini"),
        "grok" => Some("Grok Build"),
        "hermes-agent" => Some("Hermes Agent"),
        "kiro" => Some("Kiro"),
        "omp" => Some("OMP"),
        "opencode" => Some("OpenCode"),
        "pi" => Some("Pi"),
        "qoder" => Some("Qoder"),
        "rovodev" => Some("Rovo Dev"),
        _ => None,
    }
}

pub(crate) struct AgentLaunchInput {
    pub(crate) accept_all_mode: Option<String>,
    pub(crate) agent_id: String,
    pub(crate) agent_session_id: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) delayed_send_deadline_at: Option<String>,
    pub(crate) first_user_message: Option<String>,
    pub(crate) global_accept_all_enabled: bool,
    pub(crate) icon: Option<String>,
}

pub(crate) fn build_agent_launch_plan(input: AgentLaunchInput) -> Value {
    let base_command = input
        .command
        .or_else(|| default_agent_command(&input.agent_id).map(str::to_string))
        .unwrap_or_default();
    let launch_command = resolve_agent_launch_command(
        &input.agent_id,
        &base_command,
        input.accept_all_mode.as_deref(),
        input.global_accept_all_enabled,
        input.icon.as_deref(),
    );
    let command = if input.agent_id == "cursor" {
        input
            .agent_session_id
            .filter(|value| !value.trim().is_empty())
            .and_then(|session_id| get_cursor_chat_session_id(Some(&session_id)))
            .map(|chat_id| {
                format!(
                    "{launch_command} --resume {}",
                    quote_shell_double_arg(&chat_id)
                )
            })
            .unwrap_or(launch_command)
    } else {
        launch_command
    };
    let mut plan = Map::new();
    plan.insert("agentCommand".to_string(), Value::String(base_command));
    plan.insert("command".to_string(), Value::String(command.clone()));
    if let Some(deadline) = input.delayed_send_deadline_at {
        plan.insert(
            "delayedSend".to_string(),
            json!({ "deadlineAt": deadline, "disposition": "scheduled" }),
        );
    }
    if let Some(message) = input.first_user_message {
        plan.insert("firstUserMessage".to_string(), Value::String(message));
    }
    plan.insert(
        "startupText".to_string(),
        Value::String(if command.is_empty() {
            String::new()
        } else {
            as_atuin_ignored_shell_input(&command)
        }),
    );
    plan.insert(
        "startupTextDisposition".to_string(),
        Value::String(if command.is_empty() {
            "none".to_string()
        } else {
            "queueAfterTerminalReady".to_string()
        }),
    );
    Value::Object(plan)
}

pub(crate) fn resolve_project_agent_config(
    project: &Value,
    agent_id: &str,
    launch_settings: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let normalized_agent_id = agent_id.trim().to_ascii_lowercase();
    if let Some(agent) = project
        .get("customAgents")
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents.iter().find(|candidate| {
                candidate
                    .as_object()
                    .and_then(|agent| agent.get("agentId").and_then(Value::as_str))
                    .map(|id| id.trim().eq_ignore_ascii_case(&normalized_agent_id))
                    .unwrap_or(false)
            })
        })
        .and_then(Value::as_object)
    {
        return agent.clone();
    }
    launch_settings
        .filter(|settings| read_text_from_map(settings, "agentCommand").is_some())
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn resolve_agent_launch_command(
    agent_id: &str,
    command: &str,
    accept_all_mode: Option<&str>,
    global_accept_all_enabled: bool,
    icon: Option<&str>,
) -> String {
    let enabled = match accept_all_mode {
        Some("enabled") => true,
        Some("disabled") => false,
        _ => global_accept_all_enabled,
    };
    apply_accept_all_spec(
        command,
        agent_id,
        enabled,
        icon,
        accept_all_mode == Some("disabled"),
    )
}
