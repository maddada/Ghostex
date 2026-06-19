use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::{
    domain::{read_project_id, read_session_id, DomainRepository, DomainStateError},
    ids::is_gxserver_session_id,
    presentation::project_session_title_projection,
    zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext},
};

const AGENT_SETTINGS_METADATA_KEY: &str = "agents.settings.v1";
const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
const MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH: usize = 120;
const INITIAL_ACTIVITY_SUPPRESSION_MS: i64 = 12_000;
const ESCAPE_ATTENTION_SUPPRESSION_MS: i64 = 5_000;

#[derive(Debug)]
pub enum AgentEndpointError {
    DependencyUnavailable(String),
    Domain(DomainStateError),
}

impl From<DomainStateError> for AgentEndpointError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ZmxEndpointError> for AgentEndpointError {
    fn from(error: ZmxEndpointError) -> Self {
        match error {
            ZmxEndpointError::DependencyUnavailable(message) => {
                Self::DependencyUnavailable(message)
            }
            ZmxEndpointError::Domain(error) => Self::Domain(error),
        }
    }
}

pub struct AgentEndpointOutput {
    pub presentation_session: Option<(String, String)>,
    pub result: Value,
}

/*
CDXC:GxserverRustPort 2026-06-16-10:00:
Phase 6 moves agent policy, launch/resume planning, passive title/status ingestion, and fork planning into Rust while keeping the TypeScript RPC shape. These handlers mutate only durable session metadata and never log raw titles, prompts, hook payloads, or command output.

CDXC:GxserverAgentSettings 2026-06-19-13:59:
Agent settings parity uses the TypeScript metadata key `agents.settings.v1` and stores Default Prompt Agent beside global Accept All. Normalize the prompt-agent id at the daemon boundary by trimming whitespace, falling back to `codex`, and capping it to 120 chars without validating against a client-local agent registry.
*/
pub fn dispatch_agent_endpoint(
    repository: &DomainRepository<'_>,
    db: &Connection,
    endpoint_path: &str,
    params: &Map<String, Value>,
    zmx_context: Option<&ZmxServerContext>,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let output = match endpoint_path {
        "/api/readAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: read_agent_settings_with_metadata(db)?,
        },
        "/api/updateAgentSettings" => AgentEndpointOutput {
            presentation_session: None,
            result: json!({ "settings": update_agent_settings(db, params)? }),
        },
        "/api/readAgentLaunchPlan" => {
            let project_id = read_project_id(params)?;
            let project = require_project(repository, &project_id)?;
            let agent_id = read_required_text(params.get("agentId"), "agentId")?;
            let settings = read_agent_settings(db)?;
            AgentEndpointOutput {
                presentation_session: None,
                result: json!({
                    "plan": build_project_agent_launch_plan(
                        &project,
                        &agent_id,
                        read_text(params, "agentSessionId"),
                        &settings,
                    )
                }),
            }
        }
        "/api/readAgentResumePlan" => {
            let lifecycle = read_lifecycle(params)?;
            let project = require_project(repository, &lifecycle.project_id)?;
            let session = require_session(repository, &lifecycle)?;
            let settings = read_agent_settings(db)?;
            AgentEndpointOutput {
                presentation_session: None,
                result: json!({
                    "plan": build_agent_resume_plan(&project, &session, &settings),
                    "session": session,
                }),
            }
        }
        "/api/forkSession" => {
            let context = zmx_context.ok_or_else(|| {
                AgentEndpointError::DependencyUnavailable(
                    "Cannot fork session without gxserver zmx context.".to_string(),
                )
            })?;
            let lifecycle = read_lifecycle(params)?;
            fork_session(repository, &lifecycle, db, context)?
        }
        "/api/requestSessionRename" => {
            let lifecycle = read_lifecycle(params)?;
            let result = request_session_rename(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/cancelFirstPromptAutoTitle" => {
            let lifecycle = read_lifecycle(params)?;
            let result = cancel_first_prompt_auto_title(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestSessionStateEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let result = ingest_session_state_event(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestTerminalTitleEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let result = ingest_terminal_title_event(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/updateAgentActivity" => {
            let lifecycle = read_lifecycle(params)?;
            let result = update_agent_activity_endpoint(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestAgentHookEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let result = ingest_agent_hook_event(repository, &lifecycle, params)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        _ => {
            return Err(DomainStateError::not_found(format!(
                "{endpoint_path} is not a gxserver agent endpoint."
            ))
            .into())
        }
    };
    Ok(output)
}

fn read_agent_settings_with_metadata(db: &Connection) -> Result<Value, DomainStateError> {
    let row = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [AGENT_SETTINGS_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(json!({
        "isPersisted": row.is_some(),
        "settings": normalize_agent_settings(row.as_deref().map(parse_json_object).as_ref()),
    }))
}

fn read_agent_settings(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
    let value = read_agent_settings_with_metadata(db)?;
    Ok(object_field(&value, "settings"))
}

fn update_agent_settings(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut settings = read_agent_settings(db)?;
    if let Some(value) = params.get("agentAcceptAllEnabled").and_then(Value::as_bool) {
        settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(value));
    }
    if let Some(value) = params.get("defaultPromptAgentId").and_then(Value::as_str) {
        settings.insert(
            "defaultPromptAgentId".to_string(),
            Value::String(value.to_string()),
        );
    }
    let value = Value::Object(normalize_agent_settings(Some(&Value::Object(settings))));
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            AGENT_SETTINGS_METADATA_KEY,
            serde_json::to_string(&value).map_err(|error| {
                DomainStateError::bad_request(format!(
                    "Agent settings must be JSON serializable: {error}"
                ))
            })?,
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(value)
}

fn normalize_agent_settings(value: Option<&Value>) -> Map<String, Value> {
    let object = value.and_then(Value::as_object);
    let accept_all = object
        .and_then(|settings| settings.get("agentAcceptAllEnabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut settings = Map::new();
    settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(accept_all));
    settings.insert(
        "defaultPromptAgentId".to_string(),
        Value::String(normalize_default_prompt_agent_id(
            object
                .and_then(|settings| settings.get("defaultPromptAgentId"))
                .and_then(Value::as_str),
        )),
    );
    settings
}

fn normalize_default_prompt_agent_id(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or_default().trim();
    let normalized = if trimmed.is_empty() {
        DEFAULT_PROMPT_AGENT_ID
    } else {
        trimmed
    };
    normalized
        .chars()
        .take(MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH)
        .collect()
}

fn build_project_agent_launch_plan(
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

struct AgentLaunchInput {
    accept_all_mode: Option<String>,
    agent_id: String,
    agent_session_id: Option<String>,
    command: Option<String>,
    delayed_send_deadline_at: Option<String>,
    first_user_message: Option<String>,
    global_accept_all_enabled: bool,
    icon: Option<String>,
}

fn build_agent_launch_plan(input: AgentLaunchInput) -> Value {
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
            .map(|session_id| {
                format!(
                    "{launch_command} --resume {}",
                    quote_shell_double_arg(&session_id)
                )
            })
            .unwrap_or(launch_command)
    } else {
        launch_command
    };
    let mut plan = Map::new();
    if !base_command.is_empty() {
        plan.insert("agentCommand".to_string(), Value::String(base_command));
    }
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

fn build_agent_resume_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
    let agent_id = read_text_value(session, "agentId");
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let agent_config = resolve_project_agent_config(
        project,
        agent_id.as_deref().unwrap_or(""),
        Some(&launch_settings),
    );
    let base_command = read_text_from_map(&runtime_settings, "agentCommand")
        .or_else(|| read_text_from_map(&agent_config, "command"))
        .or_else(|| {
            agent_id
                .as_deref()
                .and_then(default_agent_command)
                .map(str::to_string)
        });
    let runtime_command = agent_id.as_deref().and_then(|agent_id| {
        base_command.as_ref().map(|command| {
            resolve_agent_launch_command(
                agent_id,
                command,
                read_text_from_map(&agent_config, "acceptAllMode")
                    .or_else(|| read_text_from_map(&launch_settings, "acceptAllMode"))
                    .as_deref(),
                settings
                    .get("agentAcceptAllEnabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                read_text_from_map(&agent_config, "icon")
                    .or_else(|| read_text_from_map(&launch_settings, "icon"))
                    .as_deref(),
            )
        })
    });
    let primary_command = match (agent_id.as_deref(), runtime_command.as_deref()) {
        (Some(agent_id), Some(command)) => build_agent_resume_command(
            agent_id,
            command,
            read_text_from_map(&runtime_settings, "agentSessionId")
                .or_else(|| read_text_from_map(&runtime_settings, "agentSessionPath")),
            trusted_resume_title(session),
        ),
        _ => None,
    };
    let copy_command = match (agent_id.as_deref(), runtime_command.as_deref()) {
        (Some(agent_id), Some(command)) => build_agent_resume_command(
            agent_id,
            command,
            read_text_from_map(&runtime_settings, "agentSessionId")
                .or_else(|| read_text_from_map(&runtime_settings, "agentSessionPath")),
            None,
        ),
        _ => None,
    };
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", agent_id);
    insert_optional_string(&mut plan, "baseCommand", base_command.clone());
    insert_optional_string(&mut plan, "copyCommand", copy_command);
    insert_optional_string(&mut plan, "displayCommand", primary_command.clone());
    insert_optional_string(&mut plan, "lookupCommand", base_command);
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(&mut plan, "runtimeCommand", runtime_command);
    if let Some(command) = primary_command {
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&command)),
        );
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("queueAfterTerminalReady".to_string()),
        );
    } else {
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("none".to_string()),
        );
    }
    Value::Object(plan)
}

fn build_agent_resume_command(
    agent_id: &str,
    command: &str,
    exact_reference: Option<String>,
    title_reference: Option<String>,
) -> Option<String> {
    let reference = exact_reference.or(title_reference)?;
    let quoted = quote_shell_double_arg(&reference);
    match agent_id {
        "amp" => Some(format!("{command} threads continue {quoted}")),
        "antigravity" => Some(format!("{command} --conversation {quoted}")),
        "claude" => Some(format!("{command} --resume {quoted}")),
        "codex" => Some(format!("{command} resume {quoted}")),
        "cursor" => Some(format!("{command} --resume {quoted}")),
        "grok" => Some(format!("{command} -r {quoted}")),
        "kiro" => Some(format!("{command} --resume-id {quoted}")),
        "omp" | "opencode" | "pi" => Some(format!("{command} --session {quoted}")),
        "rovodev" => Some(if command.contains("rovodev") {
            format!("{command} --restore {quoted}")
        } else {
            format!("{command} rovodev run --restore {quoted}")
        }),
        "codebuddy" | "copilot" | "droid" | "gemini" | "hermes-agent" | "qoder" => {
            Some(format!("{command} --resume {quoted}"))
        }
        _ => None,
    }
}

fn fork_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    db: &Connection,
    context: &ZmxServerContext,
) -> Result<AgentEndpointOutput, AgentEndpointError> {
    let project = require_project(repository, &lifecycle.project_id)?;
    let source_session = require_session(repository, lifecycle)?;
    let settings = read_agent_settings(db)?;
    let plan = build_agent_fork_plan(&project, &source_session, &settings);
    if plan
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        || plan
            .get("primaryCommand")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(DomainStateError::bad_request(
            "Fork is only available for Codex, Claude, and Pi sessions with a restorable identity.",
        )
        .into());
    }
    let fork_params = create_agent_fork_session_params(&project, &source_session, &plan);
    let created_session = repository.create_session(
        fork_params
            .as_object()
            .ok_or_else(|| DomainStateError::corrupt_state("Fork params must be an object."))?,
        false,
    )?;
    let project_id = read_text_value(&created_session, "projectId")
        .ok_or_else(|| DomainStateError::corrupt_state("Forked session is missing projectId."))?;
    let session_id = read_text_value(&created_session, "sessionId")
        .ok_or_else(|| DomainStateError::corrupt_state("Forked session is missing sessionId."))?;
    let start_params = json!({ "projectId": project_id, "sessionId": session_id });
    let provider = dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        start_params
            .as_object()
            .ok_or_else(|| DomainStateError::corrupt_state("Fork provider params missing."))?,
        context,
    )?;
    let session = provider
        .result
        .get("session")
        .cloned()
        .unwrap_or(created_session);
    Ok(AgentEndpointOutput {
        presentation_session: read_session_target(&session),
        result: json!({
            "fork": {
                "plan": plan,
                "provider": provider.result,
                "session": session,
                "sourceSession": source_session,
            }
        }),
    })
}

fn build_agent_fork_plan(project: &Value, session: &Value, settings: &Map<String, Value>) -> Value {
    let resume_plan = build_agent_resume_plan(project, session, settings);
    let agent_id = resume_plan.get("agentId").and_then(Value::as_str);
    let runtime_command = resume_plan.get("runtimeCommand").and_then(Value::as_str);
    let exact_reference = object_field(session, "runtimeSettings")
        .get("agentSessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let trusted_title = trusted_resume_title(session);
    let reference = exact_reference.or(trusted_title);
    let primary_command = match (agent_id, runtime_command, reference) {
        (Some("codex"), Some(command), Some(reference)) => Some(format!(
            "{command} fork {}",
            quote_shell_double_arg(&reference)
        )),
        (Some("claude"), Some(command), Some(reference)) => Some(format!(
            "{command} --resume {} --fork-session",
            quote_shell_double_arg(&reference)
        )),
        (Some("pi"), Some(command), Some(reference)) => Some(format!(
            "{command} --fork {}",
            quote_shell_double_arg(&reference)
        )),
        _ => None,
    };
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", agent_id.map(str::to_string));
    insert_optional_string(
        &mut plan,
        "baseCommand",
        resume_plan
            .get("baseCommand")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    insert_optional_string(&mut plan, "displayCommand", primary_command.clone());
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(
        &mut plan,
        "runtimeCommand",
        runtime_command.map(str::to_string),
    );
    if let Some(command) = primary_command {
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&command)),
        );
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("queueAfterTerminalReady".to_string()),
        );
    } else {
        plan.insert(
            "startupTextDisposition".to_string(),
            Value::String("none".to_string()),
        );
    }
    Value::Object(plan)
}

fn create_agent_fork_session_params(
    project: &Value,
    source_session: &Value,
    plan: &Value,
) -> Value {
    let agent_id = plan
        .get("agentId")
        .and_then(Value::as_str)
        .or_else(|| source_session.get("agentId").and_then(Value::as_str))
        .unwrap_or("codex");
    let source_session_id = source_session
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let startup_text = plan
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty());
    let title = format!(
        "{} Fork",
        source_session
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Terminal Session")
    );
    let cwd = read_text_value(source_session, "cwd").or_else(|| read_text_value(project, "path"));
    let mut launch_plan = Map::new();
    insert_optional_string(
        &mut launch_plan,
        "agentCommand",
        plan.get("baseCommand")
            .and_then(Value::as_str)
            .map(str::to_string),
    );
    launch_plan.insert(
        "command".to_string(),
        Value::String(
            plan.get("primaryCommand")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    launch_plan.insert(
        "startupText".to_string(),
        Value::String(startup_text.clone().unwrap_or_default()),
    );
    launch_plan.insert(
        "startupTextDisposition".to_string(),
        plan.get("startupTextDisposition")
            .cloned()
            .unwrap_or_else(|| json!("none")),
    );
    let mut params = Map::new();
    params.insert("agentId".to_string(), json!(agent_id));
    insert_optional_string(&mut params, "cwd", cwd);
    params.insert("kind".to_string(), json!("agent"));
    params.insert(
        "launchSettings".to_string(),
        json!({
            "agentLaunchPlan": launch_plan,
            "forkedFromSessionId": source_session_id,
            "runtimeRelevant": { "queueProviderStartupText": startup_text.is_some() },
        }),
    );
    params.insert("lifecycleState".to_string(), json!("running"));
    params.insert(
        "providerState".to_string(),
        json!({ "lifecycleState": "missing", "provider": "zmx" }),
    );
    params.insert(
        "projectId".to_string(),
        project.get("projectId").cloned().unwrap_or(Value::Null),
    );
    params.insert(
        "restoredFromSessionId".to_string(),
        json!(source_session_id),
    );
    params.insert(
        "runtimeSettings".to_string(),
        json!({
            "agentActivity": default_activity(Some(agent_id), startup_text.is_some().then_some("working")),
            "agentCommand": plan.get("baseCommand").and_then(Value::as_str),
            "launchAgentId": agent_id,
            "agentName": agent_id,
            "forkedFromSessionId": source_session_id,
            "startupText": startup_text,
            "titleSource": "placeholder",
        }),
    );
    if let Some(surface) = source_session.get("surface").cloned() {
        params.insert("surface".to_string(), surface);
    }
    params.insert("title".to_string(), Value::String(title));
    Value::Object(params)
}

fn request_session_rename(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let requested_title = read_text(params, "title");
    let Some(title) = requested_title else {
        return Ok(json!({
            "changed": false,
            "pendingAgentMetadata": false,
            "projection": project_session_title_projection(&session),
            "reason": "empty-title",
            "session": session,
            "shouldSendAgentRenameCommand": false,
        }));
    };
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(&session, "agentId"),
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: object_field(&session, "runtimeSettings"),
        startup_text: None,
    });
    if is_agent_associated(&session, &identity) {
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        if let Some(agent_id) = identity.agent_id.clone() {
            runtime_settings.insert("agentName".to_string(), json!(agent_id));
        }
        insert_optional_string(
            &mut runtime_settings,
            "agentSessionId",
            identity.agent_session_id,
        );
        insert_optional_string(
            &mut runtime_settings,
            "agentSessionPath",
            identity.agent_session_path,
        );
        runtime_settings.insert(
            "pendingAgentTitleRequestRequestedAt".to_string(),
            json!(now_iso()),
        );
        runtime_settings.insert(
            "pendingAgentTitleRequestStatus".to_string(),
            json!("pending"),
        );
        runtime_settings.insert("pendingAgentTitleRequestTitle".to_string(), json!(title));
        runtime_settings.insert(
            "pendingAgentTitleRequestTitleSource".to_string(),
            json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
        );
        let mut update = lifecycle_update(lifecycle);
        if session.get("kind").and_then(Value::as_str) == Some("terminal") {
            update.insert("kind".to_string(), json!("agent"));
        }
        if session.get("agentId").and_then(Value::as_str).is_none() {
            insert_optional_string(&mut update, "agentId", identity.agent_id);
        }
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let updated = repository.update_session(&update)?;
        return Ok(json!({
            "changed": updated.get("updatedAt") != session.get("updatedAt"),
            "pendingAgentMetadata": true,
            "projection": project_session_title_projection(&updated),
            "reason": "agent-rename-request-pending-metadata",
            "session": updated,
            "shouldSendAgentRenameCommand": true,
        }));
    }
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    runtime_settings.insert(
        "titleSource".to_string(),
        json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    update.insert("title".to_string(), Value::String(title));
    let updated = repository.update_session(&update)?;
    Ok(json!({
        "changed": true,
        "pendingAgentMetadata": false,
        "projection": project_session_title_projection(&updated),
        "reason": "non-agent-title-applied",
        "session": updated,
        "shouldSendAgentRenameCommand": false,
    }))
}

fn cancel_first_prompt_auto_title(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let runtime_settings = object_field(&session, "runtimeSettings");
    let previous_status =
        read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus");
    if previous_status.as_deref() != Some("running") {
        let mut result = Map::new();
        result.insert("changed".to_string(), Value::Bool(false));
        if let Some(status) = previous_status.clone() {
            result.insert("previousStatus".to_string(), Value::String(status.clone()));
            result.insert(
                "reason".to_string(),
                Value::String(format!("already-{status}")),
            );
        } else {
            result.insert(
                "reason".to_string(),
                Value::String("not-running".to_string()),
            );
        }
        result.insert("session".to_string(), session);
        return Ok(Value::Object(result));
    }
    let mut next_runtime = runtime_settings;
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleCancelledAt".to_string(),
        json!(now_iso()),
    );
    if let Some(prompt) = read_text_from_map(&next_runtime, "firstUserMessage") {
        next_runtime.insert(
            "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
            json!(prompt),
        );
    }
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleReason".to_string(),
        json!(read_text(params, "reason").unwrap_or_else(|| "userCancelled".to_string())),
    );
    next_runtime.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("cancelled"),
    );
    let mut update = lifecycle_update(lifecycle);
    update.insert("runtimeSettings".to_string(), Value::Object(next_runtime));
    let updated = repository.update_session(&update)?;
    Ok(json!({
        "changed": true,
        "previousStatus": "running",
        "reason": "cancelled",
        "session": updated,
    }))
}

fn ingest_session_state_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let incoming_identity = resolve_session_identity(&IdentityInput {
        agent_id: None,
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: Map::new(),
        startup_text: read_text(params, "startupText"),
    });
    if launch_agent_mismatch(&current, incoming_identity.agent_id.as_deref()) {
        return Ok(json!({
            "changed": false,
            "projection": project_session_title_projection(&current),
            "reason": "session-state-agent-mismatch",
            "session": current,
        }));
    }
    let (mut result, session) =
        apply_session_state_update(repository, lifecycle, params, "passive")?;
    let claimed =
        claim_first_prompt_auto_title(repository, &session, read_text(params, "firstUserMessage"))?;
    if let Some(claimed_session) = claimed {
        result.insert("changed".to_string(), Value::Bool(true));
        result.insert(
            "projection".to_string(),
            project_session_title_projection(&claimed_session),
        );
        result.insert(
            "reason".to_string(),
            Value::String("first-prompt-auto-title-claimed".to_string()),
        );
        result.insert("session".to_string(), claimed_session);
    }
    Ok(Value::Object(result))
}

fn ingest_terminal_title_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let raw_title = read_text(params, "rawTitle");
    let visible_title = raw_title.as_deref().and_then(get_visible_terminal_title);
    let title_detected_agent = raw_title
        .as_deref()
        .and_then(get_terminal_title_detected_agent_name);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    let agent_name = read_text(params, "agentName")
        .or(title_detected_agent.clone())
        .or_else(|| read_text_from_map(&runtime_settings, "agentName"))
        .or_else(|| read_text_value(&current, "agentId"));
    let captured_agent_session_id = raw_title
        .as_deref()
        .and_then(get_codex_session_id_from_title)
        .filter(|_| normalize_agent_id(agent_name.as_deref()).as_deref() == Some("codex"));
    /*
    CDXC:GxserverRustPort 2026-06-16-00:49:
    Codex zmx terminal titles can carry a UUID identity without a user-visible title. Treat a newly captured UUID as a real metadata change so Rust matches the TypeScript changed contract while still keeping the raw title out of logs and sidebar titles.
    */
    let previous_agent_session_id = read_text_from_map(&runtime_settings, "agentSessionId");
    let captured_agent_session_changed = captured_agent_session_id
        .as_deref()
        .is_some_and(|next| previous_agent_session_id.as_deref() != Some(next));
    if let Some(agent_session_id) = captured_agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    let mut changed = captured_agent_session_changed;
    let mut reason = if captured_agent_session_id.is_some() {
        "captured-agent-session-id".to_string()
    } else {
        "terminal-title-not-visible".to_string()
    };
    let mut update = lifecycle_update(lifecycle);
    if let Some(agent) = title_detected_agent.clone() {
        update.insert("kind".to_string(), json!("agent"));
        update.insert("agentId".to_string(), json!(agent.clone()));
        runtime_settings.insert("agentName".to_string(), json!(agent));
        changed = true;
    }
    if let Some(title) = visible_title.clone() {
        if params
            .get("protectStoredTitleFromAutomation")
            .and_then(Value::as_bool)
            != Some(true)
            && current.get("title").and_then(Value::as_str) != Some(title.as_str())
        {
            update.insert("title".to_string(), json!(title));
            runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
            changed = true;
            reason = "zmx-terminal-title-from-user".to_string();
        } else if reason == "terminal-title-not-visible" {
            reason = "already-synced".to_string();
        }
    }
    let activity_update = compute_activity_update(&current, params, Some("title"));
    let should_update_activity = should_persist_activity_update(&current, &activity_update);
    if should_update_activity {
        runtime_settings.insert(
            "agentActivity".to_string(),
            activity_update.activity.clone(),
        );
        if let Some(last_active_at) = activity_update.last_active_at.clone() {
            update.insert("lastActiveAt".to_string(), json!(last_active_at));
        }
    }
    if changed || captured_agent_session_id.is_some() || should_update_activity {
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let session = repository.update_session(&update)?;
        Ok(json!({
            "agentSessionId": captured_agent_session_id,
            "activity": activity_update.activity,
            "changed": changed,
            "enteredAttention": activity_update.entered_attention,
            "previousActivity": activity_update.previous_activity,
            "projection": project_session_title_projection(&session),
            "reason": reason,
            "session": session,
            "visibleTitle": visible_title,
        }))
    } else {
        Ok(json!({
            "agentSessionId": captured_agent_session_id,
            "activity": activity_update.activity,
            "changed": false,
            "enteredAttention": activity_update.entered_attention,
            "previousActivity": activity_update.previous_activity,
            "projection": project_session_title_projection(&current),
            "reason": reason,
            "session": current,
            "visibleTitle": visible_title,
        }))
    }
}

fn update_agent_activity_endpoint(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    if launch_agent_mismatch(&current, read_text(params, "agentName").as_deref()) {
        let previous = normalize_agent_activity_state(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        return Ok(json!({
            "activity": previous,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "session": current,
        }));
    }
    let update = compute_activity_update(&current, params, None);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    runtime_settings.insert("agentActivity".to_string(), update.activity.clone());
    let mut session_update = lifecycle_update(lifecycle);
    session_update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if let Some(last_active_at) = update.last_active_at.clone() {
        session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
    }
    let session = repository.update_session(&session_update)?;
    Ok(json!({
        "activity": update.activity,
        "enteredAttention": update.entered_attention,
        "previousActivity": update.previous_activity,
        "session": session,
    }))
}

fn ingest_agent_hook_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let hook_activity = normalize_agent_hook_activity(
        params.get("status"),
        params
            .get("eventName")
            .or_else(|| params.get("rawEventName")),
        params.get("agentName"),
    );
    if launch_agent_mismatch(&current, read_text(params, "agentName").as_deref()) {
        let previous = normalize_agent_activity_state(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        return Ok(json!({
            "activity": previous,
            "changed": false,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "projection": project_session_title_projection(&current),
            "reason": "agent-hook-agent-mismatch",
            "session": current,
        }));
    }
    let (metadata_result, mut session) =
        apply_session_state_update(repository, lifecycle, params, "passive")?;
    let mut changed = metadata_result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut reason = metadata_result
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("unchanged")
        .to_string();
    let mut activity_update: Option<ActivityUpdate> = None;
    if let Some(activity) = hook_activity {
        let now_ms = params
            .get("statusUpdatedAt")
            .and_then(Value::as_str)
            .and_then(parse_iso_ms)
            .unwrap_or_else(now_ms);
        let mut activity_params = params.clone();
        activity_params.insert("activity".to_string(), json!(activity));
        activity_params.insert(
            "nowMs".to_string(),
            Value::Number(serde_json::Number::from(now_ms)),
        );
        let update = compute_activity_update(&session, &activity_params, None);
        if !is_stale_activity_event(&session, now_ms) {
            let mut runtime_settings = object_field(&session, "runtimeSettings");
            runtime_settings.insert("agentActivity".to_string(), update.activity.clone());
            let mut session_update = lifecycle_update(lifecycle);
            session_update.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime_settings),
            );
            if let Some(last_active_at) = update.last_active_at.clone() {
                session_update.insert("lastActiveAt".to_string(), json!(last_active_at));
            }
            session = repository.update_session(&session_update)?;
            changed = true;
            reason = "activity-updated".to_string();
            activity_update = Some(update);
        } else {
            reason = "stale-activity-event".to_string();
        }
    }
    if let Some(claimed_session) =
        claim_first_prompt_auto_title(repository, &session, read_text(params, "firstUserMessage"))?
    {
        session = claimed_session;
        changed = true;
        reason = "first-prompt-auto-title-claimed".to_string();
    }
    let mut result = Map::new();
    if let Some(update) = activity_update {
        result.insert("activity".to_string(), update.activity);
        result.insert(
            "enteredAttention".to_string(),
            Value::Bool(update.entered_attention),
        );
        result.insert(
            "previousActivity".to_string(),
            Value::String(update.previous_activity),
        );
    } else {
        result.insert("enteredAttention".to_string(), Value::Bool(false));
    }
    result.insert("changed".to_string(), Value::Bool(changed));
    result.insert(
        "projection".to_string(),
        project_session_title_projection(&session),
    );
    result.insert("reason".to_string(), Value::String(reason));
    result.insert("session".to_string(), session);
    Ok(Value::Object(result))
}

fn apply_session_state_update(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    identity_update_source: &str,
) -> Result<(Map<String, Value>, Value), DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(&session, "agentId"),
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: object_field(&session, "runtimeSettings"),
        startup_text: read_text(params, "startupText"),
    });
    let mut runtime_settings = object_field(&session, "runtimeSettings");
    let current_agent = normalize_agent_id(read_text_value(&session, "agentId").as_deref());
    let next_agent = identity.agent_id.clone().or(current_agent.clone());
    let agent_changed =
        current_agent.is_some() && next_agent.is_some() && current_agent != next_agent;
    if let Some(agent_id) = next_agent.clone() {
        runtime_settings.insert("agentName".to_string(), json!(agent_id));
        if identity_update_source == "live-process" {
            runtime_settings.insert("launchAgentId".to_string(), json!(agent_id));
        }
    }
    if let Some(agent_session_id) = identity.agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    } else if agent_changed {
        runtime_settings.remove("agentSessionId");
    }
    if let Some(agent_session_path) = identity.agent_session_path.clone() {
        runtime_settings.insert("agentSessionPath".to_string(), json!(agent_session_path));
    } else if agent_changed {
        runtime_settings.remove("agentSessionPath");
    }
    if agent_changed {
        runtime_settings.remove("agentId");
        runtime_settings.remove("agentActivity");
    }
    insert_optional_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationAgent",
    );
    insert_optional_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationCommand",
    );
    insert_optional_from_params(&mut runtime_settings, params, "firstUserMessage");

    let should_promote_agent = next_agent.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some();
    let mut title =
        read_text_value(&session, "title").unwrap_or_else(|| "Terminal Session".to_string());
    let mut reason = "identity-updated".to_string();
    if trusted_resume_title(&session).is_none() {
        if let Some(candidate) = read_text(params, "title")
            .and_then(|title| get_visible_terminal_title(&title))
            .filter(|title| !is_rejected_resume_title(title))
        {
            title = candidate;
            runtime_settings.insert(
                "titleSource".to_string(),
                json!(read_text(params, "titleSource").unwrap_or_else(|| "user".to_string())),
            );
            reason = "event-title".to_string();
        } else if let Some(project_name) = read_text_value(&project, "name") {
            let _ = project_name;
        }
    } else {
        reason = "current-title-already-trusted".to_string();
    }

    let mut update = lifecycle_update(lifecycle);
    if let Some(agent_id) = next_agent.clone() {
        update.insert("agentId".to_string(), json!(agent_id));
    }
    if should_promote_agent {
        update.insert("kind".to_string(), json!("agent"));
    }
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings.clone()),
    );
    update.insert("title".to_string(), json!(title));
    let needs_update = update.get("title") != session.get("title")
        || update.get("kind") != session.get("kind")
        || next_agent != read_text_value(&session, "agentId")
        || Value::Object(runtime_settings) != object_field_value(&session, "runtimeSettings");
    let updated = if needs_update {
        repository.update_session(&update)?
    } else {
        session
    };
    let result = json!({
        "changed": needs_update,
        "projection": project_session_title_projection(&updated),
        "reason": if needs_update { reason } else { "unchanged".to_string() },
        "session": updated.clone(),
    });
    Ok((object_from_value(result), updated))
}

fn claim_first_prompt_auto_title(
    repository: &DomainRepository<'_>,
    session: &Value,
    prompt: Option<String>,
) -> Result<Option<Value>, DomainStateError> {
    let Some(prompt) = prompt.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let mut runtime_settings = object_field(session, "runtimeSettings");
    let status = read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus");
    if matches!(
        status.as_deref(),
        Some("running" | "applied" | "failed" | "skipped" | "cancelled")
    ) {
        return Ok(None);
    }
    if !is_generic_agent_session_title(
        read_text_from_map(&runtime_settings, "agentName")
            .or_else(|| read_text_value(session, "agentId"))
            .as_deref(),
        read_text_value(session, "title").as_deref(),
    ) {
        return Ok(None);
    }
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledAt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledPrompt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleReason");
    runtime_settings.insert("firstUserMessage".to_string(), json!(prompt));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("running"),
    );
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStartedAt".to_string(),
        json!(now_iso()),
    );
    let lifecycle = LifecycleParams {
        project_id: read_text_value(session, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing projectId."))?,
        session_id: read_text_value(session, "sessionId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing sessionId."))?,
    };
    let mut update = lifecycle_update(&lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    repository.update_session(&update).map(Some)
}

struct ActivityUpdate {
    activity: Value,
    entered_attention: bool,
    last_active_at: Option<String>,
    previous_activity: String,
}

fn compute_activity_update(
    session: &Value,
    params: &Map<String, Value>,
    forced_event: Option<&str>,
) -> ActivityUpdate {
    let runtime_settings = object_field(session, "runtimeSettings");
    let previous = normalize_agent_activity_state(runtime_settings.get("agentActivity"), "idle");
    let previous_activity = previous
        .get("activity")
        .and_then(Value::as_str)
        .unwrap_or("idle")
        .to_string();
    let now_ms_value = params
        .get("nowMs")
        .and_then(Value::as_i64)
        .unwrap_or_else(now_ms);
    let now_iso_value = iso_from_ms(now_ms_value);
    let event = forced_event
        .map(str::to_string)
        .or_else(|| read_text(params, "event"));
    let explicit_activity = params
        .get("activity")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "idle" | "working" | "attention"))
        .map(str::to_string);
    let agent_name = normalize_status_agent_name(
        read_text(params, "agentName")
            .or_else(|| read_text_value(session, "agentId"))
            .or_else(|| {
                read_text_from_map(
                    &previous.as_object().cloned().unwrap_or_default(),
                    "agentName",
                )
            })
            .as_deref(),
    );
    let mut next = previous.as_object().cloned().unwrap_or_default();
    match event.as_deref() {
        Some("launch" | "resume" | "agentDetected" | "wake") => {
            next = default_activity(agent_name.as_deref(), Some("idle"))
                .as_object()
                .cloned()
                .unwrap_or_default();
            next.insert(
                "suppressedUntil".to_string(),
                json!(iso_from_ms(now_ms_value + INITIAL_ACTIVITY_SUPPRESSION_MS)),
            );
        }
        Some("acknowledge") => {
            next.insert("activity".to_string(), json!("idle"));
            next.insert("isAcknowledged".to_string(), Value::Bool(true));
            next.insert("lastChangedAt".to_string(), json!(now_iso_value.clone()));
        }
        Some("escape") => {
            if previous_activity == "attention" {
                next.insert("activity".to_string(), json!("idle"));
                next.remove("attentionEventId");
                next.remove("workingSource");
                next.remove("workingStartedAt");
            }
            next.insert("isAcknowledged".to_string(), Value::Bool(true));
            next.insert(
                "attentionSuppressedUntil".to_string(),
                json!(iso_from_ms(now_ms_value + ESCAPE_ATTENTION_SUPPRESSION_MS)),
            );
            if previous_activity == "attention" {
                next.insert("lastChangedAt".to_string(), json!(now_iso_value.clone()));
            }
        }
        _ => {
            let title_signal = if event.as_deref() == Some("title") {
                classify_terminal_title_status(
                    read_text(params, "title").as_deref(),
                    agent_name.as_deref(),
                )
            } else {
                None
            };
            let requested = explicit_activity
                .or_else(|| match event.as_deref() {
                    Some("bell" | "terminalError") => Some("attention".to_string()),
                    Some("terminalExited") => Some("idle".to_string()),
                    _ => None,
                })
                .or_else(|| title_signal.as_ref().map(|(_, state)| state.clone()))
                .unwrap_or_else(|| previous_activity.clone());
            next.insert("activity".to_string(), json!(requested.clone()));
            insert_optional_string(&mut next, "agentName", agent_name);
            next.insert("lastChangedAt".to_string(), json!(now_iso_value.clone()));
            if requested == "working" {
                next.insert("hasSeenWorking".to_string(), Value::Bool(true));
                next.insert("isAcknowledged".to_string(), Value::Bool(false));
                next.insert("workingStartedAt".to_string(), json!(now_iso_value.clone()));
                next.insert(
                    "workingSource".to_string(),
                    json!(if event.as_deref() == Some("title") {
                        "title"
                    } else {
                        "explicit"
                    }),
                );
            } else if requested == "attention" {
                next.insert(
                    "attentionEventId".to_string(),
                    json!(format!("attn_{}", now_ms_value)),
                );
                next.insert("hasSeenWorking".to_string(), Value::Bool(true));
                next.insert("isAcknowledged".to_string(), Value::Bool(false));
            } else {
                next.remove("workingSource");
                next.remove("workingStartedAt");
            }
            if let Some(title) = read_text(params, "title") {
                let same_title =
                    next.get("lastTitle").and_then(Value::as_str) == Some(title.as_str());
                next.insert("lastTitle".to_string(), json!(title));
                if !same_title {
                    next.insert(
                        "lastTitleChangeAt".to_string(),
                        json!(now_iso_value.clone()),
                    );
                }
            }
        }
    }
    let activity = Value::Object(next);
    let next_activity = activity
        .get("activity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let last_active_at = if matches!(next_activity, "working" | "attention") {
        activity
            .get("lastChangedAt")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(Some(now_iso_value))
    } else {
        read_text_value(session, "lastActiveAt")
    };
    ActivityUpdate {
        entered_attention: previous_activity != "attention" && next_activity == "attention",
        activity,
        last_active_at,
        previous_activity,
    }
}

fn should_persist_activity_update(session: &Value, update: &ActivityUpdate) -> bool {
    read_text_value(session, "lastActiveAt") != update.last_active_at
        || object_field(session, "runtimeSettings").get("agentActivity") != Some(&update.activity)
}

fn normalize_agent_activity_state(value: Option<&Value>, fallback: &str) -> Value {
    let record = value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let activity = record
        .get("activity")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "idle" | "working" | "attention"))
        .unwrap_or(fallback);
    let mut output = Map::new();
    output.insert("activity".to_string(), json!(activity));
    for key in [
        "agentName",
        "attentionEventId",
        "attentionSuppressedUntil",
        "lastChangedAt",
        "lastTitle",
        "lastTitleChangeAt",
        "suppressedUntil",
        "workingSource",
        "workingStartedAt",
    ] {
        insert_optional_string(
            &mut output,
            key,
            record.get(key).and_then(Value::as_str).map(str::to_string),
        );
    }
    for key in ["hasSeenWorking", "isAcknowledged"] {
        if let Some(value) = record.get(key).and_then(Value::as_bool) {
            output.insert(key.to_string(), Value::Bool(value));
        }
    }
    Value::Object(output)
}

fn normalize_agent_hook_activity(
    status: Option<&Value>,
    event_name: Option<&Value>,
    agent_name: Option<&Value>,
) -> Option<String> {
    let normalized_agent = normalize_agent_id(agent_name.and_then(Value::as_str));
    let event = event_name
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let lower = event.to_ascii_lowercase().replace(['_', '-', '.'], "");
    if normalized_agent.as_deref() == Some("claude") {
        if matches!(lower.as_str(), "stop" | "idle" | "sessionend") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "userpromptsubmit" | "pretooluse") {
            return Some("working".to_string());
        }
    }
    if matches!(
        lower.as_str(),
        "stop"
            | "agentresponse"
            | "afteragent"
            | "afteragentresponse"
            | "agentend"
            | "oncomplete"
            | "onerror"
            | "turncompletion"
            | "sessionend"
    ) {
        return Some("idle".to_string());
    }
    if matches!(
        lower.as_str(),
        "pretooluse"
            | "posttooluse"
            | "pretoolcall"
            | "beforeagent"
            | "preinvocation"
            | "userpromptsubmit"
            | "agentstart"
            | "beforeshellexecution"
            | "beforesubmitprompt"
    ) {
        return Some("working".to_string());
    }
    if matches!(
        lower.as_str(),
        "notification" | "permissionrequest" | "messageupdated" | "permissionupdated"
    ) {
        return Some("attention".to_string());
    }
    status
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "idle" | "working" | "attention"))
        .map(str::to_string)
}

fn is_stale_activity_event(session: &Value, incoming_now_ms: i64) -> bool {
    let current_changed_at = object_field(session, "runtimeSettings")
        .get("agentActivity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("lastChangedAt"))
        .and_then(Value::as_str)
        .and_then(parse_iso_ms);
    current_changed_at
        .map(|current| incoming_now_ms < current)
        .unwrap_or(false)
}

#[derive(Clone)]
struct IdentityInput {
    agent_id: Option<String>,
    agent_name: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    runtime_settings: Map<String, Value>,
    startup_text: Option<String>,
}

#[derive(Clone, Default)]
struct ResolvedIdentity {
    agent_id: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
}

fn resolve_session_identity(input: &IdentityInput) -> ResolvedIdentity {
    let resume = parse_agent_resume_identity(input.startup_text.as_deref());
    let agent_session_path = input
        .agent_session_path
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionPath"));
    let agent_session_id = input
        .agent_session_id
        .clone()
        .or_else(|| read_text_from_map(&input.runtime_settings, "agentSessionId"))
        .or(resume.agent_session_id);
    let agent_id = normalize_agent_id(input.agent_id.as_deref())
        .or_else(|| normalize_agent_id(input.agent_name.as_deref()))
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentName").as_deref())
        })
        .or_else(|| {
            normalize_agent_id(read_text_from_map(&input.runtime_settings, "agentId").as_deref())
        })
        .or_else(|| infer_agent_id_from_path(agent_session_path.as_deref()))
        .or(resume.agent_id);
    ResolvedIdentity {
        agent_id,
        agent_session_id,
        agent_session_path,
    }
}

fn parse_agent_resume_identity(text: Option<&str>) -> ResolvedIdentity {
    let text = text.unwrap_or_default();
    for (agent_id, needle) in [
        ("codex", "codex"),
        ("claude", "claude"),
        ("cursor", "cursor-agent"),
        ("opencode", "opencode"),
        ("pi", "pi"),
        ("kiro", "kiro-cli"),
        ("omp", "omp"),
    ] {
        let lower = text.to_ascii_lowercase();
        if !lower.contains(needle) {
            continue;
        }
        if let Some(reference) = quoted_or_next_resume_reference(text, needle) {
            return ResolvedIdentity {
                agent_id: Some(agent_id.to_string()),
                agent_session_id: Some(reference),
                agent_session_path: None,
            };
        }
    }
    ResolvedIdentity::default()
}

fn quoted_or_next_resume_reference(text: &str, command: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let index = lower.find(command)?;
    let tail = &text[index + command.len()..];
    let tokens = tail
        .split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', '\r', '\n', ';']))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            *token,
            "resume" | "fork" | "--resume" | "--session" | "-s" | "--resume-id"
        ) {
            return tokens.get(index + 1).map(|value| (*value).to_string());
        }
    }
    None
}

fn normalize_agent_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase().replace('_', " ");
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let mapped = match normalized.as_str() {
        "codex" | "openai codex" | "codex cli" => "codex",
        "claude" | "claude code" => "claude",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "opencode" | "open code" => "opencode",
        "pi" | "π" => "pi",
        "omp" => "omp",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "amp" | "amp cli" => "amp",
        "copilot" | "github copilot" => "copilot",
        "droid" | "factory" | "factory droid" => "droid",
        "grok" | "grok build" => "grok",
        "kiro" | "kiro cli" | "kiro-cli" => "kiro",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        "codebuddy" | "code buddy" => "codebuddy",
        "qoder" | "qodercli" => "qoder",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

fn normalize_status_agent_name(value: Option<&str>) -> Option<String> {
    let agent = normalize_agent_id(value)?;
    matches!(
        agent.as_str(),
        "antigravity" | "claude" | "codex" | "copilot" | "cursor" | "gemini" | "opencode" | "pi"
    )
    .then_some(agent)
}

fn infer_agent_id_from_path(path: Option<&str>) -> Option<String> {
    let lower = path?.to_ascii_lowercase();
    if lower.contains("/.cursor/") && (lower.ends_with(".json") || lower.ends_with(".jsonl")) {
        return Some("cursor".to_string());
    }
    if lower.contains("/.claude/") && lower.ends_with(".jsonl") {
        return Some("claude".to_string());
    }
    if lower.contains("/.codex/") || lower.contains("/.codex-profiles/") {
        return Some("codex".to_string());
    }
    if lower.contains("/.opencode/") || lower.contains("/.config/opencode/") {
        return Some("opencode".to_string());
    }
    if lower.contains("/.pi/agent/") {
        return Some("pi".to_string());
    }
    None
}

fn launch_agent_mismatch(session: &Value, incoming_agent_id: Option<&str>) -> bool {
    let Some(incoming) = normalize_agent_id(incoming_agent_id) else {
        return false;
    };
    let locked = object_field(session, "runtimeSettings")
        .get("launchAgentId")
        .and_then(Value::as_str)
        .and_then(|value| normalize_agent_id(Some(value)))
        .or_else(|| {
            object_field(session, "launchSettings")
                .get("agentLaunchPlan")
                .and_then(Value::as_object)
                .and_then(|plan| {
                    plan.get("agentCommand")
                        .and_then(Value::as_str)
                        .or_else(|| plan.get("command").and_then(Value::as_str))
                })
                .and_then(infer_agent_id_from_command)
        })
        .or_else(|| {
            object_field(session, "launchSettings")
                .get("startupText")
                .and_then(Value::as_str)
                .and_then(infer_agent_id_from_command)
        });
    locked.map(|locked| locked != incoming).unwrap_or(false)
}

fn infer_agent_id_from_command(command: &str) -> Option<String> {
    let command = command.to_ascii_lowercase();
    for (agent, needle) in [
        ("cursor", "cursor-agent"),
        ("hermes-agent", "hermes"),
        ("codebuddy", "codebuddy"),
        ("antigravity", "agy"),
        ("opencode", "opencode"),
        ("rovodev", "rovodev"),
        ("qoder", "qodercli"),
        ("claude", "claude"),
        ("copilot", "copilot"),
        ("gemini", "gemini"),
        ("codex", "codex"),
        ("droid", "droid"),
        ("grok", "grok"),
        ("amp", "amp"),
        ("pi", "pi"),
    ] {
        if command
            .split(|char: char| {
                char.is_whitespace() || matches!(char, ';' | '&' | '|' | '(' | ')' | '/')
            })
            .any(|token| token == needle)
        {
            return Some(agent.to_string());
        }
    }
    None
}

fn is_agent_associated(session: &Value, identity: &ResolvedIdentity) -> bool {
    session.get("kind").and_then(Value::as_str) == Some("agent")
        || session.get("agentId").and_then(Value::as_str).is_some()
        || identity.agent_id.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some()
        || read_text_from_map(&object_field(session, "runtimeSettings"), "agentName").is_some()
}

fn resolve_project_agent_config(
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
                    .and_then(|agent| {
                        agent
                            .get("agentId")
                            .or_else(|| agent.get("id"))
                            .and_then(Value::as_str)
                    })
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

fn resolve_agent_launch_command(
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

fn apply_accept_all_spec(
    command: &str,
    agent_id: &str,
    enabled: bool,
    icon: Option<&str>,
    strip_when_disabled: bool,
) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let spec = accept_all_spec(agent_id).or_else(|| icon.and_then(accept_all_spec));
    let Some(spec) = spec else {
        return trimmed.to_string();
    };
    if let AcceptAllSpec::Environment {
        assignments,
        legacy_aliases,
    } = spec
    {
        let stripped = strip_accept_all_markers(trimmed, &assignments, &legacy_aliases);
        return if enabled {
            format!("{} {}", assignments.join(" "), stripped)
                .trim()
                .to_string()
        } else {
            stripped
        };
    }
    let AcceptAllSpec::Flag { aliases, canonical } = spec else {
        unreachable!();
    };
    if !enabled {
        return if strip_when_disabled {
            strip_accept_all_flags(trimmed, &aliases)
        } else {
            trimmed.to_string()
        };
    }
    let deduped = strip_duplicate_accept_all_flags(trimmed, &aliases);
    if command_includes_accept_all_flag(&deduped, &aliases) {
        deduped
    } else {
        format!("{deduped} {canonical}").trim().to_string()
    }
}

enum AcceptAllSpec {
    Environment {
        assignments: Vec<String>,
        legacy_aliases: Vec<String>,
    },
    Flag {
        aliases: Vec<String>,
        canonical: &'static str,
    },
}

fn accept_all_spec(agent_id: &str) -> Option<AcceptAllSpec> {
    Some(match agent_id {
        "amp" => AcceptAllSpec::Flag {
            aliases: vec!["--dangerously-allow-all".to_string()],
            canonical: "--dangerously-allow-all",
        },
        "antigravity" | "claude" => AcceptAllSpec::Flag {
            aliases: vec!["--dangerously-skip-permissions".to_string()],
            canonical: "--dangerously-skip-permissions",
        },
        "codex" => AcceptAllSpec::Flag {
            aliases: vec!["--yolo".to_string()],
            canonical: "--yolo",
        },
        "copilot" | "cursor" | "gemini" => AcceptAllSpec::Flag {
            aliases: vec![
                "--allow-all".to_string(),
                "--yolo".to_string(),
                "-y".to_string(),
            ],
            canonical: "--yolo",
        },
        "grok" => AcceptAllSpec::Flag {
            aliases: vec!["--always-approve".to_string()],
            canonical: "--always-approve",
        },
        "opencode" => AcceptAllSpec::Environment {
            assignments: vec!["OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'".to_string()],
            legacy_aliases: vec![
                "--dangerously-skip-permissions".to_string(),
                "--yolo".to_string(),
            ],
        },
        _ => return None,
    })
}

fn strip_accept_all_markers(command: &str, assignments: &[String], aliases: &[String]) -> String {
    command
        .split_whitespace()
        .filter(|token| !assignments.iter().any(|assignment| assignment == token))
        .filter(|token| !is_accept_all_flag_token(token, aliases))
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_accept_all_flags(command: &str, aliases: &[String]) -> String {
    command
        .split_whitespace()
        .filter(|token| !is_accept_all_flag_token(token, aliases))
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_duplicate_accept_all_flags(command: &str, aliases: &[String]) -> String {
    let mut seen = false;
    let mut output = Vec::new();
    for token in command.split_whitespace() {
        if is_accept_all_flag_token(token, aliases) {
            if !seen {
                output.push(token);
                seen = true;
            }
            continue;
        }
        output.push(token);
    }
    output.join(" ")
}

fn command_includes_accept_all_flag(command: &str, aliases: &[String]) -> bool {
    command
        .split_whitespace()
        .any(|token| is_accept_all_flag_token(token, aliases))
}

fn is_accept_all_flag_token(token: &str, aliases: &[String]) -> bool {
    aliases.iter().any(|alias| {
        token == alias
            || token
                .strip_prefix(alias)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn default_agent_command(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some("amp"),
        "antigravity" => Some("agy"),
        "claude" => Some("claude"),
        "codebuddy" => Some("codebuddy"),
        "codex" => Some("codex"),
        "copilot" => Some("copilot"),
        "cursor" => Some("cursor-agent"),
        "droid" => Some("droid"),
        "gemini" => Some("gemini"),
        "grok" => Some("grok"),
        "hermes-agent" => Some("hermes"),
        "kiro" => Some("kiro-cli chat --agent ghostex"),
        "omp" => Some("omp"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "qoder" => Some("qodercli"),
        "rovodev" => Some("acli rovodev run"),
        "t3" => Some("npx --yes t3"),
        _ => None,
    }
}

fn get_visible_terminal_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    let lower = normalized.to_ascii_lowercase();
    if lower.starts_with('/')
        || lower.starts_with("~/")
        || lower == "terminal session"
        || lower == "search by text"
        || is_uuid(&normalized)
        || is_generic_agent_title(&lower)
        || is_status_word_title(&lower)
        || normalized.starts_with("Session ")
    {
        return None;
    }
    Some(normalized)
}

fn normalize_terminal_title(title: &str) -> Option<String> {
    let mut value = title.trim().to_string();
    while let Some(first) = value.chars().next() {
        if first.is_whitespace()
            || matches!(
                first,
                '⠸' | '⠴'
                    | '⠼'
                    | '⠧'
                    | '⠦'
                    | '⠏'
                    | '⠋'
                    | '⠇'
                    | '⠙'
                    | '⠹'
                    | '·'
                    | '•'
                    | '⋅'
                    | '◦'
                    | '✳'
                    | '*'
                    | '∗'
                    | '✶'
                    | '✻'
                    | '✽'
                    | '✸'
                    | '✹'
                    | '✺'
                    | '✷'
                    | '✴'
                    | '✦'
                    | '◇'
                    | '🤖'
                    | '🔔'
            )
        {
            value = value[first.len_utf8()..].trim_start().to_string();
        } else {
            break;
        }
    }
    if value.to_ascii_uppercase().starts_with("OC |") {
        value = value[4..].trim().to_string();
    }
    if value.ends_with("✅ Ready") {
        value = value
            .trim_end_matches("✅ Ready")
            .trim_end_matches('-')
            .trim()
            .to_string();
    }
    if value.contains("⏳ Working") {
        value = value
            .split("⏳ Working")
            .next()
            .unwrap_or_default()
            .trim_end_matches('-')
            .trim()
            .to_string();
    }
    if value.starts_with("π") {
        let parts = value
            .split(" - ")
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if parts.len() >= 3 {
            value = parts[1..parts.len() - 1].join(" - ");
        } else {
            value = "π".to_string();
        }
    }
    (!value.trim().is_empty()).then_some(value.trim().to_string())
}

fn get_codex_session_id_from_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    is_uuid(&normalized).then(|| normalized.to_ascii_lowercase())
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn trusted_resume_title(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let title = read_text_value(session, "title")?;
    if read_text_from_map(&runtime_settings, "titleSource").as_deref() == Some("placeholder")
        || is_temporary_title(&title)
    {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_resume_title(&visible)).then_some(visible)
}

fn is_rejected_resume_title(title: &str) -> bool {
    let lower = title.trim().to_ascii_lowercase();
    is_temporary_title(title)
        || is_gxserver_session_id(title.trim())
        || is_status_word_title(&lower)
        || lower.starts_with("codex ")
        || lower.starts_with("claude ")
        || lower.starts_with("cursor-agent ")
        || lower.starts_with("opencode ")
}

fn is_temporary_title(title: &str) -> bool {
    matches!(
        title.trim().to_ascii_lowercase().as_str(),
        "terminal session"
            | "search by text"
            | "codex session"
            | "codex cli session"
            | "claude session"
            | "claude code session"
            | "cursor session"
            | "cursor agent session"
    ) || title.trim().starts_with("Session ")
}

fn is_generic_agent_title(lower: &str) -> bool {
    matches!(
        lower,
        "amp"
            | "amp cli"
            | "agy"
            | "antigravity"
            | "antigravity cli"
            | "claude"
            | "claude code"
            | "codex"
            | "codex cli"
            | "cursor"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "droid"
            | "factory droid"
            | "grok"
            | "grok build"
            | "kiro"
            | "kiro cli"
            | "kiro-cli"
            | "omp"
            | "openai codex"
            | "pi"
            | "π"
            | "ghostex"
    )
}

fn is_status_word_title(lower: &str) -> bool {
    matches!(
        lower.trim(),
        "done" | "error" | "idle" | "thinking" | "working"
    )
}

fn is_generic_agent_session_title(agent_name: Option<&str>, title: Option<&str>) -> bool {
    let title = title.unwrap_or_default().trim().to_ascii_lowercase();
    if is_temporary_title(&title) {
        return true;
    }
    let Some(agent_name) = normalize_agent_id(agent_name) else {
        return false;
    };
    title == format!("{agent_name} session")
        || title == format!("{} cli session", agent_name)
        || title == "terminal session"
}

fn get_terminal_title_detected_agent_name(title: &str) -> Option<String> {
    let normalized = title
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = normalized.to_ascii_lowercase();
    if lower == "claude code" {
        return Some("claude".to_string());
    }
    if lower == "codex" || lower == "codex cli" {
        return Some("codex".to_string());
    }
    if lower == "cursor agent" || lower == "cursor agent - ✅ ready" {
        return Some("cursor".to_string());
    }
    if lower == "agy" || lower == "🔔 agy" {
        return Some("antigravity".to_string());
    }
    if lower == "gemini" || lower == "gemini cli" {
        return Some("gemini".to_string());
    }
    if lower.starts_with("π") {
        return Some("pi".to_string());
    }
    None
}

fn classify_terminal_title_status(
    title: Option<&str>,
    known_agent_name: Option<&str>,
) -> Option<(String, String)> {
    let title = title?;
    let normalized = title
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = normalized.to_ascii_lowercase();
    let agent = normalize_status_agent_name(known_agent_name);
    if lower.contains("action required") && agent.as_deref() == Some("codex") {
        return Some(("codex".to_string(), "attention".to_string()));
    }
    if lower.contains("codex") || agent.as_deref() == Some("codex") {
        let working = normalized.chars().any(|char| "⠸⠴⠼⠧⠦⠏⠋⠇⠙⠹".contains(char));
        return Some((
            "codex".to_string(),
            if working { "working" } else { "idle" }.to_string(),
        ));
    }
    if lower.contains("claude") || agent.as_deref() == Some("claude") {
        let working = normalized.chars().any(|char| "⠐⠂·✶✻✽✸✹✺✷✴".contains(char));
        return Some((
            "claude".to_string(),
            if working { "working" } else { "idle" }.to_string(),
        ));
    }
    if lower.contains("cursor agent") || lower.contains("⏳ working") {
        return Some((
            "cursor".to_string(),
            if lower.contains("⏳ working") {
                "working"
            } else {
                "idle"
            }
            .to_string(),
        ));
    }
    None
}

fn default_activity(agent_id: Option<&str>, override_activity: Option<&str>) -> Value {
    let timestamp = now_iso();
    let mut activity = Map::new();
    activity.insert(
        "activity".to_string(),
        Value::String(override_activity.unwrap_or("idle").to_string()),
    );
    if let Some(agent_id) = agent_id.and_then(|value| normalize_status_agent_name(Some(value))) {
        activity.insert("agentName".to_string(), Value::String(agent_id));
    }
    activity.insert("hasSeenWorking".to_string(), Value::Bool(false));
    activity.insert("isAcknowledged".to_string(), Value::Bool(true));
    activity.insert(
        "lastChangedAt".to_string(),
        Value::String(timestamp.clone()),
    );
    activity.insert("suppressedUntil".to_string(), Value::String(timestamp));
    Value::Object(activity)
}

#[derive(Clone)]
struct LifecycleParams {
    project_id: String,
    session_id: String,
}

fn read_lifecycle(params: &Map<String, Value>) -> Result<LifecycleParams, DomainStateError> {
    Ok(LifecycleParams {
        project_id: read_project_id(params)?,
        session_id: read_session_id(params)?,
    })
}

fn lifecycle_update(lifecycle: &LifecycleParams) -> Map<String, Value> {
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    update
}

fn require_project(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<Value, DomainStateError> {
    repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("Project {project_id} does not exist.")))
}

fn require_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {}/{} does not exist.",
                lifecycle.project_id, lifecycle.session_id
            ))
        })
}

fn read_required_text(value: Option<&Value>, field: &str) -> Result<String, DomainStateError> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{field} is required.")))
}

fn read_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_text_value(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_text_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn object_field(value: &Value, key: &str) -> Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn object_field_value(value: &Value, key: &str) -> Value {
    Value::Object(object_field(value, key))
}

fn object_from_value(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_from_params(
    target: &mut Map<String, Value>,
    params: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = params.get(key).cloned().filter(|value| !value.is_null()) {
        target.insert(key.to_string(), value);
    }
}

fn read_session_target(session: &Value) -> Option<(String, String)> {
    Some((
        read_text_value(session, "projectId")?,
        read_text_value(session, "sessionId")?,
    ))
}

fn quote_shell_double_arg(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}

fn as_atuin_ignored_shell_input(command: &str) -> String {
    let text = command.trim_end_matches(['\r', '\n']);
    if text.starts_with(' ') {
        format!("{text}\r")
    } else {
        format!(" {text}\r")
    }
}

fn parse_json_object(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or(Value::Null)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn iso_from_ms(ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse_iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.timestamp_millis())
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite agent-state error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };

    fn open_test_database() -> (tempfile::TempDir, Connection) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }

    #[test]
    fn agent_settings_use_current_metadata_key_and_default_prompt_agent() {
        let (_temp, db) = open_test_database();
        let initial = read_agent_settings_with_metadata(&db).expect("initial settings");
        assert_eq!(initial.get("isPersisted"), Some(&json!(false)));
        assert_eq!(
            initial
                .get("settings")
                .and_then(|settings| settings.get("agentAcceptAllEnabled")),
            Some(&json!(true))
        );
        assert_eq!(
            initial
                .get("settings")
                .and_then(|settings| settings.get("defaultPromptAgentId")),
            Some(&json!("codex"))
        );

        let updated = update_agent_settings(
            &db,
            json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": " claude " })
                .as_object()
                .expect("params"),
        )
        .expect("update settings");
        assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(false)));
        assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));
        let persisted: String = db
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                [AGENT_SETTINGS_METADATA_KEY],
                |row| row.get(0),
            )
            .expect("persisted settings");
        let persisted_value = parse_json_object(&persisted);
        assert_eq!(
            persisted_value
                .get("defaultPromptAgentId")
                .and_then(Value::as_str),
            Some("claude")
        );
        let legacy_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE key = 'gxserverAgentSettings'",
                [],
                |row| row.get(0),
            )
            .expect("legacy count");
        assert_eq!(legacy_count, 0);
    }

    #[test]
    fn agent_settings_normalize_default_prompt_agent_id() {
        let (_temp, db) = open_test_database();
        let blank = update_agent_settings(
            &db,
            json!({ "defaultPromptAgentId": "   " })
                .as_object()
                .expect("params"),
        )
        .expect("blank update");
        assert_eq!(blank.get("defaultPromptAgentId"), Some(&json!("codex")));

        let long_id = "x".repeat(MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH + 10);
        let capped = update_agent_settings(
            &db,
            json!({ "defaultPromptAgentId": long_id })
                .as_object()
                .expect("params"),
        )
        .expect("long update");
        let stored = capped
            .get("defaultPromptAgentId")
            .and_then(Value::as_str)
            .expect("stored id");
        assert_eq!(stored.len(), MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH);
    }

    #[test]
    fn launch_plan_applies_agent_settings_accept_all() {
        let (_temp, db) = open_test_database();
        let project = json!({
            "customAgents": [{ "agentId": "codex", "command": "codex" }],
            "launchSettings": {},
        });
        let default_settings = read_agent_settings(&db).expect("settings");
        let plan = build_project_agent_launch_plan(&project, "codex", None, &default_settings);
        assert_eq!(plan.get("command"), Some(&json!("codex --yolo")));
        update_agent_settings(
            &db,
            json!({ "agentAcceptAllEnabled": false })
                .as_object()
                .expect("params"),
        )
        .expect("update settings");
        let disabled_settings = read_agent_settings(&db).expect("disabled settings");
        let plan = build_project_agent_launch_plan(&project, "codex", None, &disabled_settings);
        assert_eq!(plan.get("command"), Some(&json!("codex")));
    }

    #[test]
    fn resume_and_fork_plans_shape_agent_commands() {
        let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
        let session = json!({
            "agentId": "codex",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "codex",
                "agentSessionId": "12345678-1234-1234-1234-123456789abc",
                "titleSource": "terminal-auto"
            },
            "title": "Investigate bug",
        });
        let settings = normalize_agent_settings(None);
        let resume = build_agent_resume_plan(&project, &session, &settings);
        assert_eq!(
            resume.get("primaryCommand"),
            Some(&json!(
                "codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""
            ))
        );
        let fork = build_agent_fork_plan(&project, &session, &settings);
        assert_eq!(
            fork.get("primaryCommand"),
            Some(&json!(
                "codex --yolo fork \"12345678-1234-1234-1234-123456789abc\""
            ))
        );
    }

    #[test]
    fn resume_plan_rejects_gxserver_session_id_titles() {
        let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
        let session = json!({
            "agentId": "cursor",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "cursor-agent",
                "titleSource": "user"
            },
            "title": "G3gnt",
        });
        let settings = normalize_agent_settings(None);
        let resume = build_agent_resume_plan(&project, &session, &settings);
        assert_eq!(resume.get("primaryCommand"), None);
        assert_eq!(resume.get("startupTextDisposition"), Some(&json!("none")));
    }

    #[test]
    fn activity_escape_suppresses_attention_without_logging_titles() {
        let session = json!({
            "agentId": "codex",
            "lastActiveAt": "2026-06-16T09:59:00.000Z",
            "runtimeSettings": {
                "agentActivity": {
                    "activity": "attention",
                    "agentName": "codex",
                    "attentionEventId": "attn_old",
                    "hasSeenWorking": true,
                    "isAcknowledged": false,
                    "lastChangedAt": "2026-06-16T10:00:00.000Z"
                }
            }
        });
        let update = compute_activity_update(
            &session,
            json!({ "event": "escape", "nowMs": 1781604000000_i64 })
                .as_object()
                .expect("params"),
            None,
        );
        assert_eq!(update.previous_activity, "attention");
        assert_eq!(
            update.activity.get("activity").and_then(Value::as_str),
            Some("idle")
        );
        assert!(update.activity.get("attentionSuppressedUntil").is_some());
        assert!(update.activity.get("attentionEventId").is_none());
    }

    #[test]
    fn hook_activity_normalizes_provider_events() {
        assert_eq!(
            normalize_agent_hook_activity(
                None,
                Some(&json!("UserPromptSubmit")),
                Some(&json!("Claude Code"))
            ),
            Some("working".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("Stop")), Some(&json!("Claude Code"))),
            Some("idle".to_string())
        );
    }
}
