use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::{
    agent_transcripts::resolve_session_transcript_path,
    domain::{read_project_id, read_session_id, DomainRepository, DomainStateError},
    ids::is_gxserver_session_id,
    presentation::{normalize_pi_terminal_title, project_session_title_projection},
    session_status::{
        compute_activity_update, is_stale_activity_event, normalize_agent_activity_value,
        parse_iso_ms, ActivityUpdate,
    },
    zmx::{dispatch_zmx_lifecycle_endpoint, ZmxEndpointError, ZmxServerContext},
};

const AGENT_SETTINGS_METADATA_KEY: &str = "agents.settings.v1";
const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
const MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH: usize = 120;
const GROK_PERMISSION_MODE_FLAG: &str = "--permission-mode";
const GROK_BYPASS_PERMISSIONS_VALUE: &str = "bypassPermissions";
pub(crate) const FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY: &str =
    "gxserverFirstPromptAutoTitleAttemptId";
/*
CDXC:GxserverFirstUserInputDraft 2026-08-20:
`firstUserMessage` is a prompt gxserver submits for the user. A draft is the
opposite contract: text staged into the new agent's composer and never sent, so
the user writes their own prompt around it (the "Export transcript" result
dialog stages `@<exported markdown> ` this way). The status key is the
consume-once marker, mirroring `gxserverForkInitialRenameStatus`: created
`pending`, claimed by the first provider start, then `applied`/`failed`, so a
wake, re-attach, or daemon restart never retypes it.
*/
pub(crate) const FIRST_USER_INPUT_DRAFT_KEY: &str = "firstUserInputDraft";
pub(crate) const FIRST_USER_INPUT_DRAFT_STATUS_KEY: &str = "gxserverFirstUserInputDraftStatus";
pub(crate) const FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY: &str =
    "gxserverFirstUserInputDraftUpdatedAt";

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

CDXC:GxserverAgentSettings 2026-06-22-07:33:
Agent settings persistence must match TypeScript gxserver exactly: read and write only `agents.settings.v1`. Legacy or sidebar-local keys are not daemon settings and must not make `/api/readAgentSettings` report persisted values.
*/
pub fn dispatch_agent_endpoint(
    repository: &DomainRepository<'_>,
    db: &Connection,
    home_dir: &Path,
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
            let result = request_session_rename(repository, &lifecycle, params, home_dir)?;
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
            let result = ingest_session_state_event(repository, &lifecycle, params, home_dir)?;
            AgentEndpointOutput {
                presentation_session: Some((lifecycle.project_id, lifecycle.session_id)),
                result,
            }
        }
        "/api/ingestTerminalTitleEvent" => {
            let lifecycle = read_lifecycle(params)?;
            let output =
                ingest_terminal_title_event_with_home(repository, &lifecycle, params, home_dir)?;
            AgentEndpointOutput {
                presentation_session: output
                    .schedule_presentation_delta
                    .then_some((lifecycle.project_id, lifecycle.session_id)),
                result: output.result,
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
            let result = ingest_agent_hook_event(repository, &lifecycle, params, home_dir)?;
            let rejected = matches!(
                result.get("reason").and_then(Value::as_str),
                Some("agent-hook-agent-mismatch" | "passive-session-identity-conflict")
            );
            AgentEndpointOutput {
                presentation_session: (!rejected)
                    .then_some((lifecycle.project_id, lifecycle.session_id)),
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
    let row = read_agent_settings_metadata_value(db)?;
    let parsed = row.as_deref().map(parse_json_object);
    Ok(json!({
        "isPersisted": row.is_some(),
        "settings": normalize_agent_settings(parsed.as_ref()),
    }))
}

fn read_agent_settings_metadata_value(db: &Connection) -> Result<Option<String>, DomainStateError> {
    read_metadata_value(db, AGENT_SETTINGS_METADATA_KEY)
}

fn read_metadata_value(db: &Connection, key: &str) -> Result<Option<String>, DomainStateError> {
    db.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map_err(sql_error)
}

pub(crate) fn read_agent_settings(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
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

fn create_agent_session_default_title(agent_name: Option<&str>, agent_id: Option<&str>) -> String {
    let title_name = normalize_agent_session_title_name(agent_name)
        .or_else(|| {
            default_agent_session_title_name(agent_id.unwrap_or_default()).map(str::to_string)
        })
        .or_else(|| normalize_agent_session_title_name(agent_id));
    title_name
        .map(|name| format!("{name} Session"))
        .unwrap_or_else(|| "Terminal Session".to_string())
}

fn normalize_agent_session_title_name(value: Option<&str>) -> Option<String> {
    let normalized = value?
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn default_agent_session_title_name(agent_id: &str) -> Option<&'static str> {
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

fn build_agent_resume_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
    let input = to_agent_resume_input(project, session, settings);
    let primary_command =
        build_agent_resume_command(&input, ResumeCommandOptions { display: false });
    let display_command = primary_command
        .as_ref()
        .and_then(|_| build_agent_resume_command(&input, ResumeCommandOptions { display: true }))
        .or_else(|| primary_command.clone());
    let fallback_command = build_agent_resume_fallback_command(&input);
    let copy_command = build_agent_resume_copy_command(&input);
    let mut plan = Map::new();
    insert_optional_string(&mut plan, "agentId", input.agent_id.clone());
    insert_optional_string(&mut plan, "baseCommand", input.agent_lookup_command.clone());
    insert_optional_string(&mut plan, "copyCommand", copy_command);
    insert_optional_string(&mut plan, "displayCommand", display_command.clone());
    insert_optional_string(&mut plan, "fallbackCommand", fallback_command.clone());
    insert_optional_string(
        &mut plan,
        "lookupCommand",
        input.agent_lookup_command.clone(),
    );
    insert_optional_string(&mut plan, "primaryCommand", primary_command.clone());
    insert_optional_string(&mut plan, "runtimeCommand", input.agent_command.clone());
    if let Some(command) = primary_command {
        /*
        CDXC:GxserverZmxLifecycle 2026-06-22-06:58:
        Provider startup must feed zmx the same restored-session startup script shape as TypeScript gxserver. Wrap daemon-owned resume commands before they reach attach metadata or `startSessionProvider` so wake/start paths print restore context and keep the command in the initial provider startup text instead of changing zmx lifecycle decisions.

        CDXC:AgentResume 2026-06-22-07:47:
        Resume planning must keep TypeScript's separate primary/display/copy/fallback command roles. Exact Codex restores validate the stored id first, then the startup wrapper can try a trusted-title fallback without making Copy Resume include lookup shell code.
        */
        let startup_text = wrap_restored_terminal_resume_command(
            &command,
            display_command.as_deref().unwrap_or(&command),
            fallback_command.as_deref(),
        );
        plan.insert(
            "startupText".to_string(),
            Value::String(as_atuin_ignored_shell_input(&startup_text)),
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

pub(crate) fn get_agent_startup_text_for_session(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Option<String> {
    /*
    CDXC:GxserverSessionIO 2026-06-22-06:53:
    zmx attach metadata must preserve TypeScript's startup-text precedence: explicit renderer text, then queued fresh-launch text, then the daemon-owned agent resume plan shaped by current agent settings. This keeps missing-provider reattach/wake metadata from dropping restorable agent commands after the Rust cutover.
    */
    build_agent_resume_plan(project, session, settings)
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

#[derive(Clone)]
struct AgentResumeInput {
    agent_command: Option<String>,
    agent_id: Option<String>,
    agent_lookup_command: Option<String>,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    first_user_message: Option<String>,
    project_path: Option<String>,
    stored_command_candidates: Vec<String>,
    title: Option<String>,
    title_source: Option<String>,
}

#[derive(Clone, Copy)]
struct ResumeCommandOptions {
    display: bool,
}

fn to_agent_resume_input(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> AgentResumeInput {
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
    let runtime_command = agent_id
        .as_ref()
        .and_then(|agent_id| {
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
        })
        .or_else(|| base_command.clone());
    AgentResumeInput {
        agent_command: runtime_command,
        agent_id,
        agent_lookup_command: base_command,
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        first_user_message: read_text_from_map(&runtime_settings, "firstUserMessage")
            .or_else(|| read_text_from_map(&launch_settings, "firstUserMessage")),
        project_path: read_text_value(session, "cwd").or_else(|| read_text_value(project, "path")),
        stored_command_candidates: collect_stored_agent_resume_command_candidates(session),
        title: read_text_value(session, "title"),
        title_source: read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .or_else(|| Some("user".to_string())),
    }
}

fn build_agent_resume_command(
    input: &AgentResumeInput,
    options: ResumeCommandOptions,
) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = if agent_id == "pi" {
        None
    } else {
        trusted_resume_title_for_input(input)
    };
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    let codex_exact_reference = (agent_id == "codex")
        .then(|| get_codex_session_reference(input))
        .flatten();
    let codex_reference = if agent_id == "codex" {
        codex_exact_reference
            .clone()
            .or_else(|| resume_title.clone())
    } else {
        None
    };
    let claude_exact_reference = (agent_id == "claude")
        .then(|| get_claude_session_reference(input))
        .flatten();
    let cursor_reference = (agent_id == "cursor")
        .then(|| get_cursor_session_reference(input))
        .flatten();
    let opencode_reference = (agent_id == "opencode")
        .then(|| get_opencode_session_reference(input))
        .flatten();
    let pi_reference = (agent_id == "pi")
        .then(|| get_pi_session_reference(input))
        .flatten();

    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "gemini" | "hermes-agent" | "qoder" => exact_reference
            .map(|reference| {
                format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                )
            }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "omp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codex" => {
            let reference = codex_reference?;
            if options.display {
                return Some(if let Some(exact) = codex_exact_reference {
                    format!("{agent_command} resume {}", quote_shell_double_arg(&exact))
                } else {
                    format!(
                        "{agent_command} resume {}  # lookup Codex session id by title",
                        quote_shell_double_arg(&reference)
                    )
                });
            }
            if let Some(exact) = codex_exact_reference {
                Some(build_codex_validated_resume_command(agent_command, &exact))
            } else {
                Some(build_codex_resume_lookup_command(agent_command, &reference))
            }
        }
        "claude" => {
            if let Some(exact) = claude_exact_reference {
                return Some(format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&exact)
                ));
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{agent_command} --resume {}  # lookup Claude session id by title",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_claude_resume_lookup_command(
                    agent_command,
                    input,
                    &resume_title,
                ))
            }
        }
        "cursor" => {
            if let Some(reference) = cursor_reference {
                return Some(format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            let project_path = input.project_path.as_deref()?;
            if options.display {
                Some(format!(
                    "{agent_command} --resume {}  # lookup chat id in Cursor chat store",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_cursor_resume_lookup_command(
                    agent_command,
                    project_path,
                    &resume_title,
                ))
            }
        }
        "opencode" => {
            if let Some(reference) = opencode_reference {
                return Some(format!(
                    "{agent_command} --session {}",
                    quote_shell_double_arg(&reference)
                ));
            }
            let resume_title = resume_title?;
            if options.display {
                Some(format!(
                    "{agent_command} -s {}  # lookup session id in OpenCode session list",
                    quote_shell_double_arg(&resume_title)
                ))
            } else {
                Some(build_opencode_resume_command(
                    agent_command,
                    &resume_title,
                    agent_lookup_command,
                ))
            }
        }
        "pi" => pi_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

fn build_agent_resume_copy_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let exact_reference = get_exact_agent_session_reference(agent_id, input);
    match agent_id {
        "amp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} threads continue {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "antigravity" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --conversation {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codebuddy" | "copilot" | "droid" | "gemini" | "hermes-agent" | "qoder" => exact_reference
            .map(|reference| {
                format!(
                    "{agent_command} --resume {}",
                    quote_shell_double_arg(&reference)
                )
            }),
        "grok" => exact_reference
            .map(|reference| format!("{agent_command} -r {}", quote_shell_double_arg(&reference))),
        "kiro" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --resume-id {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "omp" => exact_reference.map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "codex" => get_codex_session_reference(input).map(|reference| {
            format!(
                "{agent_command} resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "claude" => get_claude_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "cursor" => get_cursor_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --resume {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "opencode" => get_opencode_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "pi" => get_pi_session_reference(input).map(|reference| {
            format!(
                "{agent_command} --session {}",
                quote_shell_double_arg(&reference)
            )
        }),
        "rovodev" => {
            exact_reference.map(|reference| build_rovodev_resume_command(agent_command, &reference))
        }
        _ => None,
    }
}

fn build_agent_resume_fallback_command(input: &AgentResumeInput) -> Option<String> {
    let agent_id = restorable_agent_id(input.agent_id.as_deref())?;
    let agent_command = input.agent_command.as_deref()?;
    let agent_lookup_command = input
        .agent_lookup_command
        .as_deref()
        .unwrap_or(agent_command);
    let resume_title = trusted_resume_title_for_input(input)?;
    match agent_id {
        "codex" => {
            let exact = get_codex_session_reference(input)?;
            (exact != resume_title)
                .then(|| build_codex_resume_lookup_command(agent_command, &resume_title))
        }
        "claude" => {
            let _exact = get_claude_session_reference(input)?;
            Some(build_claude_resume_lookup_command(
                agent_command,
                input,
                &resume_title,
            ))
        }
        "opencode" => {
            let exact = get_opencode_session_reference(input)?;
            (exact != resume_title).then(|| {
                build_opencode_resume_command(agent_command, &resume_title, agent_lookup_command)
            })
        }
        "cursor" => {
            let _exact = get_cursor_session_reference(input)?;
            let project_path = input.project_path.as_deref()?;
            Some(build_cursor_resume_lookup_command(
                agent_command,
                project_path,
                &resume_title,
            ))
        }
        _ => None,
    }
}

fn restorable_agent_id(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    match value {
        "amp" | "antigravity" | "claude" | "codebuddy" | "codex" | "copilot" | "cursor"
        | "droid" | "gemini" | "grok" | "hermes-agent" | "kiro" | "omp" | "opencode" | "pi"
        | "qoder" | "rovodev" => Some(value),
        _ => None,
    }
}

fn get_exact_agent_session_reference(agent_id: &str, input: &AgentResumeInput) -> Option<String> {
    match agent_id {
        "codex" => get_codex_session_reference(input),
        "cursor" => get_cursor_session_reference(input),
        "pi" => get_pi_session_reference(input),
        _ => input.agent_session_id.clone(),
    }
}

fn get_codex_session_reference(input: &AgentResumeInput) -> Option<String> {
    let session_id = input.agent_session_id.as_deref()?.trim();
    if session_id.is_empty() {
        return None;
    }
    get_uuid_from_text(session_id).or_else(|| Some(session_id.to_string()))
}

fn get_claude_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_id
        .clone()
        .or_else(|| get_claude_session_id(input.agent_session_path.as_deref()))
        .or_else(|| get_claude_session_id_from_stored_commands(&input.stored_command_candidates))
}

fn get_opencode_session_reference(input: &AgentResumeInput) -> Option<String> {
    input.agent_session_id.clone()
}

fn get_pi_session_reference(input: &AgentResumeInput) -> Option<String> {
    input
        .agent_session_path
        .clone()
        .or_else(|| input.agent_session_id.clone())
}

fn get_cursor_session_reference(input: &AgentResumeInput) -> Option<String> {
    get_cursor_chat_session_id(input.agent_session_id.as_deref())
        .or_else(|| get_cursor_chat_session_id(input.agent_session_path.as_deref()))
        .or_else(|| {
            get_cursor_chat_session_id_from_stored_commands(&input.stored_command_candidates)
        })
}

fn get_cursor_chat_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if is_uuid(normalized) {
        return Some(normalized.to_ascii_lowercase());
    }
    let normalized_path = normalized.replace('\\', "/");
    let marker = "/agent-transcripts/";
    let index = normalized_path.to_ascii_lowercase().find(marker)?;
    let tail = &normalized_path[index + marker.len()..];
    let segment = tail.split('/').next()?.trim();
    is_uuid(segment).then(|| segment.to_ascii_lowercase())
}

fn get_cursor_chat_session_id_from_stored_commands(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_cursor_chat_session_id(Some(&value)))
}

fn get_claude_session_id_from_stored_commands(candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .filter_map(|candidate| get_resume_flag_value_from_stored_command(candidate, "--resume"))
        .find_map(|value| get_claude_session_id(Some(&value)))
}

fn get_claude_session_id(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim();
    if normalized.is_empty() {
        return None;
    }
    if let Some(uuid) = get_uuid_from_text(normalized) {
        return Some(uuid);
    }
    let cleaned = normalized.trim_end_matches(".jsonl");
    if is_claude_ses_id(cleaned) {
        return Some(cleaned.to_string());
    }
    let normalized_path = normalized.replace('\\', "/");
    normalized_path
        .split('/')
        .filter_map(|part| {
            let candidate = part.trim_end_matches(".jsonl");
            is_claude_ses_id(candidate).then(|| candidate.to_string())
        })
        .next_back()
}

fn is_claude_ses_id(value: &str) -> bool {
    value.strip_prefix("ses_").is_some_and(|rest| {
        !rest.is_empty() && rest.chars().all(|char| char.is_ascii_alphanumeric())
    })
}

fn get_resume_flag_value_from_stored_command(command: &str, flag: &str) -> Option<String> {
    let bytes = command.as_bytes();
    let flag_bytes = flag.as_bytes();
    let mut index = 0;
    while index + flag_bytes.len() <= bytes.len() {
        if &bytes[index..index + flag_bytes.len()] != flag_bytes {
            index += 1;
            continue;
        }
        if index > 0 && !bytes[index - 1].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let mut cursor = index + flag_bytes.len();
        if bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
        } else if bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
        } else {
            index += 1;
            continue;
        }
        if cursor >= bytes.len() {
            return None;
        }
        let quote = match bytes[cursor] {
            b'\'' | b'"' => {
                cursor += 1;
                Some(bytes[cursor - 1])
            }
            _ => None,
        };
        let start = cursor;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if quote == Some(byte)
                || (quote.is_none()
                    && (byte.is_ascii_whitespace() || matches!(byte, b';' | b'&' | b'|')))
            {
                break;
            }
            cursor += 1;
        }
        let value = command[start..cursor].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
        index = cursor.saturating_add(1);
    }
    None
}

fn collect_stored_agent_resume_command_candidates(session: &Value) -> Vec<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    let launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let resume_plan = launch_settings
        .get("agentResumePlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let values = [
        read_text_from_map(&runtime_settings, "agentResumeCommand"),
        read_text_from_map(&runtime_settings, "resumeCommand"),
        read_text_from_map(&runtime_settings, "resumeFallbackCommand"),
        read_text_from_map(&runtime_settings, "copyCommand"),
        read_text_from_map(&runtime_settings, "startupText"),
        read_text_from_map(&launch_settings, "agentResumeCommand"),
        read_text_from_map(&launch_settings, "resumeCommand"),
        read_text_from_map(&launch_settings, "resumeFallbackCommand"),
        read_text_from_map(&launch_settings, "copyCommand"),
        read_text_from_map(&launch_settings, "startupText"),
        read_text_from_map(&launch_plan, "command"),
        read_text_from_map(&launch_plan, "startupText"),
        read_text_from_map(&resume_plan, "primaryCommand"),
        read_text_from_map(&resume_plan, "copyCommand"),
        read_text_from_map(&resume_plan, "displayCommand"),
        read_text_from_map(&resume_plan, "startupText"),
    ];
    let mut output = Vec::new();
    for value in values.into_iter().flatten() {
        if !output.iter().any(|candidate| candidate == &value) {
            output.push(value);
        }
    }
    output
}

fn trusted_resume_title_for_input(input: &AgentResumeInput) -> Option<String> {
    let title = input.title.as_deref()?;
    let title_source = normalize_title_source(input.title_source.as_deref(), title);
    if title_source == "placeholder" {
        return None;
    }
    let visible = get_visible_terminal_title(title)?.trim().to_string();
    (!visible.is_empty() && !is_rejected_resume_title(&visible)).then_some(visible)
}

fn build_rovodev_resume_command(agent_command: &str, session_reference: &str) -> String {
    let quoted = quote_shell_double_arg(session_reference);
    if agent_command
        .split_whitespace()
        .any(|token| token == "rovodev")
    {
        format!("{agent_command} --restore {quoted}")
    } else {
        format!("{agent_command} rovodev run --restore {quoted}")
    }
}

fn build_claude_resume_lookup_command(
    agent_command: &str,
    input: &AgentResumeInput,
    resume_title: &str,
) -> String {
    let args = [
        quote_shell_arg(input.project_path.as_deref().unwrap_or_default()),
        quote_shell_arg(resume_title),
        quote_shell_arg(input.first_user_message.as_deref().unwrap_or_default()),
    ]
    .join(" ");
    let resume_invocation = format!("{agent_command} --resume \"$CLAUDE_RESUME_SESSION_ID\"");
    [
        "CLAUDE_RESUME_SESSION_ID=\"$(".to_string(),
        format!("{} claude {args}", build_resume_lookup_command()),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CLAUDE_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        resume_invocation,
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Claude session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

fn build_cursor_resume_lookup_command(
    agent_command: &str,
    project_path: &str,
    resume_title: &str,
) -> String {
    [
        "CURSOR_CHAT_ID=\"$(".to_string(),
        format!(
            "{} cursor {} {}",
            build_resume_lookup_command(),
            quote_shell_arg(project_path),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CURSOR_CHAT_ID\"".to_string(),
        "&&".to_string(),
        format!("{agent_command} --resume \"$CURSOR_CHAT_ID\""),
        "||".to_string(),
        format!(
            "printf '%s\\n' {}",
            quote_shell_arg(&format!(
                "Unable to find Cursor chat id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

fn build_opencode_resume_command(
    agent_command: &str,
    resume_title: &str,
    lookup_agent_command: &str,
) -> String {
    format!(
        "{agent_command} -s \"$({lookup_agent_command} session list --format json | {} opencode {})\"",
        build_resume_lookup_command(),
        quote_shell_arg(resume_title)
    )
}

fn build_codex_validated_resume_command(agent_command: &str, session_reference: &str) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --exact {}",
            build_resume_lookup_command(),
            quote_shell_arg(session_reference)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        format!("{agent_command} resume \"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to restore Codex session \"{session_reference}\"."
            ))
        ),
    ]
    .join(" ")
}

fn build_codex_resume_lookup_command(agent_command: &str, resume_title: &str) -> String {
    [
        "CODEX_RESUME_SESSION_ID=\"$(".to_string(),
        format!(
            "{} codex --title {}",
            build_resume_lookup_command(),
            quote_shell_arg(resume_title)
        ),
        ")\"".to_string(),
        "&&".to_string(),
        "test -n \"$CODEX_RESUME_SESSION_ID\"".to_string(),
        "&&".to_string(),
        format!("{agent_command} resume \"$CODEX_RESUME_SESSION_ID\""),
        "||".to_string(),
        format!(
            "{{ printf '%s\\n' {}; false; }}",
            quote_shell_arg(&format!(
                "Unable to find restorable Codex session id for \"{resume_title}\"."
            ))
        ),
    ]
    .join(" ")
}

fn build_resume_lookup_command() -> String {
    /*
    CDXC:RemoteMinimalDeps 2026-07-13:
    Resume lookups used to run as `node -e <script>` against the bundled
    code-server Node, which forced every host (including remote Linux
    packages) to carry a Node runtime for session restore. The lookups are
    now `gxserver resume-lookup <provider> ...` subcommands of this binary,
    resolved the same way agent hooks resolve their notify executable.
    */
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "gxserver".to_string());
    format!("{} resume-lookup", quote_shell_arg(&executable))
}

fn get_uuid_from_text(value: &str) -> Option<String> {
    let text = value.as_bytes();
    for start in 0..text.len().saturating_sub(35) {
        let end = start + 36;
        let candidate = &value[start..end];
        if is_uuid(candidate) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
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
        &settings,
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
    let source_title = source_session
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Terminal Session");
    let title = format!("Fork: {source_title}");
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
            "autoTitleFromFirstPrompt": false,
            "forkFirstPromptAutoTitlePending": true,
            "forkedFromSessionId": source_session_id,
            "gxserverForkInitialRenameStatus": "pending",
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
    home_dir: &Path,
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
        let pending_changed = updated.get("updatedAt") != session.get("updatedAt");
        /*
        CDXC:GxserverAgentTitles 2026-06-21-15:35:
        Rust requestSessionRename must mirror TypeScript gxserver for Agent CLI renames: the renderer command can ask the CLI to rename, but the app sidebar title must be reconciled from the agent's own structured session metadata before the RPC returns so clients receive the canonical session projection.
        */
        let reconciled =
            reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
        let _metadata_title_found = reconciled.metadata_title_found;
        let reconciled_changed = reconciled.changed;
        let reason = if reconciled_changed {
            reconciled.reason
        } else {
            "agent-rename-request-pending-metadata".to_string()
        };
        let final_session = reconciled.session.unwrap_or(updated);
        return Ok(json!({
            "changed": pending_changed || reconciled_changed,
            "pendingAgentMetadata": true,
            "projection": project_session_title_projection(&final_session),
            "reason": reason,
            "session": final_session,
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

struct AgentMetadataTitle {
    agent_session_id: Option<String>,
    provider: &'static str,
    title: String,
    updated_at: Option<String>,
}

struct AgentTitleReconcileResult {
    changed: bool,
    metadata_title_found: bool,
    reason: String,
    session: Option<Value>,
}

pub(crate) fn reconcile_agent_metadata_title_for_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    home_dir: &Path,
    pending_mismatch_status: &str,
) -> Result<bool, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let result =
        reconcile_agent_metadata_title(repository, &lifecycle, home_dir, pending_mismatch_status)?;
    Ok(result.changed)
}

fn reconcile_agent_metadata_title(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    home_dir: &Path,
    pending_mismatch_status: &str,
) -> Result<AgentTitleReconcileResult, DomainStateError> {
    let Some(session) = repository.get_session(&lifecycle.project_id, &lifecycle.session_id)?
    else {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "session-missing".to_string(),
            session: None,
        });
    };
    let runtime_settings = object_field(&session, "runtimeSettings");
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(&session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: runtime_settings.clone(),
        startup_text: None,
    });
    if !is_agent_associated(&session, &identity) {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "not-agent-associated".to_string(),
            session: Some(session),
        });
    }
    let pending_title = read_text_from_map(&runtime_settings, "pendingAgentTitleRequestTitle");
    let pending_requested_at =
        read_text_from_map(&runtime_settings, "pendingAgentTitleRequestRequestedAt");
    let metadata_title = read_agent_metadata_title(home_dir, &session).or_else(|| {
        read_pending_codex_rename_metadata_title(
            home_dir,
            &identity,
            pending_title.as_deref(),
            pending_requested_at.as_deref(),
        )
    });
    let Some(metadata_title) = metadata_title else {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: false,
            reason: "metadata-title-missing".to_string(),
            session: Some(session),
        });
    };

    let pending_status = pending_title.as_deref().map(|pending_title| {
        if titles_match(pending_title, &metadata_title.title) {
            "confirmed"
        } else {
            pending_mismatch_status
        }
    });
    let mut next_runtime_settings = runtime_settings.clone();
    next_runtime_settings.insert("titleMetadataCheckedAt".to_string(), json!(now_iso()));
    next_runtime_settings.insert(
        "titleMetadataProvider".to_string(),
        json!(metadata_title.provider),
    );
    next_runtime_settings.insert("titleMetadataSource".to_string(), json!("agent-metadata"));
    next_runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
    if let Some(agent_session_id) = metadata_title.agent_session_id.as_deref() {
        next_runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    if let Some(updated_at) = metadata_title.updated_at.as_deref() {
        next_runtime_settings.insert("titleMetadataUpdatedAt".to_string(), json!(updated_at));
    }
    if let Some(status) = pending_status {
        next_runtime_settings.insert("pendingAgentTitleRequestStatus".to_string(), json!(status));
    }
    let needs_update = session.get("title").and_then(Value::as_str)
        != Some(metadata_title.title.as_str())
        || runtime_settings.get("titleSource") != next_runtime_settings.get("titleSource")
        || runtime_settings.get("titleMetadataSource")
            != next_runtime_settings.get("titleMetadataSource")
        || runtime_settings.get("titleMetadataProvider")
            != next_runtime_settings.get("titleMetadataProvider")
        || runtime_settings.get("agentSessionId") != next_runtime_settings.get("agentSessionId")
        || runtime_settings.get("titleMetadataUpdatedAt")
            != next_runtime_settings.get("titleMetadataUpdatedAt")
        || runtime_settings.get("pendingAgentTitleRequestStatus")
            != next_runtime_settings.get("pendingAgentTitleRequestStatus");

    if !needs_update {
        return Ok(AgentTitleReconcileResult {
            changed: false,
            metadata_title_found: true,
            reason: "metadata-title-already-current".to_string(),
            session: Some(session),
        });
    }

    let mut update = lifecycle_update(lifecycle);
    update.insert(
        "runtimeSettings".to_string(),
        Value::Object(next_runtime_settings),
    );
    update.insert("title".to_string(), Value::String(metadata_title.title));
    let updated = repository.update_session(&update)?;
    Ok(AgentTitleReconcileResult {
        changed: true,
        metadata_title_found: true,
        reason: "metadata-title-applied".to_string(),
        session: Some(updated),
    })
}

/*
CDXC:GxserverAgentTitles 2026-08-18:
A rename of an agent session is only confirmed once the Agent CLI writes the
new name into its own session metadata, so every agent Ghostex renames through
`/rename` needs a reader here. Codex publishes `thread_name` in the shared
`session_index.jsonl`; Claude Code writes a `custom-title` record into the
session transcript. While Claude had no reader its renames stayed pending
forever, `title` was never promoted, and the sidebar card kept the previous
name until Claude happened to push an unrelated terminal title.
*/
enum AgentMetadataTitleSource {
    ClaudeTranscript {
        transcript_path: PathBuf,
    },
    CodexSessionIndex {
        agent_session_id: String,
        index_paths: Vec<PathBuf>,
    },
}

impl AgentMetadataTitleSource {
    fn revision_paths(&self) -> Vec<&Path> {
        match self {
            Self::ClaudeTranscript { transcript_path } => vec![transcript_path.as_path()],
            Self::CodexSessionIndex { index_paths, .. } => {
                index_paths.iter().map(PathBuf::as_path).collect()
            }
        }
    }
}

fn read_agent_metadata_title(home_dir: &Path, session: &Value) -> Option<AgentMetadataTitle> {
    match agent_metadata_title_source(home_dir, session)? {
        AgentMetadataTitleSource::ClaudeTranscript { transcript_path } => {
            read_claude_transcript_title(&transcript_path)
        }
        AgentMetadataTitleSource::CodexSessionIndex {
            agent_session_id,
            index_paths,
        } => read_codex_session_index_title(&index_paths, &agent_session_id),
    }
}

fn agent_metadata_title_source(
    home_dir: &Path,
    session: &Value,
) -> Option<AgentMetadataTitleSource> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings,
        startup_text: None,
    });
    let agent_session_id = identity.agent_session_id.as_deref()?.trim();
    if agent_session_id.is_empty() {
        return None;
    }
    match identity.agent_id.as_deref() {
        Some("claude") => Some(AgentMetadataTitleSource::ClaudeTranscript {
            transcript_path: resolve_session_transcript_path(
                "claude",
                Some(agent_session_id),
                identity.agent_session_path.as_deref(),
            )?,
        }),
        Some("codex") => Some(AgentMetadataTitleSource::CodexSessionIndex {
            agent_session_id: agent_session_id.to_string(),
            index_paths: get_codex_session_index_candidate_paths(
                home_dir,
                identity.agent_session_path.as_deref(),
            ),
        }),
        _ => None,
    }
}

pub(crate) fn agent_metadata_title_revision(home_dir: &Path, session: &Value) -> Option<String> {
    let source = agent_metadata_title_source(home_dir, session)?;
    let mut revisions = Vec::new();
    for path in source.revision_paths() {
        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|modified| {
                modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_nanos())
            })
            .unwrap_or_default();
        revisions.push(format!(
            "{}:{}:{modified_ns}",
            path.to_string_lossy(),
            metadata.len(),
        ));
    }
    (!revisions.is_empty()).then(|| revisions.join("|"))
}

/*
CDXC:GxserverAgentTitles 2026-08-18:
Claude Code rewrites its `custom-title` state record on every turn, so the
current name always sits within the last few kilobytes of a live transcript.
Scan a bounded tail window rather than the whole file: these transcripts reach
several megabytes and the metadata sync pass re-reads every running session's
transcript each second. The transcript belongs to exactly one session, so the
newest record wins without matching the embedded `sessionId`, which diverges
from the resolved identity on resumed and forked Claude sessions.
*/
const CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES: u64 = 256 * 1024;

fn read_claude_transcript_title(transcript_path: &Path) -> Option<AgentMetadataTitle> {
    let tail = read_transcript_tail_text(transcript_path, CLAUDE_TRANSCRIPT_TITLE_TAIL_BYTES)?;
    for line in tail.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("custom-title") {
            continue;
        }
        let title = normalize_metadata_title(entry.get("customTitle"))?;
        return Some(AgentMetadataTitle {
            agent_session_id: None,
            provider: "claude-transcript",
            title,
            updated_at: None,
        });
    }
    None
}

fn read_transcript_tail_text(path: &Path, tail_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(tail_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity(length.saturating_sub(start) as usize);
    file.read_to_end(&mut bytes).ok()?;
    if start > 0 {
        match bytes.iter().position(|byte| *byte == b'\n') {
            Some(first_newline) => {
                bytes.drain(..=first_newline);
            }
            None => bytes.clear(),
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_codex_session_index_title(
    index_paths: &[PathBuf],
    agent_session_id: &str,
) -> Option<AgentMetadataTitle> {
    for index_path in index_paths {
        if let Some(title) = read_codex_session_index_title_from_path(index_path, agent_session_id)
        {
            return Some(title);
        }
    }
    None
}

fn read_codex_session_index_title_from_path(
    index_path: &Path,
    agent_session_id: &str,
) -> Option<AgentMetadataTitle> {
    let text = fs::read_to_string(index_path).ok()?;
    for line in text.lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if entry.get("id").and_then(Value::as_str) != Some(agent_session_id) {
            continue;
        }
        let title = normalize_metadata_title(
            entry
                .get("thread_name")
                .or_else(|| entry.get("title"))
                .or_else(|| entry.get("name")),
        )?;
        return Some(AgentMetadataTitle {
            agent_session_id: Some(agent_session_id.to_string()),
            provider: "codex-session-index",
            title,
            updated_at: entry
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        });
    }
    None
}

/*
CDXC:GxserverAgentTitles 2026-08-22:
Plain `codex` launches do not always expose their active rollout through argv,
an open file descriptor, or a hook before the user renames the session. A
pending rename still has an exact, independently written confirmation: Codex
appends that requested title to `session_index.jsonl` after the request time.
Use only that post-request exact-title record, and adopt its session id, so the
sidebar can confirm the rename without guessing from transcript recency.
*/
fn read_pending_codex_rename_metadata_title(
    home_dir: &Path,
    identity: &ResolvedIdentity,
    pending_title: Option<&str>,
    pending_requested_at: Option<&str>,
) -> Option<AgentMetadataTitle> {
    if identity.agent_id.as_deref() != Some("codex") || identity.agent_session_id.is_some() {
        return None;
    }
    let pending_title = pending_title?.trim();
    let requested_at = chrono::DateTime::parse_from_rfc3339(pending_requested_at?.trim()).ok()?;
    for index_path in
        get_codex_session_index_candidate_paths(home_dir, identity.agent_session_path.as_deref())
    {
        let Ok(text) = fs::read_to_string(index_path) else {
            continue;
        };
        for line in text.lines().rev() {
            let Ok(entry) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            let Some(entry) = entry.as_object() else {
                continue;
            };
            let Some(title) = normalize_metadata_title(
                entry
                    .get("thread_name")
                    .or_else(|| entry.get("title"))
                    .or_else(|| entry.get("name")),
            ) else {
                continue;
            };
            if !titles_match(pending_title, &title) {
                continue;
            }
            let Some(updated_at) = entry
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Ok(updated_at_parsed) = chrono::DateTime::parse_from_rfc3339(updated_at) else {
                continue;
            };
            if updated_at_parsed < requested_at {
                continue;
            }
            let Some(agent_session_id) = entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            return Some(AgentMetadataTitle {
                agent_session_id: Some(agent_session_id.to_string()),
                provider: "codex-session-index-pending-rename",
                title,
                updated_at: Some(updated_at.to_string()),
            });
        }
    }
    None
}

fn get_codex_session_index_candidate_paths(
    home_dir: &Path,
    agent_session_path: Option<&str>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = get_codex_root_from_session_path(agent_session_path) {
        roots.push(root);
    }
    let home_root = home_dir.join(".codex");
    if !roots.iter().any(|root| root == &home_root) {
        roots.push(home_root);
    }
    roots
        .into_iter()
        .map(|root| root.join("session_index.jsonl"))
        .collect()
}

fn get_codex_root_from_session_path(agent_session_path: Option<&str>) -> Option<PathBuf> {
    let normalized_path = agent_session_path?.trim().replace('\\', "/");
    if normalized_path.is_empty() {
        return None;
    }
    let sessions_marker_index = normalized_path.rfind("/sessions/")?;
    (sessions_marker_index > 0).then(|| PathBuf::from(&normalized_path[..sessions_marker_index]))
}

fn normalize_metadata_title(value: Option<&Value>) -> Option<String> {
    let title = get_visible_terminal_title(value?.as_str()?)?
        .trim()
        .to_string();
    (!title.is_empty() && !is_rejected_resume_title(&title)).then_some(title)
}

fn titles_match(left: &str, right: &str) -> bool {
    left.split_whitespace().collect::<Vec<_>>().join(" ")
        == right.split_whitespace().collect::<Vec<_>>().join(" ")
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
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let observed_identity = align_observed_identity_with_launch_profile(
        &current,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    if launch_agent_mismatch(&current, observed_identity.agent_id.as_deref()) {
        return Ok(json!({
            "changed": false,
            "projection": project_session_title_projection(&current),
            "reason": "session-state-agent-mismatch",
            "session": current,
        }));
    }
    let (mut result, session) = apply_session_state_update(
        repository,
        lifecycle,
        params,
        SessionIdentityUpdateSource::Passive,
    )?;
    if result.get("reason").and_then(Value::as_str) == Some("passive-session-identity-conflict") {
        return Ok(Value::Object(result));
    }
    let reconciled = reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
    let mut session = reconciled.session.unwrap_or(session);
    if reconciled.changed {
        result.insert("changed".to_string(), Value::Bool(true));
        result.insert(
            "projection".to_string(),
            project_session_title_projection(&session),
        );
        result.insert("reason".to_string(), Value::String(reconciled.reason));
        result.insert("session".to_string(), session.clone());
    }
    let claimed = claim_first_prompt_auto_title(
        repository,
        &session,
        read_text(params, "firstUserMessage"),
        false,
    )?;
    if let Some(claimed_session) = claimed {
        session = claimed_session;
        result.insert("changed".to_string(), Value::Bool(true));
        result.insert(
            "projection".to_string(),
            project_session_title_projection(&session),
        );
        result.insert(
            "reason".to_string(),
            Value::String("first-prompt-auto-title-claimed".to_string()),
        );
        result.insert("session".to_string(), session);
    }
    Ok(Value::Object(result))
}

pub(crate) fn apply_live_process_session_identity(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent_id: Option<String>,
    mut agent_session_id: Option<String>,
    agent_session_path: Option<String>,
) -> Result<bool, DomainStateError> {
    // A manually launched Codex can publish its UUID title before the process
    // scan identifies the terminal as Codex. Consume that already-observed
    // identity now so either event ordering reaches the same agent session.
    if agent_session_id.is_none()
        && normalize_agent_id(agent_id.as_deref()).as_deref() == Some("codex")
    {
        let current = repository.get_session(project_id, session_id)?;
        let runtime_settings = current
            .as_ref()
            .map(|session| object_field(session, "runtimeSettings"))
            .unwrap_or_default();
        if read_text_from_map(&runtime_settings, "agentSessionId").is_none() {
            agent_session_id = runtime_settings
                .get("agentActivity")
                .and_then(Value::as_object)
                .and_then(|activity| read_text_from_map(activity, "lastTitle"))
                .and_then(|title| get_codex_session_id_from_title(&title));
        }
    }
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let mut params = Map::new();
    insert_optional_string(&mut params, "agentName", agent_id);
    insert_optional_string(&mut params, "agentSessionId", agent_session_id);
    insert_optional_string(&mut params, "agentSessionPath", agent_session_path);
    let (result, _) = apply_session_state_update(
        repository,
        &lifecycle,
        &params,
        SessionIdentityUpdateSource::LiveProcess,
    )?;
    Ok(result.get("changed").and_then(Value::as_bool) == Some(true))
}

/*
CDXC:SessionChatIdentity 2026-08-02:
Transcript-proven identity repair. Claude Code writes a NEW transcript on
compaction/resume and only an agent hook tells the daemon about it — background
job continuations never fire hooks, so the stored `agentSessionId` can point at
a conversation that stopped receiving turns days ago. The Session Chat follower
proves the successor from the transcripts themselves (own-id + lineage to the
stale id) and lands it HERE, through the very same passive update path a hook
observation uses, so chat, title generation, prompts and the CLI all follow one
identity instead of each surface guessing.

`expected_agent_session_id` makes the write compare-and-set: the follower read
the stale id some milliseconds ago, and a real hook observation that landed in
between must always win.
*/
pub(crate) fn apply_transcript_successor_session_identity(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    expected_agent_session_id: Option<&str>,
    agent_session_id: &str,
    agent_session_path: &str,
) -> Result<bool, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let current = require_session(repository, &lifecycle)?;
    let stored_agent_session_id =
        read_text_from_map(&object_field(&current, "runtimeSettings"), "agentSessionId");
    if stored_agent_session_id.as_deref() != expected_agent_session_id {
        return Ok(false);
    }
    if stored_agent_session_id.as_deref() == Some(agent_session_id) {
        return Ok(false);
    }
    let mut params = Map::new();
    params.insert(
        "agentSessionId".to_string(),
        json!(agent_session_id.to_string()),
    );
    params.insert(
        "agentSessionPath".to_string(),
        json!(agent_session_path.to_string()),
    );
    let (result, _) = apply_session_state_update(
        repository,
        &lifecycle,
        &params,
        SessionIdentityUpdateSource::Passive,
    )?;
    if result.get("reason").and_then(Value::as_str) == Some("passive-session-identity-conflict") {
        return Ok(false);
    }
    Ok(read_text_from_map(
        &object_field(
            result.get("session").unwrap_or(&Value::Null),
            "runtimeSettings",
        ),
        "agentSessionId",
    )
    .as_deref()
        == Some(agent_session_id))
}

pub(crate) fn apply_created_session_identity(
    repository: &DomainRepository<'_>,
    session: &Value,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let lifecycle = LifecycleParams {
        project_id: read_text_value(session, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing projectId."))?,
        session_id: read_text_value(session, "sessionId")
            .ok_or_else(|| DomainStateError::corrupt_state("Session missing sessionId."))?,
    };
    let runtime_settings = params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let launch_settings = params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut identity_params = Map::new();
    insert_optional_string(
        &mut identity_params,
        "agentName",
        read_text_value(session, "agentId").or_else(|| read_text(params, "agentId")),
    );
    insert_optional_string(
        &mut identity_params,
        "agentSessionId",
        read_text_from_map(&runtime_settings, "agentSessionId"),
    );
    insert_optional_string(
        &mut identity_params,
        "agentSessionPath",
        read_text_from_map(&runtime_settings, "agentSessionPath"),
    );
    insert_optional_string(
        &mut identity_params,
        "startupText",
        read_text_from_map(&runtime_settings, "startupText")
            .or_else(|| read_text_from_map(&launch_settings, "startupText")),
    );
    insert_optional_string(
        &mut identity_params,
        "titleSource",
        read_text_from_map(&runtime_settings, "titleSource"),
    );
    insert_optional_string(
        &mut identity_params,
        "title",
        read_text(params, "title").or_else(|| read_text_value(session, "title")),
    );
    let (_, updated) = apply_session_state_update(
        repository,
        &lifecycle,
        &identity_params,
        SessionIdentityUpdateSource::Lifecycle,
    )?;
    Ok(updated)
}

struct TerminalTitleIngestOutput {
    result: Value,
    schedule_presentation_delta: bool,
}

#[cfg(test)]
fn ingest_terminal_title_event(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<TerminalTitleIngestOutput, DomainStateError> {
    ingest_terminal_title_event_with_home(repository, lifecycle, params, Path::new(""))
}

fn ingest_terminal_title_event_with_home(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    home_dir: &Path,
) -> Result<TerminalTitleIngestOutput, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let raw_title = read_text(params, "rawTitle");
    let visible_title = raw_title.as_deref().and_then(get_visible_terminal_title);
    let title_detected_agent = raw_title
        .as_deref()
        .and_then(get_terminal_title_detected_agent_name);
    let mut runtime_settings = object_field(&current, "runtimeSettings");
    let decision_agent_name = read_text(params, "agentName")
        .or_else(|| read_text_from_map(&runtime_settings, "agentName"))
        .or_else(|| read_text_value(&current, "agentId"));
    let promotion_agent_name = read_text(params, "agentName").or(title_detected_agent.clone());
    let captured_agent_session_id = raw_title
        .as_deref()
        .and_then(get_codex_session_id_from_title)
        .filter(|_| normalize_agent_id(decision_agent_name.as_deref()).as_deref() == Some("codex"))
        .filter(|session_id| {
            read_text_from_map(&runtime_settings, "agentSessionId").as_deref()
                != Some(session_id.as_str())
        });
    /*
    CDXC:GxserverSessionIdentity 2026-06-21-18:25:
    Terminal-title agent/session-id observations must flow through the shared identity reducer, matching TypeScript gxserver. This keeps launch-agent mismatch protection and Codex thread conflict rules identical while still allowing zmx title streams to promote recognized CLI rows for every client.

    CDXC:GxserverSessionIdentity 2026-06-22-07:42:
    TypeScript returns the terminal-title reducer's decision reason for `/api/ingestTerminalTitleEvent`; the follow-up identity reducer only mutates the session and changed flag unless metadata reconciliation later wins. Preserve reasons such as `captured-agent-session-id` even when identity promotion reports `current-title-already-trusted`.

    CDXC:GxserverSessionTitles 2026-06-22-07:59:
    Match TypeScript terminal-title ingestion gates exactly: session kind, visible-title normalization, ellipsized rejection, protected trusted titles, zmx/agent-title trust, and previous title-source reasons all belong in gxserver-rs before status or identity promotion runs.
    */
    if let Some(agent_session_id) = captured_agent_session_id.clone() {
        runtime_settings.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    let mut decision_changed = captured_agent_session_id.is_some();
    let mut reason = terminal_title_skip_reason(
        &current,
        params,
        visible_title.as_deref(),
        &runtime_settings,
        captured_agent_session_id.as_deref(),
    );
    let mut update = lifecycle_update(lifecycle);
    if reason.is_none() {
        if let Some(title) = visible_title.clone() {
            let previous_title_source = session_title_source(&current, &runtime_settings);
            let sync_reason = terminal_title_sync_reason(
                &current,
                title.as_str(),
                decision_agent_name.as_deref(),
                read_text(params, "previousTerminalTitle").as_deref(),
                read_text(params, "sessionPersistenceProvider").as_deref(),
            );
            if let Some(sync_reason) = sync_reason {
                reason = Some(format!("{sync_reason}-from-{previous_title_source}"));
                runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
                decision_changed = true;
            } else {
                reason = Some("terminal-title-not-trusted".to_string());
            }
        }
    }
    if reason.as_deref().is_some_and(|value| {
        value.starts_with("valid-agent-terminal-title-from-")
            || value.starts_with("zmx-terminal-title-from-")
    }) {
        if let Some(title) = visible_title.clone() {
            update.insert("title".to_string(), json!(title));
        }
    }
    let should_update_title_decision = decision_changed;
    let mut session = if should_update_title_decision {
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update)?
    } else {
        current
    };
    let mut changed = decision_changed;
    let decision_changed_for_metadata = changed;
    if captured_agent_session_id.is_some() || title_detected_agent.is_some() {
        let mut identity_params = Map::new();
        insert_optional_string(&mut identity_params, "agentName", promotion_agent_name);
        insert_optional_string(
            &mut identity_params,
            "agentSessionId",
            captured_agent_session_id.clone(),
        );
        let (identity_result, identity_session) = apply_session_state_update(
            repository,
            lifecycle,
            &identity_params,
            SessionIdentityUpdateSource::TerminalTitle,
        )?;
        if identity_result
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            changed = true;
        }
        session = identity_session;
    }
    let mut activity_params = params.clone();
    if let Some(raw_title) = raw_title.clone() {
        activity_params.insert("title".to_string(), json!(raw_title));
    }
    insert_optional_string(
        &mut activity_params,
        "agentName",
        read_text(params, "agentName").or(title_detected_agent.clone()),
    );
    insert_optional_string(
        &mut activity_params,
        "settledTitle",
        read_agent_metadata_settled_title(&session),
    );
    let mut activity_update = compute_activity_update(&session, &activity_params, Some("title"));
    // Title observation must not erase a pending Session Chat card (a session
    // sitting on an AskUserQuestion produces no output, so title ticks keep
    // firing while the question waits).
    carry_session_chat_prompt(&session, &mut activity_update.activity);
    let should_update_activity = should_persist_activity_update(&session, &activity_update);
    if should_update_activity {
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            activity_update.activity.clone(),
        );
        if let Some(last_active_at) = activity_update.last_active_at.clone() {
            update.insert("lastActiveAt".to_string(), json!(last_active_at));
        }
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        session = repository.update_session(&update)?;
    }
    /*
    CDXC:GxserverAgentTitles 2026-08-04:
    zmx terminal-title observation is the cross-platform notification source
    for Agent CLI `/rename`. WSL Codex may replace the renamed title almost
    immediately with a generic repository/spinner title, which is correctly
    rejected as display text above. That rejection must not suppress the
    canonical Codex metadata reconciliation: every settled zmx title event is
    still evidence that the CLI title state may have changed.
    */
    let should_check_metadata = decision_changed_for_metadata
        || changed
        || read_text(params, "sessionPersistenceProvider").as_deref() == Some("zmx");
    let reconciled = if should_check_metadata {
        Some(reconcile_agent_metadata_title(
            repository, lifecycle, home_dir, "pending",
        )?)
    } else {
        None
    };
    let reconciled_changed = reconciled.as_ref().is_some_and(|result| result.changed);
    let reconciled_reason = reconciled.as_ref().map(|result| result.reason.clone());
    if let Some(reconciled_session) = reconciled.and_then(|result| result.session) {
        session = reconciled_session;
    }
    let response_reason = if reconciled_changed {
        reconciled_reason.unwrap_or_else(|| "metadata-title-applied".to_string())
    } else {
        reason.unwrap_or_else(|| "terminal-title-not-visible".to_string())
    };
    let response_changed = changed || reconciled_changed;
    let result = json!({
        "agentSessionId": captured_agent_session_id,
        "activity": activity_update.activity,
        "changed": response_changed,
        "enteredAttention": activity_update.entered_attention,
        "previousActivity": activity_update.previous_activity,
        "projection": project_session_title_projection(&session),
        "reason": response_reason,
        "session": session,
        "visibleTitle": visible_title,
    });
    let result_activity = result
        .get("activity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let schedule_presentation_delta =
        response_changed || activity_update.previous_activity != result_activity;
    Ok(TerminalTitleIngestOutput {
        result,
        schedule_presentation_delta,
    })
}

fn terminal_title_skip_reason(
    session: &Value,
    params: &Map<String, Value>,
    visible_title: Option<&str>,
    runtime_settings: &Map<String, Value>,
    captured_agent_session_id: Option<&str>,
) -> Option<String> {
    if !matches!(
        session.get("kind").and_then(Value::as_str),
        Some("terminal" | "agent")
    ) {
        return Some("invalid-session-kind".to_string());
    }
    let Some(visible_title) = visible_title else {
        return Some(
            if captured_agent_session_id.is_some() {
                "captured-agent-session-id"
            } else {
                "terminal-title-not-visible"
            }
            .to_string(),
        );
    };
    if is_ellipsized_terminal_window_title(visible_title) {
        return Some("terminal-title-already-ellipsized".to_string());
    }
    if params
        .get("protectStoredTitleFromAutomation")
        .and_then(Value::as_bool)
        == Some(true)
        && trusted_resume_title_with_runtime(session, runtime_settings).is_some()
    {
        return Some("protected-stored-title".to_string());
    }
    if session
        .get("title")
        .and_then(Value::as_str)
        .is_some_and(|title| title.trim() == visible_title)
    {
        return Some(
            if captured_agent_session_id.is_some() {
                "captured-agent-session-id"
            } else {
                "already-synced"
            }
            .to_string(),
        );
    }
    None
}

fn terminal_title_sync_reason(
    session: &Value,
    visible_title: &str,
    agent_name: Option<&str>,
    previous_terminal_title: Option<&str>,
    session_persistence_provider: Option<&str>,
) -> Option<&'static str> {
    if is_valid_agent_terminal_title(visible_title, agent_name) {
        return Some("valid-agent-terminal-title");
    }
    let provider = session_persistence_provider.unwrap_or("zmx");
    if provider != "off" && !is_rejected_resume_title(visible_title) {
        return Some("zmx-terminal-title");
    }
    let previous_visible = previous_terminal_title.and_then(get_visible_terminal_title);
    if previous_visible.as_deref().is_some_and(|previous| {
        session
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|title| title.trim() == previous)
    }) {
        return None;
    }
    None
}

fn session_title_source(session: &Value, runtime_settings: &Map<String, Value>) -> String {
    normalize_title_source(
        read_text_from_map(runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(runtime_settings, "restoreTitleSource"))
            .as_deref(),
        session
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )
}

fn trusted_resume_title_with_runtime(
    session: &Value,
    runtime_settings: &Map<String, Value>,
) -> Option<String> {
    let title = read_text_value(session, "title")?;
    if session_title_source(session, runtime_settings) == "placeholder"
        || is_temporary_title(&title)
    {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_resume_title(&visible)).then_some(visible)
}

fn read_agent_metadata_settled_title(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    if read_text_from_map(&runtime_settings, "titleMetadataSource").as_deref()
        != Some("agent-metadata")
    {
        return None;
    }
    let title = read_text_value(session, "title")?;
    get_visible_terminal_title(&title).map(|value| value.trim().to_string())
}

fn is_ellipsized_terminal_window_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.ends_with('\u{2026}') || trimmed.ends_with("...")
}

fn is_valid_agent_terminal_title(title: &str, agent_name: Option<&str>) -> bool {
    supports_terminal_title_session_sync(agent_name)
        && title.trim().chars().count() > 1
        && title.chars().any(|ch| ch.is_alphanumeric())
        && get_visible_terminal_title(title).is_some()
        && !is_rejected_resume_title(title)
}

fn supports_terminal_title_session_sync(agent_name: Option<&str>) -> bool {
    let Some(normalized) = agent_name.map(|value| value.trim().to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        normalized.as_str(),
        "antigravity"
            | "claude"
            | "codex"
            | "copilot"
            | "cursor"
            | "gemini"
            | "hermes-agent"
            | "opencode"
            | "pi"
            | "qoder"
            | "rovodev"
            | "claude code"
            | "codex cli"
            | "agy"
            | "antigravity cli"
            | "cursor agent"
            | "cursor cli"
            | "cursor-agent"
            | "github copilot"
            | "hermes"
            | "hermes agent"
            | "open code"
            | "qodercli"
            | "rovo"
            | "rovo dev"
            | "\u{03c0}"
    )
}

fn update_agent_activity_endpoint(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    if launch_agent_mismatch(&current, read_text(params, "agentName").as_deref()) {
        let previous = normalize_agent_activity_value(
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
    let mut update = compute_activity_update(&current, params, None);
    // Explicit activity RPCs (bell, escape, acknowledge, …) must not erase a
    // pending Session Chat card; hook ingest and the transcript retire it.
    carry_session_chat_prompt(&current, &mut update.activity);
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
    home_dir: &Path,
) -> Result<Value, DomainStateError> {
    let current = require_session(repository, lifecycle)?;
    let hook_activity = normalize_agent_hook_activity(
        params.get("status"),
        params
            .get("eventName")
            .or_else(|| params.get("rawEventName")),
        params.get("agentName"),
    );
    let observed_identity = resolve_session_identity(&IdentityInput {
        agent_id: None,
        agent_name: read_text(params, "agentName"),
        agent_session_id: read_text(params, "agentSessionId"),
        agent_session_path: read_text(params, "agentSessionPath"),
        runtime_settings: Map::new(),
        startup_text: None,
    });
    if launch_agent_mismatch(&current, observed_identity.agent_id.as_deref()) {
        let previous = normalize_agent_activity_value(
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
    let (metadata_result, mut session) = apply_session_state_update(
        repository,
        lifecycle,
        params,
        SessionIdentityUpdateSource::Passive,
    )?;
    let mut changed = metadata_result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut reason = metadata_result
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("unchanged")
        .to_string();
    if reason == "passive-session-identity-conflict" {
        let previous = normalize_agent_activity_value(
            object_field(&current, "runtimeSettings").get("agentActivity"),
            "idle",
        );
        let mut result = object_from_value(json!({
            "activity": previous,
            "changed": false,
            "enteredAttention": false,
            "previousActivity": previous.get("activity").and_then(Value::as_str).unwrap_or("idle"),
            "projection": project_session_title_projection(&current),
            "reason": "passive-session-identity-conflict",
            "session": current,
        }));
        if let Some(conflict) = metadata_result.get("identityConflict").cloned() {
            result.insert("identityConflict".to_string(), conflict);
        }
        return Ok(Value::Object(result));
    }
    let mut activity_update: Option<ActivityUpdate> = None;
    let mut session_chat_prompt_changed = false;
    let mut session_chat_activity_changed = false;
    let mut activity_reason = if hook_activity.is_some() {
        "activity-unchanged".to_string()
    } else {
        "metadata-only".to_string()
    };
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
        let mut update = compute_activity_update(&session, &activity_params, None);
        /*
        CDXC:SessionChatSend 2026-07-31:
        Session Chat interactive-prompt capture. compute_activity_update
        rebuilds agentActivity from a fixed struct, so the stored
        sessionChatPrompt must be explicitly re-attached (kept, replaced, or
        dropped) here or every activity write would erase it. The key is also
        in the persistable_agent_activity_snapshot whitelist so a prompt-only
        change forces a persist.
        */
        let session_chat_prompt_before = session_chat_prompt_setting(&session);
        let session_chat_prompt_after = next_session_chat_prompt_setting(
            session_chat_prompt_before.as_deref(),
            params,
            &activity,
        );
        session_chat_prompt_changed =
            session_chat_prompt_before.as_deref() != session_chat_prompt_after.as_deref();
        if let Some(prompt_json) = session_chat_prompt_after.as_deref() {
            if let Some(activity_object) = update.activity.as_object_mut() {
                activity_object.insert("sessionChatPrompt".to_string(), json!(prompt_json));
            }
        }
        if !is_stale_activity_event(&session, now_ms) {
            let next_activity_name = update
                .activity
                .get("activity")
                .and_then(Value::as_str)
                .unwrap_or("idle");
            session_chat_activity_changed = is_session_chat_working_activity(next_activity_name)
                != is_session_chat_working_activity(&update.previous_activity);
            let activity_changed = should_persist_activity_update(&session, &update);
            if activity_changed {
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
            }
            activity_reason = if activity_changed {
                "activity-updated".to_string()
            } else {
                "activity-unchanged".to_string()
            };
            activity_update = Some(update);
        } else {
            activity_reason = "stale-activity-event".to_string();
            // Stale events persist nothing, so no prompt/activity change
            // reached disk.
            session_chat_prompt_changed = false;
            session_chat_activity_changed = false;
        }
    }
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Hook ingestion must mirror TypeScript's accepted-event reduction order: passive identity metadata, explicit activity, forced metadata-title reconciliation, then first-prompt auto-title claiming. Rejected passive conflicts stop before status, title, prompt, or presentation side effects can mutate the wrong session.
    */
    let reconciled = reconcile_agent_metadata_title(repository, lifecycle, home_dir, "pending")?;
    if let Some(reconciled_session) = reconciled.session {
        session = reconciled_session;
    }
    if reconciled.changed {
        changed = true;
    }
    let mut auto_title_claimed = false;
    if let Some(claimed_session) = claim_first_prompt_auto_title(
        repository,
        &session,
        read_text(params, "firstUserMessage"),
        is_explicit_user_prompt_submit_event(params),
    )? {
        session = claimed_session;
        changed = true;
        auto_title_claimed = true;
    }
    if reconciled.changed {
        reason = reconciled.reason;
    } else if auto_title_claimed {
        reason = "first-prompt-auto-title-claimed".to_string();
    } else if activity_reason != "metadata-only" {
        reason = activity_reason;
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
    result.insert(
        "sessionChatPromptChanged".to_string(),
        Value::Bool(session_chat_prompt_changed),
    );
    /*
    CDXC:SessionChatCore 2026-08-01:
    Session Chat's working indicator has no other source: the transcript can
    only ever SETTLE a spinner (a completed assistant row), never start one,
    because the first transcript row of a turn lands seconds after the agent
    starts. Reporting the working↔idle transition here lets the server push a
    sessionChatState frame on the chat channel, so every host gets the spinner
    and the Stop button without wiring its own activity prop. Only real
    transitions are reported — steady-state working events must not spam a
    frame every hook tick.
    */
    result.insert(
        "sessionChatActivityChanged".to_string(),
        Value::Bool(session_chat_activity_changed),
    );
    result.insert("session".to_string(), session);
    Ok(Value::Object(result))
}

/// A working↔not-working flip is the only activity change the chat channel
/// cares about (attention/idle both read as "not working").
fn is_session_chat_working_activity(activity: &str) -> bool {
    activity == "working"
}

/// The stored Session Chat interactive prompt: a JSON string in the shared
/// `SessionChatInteractivePrompt` wire shape, kept under
/// `runtimeSettings.agentActivity.sessionChatPrompt`.
pub(crate) fn session_chat_prompt_setting(session: &Value) -> Option<String> {
    object_field(session, "runtimeSettings")
        .get("agentActivity")
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("sessionChatPrompt"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/*
CDXC:SessionChatCore 2026-08-01:
Re-attach the stored Session Chat prompt to a freshly computed agentActivity
object. compute_activity_update rebuilds the object from the fixed
ActivityState struct, which does not know the key, so every non-hook activity
writer (terminal-title observation, explicit activity RPCs) must carry the
stored card forward — otherwise the very next title tick erases a
still-pending question card seconds after the PreToolUse hook stored it
(observed live 2026-08-01: an AskUserQuestion card never survived to a read).
Hook ingest must NOT use this: it re-derives the prompt per event
(replace / keep / clear via next_session_chat_prompt_setting). Lifecycle
resets (wake) also deliberately skip it — a woken session's card is stale.
*/
fn carry_session_chat_prompt(session: &Value, activity: &mut Value) {
    let Some(stored) = session_chat_prompt_setting(session) else {
        return;
    };
    if let Some(object) = activity.as_object_mut() {
        object
            .entry("sessionChatPrompt".to_string())
            .or_insert_with(|| json!(stored));
    }
}

/// Prompt disposition for one hook event: derive (AskUserQuestion-ish tool
/// input on a non-post-tool event, or PermissionRequest) → replace; post-tool
/// / Stop / SessionEnd / idle transition → clear; anything else → keep.
fn next_session_chat_prompt_setting(
    previous: Option<&str>,
    params: &Map<String, Value>,
    next_activity: &str,
) -> Option<String> {
    let event_name = params
        .get("eventName")
        .or_else(|| params.get("rawEventName"))
        .and_then(Value::as_str);
    let tool_name = params
        .get("toolName")
        .or_else(|| params.get("tool_name"))
        .and_then(Value::as_str);
    let tool_input = params.get("toolInput").or_else(|| params.get("tool_input"));
    if let Some(prompt) =
        crate::session_chat::derive_session_chat_prompt(tool_name, tool_input, event_name)
    {
        return serde_json::to_string(&prompt)
            .ok()
            .or_else(|| previous.map(str::to_string));
    }
    if crate::session_chat::should_clear_session_chat_prompt(event_name, Some(next_activity)) {
        return None;
    }
    previous.map(str::to_string)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SessionIdentityUpdateSource {
    Lifecycle,
    LiveProcess,
    Passive,
    TerminalTitle,
}

struct SessionIdentityConflict {
    agent_id: String,
    current_agent_session_id: Option<String>,
    incoming_agent_session_id: String,
    owner_project_id: Option<String>,
    owner_session_id: Option<String>,
    reason: &'static str,
    source: SessionIdentityUpdateSource,
}

fn apply_session_state_update(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
    params: &Map<String, Value>,
    identity_update_source: SessionIdentityUpdateSource,
) -> Result<(Map<String, Value>, Value), DomainStateError> {
    let session = require_session(repository, lifecycle)?;
    let project = require_project(repository, &lifecycle.project_id)?;
    let project_sessions = repository.list_sessions(Some(&lifecycle.project_id))?;
    let observed_identity = align_observed_identity_with_launch_profile(
        &session,
        resolve_session_identity(&IdentityInput {
            agent_id: None,
            agent_name: read_text(params, "agentName"),
            agent_session_id: read_text(params, "agentSessionId"),
            agent_session_path: read_text(params, "agentSessionPath"),
            runtime_settings: Map::new(),
            startup_text: read_text(params, "startupText"),
        }),
    );
    if identity_update_source != SessionIdentityUpdateSource::LiveProcess
        && launch_agent_mismatch(&session, observed_identity.agent_id.as_deref())
    {
        let result = json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "launch-agent-mismatch",
            "session": session.clone(),
        });
        return Ok((object_from_value(result), session));
    }
    let current_identity = resolve_stored_session_identity(&session);
    let resolved_identity = merge_observed_session_identity(&observed_identity, &current_identity);
    let (identity, identity_conflict) = resolve_allowed_session_identity(
        &current_identity,
        &session,
        &observed_identity,
        &resolved_identity,
        &project_sessions,
        identity_update_source,
    );
    if identity_conflict.is_some() && identity_update_source == SessionIdentityUpdateSource::Passive
    {
        let mut result = object_from_value(json!({
            "changed": false,
            "projection": project_session_title_projection(&session),
            "reason": "passive-session-identity-conflict",
            "session": session.clone(),
        }));
        if let Some(conflict) = identity_conflict {
            result.insert(
                "identityConflict".to_string(),
                session_identity_conflict_value(&conflict),
            );
        }
        return Ok((result, session));
    }

    let next_agent = identity
        .agent_id
        .clone()
        .or_else(|| read_text_value(&session, "agentId"));
    let mut runtime_settings = apply_session_identity_runtime_settings(
        &current_identity,
        &identity,
        object_field(&session, "runtimeSettings"),
        identity_update_source,
    );
    insert_truthy_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationAgent",
    );
    insert_optional_from_params(
        &mut runtime_settings,
        params,
        "firstPromptTitleGenerationCommand",
    );
    insert_truthy_from_params(&mut runtime_settings, params, "firstUserMessage");

    let should_promote_agent = next_agent.is_some()
        || identity.agent_session_id.is_some()
        || identity.agent_session_path.is_some();
    let mut title =
        read_text_value(&session, "title").unwrap_or_else(|| "Terminal Session".to_string());
    let mut reason = "identity-updated".to_string();
    let mut current_with_identity = session.clone();
    if let Some(object) = current_with_identity.as_object_mut() {
        if let Some(agent_id) = next_agent.clone() {
            object.insert("agentId".to_string(), json!(agent_id));
        }
        if should_promote_agent {
            object.insert("kind".to_string(), json!("agent"));
        }
        object.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings.clone()),
        );
    }

    if trusted_resume_title(&current_with_identity).is_none() {
        if let Some(candidate) = select_trusted_title_for_identity(
            &project,
            &project_sessions,
            &current_with_identity,
            params.get("title"),
            params.get("titleSource"),
            &identity,
        ) {
            title = candidate.title;
            runtime_settings.insert("titleSource".to_string(), json!(candidate.title_source));
            reason = candidate.reason;
        } else if let Some(agent_id) = next_agent.as_deref() {
            /*
            Plain terminals promoted by a live WSL agent process or its first
            hook should immediately gain the same neutral agent-aware title as
            sessions created from the agent launcher. Keep it a placeholder so
            first-prompt auto-title generation remains eligible to replace it.
            */
            title = create_agent_session_default_title(None, Some(agent_id));
            runtime_settings.insert("titleSource".to_string(), json!("placeholder"));
            reason = "agent-default-title-applied".to_string();
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
        || next_agent != read_text_value(&session, "agentId")
        || (should_promote_agent && session.get("kind").and_then(Value::as_str) != Some("agent"))
        || runtime_settings.get("agentName")
            != object_field(&session, "runtimeSettings").get("agentName")
        || runtime_settings.get("agentId")
            != object_field(&session, "runtimeSettings").get("agentId")
        || runtime_settings.get("agentSessionId")
            != object_field(&session, "runtimeSettings").get("agentSessionId")
        || runtime_settings.get("agentSessionPath")
            != object_field(&session, "runtimeSettings").get("agentSessionPath")
        || runtime_settings.get("launchAgentId")
            != object_field(&session, "runtimeSettings").get("launchAgentId")
        || runtime_settings.get("firstPromptTitleGenerationAgent")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationAgent")
        || runtime_settings.get("firstPromptTitleGenerationCommand")
            != object_field(&session, "runtimeSettings").get("firstPromptTitleGenerationCommand")
        || runtime_settings.get("firstUserMessage")
            != object_field(&session, "runtimeSettings").get("firstUserMessage")
        || runtime_settings.get("agentActivity")
            != object_field(&session, "runtimeSettings").get("agentActivity")
        || runtime_settings.get("titleSource")
            != object_field(&session, "runtimeSettings").get("titleSource");
    let updated = if needs_update {
        repository.update_session(&update)?
    } else {
        current_with_identity
    };
    let mut result = object_from_value(json!({
        "changed": needs_update,
        "projection": project_session_title_projection(&updated),
        "reason": if needs_update { reason } else { "unchanged".to_string() },
        "session": updated.clone(),
    }));
    if let Some(conflict) = identity_conflict {
        result.insert(
            "identityConflict".to_string(),
            session_identity_conflict_value(&conflict),
        );
    }
    Ok((result, updated))
}

fn resolve_allowed_session_identity(
    current_identity: &ResolvedIdentity,
    current_session: &Value,
    observed_identity: &ResolvedIdentity,
    resolved_identity: &ResolvedIdentity,
    sessions: &[Value],
    source: SessionIdentityUpdateSource,
) -> (ResolvedIdentity, Option<SessionIdentityConflict>) {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let resolved_agent_id = normalize_agent_id(resolved_identity.agent_id.as_deref());
    let incoming_agent_session_id = observed_identity
        .agent_session_id
        .as_deref()
        .and_then(normalize_codex_session_id);
    let current_codex_session_id = if current_agent_id.as_deref() == Some("codex") {
        current_identity
            .agent_session_id
            .as_deref()
            .and_then(normalize_codex_session_id)
    } else {
        None
    };
    let is_passive_codex_observation = source == SessionIdentityUpdateSource::Passive
        && incoming_agent_session_id.is_some()
        && (observed_agent_id.as_deref() == Some("codex")
            || (observed_agent_id.is_none()
                && current_agent_id.as_deref() == Some("codex")
                && resolved_agent_id.as_deref() == Some("codex")));
    if !is_passive_codex_observation {
        return (resolved_identity.clone(), None);
    }
    let incoming_agent_session_id = incoming_agent_session_id.expect("checked above");
    if let Some(current_codex_session_id) = current_codex_session_id {
        if current_codex_session_id != incoming_agent_session_id {
            let conflict = SessionIdentityConflict {
                agent_id: "codex".to_string(),
                current_agent_session_id: Some(current_codex_session_id),
                incoming_agent_session_id,
                owner_project_id: None,
                owner_session_id: None,
                reason: "passive-agent-session-id-replacement",
                source,
            };
            return (
                keep_current_session_identity(resolved_identity, current_identity),
                Some(conflict),
            );
        }
        return (resolved_identity.clone(), None);
    }
    if let Some(owner) =
        find_active_codex_identity_owner(sessions, current_session, &incoming_agent_session_id)
    {
        let conflict = SessionIdentityConflict {
            agent_id: "codex".to_string(),
            current_agent_session_id: None,
            incoming_agent_session_id,
            owner_project_id: read_text_value(&owner, "projectId"),
            owner_session_id: read_text_value(&owner, "sessionId"),
            reason: "active-agent-session-id-owned",
            source,
        };
        return (
            keep_current_session_identity(resolved_identity, current_identity),
            Some(conflict),
        );
    }
    (resolved_identity.clone(), None)
}

fn keep_current_session_identity(
    resolved_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    ResolvedIdentity {
        agent_id: resolved_identity.agent_id.clone(),
        agent_session_id: current_identity.agent_session_id.clone(),
        agent_session_path: current_identity.agent_session_path.clone(),
    }
}

fn merge_observed_session_identity(
    observed_identity: &ResolvedIdentity,
    current_identity: &ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(observed_identity.agent_id.as_deref());
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let agent_changed = observed_agent_id.is_some()
        && current_agent_id.is_some()
        && observed_agent_id != current_agent_id;
    ResolvedIdentity {
        agent_id: observed_agent_id.or(current_agent_id),
        agent_session_id: observed_identity.agent_session_id.clone().or_else(|| {
            (!agent_changed)
                .then(|| current_identity.agent_session_id.clone())
                .flatten()
        }),
        agent_session_path: observed_identity.agent_session_path.clone().or_else(|| {
            (!agent_changed)
                .then(|| current_identity.agent_session_path.clone())
                .flatten()
        }),
    }
}

fn resolve_stored_session_identity(session: &Value) -> ResolvedIdentity {
    let runtime_settings = object_field(session, "runtimeSettings");
    let stored_identity = resolve_session_identity(&IdentityInput {
        agent_id: read_text_value(session, "agentId"),
        agent_name: read_text_from_map(&runtime_settings, "agentName"),
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: runtime_settings.clone(),
        startup_text: None,
    });
    let transcript_path_identity = resolve_session_identity(&IdentityInput {
        agent_id: None,
        agent_name: None,
        agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
        agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
        runtime_settings: Map::new(),
        startup_text: None,
    });
    merge_observed_session_identity(&transcript_path_identity, &stored_identity)
}

fn apply_session_identity_runtime_settings(
    current_identity: &ResolvedIdentity,
    identity: &ResolvedIdentity,
    mut runtime_settings: Map<String, Value>,
    source: SessionIdentityUpdateSource,
) -> Map<String, Value> {
    let current_agent_id = normalize_agent_id(current_identity.agent_id.as_deref());
    let next_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    let agent_changed =
        current_agent_id.is_some() && next_agent_id.is_some() && current_agent_id != next_agent_id;
    let activity_agent_id = read_agent_activity_agent_id(runtime_settings.get("agentActivity"));
    let activity_owner_changed = next_agent_id.is_some()
        && activity_agent_id.is_some()
        && activity_agent_id != next_agent_id;
    if let Some(agent_id) = identity.agent_id.clone() {
        runtime_settings.insert("agentName".to_string(), json!(agent_id));
    }
    if source == SessionIdentityUpdateSource::LiveProcess {
        if let Some(agent_id) = next_agent_id.clone() {
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
    }
    if agent_changed || activity_owner_changed {
        runtime_settings.remove("agentActivity");
    }
    runtime_settings
}

fn read_agent_activity_agent_id(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("agentName"))
        .and_then(Value::as_str)
        .and_then(|value| normalize_agent_id(Some(value)))
}

fn find_active_codex_identity_owner(
    sessions: &[Value],
    current_session: &Value,
    incoming_agent_session_id: &str,
) -> Option<Value> {
    sessions.iter().find_map(|session| {
        if read_text_value(session, "sessionId") == read_text_value(current_session, "sessionId")
            && read_text_value(session, "projectId")
                == read_text_value(current_session, "projectId")
        {
            return None;
        }
        if !is_active_identity_owner(session) {
            return None;
        }
        let runtime_settings = object_field(session, "runtimeSettings");
        let identity = resolve_session_identity(&IdentityInput {
            agent_id: read_text_value(session, "agentId"),
            agent_name: None,
            agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
            agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
            runtime_settings,
            startup_text: None,
        });
        let is_match = normalize_agent_id(identity.agent_id.as_deref()).as_deref() == Some("codex")
            && identity
                .agent_session_id
                .as_deref()
                .and_then(normalize_codex_session_id)
                .as_deref()
                == Some(incoming_agent_session_id);
        is_match.then(|| session.clone())
    })
}

/// A session that could still be tailing the provider conversation it is bound
/// to. Stopped history rows are NOT owners — the registry keeps every session
/// ever created, so treating them as owners blocks legitimate re-binding.
pub(crate) fn is_active_identity_owner(session: &Value) -> bool {
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        || session.get("lifecycleState").and_then(Value::as_str) == Some("sleeping")
        || (session.get("lifecycleState").and_then(Value::as_str) != Some("stopped")
            && object_field(session, "providerState")
                .get("lifecycleState")
                .and_then(Value::as_str)
                == Some("exists"))
}

fn session_identity_conflict_value(conflict: &SessionIdentityConflict) -> Value {
    let mut output = Map::new();
    output.insert("agentId".to_string(), json!(conflict.agent_id));
    insert_optional_string(
        &mut output,
        "currentAgentSessionId",
        conflict.current_agent_session_id.clone(),
    );
    output.insert(
        "incomingAgentSessionId".to_string(),
        json!(conflict.incoming_agent_session_id),
    );
    insert_optional_string(
        &mut output,
        "ownerProjectId",
        conflict.owner_project_id.clone(),
    );
    insert_optional_string(
        &mut output,
        "ownerSessionId",
        conflict.owner_session_id.clone(),
    );
    output.insert("reason".to_string(), json!(conflict.reason));
    output.insert(
        "source".to_string(),
        json!(identity_update_source_name(conflict.source)),
    );
    Value::Object(output)
}

fn identity_update_source_name(source: SessionIdentityUpdateSource) -> &'static str {
    match source {
        SessionIdentityUpdateSource::Lifecycle => "lifecycle",
        SessionIdentityUpdateSource::LiveProcess => "live-process",
        SessionIdentityUpdateSource::Passive => "passive",
        SessionIdentityUpdateSource::TerminalTitle => "terminal-title",
    }
}

struct TrustedTitleCandidate {
    reason: String,
    title: String,
    title_source: String,
    updated_at: Option<String>,
}

fn select_trusted_title_for_identity(
    project: &Value,
    sessions: &[Value],
    current_session: &Value,
    event_title: Option<&Value>,
    event_title_source: Option<&Value>,
    identity: &ResolvedIdentity,
) -> Option<TrustedTitleCandidate> {
    if let Some(candidate) =
        create_trusted_title_candidate(event_title, event_title_source, "event-title", None)
    {
        return Some(candidate);
    }

    let current_session_id = read_text_value(current_session, "sessionId");
    let live_candidate = select_newest_candidate(
        sessions
            .iter()
            .filter(|session| read_text_value(session, "sessionId") != current_session_id)
            .filter_map(|session| {
                let runtime_settings = object_field(session, "runtimeSettings");
                let candidate_identity = ResolvedIdentity {
                    agent_id: read_text_value(session, "agentId")
                        .or_else(|| read_text_from_map(&runtime_settings, "agentName")),
                    agent_session_id: read_text_from_map(&runtime_settings, "agentSessionId"),
                    agent_session_path: read_text_from_map(&runtime_settings, "agentSessionPath"),
                };
                if !identities_match(identity, &candidate_identity) {
                    return None;
                }
                let title = trusted_resume_title(session)?;
                Some(TrustedTitleCandidate {
                    reason: format!(
                        "matching-live-session:{}",
                        read_text_value(session, "sessionId").unwrap_or_default()
                    ),
                    title_source: normalize_title_source(
                        object_field(session, "runtimeSettings")
                            .get("titleSource")
                            .and_then(Value::as_str),
                        &title,
                    ),
                    updated_at: read_text_value(session, "lastActiveAt")
                        .or_else(|| read_text_value(session, "updatedAt")),
                    title,
                })
            })
            .collect(),
    );
    if live_candidate.is_some() {
        return live_candidate;
    }

    select_newest_candidate(
        project
            .get("previousSessionHistory")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| create_history_title_candidate(item, identity))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

fn create_history_title_candidate(
    value: &Value,
    identity: &ResolvedIdentity,
) -> Option<TrustedTitleCandidate> {
    let record = value.as_object()?;
    let session_record = record.get("sessionRecord").and_then(Value::as_object);
    let hidden_record = record
        .get("hiddenRestoreMetadata")
        .and_then(Value::as_object)
        .and_then(|hidden| hidden.get("sessionRecord"))
        .and_then(Value::as_object);
    let candidate_identity = ResolvedIdentity {
        agent_id: read_text_from_record(record, "agentId")
            .or_else(|| read_text_from_record(record, "agentName"))
            .or_else(|| session_record.and_then(|item| read_text_from_record(item, "agentName")))
            .or_else(|| hidden_record.and_then(|item| read_text_from_record(item, "agentName")))
            .and_then(|value| normalize_agent_id(Some(&value))),
        agent_session_id: read_text_from_record(record, "agentSessionId")
            .or_else(|| {
                session_record.and_then(|item| read_text_from_record(item, "agentSessionId"))
            })
            .or_else(|| {
                hidden_record.and_then(|item| read_text_from_record(item, "agentSessionId"))
            }),
        agent_session_path: read_text_from_record(record, "agentSessionPath")
            .or_else(|| {
                session_record.and_then(|item| read_text_from_record(item, "agentSessionPath"))
            })
            .or_else(|| {
                hidden_record.and_then(|item| read_text_from_record(item, "agentSessionPath"))
            }),
    };
    if !identities_match(identity, &candidate_identity) {
        return None;
    }

    let updated_at = read_text_from_record(record, "lastInteractionAt")
        .or_else(|| read_text_from_record(record, "closedAt"));
    if let Some(session_record) = session_record {
        if let Some(candidate) = create_trusted_title_candidate(
            session_record.get("title"),
            session_record.get("titleSource"),
            "previous-session-record-title",
            updated_at.clone(),
        ) {
            return Some(candidate);
        }
    }
    create_trusted_title_candidate(
        record.get("primaryTitle"),
        Some(&json!(if record
            .get("isPrimaryTitleTerminalTitle")
            .and_then(Value::as_bool)
            == Some(true)
        {
            "terminal-auto"
        } else {
            "user"
        })),
        "previous-session-primary-title",
        updated_at.clone(),
    )
    .or_else(|| {
        create_trusted_title_candidate(
            record.get("terminalTitle"),
            Some(&json!("terminal-auto")),
            "previous-session-terminal-title",
            updated_at,
        )
    })
}

fn create_trusted_title_candidate(
    title: Option<&Value>,
    title_source: Option<&Value>,
    reason: &str,
    updated_at: Option<String>,
) -> Option<TrustedTitleCandidate> {
    let normalized_title = get_visible_terminal_title(title?.as_str()?)?
        .trim()
        .to_string();
    if normalized_title.is_empty() || is_rejected_resume_title(&normalized_title) {
        return None;
    }
    let normalized_source =
        normalize_title_source(title_source.and_then(Value::as_str), &normalized_title);
    if normalized_source == "placeholder" {
        return None;
    }
    Some(TrustedTitleCandidate {
        reason: reason.to_string(),
        title: normalized_title,
        title_source: normalized_source,
        updated_at,
    })
}

fn select_newest_candidate(
    mut candidates: Vec<TrustedTitleCandidate>,
) -> Option<TrustedTitleCandidate> {
    candidates.sort_by(|left, right| {
        timestamp_value(right.updated_at.as_deref())
            .cmp(&timestamp_value(left.updated_at.as_deref()))
    });
    candidates.into_iter().next()
}

fn timestamp_value(value: Option<&str>) -> i64 {
    value.and_then(parse_iso_ms).unwrap_or(0)
}

fn normalize_title_source(source: Option<&str>, title: &str) -> String {
    match source {
        Some("browser-auto") => "browser-auto".to_string(),
        Some("generated") => "generated".to_string(),
        Some("terminal-auto") => "terminal-auto".to_string(),
        Some("user") => "user".to_string(),
        Some("placeholder") => "placeholder".to_string(),
        _ if is_temporary_title(title) => "placeholder".to_string(),
        _ => "user".to_string(),
    }
}

fn read_text_from_record(record: &Map<String, Value>, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn identities_match(left: &ResolvedIdentity, right: &ResolvedIdentity) -> bool {
    let left_agent = normalize_agent_id(left.agent_id.as_deref());
    let right_agent = normalize_agent_id(right.agent_id.as_deref());
    if left_agent.is_some() && right_agent.is_some() && left_agent != right_agent {
        return false;
    }
    if left.agent_session_id.is_some()
        && right.agent_session_id.is_some()
        && left.agent_session_id == right.agent_session_id
    {
        return true;
    }
    left.agent_session_path.is_some()
        && right.agent_session_path.is_some()
        && left.agent_session_path == right.agent_session_path
}

fn normalize_codex_session_id(value: &str) -> Option<String> {
    is_uuid(value.trim()).then(|| value.trim().to_ascii_lowercase())
}

fn is_explicit_user_prompt_submit_event(params: &Map<String, Value>) -> bool {
    params
        .get("eventName")
        .or_else(|| params.get("rawEventName"))
        .and_then(Value::as_str)
        .is_some_and(|event_name| event_name.trim().eq_ignore_ascii_case("UserPromptSubmit"))
}

fn claim_first_prompt_auto_title(
    repository: &DomainRepository<'_>,
    session: &Value,
    prompt: Option<String>,
    is_explicit_user_prompt_submit: bool,
) -> Result<Option<Value>, DomainStateError> {
    let decision = decide_first_prompt_auto_title_claim(
        session,
        prompt.as_deref(),
        false,
        is_explicit_user_prompt_submit,
    );
    if !decision.should_run {
        return Ok(None);
    };
    let Some(prompt) = prompt else {
        return Ok(None);
    };
    let mut runtime_settings = object_field(session, "runtimeSettings");
    /*
    CDXC:GxserverForkTitles 2026-07-11:
    A fork's initial `Fork: …` CLI rename is provisional, not the first-prompt
    generated name. Remove the defensive auto-title bit when the fork's first
    real prompt is claimed, while keeping the fork marker through the async
    job so its non-generic provisional title remains eligible.
    */
    runtime_settings.remove("autoTitleFromFirstPrompt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledAt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleCancelledPrompt");
    runtime_settings.remove("gxserverFirstPromptAutoTitleReason");
    runtime_settings.insert("firstUserMessage".to_string(), json!(prompt));
    runtime_settings.insert(
        "gxserverFirstPromptAutoTitleStatus".to_string(),
        json!("running"),
    );
    runtime_settings.insert(
        FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
        json!(Uuid::new_v4().to_string()),
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

#[derive(Debug, PartialEq, Eq)]
struct FirstPromptAutoTitleClaimDecision {
    normalized_prompt: Option<String>,
    reason: String,
    should_run: bool,
    strategy: Option<&'static str>,
}

/*
CDXC:GxserverSessionTitle 2026-06-22-08:12:
Rust must match TypeScript gxserver's first-prompt claim boundary, not only the later background job. Claim only supported providers with generic titles and real user prompts, and skip meta and slash-command prompts without setting `running`.

CDXC:GxserverSessionTitle 2026-08-03:
Escape cancels one title-generation attempt, not every later submission of the
same prompt. A fresh explicit UserPromptSubmit may therefore re-arm identical
cancelled text, while passive sidecar, lifecycle, and later hook replays remain
blocked so cancellation cannot restart itself.
*/
fn decide_first_prompt_auto_title_claim(
    session: &Value,
    prompt: Option<&str>,
    allow_running: bool,
    is_explicit_user_prompt_submit: bool,
) -> FirstPromptAutoTitleClaimDecision {
    let runtime_settings = object_field(session, "runtimeSettings");
    let fork_first_prompt_rearmed = runtime_settings
        .get("forkFirstPromptAutoTitlePending")
        .and_then(Value::as_bool)
        == Some(true);
    let status = read_text_from_map(&runtime_settings, "gxserverFirstPromptAutoTitleStatus");
    let normalized_prompt = normalize_first_prompt_title_claim_prompt(prompt);
    let cancelled_prompt = normalize_first_prompt_title_claim_prompt(
        read_text_from_map(
            &runtime_settings,
            "gxserverFirstPromptAutoTitleCancelledPrompt",
        )
        .as_deref(),
    )
    .or_else(|| {
        normalize_first_prompt_title_claim_prompt(
            read_text_from_map(&runtime_settings, "firstUserMessage").as_deref(),
        )
    });
    let is_cancelled_retry_prompt = status.as_deref() == Some("cancelled")
        && normalized_prompt.is_some()
        && (normalized_prompt != cancelled_prompt || is_explicit_user_prompt_submit);
    if (status.as_deref() == Some("running") && !allow_running)
        || matches!(status.as_deref(), Some("applied" | "failed" | "skipped"))
        || (status.as_deref() == Some("cancelled") && !is_cancelled_retry_prompt)
    {
        return first_prompt_claim_decision(
            normalized_prompt,
            &format!("already-{}", status.unwrap_or_default()),
            false,
            None,
        );
    }
    if !fork_first_prompt_rearmed
        && runtime_settings
            .get("autoTitleFromFirstPrompt")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return first_prompt_claim_decision(normalized_prompt, "alreadyAutoNamed", false, None);
    }
    let agent_name = first_prompt_claim_agent_name(session, &runtime_settings);
    let strategy = first_prompt_claim_strategy(agent_name.as_deref());
    if strategy.is_none() {
        return first_prompt_claim_decision(normalized_prompt, "unsupportedAgent", false, None);
    }
    let Some(normalized) = normalized_prompt.clone() else {
        return first_prompt_claim_decision(normalized_prompt, "emptyPrompt", false, strategy);
    };
    if is_first_prompt_claim_meta_prompt(&normalized) {
        return first_prompt_claim_decision(Some(normalized), "metaPrompt", false, strategy);
    }
    if is_first_prompt_claim_slash_command(prompt, &normalized) {
        return first_prompt_claim_decision(Some(normalized), "slashCommand", false, strategy);
    }
    if !fork_first_prompt_rearmed
        && !is_first_prompt_claim_generic_title(
            agent_name.as_deref(),
            read_text_value(session, "title").as_deref(),
        )
    {
        return first_prompt_claim_decision(
            Some(normalized),
            "nonGenericCurrentTitle",
            false,
            strategy,
        );
    }
    first_prompt_claim_decision(Some(normalized), "eligible", true, strategy)
}

fn first_prompt_claim_decision(
    normalized_prompt: Option<String>,
    reason: &str,
    should_run: bool,
    strategy: Option<&'static str>,
) -> FirstPromptAutoTitleClaimDecision {
    FirstPromptAutoTitleClaimDecision {
        normalized_prompt,
        reason: reason.to_string(),
        should_run,
        strategy,
    }
}

fn first_prompt_claim_agent_name(
    session: &Value,
    runtime_settings: &Map<String, Value>,
) -> Option<String> {
    read_text_value(session, "agentId")
        .or_else(|| read_text_from_map(runtime_settings, "agentName"))
}

fn first_prompt_claim_strategy(agent_name: Option<&str>) -> Option<&'static str> {
    match normalize_first_prompt_claim_agent_name(agent_name).as_deref() {
        Some("claude") => Some("sendBareRenameCommand"),
        Some("codex") => Some("generateTitleAndRename"),
        Some("pi") => Some("generateTitleAndName"),
        _ => None,
    }
}

fn normalize_first_prompt_claim_agent_name(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "openai codex" | "codex cli" => Some("codex".to_string()),
        "claude code" => Some("claude".to_string()),
        "π" => Some("pi".to_string()),
        other => Some(other.to_string()),
    }
}

fn is_first_prompt_claim_generic_title(agent_name: Option<&str>, title: Option<&str>) -> bool {
    let normalized_title = title
        .map(|value| {
            value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    if normalized_title.is_empty() {
        return true;
    }
    let normalized_agent = normalize_first_prompt_claim_agent_name(agent_name);
    let generic = [
        "terminal",
        "terminal session",
        "agent",
        "agent session",
        "claude",
        "claude code",
        "claude session",
        "codex",
        "codex cli",
        "codex session",
        "openai codex",
        "openai codex session",
        "pi",
        "\u{03c0}",
        "pi session",
    ];
    generic.contains(&normalized_title.as_str())
        || normalized_agent.as_deref() == Some(normalized_title.as_str())
}

fn normalize_first_prompt_title_claim_prompt(prompt: Option<&str>) -> Option<String> {
    let normalized = prompt?.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let stripped = strip_first_prompt_title_claim_prefixes(normalized);
    let cleaned = stripped
        .trim()
        .trim_end_matches(['.', '?', '!', ':', ';', ','])
        .trim();
    Some(
        if cleaned.is_empty() {
            normalized
        } else {
            cleaned
        }
        .to_string(),
    )
}

fn strip_first_prompt_title_claim_prefixes(value: &str) -> &str {
    let mut stripped = value;
    loop {
        let lower = stripped.to_lowercase();
        let prefix = [
            "please ",
            "kindly ",
            "hey ",
            "hi ",
            "hello ",
            "can you ",
            "could you ",
            "would you ",
            "will you ",
            "can we ",
            "could we ",
            "would we ",
            "help me ",
            "i need you to ",
            "i need to ",
            "i need ",
            "how do i ",
            "how does ",
            "is there any way to ",
            "is there way to ",
        ]
        .into_iter()
        .find(|prefix| lower.starts_with(prefix));
        let Some(prefix) = prefix else {
            return stripped;
        };
        stripped = &stripped[prefix.len()..];
    }
}

fn is_first_prompt_claim_meta_prompt(prompt: &str) -> bool {
    prompt.starts_with("# AGENTS")
        || prompt.contains("tool_use_id")
        || [
            "<command",
            "<environment_context",
            "<permissions instructions>",
            "<user_instructions>",
            "<INSTRUCTIONS>",
            "<collaboration_mode>",
            "<app-context>",
            "<turn_aborted>",
            "<ide_opened_file>",
            "<local-",
            "[Tool Result]",
            "Caveat:",
        ]
        .iter()
        .any(|prefix| prompt.starts_with(prefix))
}

fn is_first_prompt_claim_slash_command(raw_prompt: Option<&str>, normalized_prompt: &str) -> bool {
    if normalized_prompt.encode_utf16().count() > 50 {
        return false;
    }
    let Some(raw_prompt) = raw_prompt else {
        return false;
    };
    raw_prompt
        .split('\n')
        .any(is_first_prompt_claim_slash_command_line)
}

fn is_first_prompt_claim_slash_command_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let Some(rest) = trimmed.strip_prefix('/') else {
        return false;
    };
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut consumed_bytes = first.len_utf8();
    for (index, ch) in chars {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            consumed_bytes = index + ch.len_utf8();
            continue;
        }
        consumed_bytes = index;
        break;
    }
    let suffix = &rest[consumed_bytes..];
    suffix
        .chars()
        .next()
        .map(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ')' | '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '`'
                )
        })
        .unwrap_or(true)
}

fn should_persist_activity_update(session: &Value, update: &ActivityUpdate) -> bool {
    read_text_value(session, "lastActiveAt") != update.last_active_at
        || persistable_agent_activity_snapshot(
            object_field(session, "runtimeSettings").get("agentActivity"),
        ) != persistable_agent_activity_snapshot(Some(&update.activity))
}

fn persistable_agent_activity_snapshot(value: Option<&Value>) -> Value {
    let Some(activity) = value.and_then(Value::as_object) else {
        return json!({});
    };
    let mut snapshot = Map::new();
    for key in [
        "activity",
        "agentName",
        "attentionEventId",
        "attentionSuppressedUntil",
        "hasSeenWorking",
        "isAcknowledged",
        "lastMeaningfulActivityAt",
        "lastTitle",
        "lastTitleChangeAt",
        // Session Chat interactive prompt (question/approval card JSON).
        // REQUIRED here: a key missing from this whitelist is invisible to
        // change detection, so should_persist_activity_update would return
        // false and the prompt would never reach disk.
        "sessionChatPrompt",
        "suppressedUntil",
        "workingSource",
        "workingStartedAt",
    ] {
        if let Some(value) = activity.get(key) {
            snapshot.insert(key.to_string(), value.clone());
        }
    }
    if activity.get("activity").and_then(Value::as_str) != Some("idle") {
        if let Some(value) = activity.get("lastChangedAt") {
            snapshot.insert("lastChangedAt".to_string(), value.clone());
        }
    }
    Value::Object(snapshot)
}

pub(crate) fn normalize_agent_hook_activity(
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
    let lower = event.to_ascii_lowercase();
    /*
    CDXC:AgentHookStatus 2026-06-22-08:31:
    Server-side hook ingestion must use provider event semantics before trusting
    sidecar status. Codex Stop is an authoritative completed-turn boundary, so
    it enters attention; SessionEnd still clears the session to idle. Keeping
    the event mapping here aligned with the hook helper prevents a later
    sidecar sync from erasing the attention transition.
    */
    if normalized_agent.as_deref() == Some("codex") {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if normalized_agent.as_deref() == Some("claude") {
        if matches!(lower.as_str(), "stop" | "idle") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit" | "prompt-submit" | "pretooluse" | "pre-tool-use"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(
        normalized_agent.as_deref(),
        Some("copilot" | "codebuddy" | "droid" | "qoder")
    ) {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if normalized_agent.as_deref() == Some("antigravity") {
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    if matches!(
        lower.as_str(),
        "stop"
            | "agent-response"
            | "afteragent"
            | "afteragentresponse"
            | "agent.end"
            | "agent_end"
            | "on_complete"
            | "on_error"
            | "post_llm_call"
            | "turn-completion"
    ) {
        return Some("idle".to_string());
    }
    if matches!(
        lower.as_str(),
        "on_tool_permission"
            | "post_approval_response"
            | "pretooluse"
            | "posttooluse"
            | "pre_tool_call"
            | "beforeagent"
            | "preinvocation"
            | "userpromptsubmit"
            | "agent.start"
            | "agent_start"
            | "beforeshellexecution"
            | "beforesubmitprompt"
    ) {
        return Some("working".to_string());
    }
    if let Some(status) = status
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "idle" | "working" | "attention"))
    {
        return Some(status.to_string());
    }
    if event.is_empty() {
        return None;
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "BeforeAgent",
            "PreInvocation",
            "PreToolUse",
            "UserPromptSubmit",
            "agent.start",
            "agent_start",
            "agentSpawn",
            "beforeShellExecution",
            "beforeSubmitPrompt",
            "on_session_reset",
            "on_session_start",
            "on_tool_permission",
            "post_approval_response",
            "postToolUse",
            "pre_llm_call",
            "pre_tool_call",
            "preToolUse",
            "userPromptSubmit",
        ],
    ) {
        return Some("working".to_string());
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "Notification",
            "PermissionRequest",
            "message.updated",
            "permission.updated",
            "pre_approval_request",
            "session.updated",
        ],
    ) {
        return Some("attention".to_string());
    }
    if agent_hook_event_matches(
        &event,
        &lower,
        &[
            "AfterAgent",
            "SessionEnd",
            "Stop",
            "afterAgentResponse",
            "agent.end",
            "agent_end",
            "on_complete",
            "on_error",
            "on_session_end",
            "on_session_finalize",
            "release",
            "session.end",
            "session_shutdown",
            "turn-completion",
        ],
    ) {
        return Some("idle".to_string());
    }
    None
}

fn agent_hook_event_matches(event: &str, lower_event: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| event == *candidate || lower_event == *candidate)
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

/*
CDXC:GxserverSessionIdentity 2026-06-24-04:49:
Passive hook and sidecar events must not let stale Droid metadata replace a row gxserver already owns as Pi or another agent.
Treat stored agentId/runtime agentName as an identity lock when older rows do not yet have launchAgentId, while still allowing unowned terminal rows to be promoted by first matching observations.
*/
fn locked_session_agent_id(session: &Value) -> Option<String> {
    let runtime_settings = object_field(session, "runtimeSettings");
    let launch_settings = object_field(session, "launchSettings");
    normalize_agent_id(
        runtime_settings
            .get("launchAgentId")
            .and_then(Value::as_str),
    )
    .or_else(|| normalize_agent_id(read_text_value(session, "agentId").as_deref()))
    .or_else(|| normalize_agent_id(runtime_settings.get("agentName").and_then(Value::as_str)))
    .or_else(|| {
        launch_settings
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
        launch_settings
            .get("startupText")
            .and_then(Value::as_str)
            .and_then(infer_agent_id_from_command)
    })
}

fn launch_agent_mismatch(session: &Value, incoming_agent_id: Option<&str>) -> bool {
    let Some(incoming) = normalize_agent_id(incoming_agent_id) else {
        return false;
    };
    locked_session_agent_id(session)
        .map(|locked| {
            locked != incoming
                && session_launch_agent_provider_id(session).as_deref() != Some(incoming.as_str())
        })
        .unwrap_or(false)
}

fn session_launch_agent_provider_id(session: &Value) -> Option<String> {
    normalize_agent_id(
        object_field(session, "launchSettings")
            .get("icon")
            .and_then(Value::as_str),
    )
}

fn align_observed_identity_with_launch_profile(
    session: &Value,
    mut identity: ResolvedIdentity,
) -> ResolvedIdentity {
    let observed_agent_id = normalize_agent_id(identity.agent_id.as_deref());
    if observed_agent_id.is_some() && observed_agent_id == session_launch_agent_provider_id(session)
    {
        if let Some(launch_agent_id) = locked_session_agent_id(session) {
            identity.agent_id = Some(launch_agent_id);
        }
    }
    identity
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
    let spec = accept_all_spec(agent_id).or_else(|| {
        icon.and_then(default_agent_icon_to_id)
            .and_then(accept_all_spec)
    });
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
        "copilot" => AcceptAllSpec::Flag {
            aliases: vec!["--allow-all".to_string(), "--yolo".to_string()],
            canonical: "--yolo",
        },
        "cursor" => AcceptAllSpec::Flag {
            aliases: vec!["--force".to_string(), "--yolo".to_string()],
            canonical: "--yolo",
        },
        "gemini" => AcceptAllSpec::Flag {
            aliases: vec!["-y".to_string(), "--yolo".to_string()],
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
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_accept_all_flag_token(token, aliases) {
            index += 1;
            continue;
        }
        if token == GROK_PERMISSION_MODE_FLAG {
            if let Some(value_token) = tokens.get(index + 1) {
                if is_grok_bypass_value_or_assignment_token(value_token) {
                    index += 2;
                    continue;
                }
            }
        }
        if is_grok_permission_mode_equals_token(token) {
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

fn strip_duplicate_accept_all_flags(command: &str, aliases: &[String]) -> String {
    let mut seen = false;
    let mut output = Vec::new();
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if is_accept_all_flag_token(token, aliases) {
            if !seen {
                output.push(token);
                seen = true;
            }
            index += 1;
            continue;
        }
        if token == GROK_PERMISSION_MODE_FLAG
            && tokens.get(index + 1) == Some(&GROK_BYPASS_PERMISSIONS_VALUE)
        {
            if !seen {
                output.push(token);
                output.push(tokens[index + 1]);
                seen = true;
            }
            index += 2;
            continue;
        }
        if is_grok_permission_mode_equals_token(token) {
            if !seen {
                output.push(token);
                seen = true;
            }
            index += 1;
            continue;
        }
        output.push(token);
        index += 1;
    }
    output.join(" ")
}

fn command_includes_accept_all_flag(command: &str, aliases: &[String]) -> bool {
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if is_accept_all_flag_token(token, aliases) {
            return true;
        }
        if *token == GROK_PERMISSION_MODE_FLAG
            && tokens.get(index + 1) == Some(&GROK_BYPASS_PERMISSIONS_VALUE)
        {
            return true;
        }
        if is_grok_permission_mode_equals_token(token) {
            return true;
        }
    }
    false
}

fn is_accept_all_flag_token(token: &str, aliases: &[String]) -> bool {
    aliases.iter().any(|alias| {
        token == alias
            || token
                .strip_prefix(alias)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn is_grok_permission_mode_equals_token(token: &str) -> bool {
    token
        .strip_prefix(GROK_PERMISSION_MODE_FLAG)
        .and_then(|rest| rest.strip_prefix('='))
        .is_some_and(|value| value == GROK_BYPASS_PERMISSIONS_VALUE)
}

fn is_grok_bypass_value_or_assignment_token(token: &str) -> bool {
    token == GROK_BYPASS_PERMISSIONS_VALUE
        || token
            .strip_prefix(GROK_BYPASS_PERMISSIONS_VALUE)
            .is_some_and(|rest| rest.starts_with('='))
}

fn default_agent_icon_to_id(icon: &str) -> Option<&'static str> {
    match icon {
        "amp-cli" => Some("amp"),
        "antigravity-cli" => Some("antigravity"),
        "claude" => Some("claude"),
        "codebuddy" => Some("codebuddy"),
        "codex" => Some("codex"),
        "copilot" => Some("copilot"),
        "cursor-cli" => Some("cursor"),
        "factory-droid" => Some("droid"),
        "gemini" => Some("gemini"),
        "grok-build" => Some("grok"),
        "hermes-agent" => Some("hermes-agent"),
        "kiro" => Some("kiro"),
        "omp" => Some("omp"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "qoder" => Some("qoder"),
        "rovo-dev" => Some("rovodev"),
        _ => None,
    }
}

pub(crate) fn default_agent_command(agent_id: &str) -> Option<&'static str> {
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
        _ => None,
    }
}

pub(crate) fn get_visible_terminal_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    if is_path_like_terminal_title(&normalized)
        || is_shell_location_terminal_title(&normalized)
        || is_ignored_placeholder_session_title(&normalized)
        || is_generic_agent_title(&normalized)
        || is_status_word_title(&normalized)
        || is_windows_default_powershell_title(&normalized)
    {
        return None;
    }
    Some(normalized)
}

fn normalize_terminal_title(title: &str) -> Option<String> {
    let value = title.trim();
    if value.is_empty() {
        return None;
    }
    let value = strip_oc_prefixes(value.trim_start_matches(is_leading_title_marker).trim());
    if let Some(cursor) = normalize_cursor_terminal_title(&value) {
        return cursor;
    }
    if let Some(antigravity) = normalize_antigravity_terminal_title(&value) {
        return antigravity;
    }
    if let Some(pi) = normalize_pi_terminal_title(&value) {
        return Some(pi);
    }
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn get_codex_session_id_from_title(title: &str) -> Option<String> {
    let normalized = normalize_terminal_title(title)?;
    is_uuid(&normalized).then(|| normalized.to_ascii_lowercase())
}

pub(crate) fn terminal_title_indicates_agent_identity(title: &str) -> bool {
    get_codex_session_id_from_title(title).is_some()
        || get_terminal_title_detected_agent_name(title).is_some()
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
    if normalize_title_source(
        read_text_from_map(&runtime_settings, "titleSource")
            .or_else(|| read_text_from_map(&runtime_settings, "restoreTitleSource"))
            .as_deref(),
        &title,
    ) == "placeholder"
    {
        return None;
    }
    let visible = get_visible_terminal_title(&title)?;
    (!is_rejected_resume_title(&visible)).then_some(visible)
}

fn is_rejected_resume_title(title: &str) -> bool {
    let normalized = title.trim();
    let lower = normalized.to_ascii_lowercase();
    normalized == "\u{00f0}^\u{00df}^\u{00d1}\u{00bb}"
        || is_temporary_title(normalized)
        || is_ghost_placeholder_session_title(normalized)
        || is_gxserver_session_id(title.trim())
        || normalized
            .chars()
            .any(|ch| (ch as u32) <= 0x1f || (ch as u32) == 0x7f)
        || (normalized.starts_with('\u{00f0}') && normalized.ends_with('\u{00bb}'))
        || is_agent_command_noise_title(&lower)
}

fn is_temporary_title(title: &str) -> bool {
    normalize_spaces(title).eq_ignore_ascii_case("search by text")
}

fn is_ignored_placeholder_session_title(title: &str) -> bool {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    is_session_number_title(&normalized)
        || get_codex_session_id_from_title(&normalized).is_some()
        || is_ghost_placeholder_session_title(&normalized)
        || is_status_word_title(&normalized)
        || is_ignored_placeholder_session_title_text(&lower)
        || is_path_like_terminal_title(&normalized)
}

fn is_ignored_placeholder_session_title_text(lower: &str) -> bool {
    matches!(
        lower,
        "terminal session"
            | "amp cli session"
            | "amp session"
            | "antigravity cli session"
            | "antigravity session"
            | "claude code session"
            | "claude session"
            | "code buddy session"
            | "codebuddy session"
            | "codex cli session"
            | "codex session"
            | "copilot session"
            | "cursor agent session"
            | "cursor cli session"
            | "cursor session"
            | "droid session"
            | "factory droid session"
            | "gemini session"
            | "grok build session"
            | "grok session"
            | "hermes agent session"
            | "hermes session"
            | "kiro cli session"
            | "kiro session"
            | "omp session"
            | "open code session"
            | "openai codex session"
            | "opencode session"
            | "pi session"
            | "qoder session"
            | "qodercli session"
            | "rovo dev session"
            | "rovo session"
            | "rovodev session"
    )
}

fn is_generic_agent_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
    matches!(
        lower.as_str(),
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
            | "\u{03c0}"
            | "ghostex"
    )
}

fn is_status_word_title(title: &str) -> bool {
    let core = title
        .trim_matches(is_agent_status_boundary_char)
        .to_ascii_lowercase();
    matches!(
        core.as_str(),
        "done" | "error" | "idle" | "thinking" | "working"
    )
}

fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_leading_title_marker(ch: char) -> bool {
    /*
    CDXC:GxserverSessionTitles 2026-06-29-01:21:
    Factory Droid terminal titles can prefix visible session names with the U+26EC status marker.
    Strip it at the same boundary as Claude, Codex, Cursor, Gemini, and Copilot title chrome so copied details and sidebar rows show the semantic title instead of provider decoration.
    */
    ch.is_whitespace()
        || ('\u{2800}'..='\u{28ff}').contains(&ch)
        || matches!(
            ch,
            '\u{00b7}'
                | '\u{2022}'
                | '\u{22c5}'
                | '\u{25e6}'
                | '\u{2733}'
                | '*'
                | '\u{2217}'
                | '\u{2736}'
                | '\u{273b}'
                | '\u{273d}'
                | '\u{2738}'
                | '\u{2739}'
                | '\u{273a}'
                | '\u{2737}'
                | '\u{2734}'
                | '\u{25d0}'
                | '\u{25d1}'
                | '\u{25d2}'
                | '\u{25d3}'
                | '\u{26ec}'
                | '\u{2726}'
                | '\u{25c7}'
                | '\u{1f916}'
                | '\u{1f514}'
        )
}

fn strip_oc_prefixes(title: &str) -> String {
    let mut rest = title;
    loop {
        let lower = rest.to_ascii_lowercase();
        if !lower.starts_with("oc") {
            break;
        }
        let after_oc = &rest[2..];
        let after_spaces = after_oc.trim_start();
        let Some(after_pipe) = after_spaces.strip_prefix('|') else {
            break;
        };
        rest = after_pipe.trim_start();
    }
    rest.to_string()
}

fn normalize_cursor_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title);
    if is_cursor_placeholder_title(&normalized) {
        return Some(None);
    }
    if normalized.ends_with("\u{2705} Ready") {
        let stripped = strip_cursor_status_suffix(&normalized, "\u{2705} Ready");
        return Some(cursor_status_title(stripped));
    }
    let working_marker = "\u{23f3} Working ";
    if let Some(index) = normalized.rfind(working_marker) {
        let trailing = &normalized[index + working_marker.len()..];
        if !trailing.is_empty() && trailing.chars().all(|ch| ch == '.' || ch == '\u{00b7}') {
            let stripped = strip_cursor_working_suffix(&normalized, index);
            return Some(cursor_status_title(stripped));
        }
    }
    None
}

fn cursor_status_title(stripped: String) -> Option<String> {
    if stripped.is_empty() || is_cursor_placeholder_title(&stripped) {
        None
    } else {
        Some(stripped)
    }
}

fn strip_cursor_status_suffix(title: &str, suffix: &str) -> String {
    let Some(prefix) = title.strip_suffix(suffix) else {
        return title.trim().to_string();
    };
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

fn strip_cursor_working_suffix(title: &str, status_index: usize) -> String {
    let prefix = &title[..status_index];
    prefix
        .trim_end()
        .strip_suffix('-')
        .map(str::trim)
        .unwrap_or(title)
        .trim()
        .to_string()
}

fn is_cursor_placeholder_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" | "cursor agent - \u{2705} ready"
    )
}

fn normalize_antigravity_terminal_title(title: &str) -> Option<Option<String>> {
    let normalized = normalize_spaces(title);
    let lower = normalized.to_ascii_lowercase();
    if lower == "agy" {
        return Some(Some("agy".to_string()));
    }
    if let Some(rest) = normalized.strip_prefix('\u{1f514}') {
        if rest.trim().eq_ignore_ascii_case("agy") {
            return Some(Some("agy".to_string()));
        }
    }
    None
}

fn is_ghost_placeholder_session_title(title: &str) -> bool {
    matches!(
        normalize_spaces(title).as_str(),
        "\u{1f47b}" | "\u{1f47b} Terminal Session"
    )
}

fn is_session_number_title(title: &str) -> bool {
    let lower = normalize_spaces(title).to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("session ") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
}

fn is_path_like_terminal_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.starts_with('~')
        || trimmed.starts_with('/')
        || trimmed.starts_with("\u{2026}/")
        || trimmed.starts_with("\u{2026}\\")
        || trimmed.starts_with(".../")
        || trimmed.starts_with("...\\")
}

fn is_shell_location_terminal_title(title: &str) -> bool {
    /*
    Interactive WSL shells commonly publish `user@host: /path` as their OSC
    title. That describes the terminal location, not the Ghostex session, and
    must never replace Terminal Session, an agent placeholder, or a generated
    first-prompt title.
    */
    let Some((user_host, location)) = title.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    if user.trim().is_empty()
        || host.trim().is_empty()
        || user.chars().any(char::is_whitespace)
        || host.chars().any(char::is_whitespace)
    {
        return false;
    }
    let location = location.trim_start();
    is_path_like_terminal_title(location) || is_windows_absolute_terminal_path(location)
}

fn is_windows_absolute_terminal_path(title: &str) -> bool {
    let bytes = title.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || title.starts_with("\\\\")
}

fn is_agent_status_boundary_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ':' | '[' | ']' | '(' | ')' | '{' | '}' | '!' | '|' | '/' | '\\' | '_' | '-'
        )
}

fn is_windows_default_powershell_title(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let rest = &lower[1..];
    let prefix = ":\\windows\\system32\\windowspowershell\\v1.0\\powershell.exe";
    let Some(suffix) = rest.strip_prefix(prefix) else {
        return false;
    };
    suffix.is_empty() || (suffix.starts_with(char::is_whitespace) && suffix.trim() == ".")
}

fn is_agent_command_noise_title(title: &str) -> bool {
    let Some(executable_name) = command_executable_name(title) else {
        return false;
    };
    if !is_agent_command_executable_name(&executable_name) {
        return false;
    }
    if title == executable_name {
        return true;
    }
    let rest = title[executable_name.len()..].trim();
    if rest.is_empty() || rest.starts_with('-') {
        return true;
    }
    let first_arg = rest.split_whitespace().next().unwrap_or_default();
    is_agent_command_subcommand_name(first_arg)
}

fn command_executable_name(command: &str) -> Option<String> {
    let first = command.split_whitespace().next()?.trim();
    let first = first.trim_matches(|ch| ch == '\'' || ch == '"');
    (!first.is_empty()).then(|| first.to_ascii_lowercase())
}

fn is_agent_command_executable_name(value: &str) -> bool {
    matches!(
        value,
        "acli"
            | "agy"
            | "amp"
            | "claude"
            | "codebuddy"
            | "codex"
            | "copilot"
            | "cursor-agent"
            | "droid"
            | "gemini"
            | "grok"
            | "hermes"
            | "kiro-cli"
            | "omp"
            | "opencode"
            | "pi"
            | "qodercli"
    )
}

fn is_agent_command_subcommand_name(value: &str) -> bool {
    matches!(
        value,
        "auth"
            | "completion"
            | "debug"
            | "exec"
            | "help"
            | "login"
            | "logout"
            | "mcp"
            | "resume"
            | "run"
            | "sandbox"
            | "session"
            | "sessions"
    )
}

fn get_terminal_title_detected_agent_name(title: &str) -> Option<String> {
    let normalized = normalize_spaces(title);
    [
        "antigravity",
        "claude",
        "codex",
        "copilot",
        "cursor",
        "gemini",
        "pi",
    ]
    .into_iter()
    .find(|agent| is_explicit_agent_program_terminal_title(&normalized, agent))
    .map(str::to_string)
}

fn is_explicit_agent_program_terminal_title(title: &str, agent_name: &str) -> bool {
    match agent_name {
        "antigravity" => {
            let lower = normalize_spaces(title).to_ascii_lowercase();
            lower == "agy" || lower == "\u{1f514} agy"
        }
        "claude" => strip_one_leading_title_marker(title).eq_ignore_ascii_case("Claude Code"),
        "codex" => matches!(
            strip_braille_dot_prefix(title)
                .to_ascii_lowercase()
                .as_str(),
            "codex" | "codex cli"
        ),
        "copilot" => matches!(
            strip_specific_prefix_markers(title, &['\u{1f916}', '\u{1f514}'])
                .to_ascii_lowercase()
                .as_str(),
            "copilot" | "copilot cli" | "github copilot" | "github copilot cli"
        ),
        "cursor" => matches!(
            normalize_spaces(title).to_ascii_lowercase().as_str(),
            "cursor agent" | "cursor agent - \u{2705} ready"
        ),
        "gemini" => matches!(
            strip_specific_prefix_markers(title, &['\u{2726}', '\u{25c7}'])
                .to_ascii_lowercase()
                .as_str(),
            "gemini" | "gemini cli"
        ),
        "pi" => {
            let stripped = title.trim_start_matches(is_leading_title_marker).trim();
            stripped.starts_with("\u{03c0} -") || stripped.starts_with("\u{03c0}-")
        }
        _ => false,
    }
}

fn strip_one_leading_title_marker(title: &str) -> String {
    let trimmed = title.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if is_leading_title_marker(first) {
        chars.as_str().trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_braille_dot_prefix(title: &str) -> String {
    title
        .trim_start_matches(|ch| {
            ('\u{2800}'..='\u{28ff}').contains(&ch)
                || matches!(ch, '\u{00b7}' | '\u{2022}' | '\u{22c5}' | '\u{25e6}')
                || ch.is_whitespace()
        })
        .trim()
        .to_string()
}

fn strip_specific_prefix_markers(title: &str, markers: &[char]) -> String {
    title
        .trim_start_matches(|ch: char| ch.is_whitespace() || markers.contains(&ch))
        .trim()
        .to_string()
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

pub(crate) fn read_text_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The staged first input draft, kept exactly as written. Every other runtime
/// text is trimmed, but the draft's trailing space (`@/path/export.md `) is
/// what separates the mention from the prompt the user types after it.
pub(crate) fn read_first_user_input_draft(session: &Value) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(FIRST_USER_INPUT_DRAFT_KEY))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn object_field(value: &Value, key: &str) -> Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
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

fn insert_truthy_from_params(
    target: &mut Map<String, Value>,
    params: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = params.get(key).cloned().filter(js_truthy_value) {
        target.insert(key.to_string(), value);
    }
}

fn js_truthy_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
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

fn quote_shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn wrap_restored_terminal_resume_command(
    command: &str,
    display_command: &str,
    fallback_command: Option<&str>,
) -> String {
    let mut lines = vec![
        format!("printf '%s\\n' {}", quote_shell_arg("Restoring session...")),
        format!("printf '> %s\\n\\n' {}", quote_shell_arg(display_command)),
        "__ghostex_restore_resume_status=0".to_string(),
        "__ghostex_restore_resume_primary() {".to_string(),
        command.to_string(),
        "}".to_string(),
        "__ghostex_restore_resume_primary || __ghostex_restore_resume_status=$?".to_string(),
        "unset -f __ghostex_restore_resume_primary".to_string(),
    ];
    if let Some(fallback_command) = fallback_command.filter(|fallback| *fallback != command) {
        lines.extend([
            "if [ \"$__ghostex_restore_resume_status\" -ne 0 ]; then".to_string(),
            format!(
                "  printf '%s\\n' {}",
                quote_shell_arg("Exact resume failed; trying saved fallback resume command.")
            ),
            "  __ghostex_restore_resume_status=0".to_string(),
            "  __ghostex_restore_resume_fallback() {".to_string(),
            fallback_command.to_string(),
            "  }".to_string(),
            "  __ghostex_restore_resume_fallback || __ghostex_restore_resume_status=$?".to_string(),
            "  unset -f __ghostex_restore_resume_fallback".to_string(),
            "fi".to_string(),
        ]);
    }
    lines.push("unset __ghostex_restore_resume_status".to_string());
    lines.join("\n")
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

    fn write_metadata_value(db: &Connection, key: &str, value: Value) {
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                key,
                serde_json::to_string(&value).expect("serialize metadata value"),
                "2026-06-19T00:00:00.000Z"
            ],
        )
        .expect("write metadata value");
    }

    fn create_codex_agent_session(
        repository: &DomainRepository<'_>,
        agent_session_id: &str,
        project_path: &Path,
    ) -> (LifecycleParams, Value) {
        let project = repository
            .create_project(
                json!({
                    "name": "Rename Test Project",
                    "path": project_path.to_string_lossy()
                })
                .as_object()
                .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "codex",
                        "agentSessionId": agent_session_id,
                        "titleSource": "placeholder"
                    },
                    "title": "Codex Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id: session
                .get("projectId")
                .and_then(Value::as_str)
                .expect("session project id")
                .to_string(),
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };
        (lifecycle, session)
    }

    fn create_pi_agent_session_without_launch_lock(
        repository: &DomainRepository<'_>,
    ) -> (LifecycleParams, Value) {
        let project = repository
            .create_project(
                json!({ "name": "Pi Lock Project" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "pi",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "pi",
                        "titleSource": "terminal-auto"
                    },
                    "title": "Pi Investigation"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id: session
                .get("projectId")
                .and_then(Value::as_str)
                .expect("session project id")
                .to_string(),
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };
        (lifecycle, session)
    }

    #[test]
    fn first_prompt_claim_decision_matches_provider_strategy_and_prompt_normalization() {
        let codex = json!({
            "agentId": "codex",
            "runtimeSettings": {},
            "title": "Terminal",
        });
        let decision = decide_first_prompt_auto_title_claim(
            &codex,
            Some("Please can you help me fix the sidebar."),
            false,
            false,
        );
        assert!(decision.should_run);
        assert_eq!(decision.reason, "eligible");
        assert_eq!(
            decision.normalized_prompt.as_deref(),
            Some("fix the sidebar")
        );
        assert_eq!(decision.strategy, Some("generateTitleAndRename"));

        let claude = json!({
            "agentId": "claude",
            "runtimeSettings": {},
            "title": "Claude Code",
        });
        let decision = decide_first_prompt_auto_title_claim(
            &claude,
            Some("Summarize the session logs"),
            false,
            false,
        );
        assert!(decision.should_run);
        assert_eq!(decision.strategy, Some("sendBareRenameCommand"));

        let pi = json!({
            "agentId": "pi",
            "runtimeSettings": {},
            "title": "\u{03c0}",
        });
        let decision = decide_first_prompt_auto_title_claim(
            &pi,
            Some("How does resource syncing work?"),
            false,
            false,
        );
        assert!(decision.should_run);
        assert_eq!(
            decision.normalized_prompt.as_deref(),
            Some("resource syncing work")
        );
        assert_eq!(decision.strategy, Some("generateTitleAndName"));
    }

    #[test]
    fn first_prompt_claim_decision_skips_non_claimable_prompts_without_running_state() {
        let codex = json!({
            "agentId": "codex",
            "runtimeSettings": {},
            "title": "Agent",
        });
        let meta = decide_first_prompt_auto_title_claim(
            &codex,
            Some("# AGENTS.md instructions for this repository"),
            false,
            false,
        );
        assert!(!meta.should_run);
        assert_eq!(meta.reason, "metaPrompt");

        let slash = decide_first_prompt_auto_title_claim(
            &codex,
            Some("notes before command\n  /status please"),
            false,
            false,
        );
        assert!(!slash.should_run);
        assert_eq!(slash.reason, "slashCommand");

        let unsupported = json!({
            "agentId": "cursor",
            "runtimeSettings": {},
            "title": "Terminal",
        });
        let unsupported = decide_first_prompt_auto_title_claim(
            &unsupported,
            Some("Summarize this"),
            false,
            false,
        );
        assert!(!unsupported.should_run);
        assert_eq!(unsupported.reason, "unsupportedAgent");

        let named = json!({
            "agentId": "codex",
            "runtimeSettings": { "autoTitleFromFirstPrompt": true },
            "title": "Codex",
        });
        let named =
            decide_first_prompt_auto_title_claim(&named, Some("Summarize this"), false, false);
        assert!(!named.should_run);
        assert_eq!(named.reason, "alreadyAutoNamed");
    }

    #[test]
    fn first_prompt_claim_retries_cancelled_job_for_new_submit_or_later_prompt() {
        let first_prompt = "Please cancel this generated title before rename";
        let session = json!({
            "agentId": "codex",
            "runtimeSettings": {
                "firstUserMessage": first_prompt,
                "gxserverFirstPromptAutoTitleCancelledAt": "2026-06-22T04:00:00.000Z",
                "gxserverFirstPromptAutoTitleCancelledPrompt": first_prompt,
                "gxserverFirstPromptAutoTitleReason": "escape",
                "gxserverFirstPromptAutoTitleStatus": "cancelled"
            },
            "title": "Terminal",
        });

        let same_passive =
            decide_first_prompt_auto_title_claim(&session, Some(first_prompt), false, false);
        assert!(!same_passive.should_run);
        assert_eq!(same_passive.reason, "already-cancelled");

        let same_explicit =
            decide_first_prompt_auto_title_claim(&session, Some(first_prompt), false, true);
        assert!(same_explicit.should_run);
        assert_eq!(same_explicit.reason, "eligible");

        let later = decide_first_prompt_auto_title_claim(
            &session,
            Some("Now explain the auto sleep defaults"),
            false,
            false,
        );
        assert!(later.should_run);
        assert_eq!(later.reason, "eligible");
        assert_eq!(
            later.normalized_prompt.as_deref(),
            Some("Now explain the auto sleep defaults")
        );
    }

    #[test]
    fn user_prompt_submit_hook_rearms_cancelled_identical_prompt() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let (lifecycle, session) = create_codex_agent_session(
            &repository,
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            temp.path(),
        );
        let first_prompt = "Please cancel this generated title before rename";
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert("firstUserMessage".to_string(), json!(first_prompt));
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
            json!(first_prompt),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("cancelled"),
        );
        runtime_settings.insert(
            FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
            json!("cancelled-attempt"),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update).expect("cancelled row");

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "eventName": "UserPromptSubmit",
                "firstUserMessage": first_prompt,
                "projectId": lifecycle.project_id.clone(),
                "sessionId": lifecycle.session_id.clone(),
                "status": "working"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(
            result.get("reason"),
            Some(&json!("first-prompt-auto-title-claimed"))
        );
        assert_eq!(
            result
                .get("session")
                .and_then(|session| session.get("runtimeSettings"))
                .and_then(Value::as_object)
                .and_then(|runtime| runtime.get("gxserverFirstPromptAutoTitleStatus")),
            Some(&json!("running"))
        );
        let replacement_attempt = result
            .get("session")
            .and_then(|session| session.get("runtimeSettings"))
            .and_then(Value::as_object)
            .and_then(|runtime| runtime.get(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY))
            .and_then(Value::as_str)
            .expect("replacement attempt id");
        assert_ne!(replacement_attempt, "cancelled-attempt");
        assert!(Uuid::parse_str(replacement_attempt).is_ok());
    }

    #[test]
    fn first_prompt_claim_clears_cancelled_metadata_for_repeated_explicit_prompt() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let (lifecycle, session) = create_codex_agent_session(
            &repository,
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            temp.path(),
        );
        let first_prompt = "Please cancel this generated title before rename";
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert("firstUserMessage".to_string(), json!(first_prompt));
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleCancelledAt".to_string(),
            json!("2026-06-22T04:00:00.000Z"),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleCancelledPrompt".to_string(),
            json!(first_prompt),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleReason".to_string(),
            json!("escape"),
        );
        runtime_settings.insert(
            "gxserverFirstPromptAutoTitleStatus".to_string(),
            json!("cancelled"),
        );
        runtime_settings.insert(
            FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY.to_string(),
            json!("cancelled-attempt"),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let cancelled = repository.update_session(&update).expect("cancelled row");

        let same = claim_first_prompt_auto_title(
            &repository,
            &cancelled,
            Some(first_prompt.to_string()),
            false,
        )
        .expect("passive same prompt claim");
        assert!(same.is_none());

        let latest = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read latest")
            .expect("latest session");
        let claimed = claim_first_prompt_auto_title(
            &repository,
            &latest,
            Some(first_prompt.to_string()),
            true,
        )
        .expect("explicit repeated prompt claim")
        .expect("claimed session");
        let runtime = object_field(&claimed, "runtimeSettings");
        assert_eq!(
            runtime
                .get("gxserverFirstPromptAutoTitleStatus")
                .and_then(Value::as_str),
            Some("running")
        );
        assert_eq!(
            runtime.get("firstUserMessage").and_then(Value::as_str),
            Some(first_prompt)
        );
        let replacement_attempt = runtime
            .get(FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY)
            .and_then(Value::as_str)
            .expect("replacement attempt id");
        assert_ne!(replacement_attempt, "cancelled-attempt");
        assert!(Uuid::parse_str(replacement_attempt).is_ok());
        assert!(runtime
            .get("gxserverFirstPromptAutoTitleCancelledAt")
            .is_none());
        assert!(runtime
            .get("gxserverFirstPromptAutoTitleCancelledPrompt")
            .is_none());
        assert!(runtime.get("gxserverFirstPromptAutoTitleReason").is_none());
    }

    #[test]
    fn terminal_title_capture_preserves_decision_reason_when_identity_title_is_trusted() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Terminal Title Capture" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "codex",
                        "agentSessionId": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                        "titleSource": "user"
                    },
                    "title": "Phase 6 Ingested Title"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id: session
                .get("projectId")
                .and_then(Value::as_str)
                .expect("session project id")
                .to_string(),
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };
        let captured_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

        let result = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "rawTitle": captured_id,
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let result = result.result;

        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("captured-agent-session-id"))
        );
        assert_eq!(result.get("agentSessionId"), Some(&json!(captured_id)));
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("Phase 6 Ingested Title")));
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentSessionId")),
            Some(&json!(captured_id))
        );
    }

    #[test]
    fn terminal_title_applies_zmx_title_with_previous_source_reason_without_agent_promotion() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Terminal Title Canonical" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "projectId": project_id,
                    "runtimeSettings": { "titleSource": "placeholder" },
                    "title": "Search by Text"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id,
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };

        let output = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "rawTitle": "Find previous Codex work",
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let result = output.result;

        assert!(output.schedule_presentation_delta);
        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("zmx-terminal-title-from-placeholder"))
        );
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("kind"), Some(&json!("terminal")));
        assert_eq!(session.get("agentId"), None);
        assert_eq!(
            session.get("title"),
            Some(&json!("Find previous Codex work"))
        );
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentName")),
            None
        );
    }

    #[test]
    fn terminal_title_strips_factory_droid_status_marker_before_sync() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Factory Droid Titles" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "droid",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "factory droid",
                        "titleSource": "placeholder"
                    },
                    "title": "Factory Droid Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id,
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };

        let output = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "factory droid",
                "rawTitle": "\u{26ec} New Session",
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let result = output.result;

        assert!(output.schedule_presentation_delta);
        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("zmx-terminal-title-from-placeholder"))
        );
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("New Session")));
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("titleSource")),
            Some(&json!("terminal-auto"))
        );
    }

    #[test]
    fn terminal_title_rejects_untrusted_provider_off_title() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Terminal Title Trust" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "projectId": project_id,
                    "runtimeSettings": { "titleSource": "user" },
                    "title": "Terminal Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id,
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };

        let output = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "rawTitle": "Untrusted local shell title",
                "sessionPersistenceProvider": "off"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let result = output.result;

        assert!(!output.schedule_presentation_delta);
        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("terminal-title-not-trusted"))
        );
        assert_eq!(
            result
                .get("session")
                .and_then(|session| session.get("title")),
            Some(&json!("Terminal Session"))
        );
    }

    #[test]
    fn terminal_title_status_bookkeeping_does_not_schedule_presentation_delta() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Terminal Title Status" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "projectId": project_id,
                    "runtimeSettings": { "titleSource": "terminal-auto" },
                    "title": "Search by Text"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id,
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };

        let output = ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "rawTitle": "Search by Text",
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let result = output.result;

        assert!(!output.schedule_presentation_delta);
        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(
            result
                .get("activity")
                .and_then(|activity| activity.get("activity")),
            Some(&json!("idle"))
        );
        assert_eq!(
            result
                .get("session")
                .and_then(|session| session.get("runtimeSettings"))
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentActivity"))
                .and_then(|activity| activity.get("lastTitle")),
            Some(&json!("Search by Text"))
        );
    }

    #[test]
    fn session_state_event_reconciles_codex_metadata_title() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-state-thread";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"State Metadata Title\"}}\n"),
        )
        .expect("write session index");

        let result = ingest_session_state_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id
            })
            .as_object()
            .expect("state params"),
            temp.path(),
        )
        .expect("state result");

        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("State Metadata Title")));
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("titleMetadataSource")),
            Some(&json!("agent-metadata"))
        );
    }

    #[test]
    fn agent_hook_rejects_cross_agent_metadata_for_stored_pi_session_without_launch_lock() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let (lifecycle, before) = create_pi_agent_session_without_launch_lock(&repository);
        assert_eq!(
            before
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("launchAgentId")),
            None
        );

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "droid",
                "agentSessionId": "d7f1ca76-435b-4102-acdb-e3e786cd72a9",
                "agentSessionPath": "/tmp/.factory/sessions/thread.jsonl",
                "eventName": "Stop",
                "firstUserMessage": "private prompt text",
                "projectId": lifecycle.project_id.clone(),
                "rawEventName": "Stop",
                "sessionId": lifecycle.session_id.clone(),
                "status": "attention",
                "statusUpdatedAt": "2026-06-24T00:08:05.000Z",
                "title": "Wrong Droid Thread"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("agent-hook-agent-mismatch"))
        );
        let response_session = result.get("session").expect("response session");
        assert_eq!(response_session.get("agentId"), Some(&json!("pi")));
        let runtime_settings = response_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(runtime_settings.get("agentName"), Some(&json!("pi")));
        assert_eq!(runtime_settings.get("agentSessionId"), None);
        assert_eq!(runtime_settings.get("firstUserMessage"), None);
        let stored = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read stored")
            .expect("stored session");
        assert_eq!(stored.get("updatedAt"), before.get("updatedAt"));
        assert_eq!(stored.get("title"), before.get("title"));
        assert_eq!(stored.get("agentId"), Some(&json!("pi")));
        let stored_runtime_settings = stored
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("stored runtime settings");
        assert_eq!(stored_runtime_settings.get("agentName"), Some(&json!("pi")));
        assert_eq!(stored_runtime_settings.get("agentSessionId"), None);
        assert_eq!(stored_runtime_settings.get("firstUserMessage"), None);
    }

    #[test]
    fn session_state_rejects_cross_agent_metadata_for_stored_pi_session_without_launch_lock() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let (lifecycle, before) = create_pi_agent_session_without_launch_lock(&repository);
        assert_eq!(
            before
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("launchAgentId")),
            None
        );

        let result = ingest_session_state_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "factory droid",
                "agentSessionId": "d7f1ca76-435b-4102-acdb-e3e786cd72a9",
                "agentSessionPath": "/tmp/.factory/sessions/thread.jsonl",
                "projectId": lifecycle.project_id.clone(),
                "sessionId": lifecycle.session_id.clone(),
                "startupText": "droid",
                "status": "working",
                "title": "Wrong Droid Thread"
            })
            .as_object()
            .expect("state params"),
            temp.path(),
        )
        .expect("state result");

        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("session-state-agent-mismatch"))
        );
        let response_session = result.get("session").expect("response session");
        assert_eq!(response_session.get("agentId"), Some(&json!("pi")));
        let runtime_settings = response_session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(runtime_settings.get("agentName"), Some(&json!("pi")));
        assert_eq!(runtime_settings.get("agentSessionId"), None);
        let stored = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read stored")
            .expect("stored session");
        assert_eq!(stored.get("updatedAt"), before.get("updatedAt"));
        assert_eq!(stored.get("title"), before.get("title"));
        assert_eq!(stored.get("agentId"), Some(&json!("pi")));
        let stored_runtime_settings = stored
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("stored runtime settings");
        assert_eq!(stored_runtime_settings.get("agentName"), Some(&json!("pi")));
        assert_eq!(stored_runtime_settings.get("agentSessionId"), None);
    }

    #[test]
    fn agent_hook_rejects_passive_identity_conflict_before_activity_prompt_and_title() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let current_codex_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
        let incoming_codex_session_id = "019e7c39-7ba7-7ac3-b79c-02757e299516";
        let (lifecycle, session) =
            create_codex_agent_session(&repository, current_codex_session_id, temp.path());
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            json!({ "activity": "idle", "isAcknowledged": true }),
        );
        runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        update.insert("title".to_string(), json!("Target Codex Thread"));
        repository
            .update_session(&update)
            .expect("prepare target session");

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": incoming_codex_session_id,
                "eventName": "Stop",
                "firstUserMessage": "private prompt text",
                "projectId": lifecycle.project_id.clone(),
                "rawEventName": "Stop",
                "sessionId": lifecycle.session_id.clone(),
                "status": "attention",
                "statusUpdatedAt": "2026-06-09T18:08:19.857Z",
                "title": "Wrong Codex Thread"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(
            result.get("reason"),
            Some(&json!("passive-session-identity-conflict"))
        );
        assert_eq!(
            result
                .get("activity")
                .and_then(|activity| activity.get("activity")),
            Some(&json!("idle"))
        );
        assert!(result.get("identityConflict").is_some());
        let response_session = result.get("session").expect("response session");
        assert_eq!(
            response_session.get("title"),
            Some(&json!("Target Codex Thread"))
        );
        assert_eq!(
            response_session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentSessionId")),
            Some(&json!(current_codex_session_id))
        );
        assert_eq!(
            response_session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("firstUserMessage")),
            None
        );
    }

    #[test]
    fn agent_hook_unchanged_activity_reports_unchanged_without_rewriting_state() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let (lifecycle, session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let activity_at = "2026-06-09T18:08:19.857Z";
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert("titleSource".to_string(), json!("terminal-auto"));
        runtime_settings.insert(
            "agentActivity".to_string(),
            json!({
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": false,
                "lastChangedAt": activity_at,
                "workingSource": "explicit",
                "workingStartedAt": activity_at
            }),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert("lastActiveAt".to_string(), json!(activity_at));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let before = repository
            .update_session(&update)
            .expect("prepare working session");

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "eventName": "PreToolUse",
                "projectId": lifecycle.project_id.clone(),
                "rawEventName": "PreToolUse",
                "sessionId": lifecycle.session_id.clone(),
                "status": "working",
                "statusUpdatedAt": activity_at
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(result.get("changed"), Some(&json!(false)));
        assert_eq!(result.get("reason"), Some(&json!("activity-unchanged")));
        assert_eq!(
            result
                .get("activity")
                .and_then(|activity| activity.get("activity")),
            Some(&json!("working"))
        );
        assert_eq!(result.get("previousActivity"), Some(&json!("working")));
        let after = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read after")
            .expect("after session");
        assert_eq!(after, before);
    }

    #[test]
    fn non_hook_activity_writes_preserve_session_chat_prompt() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let (lifecycle, session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let stored_prompt = r#"{"kind":"question","questions":[{"question":"Which color?","options":[{"label":"Red"},{"label":"Blue"}]}]}"#;
        let activity_at = "2026-08-01T05:30:00.000Z";
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            json!({
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": false,
                "lastChangedAt": activity_at,
                "sessionChatPrompt": stored_prompt,
                "workingSource": "explicit",
                "workingStartedAt": activity_at
            }),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository
            .update_session(&update)
            .expect("seed stored prompt");

        // A terminal-title observation rebuilds agentActivity from the fixed
        // ActivityState struct; the stored card must be carried forward, or a
        // pending AskUserQuestion (which produces no output, so title ticks
        // keep firing) loses its card seconds after the hook stored it.
        ingest_terminal_title_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "rawTitle": "quiet title",
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
        )
        .expect("terminal title result");
        let after_title = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read after title")
            .expect("session after title");
        assert_eq!(
            session_chat_prompt_setting(&after_title).as_deref(),
            Some(stored_prompt),
            "title observation must not erase the stored Session Chat prompt"
        );

        // Explicit activity RPCs (bell/escape/acknowledge) go through
        // update_agent_activity_endpoint and must preserve it too.
        update_agent_activity_endpoint(
            &repository,
            &lifecycle,
            json!({ "activity": "attention", "agentName": "codex" })
                .as_object()
                .expect("activity params"),
        )
        .expect("activity endpoint result");
        let after_activity = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("read after activity")
            .expect("session after activity");
        assert_eq!(
            session_chat_prompt_setting(&after_activity).as_deref(),
            Some(stored_prompt),
            "explicit activity updates must not erase the stored Session Chat prompt"
        );
    }

    #[test]
    fn agent_hook_reconciles_metadata_title_before_first_prompt_reason() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-hook-thread";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!("{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Hook Metadata Title\"}}\n"),
        )
        .expect("write session index");

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "eventName": "UserPromptSubmit",
                "firstUserMessage": "Please summarize this repository",
                "projectId": lifecycle.project_id.clone(),
                "sessionId": lifecycle.session_id.clone(),
                "status": "working",
                "statusUpdatedAt": "2026-06-09T18:08:19.857Z"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("Hook Metadata Title")));
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("firstUserMessage")),
            Some(&json!("Please summarize this repository"))
        );
    }

    #[test]
    fn terminal_title_capture_reconciles_codex_metadata_title() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Terminal Title Metadata" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let agent_session_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "codex",
                        "titleSource": "placeholder"
                    },
                    "title": "Codex Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let lifecycle = LifecycleParams {
            project_id,
            session_id: session
                .get("sessionId")
                .and_then(Value::as_str)
                .expect("session id")
                .to_string(),
        };
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Captured Metadata Title\"}}\n"
            ),
        )
        .expect("write session index");

        let output = ingest_terminal_title_event_with_home(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "rawTitle": agent_session_id,
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
            temp.path(),
        )
        .expect("terminal title result");
        let result = output.result;

        assert!(output.schedule_presentation_delta);
        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
        assert_eq!(result.get("agentSessionId"), Some(&json!(agent_session_id)));
        let session = result.get("session").expect("result session");
        assert_eq!(
            session.get("title"),
            Some(&json!("Captured Metadata Title"))
        );
        assert_eq!(
            session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentSessionId")),
            Some(&json!(agent_session_id))
        );
    }

    #[test]
    fn zmx_status_title_reconciles_codex_rename_metadata() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-zmx-status-title";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed From Agent CLI\"}}\n"
            ),
        )
        .expect("write session index");

        let output = ingest_terminal_title_event_with_home(
            &repository,
            &lifecycle,
            json!({
                "rawTitle": "⣸ ghostex",
                "sessionPersistenceProvider": "zmx"
            })
            .as_object()
            .expect("terminal title params"),
            temp.path(),
        )
        .expect("terminal title result");

        assert!(output.schedule_presentation_delta);
        assert_eq!(output.result.get("changed"), Some(&json!(true)));
        assert_eq!(
            output.result.get("reason"),
            Some(&json!("metadata-title-applied"))
        );
        assert_eq!(
            output
                .result
                .get("session")
                .and_then(|session| session.get("title")),
            Some(&json!("Renamed From Agent CLI"))
        );
    }

    #[test]
    fn request_session_rename_reconciles_codex_metadata_title() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-thread-rename";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Old title\"}}\n{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Renamed Investigation\",\"updated_at\":\"2026-06-21T15:35:00.000Z\"}}\n"
            ),
        )
        .expect("write session index");

        let result = request_session_rename(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "title": "Renamed Investigation",
                "titleSource": "user"
            })
            .as_object()
            .expect("rename params"),
            temp.path(),
        )
        .expect("rename result");

        assert_eq!(result.get("changed"), Some(&json!(true)));
        assert_eq!(result.get("pendingAgentMetadata"), Some(&json!(true)));
        assert_eq!(result.get("reason"), Some(&json!("metadata-title-applied")));
        assert_eq!(
            result.get("shouldSendAgentRenameCommand"),
            Some(&json!(true))
        );
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("Renamed Investigation")));
        let runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(
            runtime_settings.get("pendingAgentTitleRequestStatus"),
            Some(&json!("confirmed"))
        );
        assert_eq!(
            runtime_settings.get("titleMetadataProvider"),
            Some(&json!("codex-session-index"))
        );
        assert_eq!(
            runtime_settings.get("titleMetadataSource"),
            Some(&json!("agent-metadata"))
        );
        assert_eq!(
            runtime_settings.get("titleMetadataUpdatedAt"),
            Some(&json!("2026-06-21T15:35:00.000Z"))
        );
        assert_eq!(
            runtime_settings.get("titleSource"),
            Some(&json!("terminal-auto"))
        );
        assert!(runtime_settings.get("titleMetadataCheckedAt").is_some());
        let stored = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("get stored session")
            .expect("stored session");
        assert_eq!(stored.get("title"), Some(&json!("Renamed Investigation")));
    }

    #[test]
    fn request_session_rename_keeps_pending_when_codex_metadata_is_missing() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-thread-missing";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());

        let result = request_session_rename(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "title": "Requested Missing Title",
                "titleSource": "user"
            })
            .as_object()
            .expect("rename params"),
            temp.path(),
        )
        .expect("rename result");

        assert_eq!(
            result.get("reason"),
            Some(&json!("agent-rename-request-pending-metadata"))
        );
        assert_eq!(
            result.get("shouldSendAgentRenameCommand"),
            Some(&json!(true))
        );
        let session = result.get("session").expect("result session");
        assert_eq!(session.get("title"), Some(&json!("Codex Session")));
        let runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(
            runtime_settings.get("pendingAgentTitleRequestStatus"),
            Some(&json!("pending"))
        );
        assert_eq!(
            runtime_settings.get("pendingAgentTitleRequestTitle"),
            Some(&json!("Requested Missing Title"))
        );
        assert!(runtime_settings.get("titleMetadataSource").is_none());
    }

    #[test]
    fn trailing_agent_metadata_reconcile_marks_request_mismatch() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "codex-thread-trailing";
        let (lifecycle, _session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let _pending = request_session_rename(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "title": "Requested Title",
                "titleSource": "user"
            })
            .as_object()
            .expect("rename params"),
            temp.path(),
        )
        .expect("pending rename");
        let codex_dir = temp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).expect("create codex dir");
        std::fs::write(
            codex_dir.join("session_index.jsonl"),
            format!(
                "{{\"id\":\"{agent_session_id}\",\"thread_name\":\"Accepted Different Title\"}}\n"
            ),
        )
        .expect("write session index");

        let changed = reconcile_agent_metadata_title_for_session(
            &repository,
            &lifecycle.project_id,
            &lifecycle.session_id,
            temp.path(),
            "metadata-mismatch",
        )
        .expect("trailing reconcile");

        assert!(changed);
        let stored = repository
            .get_session(&lifecycle.project_id, &lifecycle.session_id)
            .expect("get stored session")
            .expect("stored session");
        assert_eq!(
            stored.get("title"),
            Some(&json!("Accepted Different Title"))
        );
        let runtime_settings = stored
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(
            runtime_settings.get("pendingAgentTitleRequestStatus"),
            Some(&json!("metadata-mismatch"))
        );
    }

    #[test]
    fn live_process_identity_promotes_running_zmx_terminal_to_codex() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Ghostex" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "sessionPersistenceProvider": "zmx",
                        "titleSource": "user"
                    },
                    "surface": "workspace",
                    "title": "Sidebar scrolls after closing (set above)"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string();

        let changed = apply_live_process_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("codex".to_string()),
            Some("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78".to_string()),
            None,
        )
        .expect("apply live process identity");

        assert!(changed);
        let updated = repository
            .get_session(&project_id, &session_id)
            .expect("get updated session")
            .expect("updated session");
        assert_eq!(updated.get("kind"), Some(&json!("agent")));
        assert_eq!(updated.get("agentId"), Some(&json!("codex")));
        assert_eq!(
            updated.get("title"),
            Some(&json!("Sidebar scrolls after closing (set above)"))
        );
        let runtime_settings = updated
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(runtime_settings.get("agentName"), Some(&json!("codex")));
        assert_eq!(runtime_settings.get("launchAgentId"), Some(&json!("codex")));
        assert_eq!(
            runtime_settings.get("agentSessionId"),
            Some(&json!("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78"))
        );
    }

    #[test]
    fn live_process_identity_claims_codex_id_observed_before_process_promotion() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({
                    "name": "Ghostex",
                    "path": temp.path().to_string_lossy()
                })
                .as_object()
                .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentActivity": {
                            "lastTitle": "019ff871-8b5c-7ce2-bcf7-5409263e2e0e"
                        },
                        "sessionPersistenceProvider": "zmx",
                        "titleSource": "placeholder"
                    },
                    "surface": "workspace",
                    "title": "Terminal Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string();

        let changed = apply_live_process_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("codex".to_string()),
            None,
            None,
        )
        .expect("apply live process identity");

        assert!(changed);
        let updated = repository
            .get_session(&project_id, &session_id)
            .expect("get updated session")
            .expect("updated session");
        assert_eq!(updated.get("kind"), Some(&json!("agent")));
        assert_eq!(updated.get("agentId"), Some(&json!("codex")));
        assert_eq!(
            updated
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("agentSessionId")),
            Some(&json!("019ff871-8b5c-7ce2-bcf7-5409263e2e0e"))
        );
    }

    #[test]
    fn live_process_identity_replaces_wsl_shell_title_and_first_hook_claims_auto_title() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({
                    "name": "Ghostex",
                    "path": temp.path().to_string_lossy()
                })
                .as_object()
                .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "kind": "terminal",
                    "lifecycleState": "running",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "sessionPersistenceProvider": "zmx",
                        "titleSource": "terminal-auto"
                    },
                    "surface": "workspace",
                    "title": "madda@M7-Desktop: /mnt/c/dev/Ghostex"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string();

        let changed = apply_live_process_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("codex".to_string()),
            Some("019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78".to_string()),
            None,
        )
        .expect("apply live process identity");

        assert!(changed);
        let updated = repository
            .get_session(&project_id, &session_id)
            .expect("get updated session")
            .expect("updated session");
        assert_eq!(updated.get("kind"), Some(&json!("agent")));
        assert_eq!(updated.get("agentId"), Some(&json!("codex")));
        assert_eq!(updated.get("title"), Some(&json!("Codex Session")));
        assert_eq!(
            updated
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("titleSource")),
            Some(&json!("placeholder"))
        );

        let lifecycle = LifecycleParams {
            project_id: project_id.clone(),
            session_id: session_id.clone(),
        };
        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": "019EB8D0-D27B-7F30-B6D7-7A04AB8FAE78",
                "eventName": "UserPromptSubmit",
                "firstUserMessage": "Please summarize this repository"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(
            result.get("reason"),
            Some(&json!("first-prompt-auto-title-claimed"))
        );
        let hooked_session = result.get("session").expect("hooked session");
        assert_eq!(hooked_session.get("title"), Some(&json!("Codex Session")));
        assert_eq!(
            hooked_session
                .get("runtimeSettings")
                .and_then(Value::as_object)
                .and_then(|settings| settings.get("gxserverFirstPromptAutoTitleStatus")),
            Some(&json!("running"))
        );
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
    fn agent_settings_ignore_legacy_metadata_key() {
        let (_temp, db) = open_test_database();
        let legacy_value =
            json!({ "agentAcceptAllEnabled": false, "defaultPromptAgentId": "claude" });
        write_metadata_value(&db, "gxserverAgentSettings", legacy_value.clone());

        let settings = read_agent_settings_with_metadata(&db).expect("legacy ignored");
        assert_eq!(settings.get("isPersisted"), Some(&json!(false)));
        assert_eq!(
            settings
                .get("settings")
                .and_then(|settings| settings.get("agentAcceptAllEnabled")),
            Some(&json!(true))
        );
        assert_eq!(
            settings
                .get("settings")
                .and_then(|settings| settings.get("defaultPromptAgentId")),
            Some(&json!("codex"))
        );

        let updated = update_agent_settings(
            &db,
            json!({ "defaultPromptAgentId": " claude " })
                .as_object()
                .expect("params"),
        )
        .expect("update settings with legacy row present");
        assert_eq!(updated.get("agentAcceptAllEnabled"), Some(&json!(true)));
        assert_eq!(updated.get("defaultPromptAgentId"), Some(&json!("claude")));

        let current: String = db
            .query_row(
                "SELECT value FROM metadata WHERE key = ?1",
                [AGENT_SETTINGS_METADATA_KEY],
                |row| row.get(0),
            )
            .expect("current metadata");
        let current_value = parse_json_object(&current);
        assert_eq!(
            current_value
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            current_value
                .get("defaultPromptAgentId")
                .and_then(Value::as_str),
            Some("claude")
        );

        let legacy: String = db
            .query_row(
                "SELECT value FROM metadata WHERE key = 'gxserverAgentSettings'",
                [],
                |row| row.get(0),
            )
            .expect("legacy metadata remains unrelated");
        assert_eq!(parse_json_object(&legacy), legacy_value);
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
    fn create_agent_session_params_use_project_agent_config_and_settings() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        update_agent_settings(
            &db,
            json!({ "agentAcceptAllEnabled": false })
                .as_object()
                .expect("settings params"),
        )
        .expect("agent settings");
        let project = repository
            .create_project(
                json!({
                    "customAgents": [{
                        "acceptAllMode": "enabled",
                        "agentId": "claude",
                        "command": "claude",
                        "icon": "claude"
                    }],
                    "name": "Agent CRUD"
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id");
        let params = json!({
            "agentId": "claude",
            "launchSettings": {
                "agentCommand": "ignored-local-command",
                "delayedSendDeadlineAt": "2026-06-22T05:40:00.000Z"
            },
            "projectId": project_id,
            "runtimeSettings": {
                "firstUserMessage": "Summarize this repository."
            },
            "title": "Claude Agent"
        });
        let create_params = create_agent_session_params_for_project(
            &db,
            &project,
            params.as_object().expect("create params"),
        )
        .expect("normalized create params");

        let launch_settings = create_params
            .get("launchSettings")
            .and_then(Value::as_object)
            .expect("launch settings");
        let launch_plan = launch_settings
            .get("agentLaunchPlan")
            .and_then(Value::as_object)
            .expect("launch plan");
        assert_eq!(launch_plan.get("agentCommand"), Some(&json!("claude")));
        assert_eq!(
            launch_plan.get("command"),
            Some(&json!("claude --dangerously-skip-permissions"))
        );
        assert_eq!(
            launch_plan.get("firstUserMessage"),
            Some(&json!("Summarize this repository."))
        );
        assert_eq!(
            launch_plan
                .get("delayedSend")
                .and_then(|value| value.get("deadlineAt")),
            Some(&json!("2026-06-22T05:40:00.000Z"))
        );
        assert_eq!(
            launch_settings
                .get("runtimeRelevant")
                .and_then(|value| value.get("queueProviderStartupText")),
            Some(&json!(true))
        );
        let runtime_settings = create_params
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .expect("runtime settings");
        assert_eq!(runtime_settings.get("agentCommand"), Some(&json!("claude")));
        assert_eq!(
            runtime_settings.get("launchAgentId"),
            Some(&json!("claude"))
        );
        assert_eq!(
            runtime_settings
                .get("agentActivity")
                .and_then(|value| value.get("activity")),
            Some(&json!("working"))
        );
        assert_eq!(
            runtime_settings
                .get("agentActivity")
                .and_then(|value| value.get("agentName")),
            Some(&json!("claude"))
        );

        let session = repository
            .create_session(&create_params, false)
            .expect("agent session created");
        assert_eq!(session.get("kind"), Some(&json!("agent")));
        assert_eq!(session.get("agentId"), Some(&json!("claude")));
        assert_eq!(
            session
                .get("launchSettings")
                .and_then(|value| value.get("agentLaunchPlan"))
                .and_then(|value| value.get("command")),
            Some(&json!("claude --dangerously-skip-permissions"))
        );
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
    fn launch_plan_keeps_typescript_custom_agent_lookup_and_empty_shape() {
        let settings = normalize_agent_settings(None);
        let project_with_id_only_agent = json!({
            "customAgents": [{ "id": "codex", "command": "codex --profile ignored" }],
            "launchSettings": {},
        });
        let plan =
            build_project_agent_launch_plan(&project_with_id_only_agent, "codex", None, &settings);
        assert_eq!(plan.get("agentCommand"), Some(&json!("codex")));
        assert_eq!(plan.get("command"), Some(&json!("codex --yolo")));

        let unknown_plan = build_project_agent_launch_plan(
            &json!({ "customAgents": [], "launchSettings": {} }),
            "custom-local",
            None,
            &settings,
        );
        assert_eq!(unknown_plan.get("agentCommand"), Some(&json!("")));
        assert_eq!(unknown_plan.get("command"), Some(&json!("")));
        assert_eq!(unknown_plan.get("startupText"), Some(&json!("")));
        assert_eq!(
            unknown_plan.get("startupTextDisposition"),
            Some(&json!("none"))
        );
    }

    #[test]
    fn accept_all_specs_match_typescript_aliases_and_icon_mapping() {
        assert_eq!(
            resolve_agent_launch_command("cursor", "cursor-agent --allow-all", None, true, None),
            "cursor-agent --allow-all --yolo"
        );
        assert_eq!(
            resolve_agent_launch_command(
                "cursor",
                "cursor-agent --force --yolo",
                Some("disabled"),
                true,
                None,
            ),
            "cursor-agent"
        );
        assert_eq!(
            resolve_agent_launch_command("gemini", "gemini --allow-all", None, true, None),
            "gemini --allow-all --yolo"
        );
        assert_eq!(
            resolve_agent_launch_command("copilot", "copilot -y", None, true, None),
            "copilot -y --yolo"
        );
        assert_eq!(
            resolve_agent_launch_command(
                "custom-cursor",
                "cursor-agent",
                None,
                true,
                Some("cursor-cli")
            ),
            "cursor-agent --yolo"
        );
        assert_eq!(
            resolve_agent_launch_command(
                "grok",
                "grok --permission-mode bypassPermissions --always-approve",
                None,
                true,
                None,
            ),
            "grok --permission-mode bypassPermissions"
        );
        assert_eq!(
            resolve_agent_launch_command(
                "grok",
                "grok --permission-mode=bypassPermissions --always-approve",
                Some("disabled"),
                true,
                None,
            ),
            "grok"
        );
    }

    #[test]
    fn cursor_launch_appends_only_normalized_resume_chat_ids() {
        let valid = build_agent_launch_plan(AgentLaunchInput {
            accept_all_mode: None,
            agent_id: "cursor".to_string(),
            agent_session_id: Some("8B16E7E6-3CE1-4D0B-9F35-78261B7F0767".to_string()),
            command: Some("cursor-agent".to_string()),
            delayed_send_deadline_at: None,
            first_user_message: None,
            global_accept_all_enabled: true,
            icon: None,
        });
        assert_eq!(
            valid.get("command"),
            Some(&json!(
                "cursor-agent --yolo --resume \"8b16e7e6-3ce1-4d0b-9f35-78261b7f0767\""
            ))
        );

        let invalid = build_agent_launch_plan(AgentLaunchInput {
            accept_all_mode: None,
            agent_id: "cursor".to_string(),
            agent_session_id: Some("not-a-chat-id".to_string()),
            command: Some("cursor-agent".to_string()),
            delayed_send_deadline_at: None,
            first_user_message: None,
            global_accept_all_enabled: true,
            icon: None,
        });
        assert_eq!(invalid.get("command"), Some(&json!("cursor-agent --yolo")));
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
        let primary_command = resume
            .get("primaryCommand")
            .and_then(Value::as_str)
            .expect("primary command");
        assert!(primary_command.contains("CODEX_RESUME_SESSION_ID"));
        assert!(primary_command.contains("--exact"));
        assert!(primary_command.contains("codex --yolo resume \"$CODEX_RESUME_SESSION_ID\""));
        assert_eq!(
            resume.get("displayCommand"),
            Some(&json!(
                "codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""
            ))
        );
        assert_eq!(
            resume.get("copyCommand"),
            Some(&json!(
                "codex --yolo resume \"12345678-1234-1234-1234-123456789abc\""
            ))
        );
        assert!(resume
            .get("fallbackCommand")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("--title")));
        let startup_text = resume
            .get("startupText")
            .and_then(Value::as_str)
            .expect("startup text");
        assert!(startup_text.starts_with(' '));
        assert!(startup_text.contains("Restoring session..."));
        assert!(startup_text
            .contains("__ghostex_restore_resume_primary || __ghostex_restore_resume_status=$?"));
        assert!(startup_text.contains("Exact resume failed; trying saved fallback resume command."));
        assert!(
            startup_text.contains("codex --yolo resume \"12345678-1234-1234-1234-123456789abc\"")
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
    fn resume_plan_extracts_provider_exact_identity_hints() {
        let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
        let settings = {
            let mut settings = normalize_agent_settings(None);
            settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));
            settings
        };
        let claude = json!({
            "agentId": "claude",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "claude",
                "agentSessionPath": "/Users/example/.claude/projects/-repo-ghostex/9970b270-b39f-4d63-a764-fa8d88083995.jsonl",
                "titleSource": "user"
            },
            "title": "Readable Claude title",
        });
        let claude_plan = build_agent_resume_plan(&project, &claude, &settings);
        assert_eq!(
            claude_plan.get("primaryCommand"),
            Some(&json!(
                "claude --resume \"9970b270-b39f-4d63-a764-fa8d88083995\""
            ))
        );
        assert_eq!(
            claude_plan.get("displayCommand"),
            claude_plan.get("primaryCommand")
        );
        assert_eq!(
            claude_plan.get("copyCommand"),
            claude_plan.get("primaryCommand")
        );
        assert!(claude_plan
            .get("fallbackCommand")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("CLAUDE_RESUME_SESSION_ID")));

        let cursor = json!({
            "agentId": "cursor",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "cursor-agent",
                "resumeCommand": "cd '/repo/ghostex' && cursor-agent --resume \"E10971DA-CBD7-459A-9AC3-B9B0313199A3\"",
                "titleSource": "user"
            },
            "title": "∗ Cursor CLI Session",
        });
        let cursor_plan = build_agent_resume_plan(&project, &cursor, &settings);
        assert_eq!(
            cursor_plan.get("primaryCommand"),
            Some(&json!(
                "cursor-agent --resume \"e10971da-cbd7-459a-9ac3-b9b0313199a3\""
            ))
        );
        assert!(cursor_plan.get("fallbackCommand").is_none());

        let pi = json!({
            "agentId": "pi",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "pi",
                "agentSessionId": "pi-id",
                "agentSessionPath": "/tmp/pi/session/path",
                "titleSource": "user"
            },
            "title": "Pi thread",
        });
        let pi_plan = build_agent_resume_plan(&project, &pi, &settings);
        assert_eq!(
            pi_plan.get("primaryCommand"),
            Some(&json!("pi --session \"/tmp/pi/session/path\""))
        );
    }

    #[test]
    fn opencode_resume_keeps_lookup_command_separate_from_runtime_accept_all() {
        let project = json!({ "path": "/repo/ghostex", "customAgents": [], "launchSettings": {} });
        let settings = normalize_agent_settings(None);
        let titled = json!({
            "agentId": "opencode",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "opencode",
                "titleSource": "user"
            },
            "title": "Readable thread title",
        });
        let plan = build_agent_resume_plan(&project, &titled, &settings);
        assert_eq!(
            plan.get("runtimeCommand"),
            Some(&json!(
                "OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode"
            ))
        );
        assert_eq!(plan.get("lookupCommand"), Some(&json!("opencode")));
        let primary = plan
            .get("primaryCommand")
            .and_then(Value::as_str)
            .expect("primary command");
        assert!(
            primary.contains("OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode -s")
        );
        assert!(primary.contains("opencode session list --format json"));
        assert!(!primary.contains(
            "OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}' opencode session list"
        ));
        assert!(plan.get("copyCommand").is_none());
    }

    #[test]
    fn attach_startup_text_uses_agent_resume_plan_and_settings() {
        let project = json!({ "path": "/tmp/project", "customAgents": [], "launchSettings": {} });
        let session = json!({
            "agentId": "codex",
            "launchSettings": {},
            "runtimeSettings": {
                "agentCommand": "codex",
                "agentSessionId": "12345678-1234-1234-1234-123456789abc"
            },
            "title": "Restorable Codex",
        });
        let mut settings = normalize_agent_settings(None);
        settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(false));

        let startup_text = get_agent_startup_text_for_session(&project, &session, &settings)
            .expect("startup text");
        assert!(startup_text.starts_with(' '));
        assert!(startup_text.ends_with('\r'));
        assert!(startup_text.contains("Restoring session..."));
        assert!(startup_text.contains(
            "printf '> %s\\n\\n' 'codex resume \"12345678-1234-1234-1234-123456789abc\"'"
        ));
        assert!(startup_text.contains("codex resume \"12345678-1234-1234-1234-123456789abc\""));
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
        assert_eq!(
            normalize_agent_hook_activity(
                Some(&json!("idle")),
                Some(&json!("Stop")),
                Some(&json!("Codex"))
            ),
            Some("attention".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(
                Some(&json!("attention")),
                Some(&json!("SessionEnd")),
                Some(&json!("Codex"))
            ),
            Some("idle".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(
                Some(&json!("attention")),
                Some(&json!("Notification")),
                Some(&json!("GitHub Copilot"))
            ),
            Some("idle".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("pre_approval_request")), None),
            Some("attention".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("session.updated")), None),
            Some("attention".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("on_session_start")), None),
            Some("working".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("on_session_finalize")), None),
            Some("idle".to_string())
        );
        assert_eq!(
            normalize_agent_hook_activity(None, Some(&json!("session_shutdown")), None),
            Some("idle".to_string())
        );
    }

    #[test]
    fn codex_stop_hook_enters_attention_from_working() {
        let (temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let agent_session_id = "019e7af5-c610-7f62-a129-db7bb510b48d";
        let (lifecycle, session) =
            create_codex_agent_session(&repository, agent_session_id, temp.path());
        let mut runtime_settings = object_field(&session, "runtimeSettings");
        runtime_settings.insert(
            "agentActivity".to_string(),
            json!({
                "activity": "working",
                "agentName": "codex",
                "hasSeenWorking": true,
                "isAcknowledged": false,
                "lastChangedAt": "2026-08-08T01:00:00.000Z",
                "lastMeaningfulActivityAt": "2026-08-08T01:00:00.000Z",
                "workingSource": "hook",
                "workingStartedAt": "2026-08-08T01:00:00.000Z"
            }),
        );
        let mut update = lifecycle_update(&lifecycle);
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update).expect("working session");

        let result = ingest_agent_hook_event(
            &repository,
            &lifecycle,
            json!({
                "agentName": "codex",
                "agentSessionId": agent_session_id,
                "eventName": "Stop",
                "status": "idle",
                "statusUpdatedAt": "2026-08-08T01:00:05.000Z"
            })
            .as_object()
            .expect("hook params"),
            temp.path(),
        )
        .expect("hook result");

        assert_eq!(result.get("previousActivity"), Some(&json!("working")));
        assert_eq!(result.get("enteredAttention"), Some(&json!(true)));
        let activity = result
            .get("activity")
            .and_then(Value::as_object)
            .expect("activity");
        assert_eq!(activity.get("activity"), Some(&json!("attention")));
        assert!(activity
            .get("attentionEventId")
            .and_then(Value::as_str)
            .is_some());
    }

    /*
    CDXC:SessionChatIdentity 2026-08-02:
    The Session Chat successor detector asks this predicate which sessions could
    still be tailing an agent conversation. The registry keeps every session ever
    created (3487 stopped rows on the machine the chat-identity bug was debugged
    on), and stopped rows still carry the agentSessionIds of conversations that
    were later continued. Counting those as owners silently blocked every
    legitimate re-binding, so the stopped cases are pinned here.
    */
    #[test]
    fn stopped_sessions_are_not_identity_owners() {
        assert!(is_active_identity_owner(
            &json!({ "lifecycleState": "running" })
        ));
        assert!(is_active_identity_owner(
            &json!({ "lifecycleState": "sleeping" })
        ));
        assert!(!is_active_identity_owner(&json!({
            "lifecycleState": "stopped",
            "providerState": { "lifecycleState": "missing" }
        })));
        assert!(!is_active_identity_owner(&json!({
            "lifecycleState": "stopped",
            "providerState": { "lifecycleState": "exists" }
        })));
        // Not stopped and the provider is still alive ⇒ still an owner.
        assert!(is_active_identity_owner(&json!({
            "lifecycleState": "unknown",
            "providerState": { "lifecycleState": "exists" }
        })));
        assert!(!is_active_identity_owner(&json!({
            "lifecycleState": "unknown",
            "providerState": { "lifecycleState": "missing" }
        })));
    }

    #[test]
    fn transcript_successor_identity_write_is_compare_and_set() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "test-server");
        let project = repository
            .create_project(
                json!({ "name": "Successor Identity Project" })
                    .as_object()
                    .expect("project params"),
            )
            .expect("create project");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "claude",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "claude",
                        "agentSessionId": "stale-session",
                        "agentSessionPath": "/Users/test/.claude/projects/demo/stale-session.jsonl"
                    },
                    "title": "Claude Session"
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("create session");
        let session_id = session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string();

        // A hook that landed after the follower read the identity must win.
        assert!(!apply_transcript_successor_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("some-other-session"),
            "successor-session",
            "/Users/test/.claude/projects/demo/successor-session.jsonl",
        )
        .expect("stale expectation refused"));

        assert!(apply_transcript_successor_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("stale-session"),
            "successor-session",
            "/Users/test/.claude/projects/demo/successor-session.jsonl",
        )
        .expect("successor identity applied"));

        let stored = repository
            .get_session(&project_id, &session_id)
            .expect("get session")
            .expect("session row");
        let runtime_settings = object_field(&stored, "runtimeSettings");
        assert_eq!(
            runtime_settings.get("agentSessionId"),
            Some(&json!("successor-session"))
        );
        assert_eq!(
            runtime_settings.get("agentSessionPath"),
            Some(&json!(
                "/Users/test/.claude/projects/demo/successor-session.jsonl"
            ))
        );
        assert_eq!(stored.get("agentId"), Some(&json!("claude")));

        // Re-running the same adoption is a no-op, not a churn write.
        assert!(!apply_transcript_successor_session_identity(
            &repository,
            &project_id,
            &session_id,
            Some("successor-session"),
            "successor-session",
            "/Users/test/.claude/projects/demo/successor-session.jsonl",
        )
        .expect("idempotent adoption"));
    }
}
