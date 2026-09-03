use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use crate::{
    agents::{
        apply_created_session_identity, build_agent_resume_plan,
        create_agent_session_params_for_project, read_agent_settings,
    },
    domain::{read_domain_rpc_params, DomainRepository, DomainStateError},
    paths::GxserverPaths,
    sidebar_hud::read_sidebar_hud,
    storage::open_gxserver_database,
    zmx::{
        dispatch_zmx_lifecycle_endpoint, dispatch_zmx_session_interaction_endpoint,
        ZmxEndpointError, ZmxServerContext,
    },
};

#[derive(Clone)]
pub struct AutomationRuntime {
    auth_token_file: String,
    base_url: String,
    paths: GxserverPaths,
    server_id: String,
}

#[derive(Clone)]
struct AutomationDefinitionRecord {
    agent_id: String,
    created_at: String,
    enabled: bool,
    execution_mode: Value,
    id: String,
    name: String,
    next_run_at: Option<String>,
    project_id: String,
    prompt: String,
    schedule: Value,
    updated_at: String,
}

#[derive(Clone)]
struct AutomationRunRecord {
    automation_id: String,
    completed_at: Option<String>,
    created_at: String,
    error_message: Option<String>,
    findings_summary: Option<String>,
    id: String,
    is_archived: bool,
    is_unread: bool,
    project_id: String,
    session_id: Option<String>,
    status: String,
    updated_at: String,
    worktree: Value,
}

struct AutomationLaunch {
    /*
    CDXC:Automations 2026-07-30:
    Launches that create a fresh agent session own an undelivered prompt.
    `createAgentSession` only records the prompt as `runtimeSettings.firstUserMessage`
    metadata, and the agent launch plan's `startupText` carries the agent command
    alone, so nothing types the prompt into the session. Carry the pending prompt
    out of the launch so the run watcher delivers it once the agent TUI is up.
    Thread launches leave this `None` because they already sent the prompt into an
    existing session.
    */
    pending_prompt: Option<String>,
    session_id: String,
    session_project_id: String,
    worktree: Option<Value>,
}

const AUTOMATION_SCHEDULER_TICK_SECONDS: u64 = 30;
const AUTOMATION_RUN_POLL_SECONDS: u64 = 5;
const AUTOMATION_RUN_POLL_LIMIT: usize = 720;
/*
CDXC:Automations 2026-07-30:
Mirror the GPUI sidebar's agent prompt contract (`GPUI_AGENT_PROMPT_READY_DELAY_MS`):
a freshly launched agent TUI needs a settle window before it accepts composer
input. The daemon waits inside the spawned run watcher, never inside the
scheduler tick or the `runAutomationNow` endpoint, so neither blocks.

CDXC:SessionChat 2026-08-26: the settle window is now the FALLBACK
for an agent with no measured composer signature; a signed agent is released as
soon as its input box appears. An automation is the case that suffered most from
the blind version — nobody is watching the pane, so a prompt typed into a boot
screen shows up only as a run that fails at the watcher timeout with no
AUTOMATION_RESULT marker.
*/
const AUTOMATION_PROMPT_READY_DELAY_SECONDS: u64 = 4;
/// Ceiling for the automation composer wait; matches the provider-startup one.
const AUTOMATION_PROMPT_COMPOSER_WAIT_TIMEOUT_MS: u64 = 10_000;
const AUTOMATION_RESULT_PREFIX: &str = "AUTOMATION_RESULT:";
const AUTOMATION_MAX_COUNT: usize = 500;
const AUTOMATION_MAX_RUN_COUNT: usize = 5_000;

impl AutomationRuntime {
    pub fn new(paths: GxserverPaths, server_id: impl Into<String>, base_url: String) -> Self {
        Self {
            auth_token_file: paths.auth_token_file.to_string_lossy().to_string(),
            base_url,
            paths,
            server_id: server_id.into(),
        }
    }

    pub fn start(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        /*
        CDXC:Automations 2026-06-29-15:55:
        Automations are daemon-owned work now, not macOS renderer timers. Keep the scheduler loop in the gxserver automation module so CLI, macOS, GPUI, and remote clients share one durable SQLite source of truth and one runner while Ghostex is open.
        */
        let runtime = self.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(AUTOMATION_SCHEDULER_TICK_SECONDS));
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => {
                        let _ = runtime.run_scheduler_tick();
                    }
                }
            }
        });
    }

    fn run_scheduler_tick(&self) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        recover_running_automation_runs(self, &repository, &db)?;
        let due = read_due_automations(&db)?;
        for automation in due {
            if has_active_run(&db, &automation.id)? {
                let run = create_run_record(
                    &automation,
                    "skipped",
                    Some("Skipped because another run for this automation is still active."),
                );
                upsert_run(&db, &run)?;
                update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
                continue;
            }
            let _ = queue_automation_run(self, &repository, &db, &automation);
            update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
        }
        Ok(())
    }

    fn spawn_run_watcher(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<String>,
    ) {
        let runtime = self.clone();
        tokio::spawn(async move {
            let _ = runtime
                .watch_automation_run(
                    project_id,
                    run_id,
                    session_project_id,
                    session_id,
                    pending_prompt,
                )
                .await;
        });
    }

    async fn watch_automation_run(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<String>,
    ) -> Result<(), DomainStateError> {
        if let Some(prompt) = pending_prompt {
            if let Err(error) = self
                .deliver_automation_prompt(&session_project_id, &session_id, &prompt)
                .await
            {
                let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
                return fail_run(&db, &project_id, &run_id, &error.message, "failed");
            }
        }
        for _ in 0..AUTOMATION_RUN_POLL_LIMIT {
            tokio::time::sleep(Duration::from_secs(AUTOMATION_RUN_POLL_SECONDS)).await;
            let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
            let repository = DomainRepository::new(&db, self.server_id.as_str());
            if !is_run_active(&db, &project_id, &run_id)? {
                return Ok(());
            }
            if let Some((status, summary)) =
                read_automation_result_from_session(&repository, &session_project_id, &session_id)?
            {
                complete_run(&db, &project_id, &run_id, &status, summary.as_deref())?;
                return Ok(());
            }
        }
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        fail_run(
            &db,
            &project_id,
            &run_id,
            "Automation did not report an AUTOMATION_RESULT marker before the watcher timeout.",
            "needs_attention",
        )
    }

    /*
    CDXC:Automations 2026-07-30:
    Deliver the automation prompt the same way the GPUI sidebar delivers a first
    user message: start the provider, let the agent TUI settle, then submit the
    text through `sendSessionMessage`. Without this the session launches its agent
    command and idles at an empty composer, so no run can ever emit an
    AUTOMATION_RESULT marker and every run fails at the watcher timeout.
    */
    async fn deliver_automation_prompt(
        &self,
        session_project_id: &str,
        session_id: &str,
        prompt: &str,
    ) -> Result<(), DomainStateError> {
        crate::session_chat_composer::wait_for_session_chat_composer_by_ids(
            &self.paths,
            self.server_id.as_str(),
            session_project_id,
            session_id,
            crate::session_chat_composer::SessionChatComposerWaitPolicy {
                settle_ms: 0,
                timeout_ms: AUTOMATION_PROMPT_COMPOSER_WAIT_TIMEOUT_MS,
                unknown_hold_ms: AUTOMATION_PROMPT_READY_DELAY_SECONDS * 1_000,
            },
        )
        .await;
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        send_automation_prompt(&repository, session_project_id, session_id, prompt)
    }
}

fn send_automation_prompt(
    repository: &DomainRepository<'_>,
    session_project_id: &str,
    session_id: &str,
    prompt: &str,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert(
        "projectId".to_string(),
        Value::String(session_project_id.to_string()),
    );
    params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    params.insert("submit".to_string(), Value::Bool(true));
    params.insert("text".to_string(), Value::String(prompt.to_string()));
    /*
    Tag the write with its origin. The send queue already uses
    `diagnosticInputSource` to attribute every byte to its caller; analytics
    reads the same tag to attribute this prompt to `automation` rather than
    lumping it in with the untagged `gx sendMessage` default.
    */
    params.insert(
        "diagnosticInputSource".to_string(),
        Value::String("automation".to_string()),
    );
    dispatch_zmx_session_interaction_endpoint(repository, "/api/sendSessionMessage", &params)
        .map_err(zmx_error)?;
    Ok(())
}

pub async fn handle_automation_endpoint(
    runtime: &AutomationRuntime,
    endpoint_path: &str,
    body: &Value,
) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let db = open_gxserver_database(&runtime.paths).map_err(internal_error)?;
    let repository = DomainRepository::new(&db, runtime.server_id.as_str());
    match endpoint_path {
        "/api/readAutomationState" => {
            let project = resolve_automation_project(&repository, &params)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/saveAutomation" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation = normalize_definition_payload(&project, &params)?;
            upsert_automation(&db, &automation)?;
            let stored = resolve_project_by_id(&repository, &automation.project_id)?;
            read_project_automation_state(&repository, &db, &stored)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/deleteAutomation" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            delete_automation(&db, &project, &automation_id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/setAutomationEnabled" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("enabled is required."))?;
            set_automation_enabled(&db, &project, &automation_id, enabled)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/runAutomationNow" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            let automation = read_automation(&db, &project, &automation_id)?;
            let _ = queue_automation_run(runtime, &repository, &db, &automation)?;
            update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/archiveAutomationRun" => {
            let project = resolve_automation_project(&repository, &params)?;
            let run_id = read_param_text(&params, "runId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("runId is required."))?;
            archive_run(
                &db,
                &repository,
                &project,
                &run_id,
                params.get("removeWorktree").and_then(Value::as_bool) == Some(true),
            )?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/markAutomationRunRead" => {
            let project = resolve_automation_project(&repository, &params)?;
            let run_id = read_param_text(&params, "runId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("runId is required."))?;
            patch_run_read(&db, &project.project_id, &run_id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver automation endpoint."
        ))),
    }
}

#[derive(Clone)]
struct ProjectRecord {
    name: String,
    path: String,
    project_id: String,
}

fn read_project_automation_state(
    repository: &DomainRepository<'_>,
    db: &Connection,
    project: &ProjectRecord,
) -> Result<Value, DomainStateError> {
    let projects = repository.list_projects()?;
    let hud = read_sidebar_hud(&projects, Some(project.project_id.as_str()));
    let agents = hud_agents_to_automation_agents(&hud);
    let agent_settings = read_agent_settings(db)?;
    let target_projects = read_automation_target_projects(repository)?;
    let worktree = worktree_availability(project);
    Ok(json!({
        "agents": agents,
        "automations": automations_to_value(read_automations_for_project(db, &project.project_id)?),
        "defaultAgentId": agent_settings.get("defaultPromptAgentId").and_then(Value::as_str).unwrap_or("codex"),
        "projectCanUseWorktrees": worktree.0,
        "projectId": project.project_id,
        "projectName": project.name,
        "projectPath": project.path,
        "projects": target_projects,
        "runs": runs_to_value(read_runs_for_project(db, &project.project_id)?),
        "worktreeUnavailableReason": worktree.1,
    }))
}

fn queue_automation_run(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
) -> Result<AutomationRunRecord, DomainStateError> {
    if has_active_run(db, &automation.id)? {
        let skipped = create_run_record(
            automation,
            "skipped",
            Some("Skipped because another run for this automation is still active."),
        );
        upsert_run(db, &skipped)?;
        return Ok(skipped);
    }
    let mut run = create_run_record(automation, "queued", None);
    upsert_run(db, &run)?;
    match launch_automation(runtime, repository, db, automation, &run) {
        Ok(launch) => {
            run.session_id = Some(launch.session_id.clone());
            run.status = "running".to_string();
            run.worktree = launch.worktree.unwrap_or_else(|| json!({}));
            run.updated_at = now_iso();
            upsert_run(db, &run)?;
            runtime.spawn_run_watcher(
                automation.project_id.clone(),
                run.id.clone(),
                launch.session_project_id,
                launch.session_id,
                launch.pending_prompt,
            );
            Ok(run)
        }
        Err(error) => {
            run.completed_at = Some(now_iso());
            run.error_message = Some(error.message);
            run.is_unread = true;
            run.status = if error.code == "needsAttention" {
                "needs_attention".to_string()
            } else {
                "failed".to_string()
            };
            run.updated_at = now_iso();
            upsert_run(db, &run)?;
            Ok(run)
        }
    }
}

fn launch_automation(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    run: &AutomationRunRecord,
) -> Result<AutomationLaunch, DomainStateError> {
    let execution_kind = automation
        .execution_mode
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("local");
    let prompt = build_automation_prompt(&automation.prompt);
    match execution_kind {
        "thread" => launch_thread_automation(runtime, repository, db, automation, &prompt, run),
        "worktree" => launch_worktree_automation(runtime, repository, db, automation, &prompt, run),
        _ => launch_local_automation(runtime, repository, db, automation, &prompt),
    }
}

fn launch_local_automation(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    prompt: &str,
) -> Result<AutomationLaunch, DomainStateError> {
    let project = resolve_project_by_id(repository, &automation.project_id)?;
    let session = create_and_start_agent_session(
        runtime, repository, db, &project, automation, prompt, None,
    )?;
    Ok(AutomationLaunch {
        pending_prompt: Some(prompt.to_string()),
        session_id: required_value_text(&session, "sessionId")?,
        session_project_id: project.project_id,
        worktree: None,
    })
}

fn launch_worktree_automation(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    prompt: &str,
    run: &AutomationRunRecord,
) -> Result<AutomationLaunch, DomainStateError> {
    let source_project = resolve_project_by_id(repository, &automation.project_id)?;
    let (can_use_worktrees, reason) = worktree_availability(&source_project);
    if !can_use_worktrees {
        return Err(DomainStateError {
            code: "needsAttention",
            message: reason
                .unwrap_or_else(|| "Worktree mode is unavailable for this project.".to_string()),
        });
    }
    let target = create_worktree_for_run(&source_project, run)?;
    let mut params = Map::new();
    params.insert("path".to_string(), Value::String(target.path.clone()));
    params.insert("name".to_string(), Value::String(target.name.clone()));
    let worktree_project_value = repository.add_project_path(&params)?;
    let worktree_project = project_record_from_value(&worktree_project_value)?;
    if let Some(setup_command) = automation
        .execution_mode
        .get("setupCommand")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        run_setup_command(&target.path, setup_command)?;
    }
    let session = create_and_start_agent_session(
        runtime,
        repository,
        db,
        &worktree_project,
        automation,
        prompt,
        None,
    )?;
    Ok(AutomationLaunch {
        pending_prompt: Some(prompt.to_string()),
        session_id: required_value_text(&session, "sessionId")?,
        session_project_id: worktree_project.project_id,
        worktree: Some(json!({
            "branch": target.branch,
            "path": target.path,
            "sourcePath": source_project.path,
        })),
    })
}

fn launch_thread_automation(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    prompt: &str,
    run: &AutomationRunRecord,
) -> Result<AutomationLaunch, DomainStateError> {
    if let Some(expires_at) = automation
        .execution_mode
        .get("expiresAt")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        if chrono::DateTime::parse_from_rfc3339(expires_at)
            .map(|date| date.timestamp_millis() <= Utc::now().timestamp_millis())
            .unwrap_or(false)
        {
            return Err(DomainStateError {
                code: "needsAttention",
                message: "Thread automation expired.".to_string(),
            });
        }
    }
    let session_id_hint = automation
        .execution_mode
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_session_id = automation
        .execution_mode
        .get("agentSessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(agent_session_id) = agent_session_id {
        if let Some((session_project_id, session_id)) = resolve_agent_thread_session(
            repository,
            &automation.project_id,
            session_id_hint,
            &automation.agent_id,
            agent_session_id,
        )? {
            send_automation_prompt(repository, &session_project_id, &session_id, prompt)?;
            return Ok(AutomationLaunch {
                pending_prompt: None,
                session_id,
                session_project_id,
                worktree: Some(json!({ "sourceRunId": run.id })),
            });
        }

        let project = resolve_project_by_id(repository, &automation.project_id)?;
        let session = create_and_start_agent_session(
            runtime,
            repository,
            db,
            &project,
            automation,
            prompt,
            Some(agent_session_id),
        )?;
        return Ok(AutomationLaunch {
            pending_prompt: Some(prompt.to_string()),
            session_id: required_value_text(&session, "sessionId")?,
            session_project_id: project.project_id,
            worktree: Some(json!({ "sourceRunId": run.id })),
        });
    }

    let session_id_hint = session_id_hint.ok_or_else(|| {
        DomainStateError::bad_request("Thread automation requires sessionId or agentSessionId.")
    })?;
    let (session_project_id, session_id) =
        resolve_thread_session(repository, &automation.project_id, session_id_hint)?;
    /*
    Thread automations reuse a session whose agent is already running, so the
    prompt goes in immediately and needs no readiness wait. Report it as already
    delivered so the run watcher does not submit it a second time.
    */
    send_automation_prompt(repository, &session_project_id, &session_id, prompt)?;
    Ok(AutomationLaunch {
        pending_prompt: None,
        session_id,
        session_project_id,
        worktree: Some(json!({ "sourceRunId": run.id })),
    })
}

fn create_and_start_agent_session(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    project: &ProjectRecord,
    automation: &AutomationDefinitionRecord,
    prompt: &str,
    resume_agent_session_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    let mut params = Map::new();
    params.insert(
        "agentId".to_string(),
        Value::String(automation.agent_id.clone()),
    );
    params.insert(
        "projectId".to_string(),
        Value::String(project.project_id.clone()),
    );
    params.insert(
        "surface".to_string(),
        Value::String("workspace".to_string()),
    );
    params.insert(
        "title".to_string(),
        Value::String(format!("Automation: {}", automation.name)),
    );
    let mut runtime_settings = json!({ "firstUserMessage": prompt });
    if let Some(agent_session_id) = resume_agent_session_id {
        runtime_settings["agentSessionId"] = Value::String(agent_session_id.to_string());
    }
    params.insert("runtimeSettings".to_string(), runtime_settings);
    params.insert("requireLaunchCommand".to_string(), Value::Bool(true));
    let project_value = repository
        .get_project(&project.project_id)?
        .ok_or_else(|| DomainStateError::not_found("Automation project not found."))?;
    let agent_settings = read_agent_settings(db)?;
    let mut create_params = create_agent_session_params_for_project(db, &project_value, &params)?;
    if resume_agent_session_id.is_some() {
        apply_resume_launch_plan(&project_value, &mut create_params, &agent_settings)?;
    }
    let created = repository.create_session(&create_params, false)?;
    let session = apply_created_session_identity(repository, &created, &create_params)?;
    let project_id = required_value_text(&session, "projectId")?;
    let session_id = required_value_text(&session, "sessionId")?;
    let mut start_params = Map::new();
    start_params.insert("projectId".to_string(), Value::String(project_id));
    start_params.insert("sessionId".to_string(), Value::String(session_id));
    let context = ZmxServerContext {
        auth_token_file: runtime.auth_token_file.clone(),
        base_url: runtime.base_url.clone(),
    };
    dispatch_zmx_lifecycle_endpoint(
        repository,
        "/api/startSessionProvider",
        &start_params,
        &context,
        &agent_settings,
    )
    .map_err(zmx_error)?;
    Ok(session)
}

fn apply_resume_launch_plan(
    project: &Value,
    create_params: &mut Map<String, Value>,
    agent_settings: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let resume_plan = build_agent_resume_plan(
        project,
        &Value::Object(create_params.clone()),
        agent_settings,
    );
    let startup_text = resume_plan
        .get("startupText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected agent conversation does not support exact resume.",
            )
        })?
        .to_string();
    let primary_command = resume_plan
        .get("primaryCommand")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "The selected agent conversation does not have a resume command.",
            )
        })?
        .to_string();

    let mut launch_settings = create_params
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut agent_launch_plan = launch_settings
        .get("agentLaunchPlan")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    agent_launch_plan.insert(
        "agentCommand".to_string(),
        resume_plan
            .get("baseCommand")
            .cloned()
            .unwrap_or(Value::Null),
    );
    agent_launch_plan.insert("command".to_string(), Value::String(primary_command));
    agent_launch_plan.insert(
        "startupText".to_string(),
        Value::String(startup_text.clone()),
    );
    agent_launch_plan.insert(
        "startupTextDisposition".to_string(),
        Value::String("queueAfterTerminalReady".to_string()),
    );
    launch_settings.insert(
        "agentLaunchPlan".to_string(),
        Value::Object(agent_launch_plan),
    );
    let mut runtime_relevant = launch_settings
        .get("runtimeRelevant")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_relevant.insert("queueProviderStartupText".to_string(), Value::Bool(true));
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        Value::Object(runtime_relevant),
    );
    create_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    if let Some(runtime_settings) = create_params
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        runtime_settings.insert("startupText".to_string(), Value::String(startup_text));
    }
    Ok(())
}

struct WorktreeTarget {
    branch: String,
    name: String,
    path: String,
}

fn create_worktree_for_run(
    source_project: &ProjectRecord,
    run: &AutomationRunRecord,
) -> Result<WorktreeTarget, DomainStateError> {
    let source_path = Path::new(&source_project.path);
    let parent = source_path
        .parent()
        .ok_or_else(|| DomainStateError::bad_request("Project path has no parent directory."))?;
    let slug = slugify(&run.id);
    let name = format!("{}-automation-{slug}", source_project.name);
    let path = parent.join(&name);
    let branch = format!("ghostex/automation/{slug}");
    let output = Command::new("git")
        .current_dir(&source_project.path)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch)
        .arg(&path)
        .arg("HEAD")
        .output()
        .map_err(internal_error)?;
    if !output.status.success() {
        return Err(DomainStateError {
            code: "needsAttention",
            message: "Could not create automation worktree.".to_string(),
        });
    }
    Ok(WorktreeTarget {
        branch,
        name,
        path: path.to_string_lossy().to_string(),
    })
}

fn run_setup_command(cwd: &str, command: &str) -> Result<(), DomainStateError> {
    let output = Command::new("/bin/zsh")
        .current_dir(cwd)
        .arg("-lc")
        .arg(command)
        .output()
        .map_err(internal_error)?;
    if !output.status.success() {
        return Err(DomainStateError {
            code: "needsAttention",
            message: "Automation worktree setup command failed.".to_string(),
        });
    }
    Ok(())
}

fn archive_run(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project: &ProjectRecord,
    run_id: &str,
    remove_worktree: bool,
) -> Result<(), DomainStateError> {
    let run = read_run(db, &project.project_id, run_id)?;
    if is_active_status(&run.status) {
        return Err(DomainStateError::bad_request(
            "Active automation runs cannot be archived.",
        ));
    }
    if remove_worktree {
        if let Some(path) = run.worktree.get("path").and_then(Value::as_str) {
            let source = run
                .worktree
                .get("sourcePath")
                .and_then(Value::as_str)
                .unwrap_or(&project.path);
            let _ = Command::new("git")
                .current_dir(source)
                .arg("worktree")
                .arg("remove")
                .arg("--force")
                .arg(path)
                .output()
                .map_err(internal_error)?;
            if let Some(worktree_project) = find_project_by_path(repository, path)? {
                let _ = repository.remove_project(&worktree_project.project_id)?;
            }
        }
    }
    db.execute(
        r#"
        UPDATE automation_runs
        SET isArchived = 1, isUnread = 0, updatedAt = ?3
        WHERE projectId = ?1 AND runId = ?2
        "#,
        params![project.project_id, run_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn recover_running_automation_runs(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
) -> Result<(), DomainStateError> {
    let running = read_running_runs(db)?;
    for run in running {
        let Some(session_id) = run.session_id.as_deref() else {
            fail_run(
                db,
                &run.project_id,
                &run.id,
                "Automation run has no linked session.",
                "needs_attention",
            )?;
            continue;
        };
        let session_project_id =
            find_run_session_project_id(repository, &run).unwrap_or_else(|| run.project_id.clone());
        if let Some((status, summary)) =
            read_automation_result_from_session(repository, &session_project_id, session_id)?
        {
            complete_run(db, &run.project_id, &run.id, &status, summary.as_deref())?;
        } else {
            /*
            A recovered run already reached the delivery step in the watcher that
            owned it, so re-adopting it must not resubmit the prompt into a
            session that is very likely already working on it.
            */
            runtime.spawn_run_watcher(
                run.project_id,
                run.id,
                session_project_id,
                session_id.to_string(),
                None,
            );
        }
    }
    Ok(())
}

fn read_automation_result_from_session(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Option<(String, Option<String>)>, DomainStateError> {
    let mut params = Map::new();
    params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    let text =
        dispatch_zmx_session_interaction_endpoint(repository, "/api/readSessionText", &params)
            .map_err(zmx_error)?
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default();
    Ok(parse_automation_result(&text))
}

/*
CDXC:Automations 2026-07-30:
The delivered prompt is echoed into the session scrollback, so the marker scan
reads the instruction block too. Scanning only the first occurrence let that echo
decide the run: every run resolved to the first status word named in the
instructions, seconds after delivery, with the remaining instruction lines stored
as the summary. Take the last occurrence that parses into a real status instead,
because the agent's closing marker always trails its own echoed instructions.
`build_automation_prompt` keeps the instruction text free of a literal
`AUTOMATION_RESULT: <status>` line so the echo can never parse at all.
*/
fn parse_automation_result(text: &str) -> Option<(String, Option<String>)> {
    let haystack = text.to_ascii_uppercase();
    let mut result = None;
    let mut search_from = 0;
    while let Some(offset) = haystack[search_from..].find(AUTOMATION_RESULT_PREFIX) {
        search_from = search_from + offset + AUTOMATION_RESULT_PREFIX.len();
        if let Some(parsed) = parse_automation_result_at(text, search_from) {
            result = Some(parsed);
        }
    }
    result
}

fn parse_automation_result_at(
    text: &str,
    after_marker_index: usize,
) -> Option<(String, Option<String>)> {
    let mut lines = text.get(after_marker_index..)?.lines();
    let status = lines.next()?.trim().to_ascii_lowercase();
    let status = match status.as_str() {
        "findings" | "no_findings" | "needs_attention" => status,
        _ => return None,
    };
    let summary = lines
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    Some((status, (!summary.is_empty()).then_some(summary)))
}

fn normalize_definition_payload(
    project: &ProjectRecord,
    params: &Map<String, Value>,
) -> Result<AutomationDefinitionRecord, DomainStateError> {
    let value = params
        .get("definition")
        .or_else(|| params.get("automation"))
        .cloned()
        .ok_or_else(|| DomainStateError::bad_request("Automation definition is required."))?;
    let object = value
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Automation definition must be an object."))?;
    let now = now_iso();
    let id = read_value_text(object, "id")
        .unwrap_or_else(|| format!("automation-{}", uuid::Uuid::new_v4()));
    let name = read_value_text(object, "name")
        .ok_or_else(|| DomainStateError::bad_request("Automation name is required."))?;
    let agent_id = read_value_text(object, "agentId")
        .ok_or_else(|| DomainStateError::bad_request("Automation agentId is required."))?;
    let prompt = read_value_text(object, "prompt")
        .ok_or_else(|| DomainStateError::bad_request("Automation prompt is required."))?;
    let schedule = normalize_schedule(object.get("schedule"))?;
    let execution_mode = normalize_execution_mode(object.get("executionMode"))?;
    let project_id = object
        .get("projectIds")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find_map(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| project.project_id.clone());
    let enabled = object
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let created_at = read_value_text(object, "createdAt").unwrap_or_else(|| now.clone());
    let next_run_at = if enabled {
        let next_run_at = if is_one_shot_schedule(&schedule) {
            compute_next_run_at(&schedule, None)
        } else {
            read_value_text(object, "nextRunAt").or_else(|| compute_next_run_at(&schedule, None))
        };
        Some(next_run_at.ok_or_else(|| {
            DomainStateError::bad_request(
                "Automation schedule must have a future run before it can be enabled.",
            )
        })?)
    } else {
        None
    };
    Ok(AutomationDefinitionRecord {
        agent_id,
        created_at,
        enabled,
        execution_mode,
        id,
        name,
        next_run_at,
        project_id,
        prompt,
        schedule,
        updated_at: now,
    })
}

fn normalize_schedule(value: Option<&Value>) -> Result<Value, DomainStateError> {
    let schedule = value
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("Automation schedule is required."))?;
    let kind = read_value_text(schedule, "kind")
        .ok_or_else(|| DomainStateError::bad_request("Automation schedule kind is required."))?;
    match kind.as_str() {
        "once" => {
            let run_at = normalize_run_at(read_value_text(schedule, "runAt"))?;
            Ok(json!({ "kind": "once", "runAt": run_at }))
        }
        "timer" => {
            let delay_ms = schedule
                .get("delayMs")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1_000 && *value <= 365 * 24 * 60 * 60 * 1000)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Timer schedule delayMs is invalid.")
                })?;
            let run_at = (Utc::now() + chrono::Duration::milliseconds(delay_ms))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            Ok(json!({ "kind": "once", "runAt": run_at }))
        }
        "interval" => {
            let every_ms = schedule
                .get("everyMs")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 60_000 && *value <= 365 * 24 * 60 * 60 * 1000)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Interval schedule everyMs is invalid.")
                })?;
            Ok(json!({ "kind": "interval", "everyMs": every_ms }))
        }
        "daily" => {
            let time = normalize_time(read_value_text(schedule, "time"))?;
            Ok(json!({
                "kind": "daily",
                "time": time,
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        "weekly" => {
            let time = normalize_time(read_value_text(schedule, "time"))?;
            let days = schedule
                .get("days")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_u64())
                .filter(|value| *value <= 6)
                .collect::<HashSet<_>>();
            if days.is_empty() {
                return Err(DomainStateError::bad_request(
                    "Weekly schedule days are required.",
                ));
            }
            let mut days = days.into_iter().collect::<Vec<_>>();
            days.sort_unstable();
            Ok(json!({
                "days": days,
                "kind": "weekly",
                "time": time,
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        "cron" => {
            let expression = read_value_text(schedule, "expression")
                .ok_or_else(|| DomainStateError::bad_request("Cron expression is required."))?;
            Ok(json!({
                "expression": expression,
                "kind": "cron",
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        _ => Err(DomainStateError::bad_request(
            "Unsupported automation schedule kind.",
        )),
    }
}

fn normalize_execution_mode(value: Option<&Value>) -> Result<Value, DomainStateError> {
    let mode = value
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("Automation executionMode is required."))?;
    let kind = read_value_text(mode, "kind").unwrap_or_else(|| "local".to_string());
    match kind.as_str() {
        "local" => Ok(json!({ "kind": "local" })),
        "worktree" => Ok(json!({
            "kind": "worktree",
            "setupCommand": read_value_text(mode, "setupCommand"),
        })),
        "thread" => {
            let session_id = read_value_text(mode, "sessionId");
            let agent_session_id = read_value_text(mode, "agentSessionId");
            if session_id.is_none() && agent_session_id.is_none() {
                return Err(DomainStateError::bad_request(
                    "Thread executionMode requires sessionId or agentSessionId.",
                ));
            }
            Ok(json!({
                "agentSessionId": agent_session_id,
                "expiresAt": read_value_text(mode, "expiresAt"),
                "kind": "thread",
                "sessionId": session_id,
            }))
        }
        _ => Err(DomainStateError::bad_request(
            "Unsupported automation executionMode kind.",
        )),
    }
}

fn upsert_automation(
    db: &Connection,
    automation: &AutomationDefinitionRecord,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO automations (
          automationId, projectId, agentId, name, prompt, enabled, scheduleJson,
          executionModeJson, nextRunAt, createdAt, updatedAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(automationId) DO UPDATE SET
          projectId = excluded.projectId,
          agentId = excluded.agentId,
          name = excluded.name,
          prompt = excluded.prompt,
          enabled = excluded.enabled,
          scheduleJson = excluded.scheduleJson,
          executionModeJson = excluded.executionModeJson,
          nextRunAt = excluded.nextRunAt,
          updatedAt = excluded.updatedAt
        "#,
        params![
            automation.id,
            automation.project_id,
            automation.agent_id,
            automation.name,
            automation.prompt,
            bool_to_int(automation.enabled),
            stringify_json(&automation.schedule)?,
            stringify_json(&automation.execution_mode)?,
            automation.next_run_at,
            automation.created_at,
            automation.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn delete_automation(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "DELETE FROM automations WHERE projectId = ?1 AND automationId = ?2",
        params![project.project_id, automation_id],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn set_automation_enabled(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
    enabled: bool,
) -> Result<(), DomainStateError> {
    let automation = read_automation(db, project, automation_id)?;
    let next_run_at = if enabled {
        Some(
            compute_next_run_at(&automation.schedule, None).ok_or_else(|| {
                DomainStateError::bad_request(
                "This one-time automation is in the past. Choose a new date before enabling it.",
            )
            })?,
        )
    } else {
        None
    };
    db.execute(
        r#"
        UPDATE automations
        SET enabled = ?3, nextRunAt = ?4, updatedAt = ?5
        WHERE projectId = ?1 AND automationId = ?2
        "#,
        params![
            project.project_id,
            automation_id,
            bool_to_int(enabled),
            next_run_at,
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn update_automation_next_run_at(
    db: &Connection,
    project_id: &str,
    automation_id: &str,
) -> Result<(), DomainStateError> {
    let project = ProjectRecord {
        name: String::new(),
        path: String::new(),
        project_id: project_id.to_string(),
    };
    let automation = read_automation(db, &project, automation_id)?;
    let next_run_at = compute_next_run_at(&automation.schedule, None);
    let enabled = !is_one_shot_schedule(&automation.schedule) || next_run_at.is_some();
    db.execute(
        "UPDATE automations SET enabled = ?3, nextRunAt = ?4, updatedAt = ?5 WHERE projectId = ?1 AND automationId = ?2",
        params![project_id, automation_id, bool_to_int(enabled), next_run_at, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn upsert_run(db: &Connection, run: &AutomationRunRecord) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO automation_runs (
          runId, automationId, projectId, status, sessionId, worktreeJson,
          errorMessage, findingsSummary, isArchived, isUnread, createdAt, completedAt, updatedAt
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(runId) DO UPDATE SET
          status = excluded.status,
          sessionId = excluded.sessionId,
          worktreeJson = excluded.worktreeJson,
          errorMessage = excluded.errorMessage,
          findingsSummary = excluded.findingsSummary,
          isArchived = excluded.isArchived,
          isUnread = excluded.isUnread,
          completedAt = excluded.completedAt,
          updatedAt = excluded.updatedAt
        "#,
        params![
            run.id,
            run.automation_id,
            run.project_id,
            run.status,
            run.session_id,
            stringify_json(&run.worktree)?,
            run.error_message,
            run.findings_summary,
            bool_to_int(run.is_archived),
            bool_to_int(run.is_unread),
            run.created_at,
            run.completed_at,
            run.updated_at,
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn complete_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
    status: &str,
    summary: Option<&str>,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE automation_runs
        SET status = ?3, findingsSummary = ?4, completedAt = ?5, isUnread = ?6, updatedAt = ?5
        WHERE projectId = ?1 AND runId = ?2
        "#,
        params![
            project_id,
            run_id,
            status,
            summary,
            now_iso(),
            bool_to_int(status != "no_findings"),
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn fail_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
    message: &str,
    status: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE automation_runs
        SET status = ?3, errorMessage = ?4, completedAt = ?5, isUnread = 1, updatedAt = ?5
        WHERE projectId = ?1 AND runId = ?2
        "#,
        params![project_id, run_id, status, message, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn patch_run_read(db: &Connection, project_id: &str, run_id: &str) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE automation_runs SET isUnread = 0, updatedAt = ?3 WHERE projectId = ?1 AND runId = ?2",
        params![project_id, run_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn read_automation(
    db: &Connection,
    project: &ProjectRecord,
    automation_id: &str,
) -> Result<AutomationDefinitionRecord, DomainStateError> {
    db.query_row(
        "SELECT * FROM automations WHERE projectId = ?1 AND automationId = ?2",
        params![project.project_id, automation_id],
        automation_from_row,
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| DomainStateError::not_found("Automation not found."))
}

fn read_automations_for_project(
    db: &Connection,
    project_id: &str,
) -> Result<Vec<AutomationDefinitionRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automations WHERE projectId = ?1 ORDER BY updatedAt DESC, automationId ASC LIMIT ?2")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![project_id, AUTOMATION_MAX_COUNT as i64],
            automation_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn read_due_automations(
    db: &Connection,
) -> Result<Vec<AutomationDefinitionRecord>, DomainStateError> {
    let now = now_iso();
    let mut statement = db
        .prepare(
            r#"
            SELECT * FROM automations
            WHERE enabled = 1 AND nextRunAt IS NOT NULL AND nextRunAt <= ?1
            ORDER BY nextRunAt ASC
            LIMIT ?2
            "#,
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![now, AUTOMATION_MAX_COUNT as i64],
            automation_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn read_run(
    db: &Connection,
    project_id: &str,
    run_id: &str,
) -> Result<AutomationRunRecord, DomainStateError> {
    db.query_row(
        "SELECT * FROM automation_runs WHERE projectId = ?1 AND runId = ?2",
        params![project_id, run_id],
        run_from_row,
    )
    .optional()
    .map_err(sql_error)?
    .ok_or_else(|| DomainStateError::not_found("Automation run not found."))
}

fn read_runs_for_project(
    db: &Connection,
    project_id: &str,
) -> Result<Vec<AutomationRunRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automation_runs WHERE projectId = ?1 ORDER BY COALESCE(completedAt, createdAt) DESC, runId ASC LIMIT ?2")
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![project_id, AUTOMATION_MAX_RUN_COUNT as i64],
            run_from_row,
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn read_running_runs(db: &Connection) -> Result<Vec<AutomationRunRecord>, DomainStateError> {
    let mut statement = db
        .prepare("SELECT * FROM automation_runs WHERE status IN ('queued', 'running')")
        .map_err(sql_error)?;
    let rows = statement.query_map([], run_from_row).map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn has_active_run(db: &Connection, automation_id: &str) -> Result<bool, DomainStateError> {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM automation_runs WHERE automationId = ?1 AND status IN ('queued', 'running')",
            [automation_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(count > 0)
}

fn is_run_active(
    db: &Connection,
    project_id: &str,
    run_id: &str,
) -> Result<bool, DomainStateError> {
    let status: Option<String> = db
        .query_row(
            "SELECT status FROM automation_runs WHERE projectId = ?1 AND runId = ?2",
            params![project_id, run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(status.as_deref().is_some_and(is_active_status))
}

fn automation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationDefinitionRecord> {
    let schedule_json: String = row.get("scheduleJson")?;
    let execution_mode_json: String = row.get("executionModeJson")?;
    Ok(AutomationDefinitionRecord {
        agent_id: row.get("agentId")?,
        created_at: row.get("createdAt")?,
        enabled: row.get::<_, i64>("enabled")? == 1,
        execution_mode: parse_json_value(&execution_mode_json),
        id: row.get("automationId")?,
        name: row.get("name")?,
        next_run_at: row.get("nextRunAt")?,
        project_id: row.get("projectId")?,
        prompt: row.get("prompt")?,
        schedule: parse_json_value(&schedule_json),
        updated_at: row.get("updatedAt")?,
    })
}

fn run_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunRecord> {
    let worktree_json: String = row.get("worktreeJson")?;
    Ok(AutomationRunRecord {
        automation_id: row.get("automationId")?,
        completed_at: row.get("completedAt")?,
        created_at: row.get("createdAt")?,
        error_message: row.get("errorMessage")?,
        findings_summary: row.get("findingsSummary")?,
        id: row.get("runId")?,
        is_archived: row.get::<_, i64>("isArchived")? == 1,
        is_unread: row.get::<_, i64>("isUnread")? == 1,
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        status: row.get("status")?,
        updated_at: row.get("updatedAt")?,
        worktree: parse_json_value(&worktree_json),
    })
}

fn automations_to_value(automations: Vec<AutomationDefinitionRecord>) -> Value {
    Value::Array(
        automations
            .into_iter()
            .map(|automation| {
                json!({
                    "agentId": automation.agent_id,
                    "createdAt": automation.created_at,
                    "enabled": automation.enabled,
                    "executionMode": automation.execution_mode,
                    "id": automation.id,
                    "name": automation.name,
                    "nextRunAt": automation.next_run_at,
                    "projectIds": [automation.project_id],
                    "prompt": automation.prompt,
                    "schedule": automation.schedule,
                    "updatedAt": automation.updated_at,
                })
            })
            .collect(),
    )
}

fn runs_to_value(runs: Vec<AutomationRunRecord>) -> Value {
    Value::Array(
        runs.into_iter()
            .map(|run| {
                json!({
                    "automationId": run.automation_id,
                    "completedAt": run.completed_at,
                    "createdAt": run.created_at,
                    "errorMessage": run.error_message,
                    "findingsSummary": run.findings_summary,
                    "id": run.id,
                    "isArchived": run.is_archived,
                    "isUnread": run.is_unread,
                    "projectId": run.project_id,
                    "sessionId": run.session_id,
                    "status": run.status,
                    "worktree": if run.worktree.as_object().is_some_and(|value| value.is_empty()) { Value::Null } else { run.worktree },
                })
            })
            .collect(),
    )
}

fn read_automation_target_projects(
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    let mut targets = Vec::new();
    for project in repository.list_projects()? {
        let record = project_record_from_value(&project)?;
        if record.path.trim().is_empty() {
            continue;
        }
        let worktree = worktree_availability(&record);
        targets.push(json!({
            "canUseWorktrees": worktree.0,
            "label": record.name,
            "path": record.path,
            "projectId": record.project_id,
            "worktreeUnavailableReason": worktree.1,
        }));
    }
    Ok(Value::Array(targets))
}

const WORKTREE_AVAILABILITY_PROBE_TTL: Duration = Duration::from_secs(60);

fn worktree_availability_probe_cache() -> &'static Mutex<HashMap<String, (Instant, bool)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn is_path_inside_git_work_tree(path: &str) -> bool {
    /*
    readAutomationState recomputes worktree availability for every registered
    project on every call, so probing git per read multiplies into hundreds of
    subprocess spawns per second. Worktree membership only changes on git
    init/worktree edits; cache probes per path briefly instead of spawning git
    each time.
    */
    if let Ok(cache) = worktree_availability_probe_cache().lock() {
        if let Some((probed_at, is_work_tree)) = cache.get(path) {
            if probed_at.elapsed() < WORKTREE_AVAILABILITY_PROBE_TTL {
                return *is_work_tree;
            }
        }
    }
    let output = Command::new("git")
        .current_dir(path)
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .output();
    let is_work_tree = matches!(
        output,
        Ok(ref output)
            if output.status.success()
                && String::from_utf8_lossy(&output.stdout).trim() == "true"
    );
    if let Ok(mut cache) = worktree_availability_probe_cache().lock() {
        cache.insert(path.to_string(), (Instant::now(), is_work_tree));
    }
    is_work_tree
}

fn worktree_availability(project: &ProjectRecord) -> (bool, Option<String>) {
    if project.path.trim().is_empty() {
        return (
            false,
            Some("Worktree mode needs an active code project.".to_string()),
        );
    }
    if is_path_inside_git_work_tree(&project.path) {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "{} is not inside a Git work tree. Use Local mode explicitly for non-Git projects.",
                project.name
            )),
        )
    }
}

fn hud_agents_to_automation_agents(hud: &Value) -> Value {
    /*
    CDXC:Automations 2026-07-02-04:10:
    Automation pickers consume this list directly, so it must contain only
    agents that automations can actually launch. Exclude commandless agents to
    match the native selector's rules.
    */
    Value::Array(
        hud.get("agents")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|agent| {
                let object = agent.as_object()?;
                let agent_id = read_value_text(object, "agentId")?;
                let command = read_value_text(object, "command")?;
                let label = read_value_text(object, "name").unwrap_or_else(|| agent_id.clone());
                let mut output = Map::new();
                output.insert("agentId".to_string(), Value::String(agent_id));
                output.insert("label".to_string(), Value::String(label));
                output.insert("command".to_string(), Value::String(command));
                if let Some(icon) = read_value_text(object, "icon") {
                    output.insert("icon".to_string(), Value::String(icon));
                }
                Some(Value::Object(output))
            })
            .collect(),
    )
}

fn create_run_record(
    automation: &AutomationDefinitionRecord,
    status: &str,
    error_message: Option<&str>,
) -> AutomationRunRecord {
    let now = now_iso();
    AutomationRunRecord {
        automation_id: automation.id.clone(),
        completed_at: (!is_active_status(status)).then_some(now.clone()),
        created_at: now.clone(),
        error_message: error_message.map(str::to_string),
        findings_summary: None,
        id: format!("automation-run-{}", uuid::Uuid::new_v4()),
        is_archived: false,
        is_unread: status != "no_findings" && status != "skipped",
        project_id: automation.project_id.clone(),
        session_id: None,
        status: status.to_string(),
        updated_at: now,
        worktree: json!({}),
    }
}

fn resolve_automation_project(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<ProjectRecord, DomainStateError> {
    if let Some(project_id) = read_param_text(params, "projectId") {
        return resolve_project_by_id(repository, &project_id);
    }
    if let Some(path) =
        read_param_text(params, "projectPath").or_else(|| read_param_text(params, "path"))
    {
        if let Some(project) = find_project_by_path(repository, &path)? {
            return Ok(project);
        }
        let mut add_params = Map::new();
        add_params.insert("path".to_string(), Value::String(path));
        return project_record_from_value(&repository.add_project_path(&add_params)?);
    }
    repository
        .list_projects()?
        .into_iter()
        .find_map(|project| {
            project_record_from_value(&project)
                .ok()
                .filter(|project| !project.path.trim().is_empty())
        })
        .ok_or_else(|| DomainStateError::bad_request("projectId or projectPath is required."))
}

fn resolve_project_by_id(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<ProjectRecord, DomainStateError> {
    let value = repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
    project_record_from_value(&value)
}

fn find_project_by_path(
    repository: &DomainRepository<'_>,
    path: &str,
) -> Result<Option<ProjectRecord>, DomainStateError> {
    let normalized = normalize_path(path);
    for project in repository.list_projects()? {
        let record = project_record_from_value(&project)?;
        if normalize_path(&record.path) == normalized {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

fn project_record_from_value(value: &Value) -> Result<ProjectRecord, DomainStateError> {
    let object = value
        .as_object()
        .ok_or_else(|| DomainStateError::corrupt_state("Project row is not an object."))?;
    Ok(ProjectRecord {
        name: read_value_text(object, "name").unwrap_or_else(|| "Project".to_string()),
        path: read_value_text(object, "path").unwrap_or_default(),
        project_id: read_value_text(object, "projectId")
            .ok_or_else(|| DomainStateError::corrupt_state("Project row missing projectId."))?,
    })
}

fn resolve_thread_session(
    repository: &DomainRepository<'_>,
    default_project_id: &str,
    session_id: &str,
) -> Result<(String, String), DomainStateError> {
    if let Some((project_id, session_id)) = session_id.split_once(':') {
        if repository.get_session(project_id, session_id)?.is_some() {
            return Ok((project_id.to_string(), session_id.to_string()));
        }
    }
    if repository
        .get_session(default_project_id, session_id)?
        .is_some()
    {
        return Ok((default_project_id.to_string(), session_id.to_string()));
    }
    for project in repository.list_projects()? {
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if repository.get_session(project_id, session_id)?.is_some() {
            return Ok((project_id.to_string(), session_id.to_string()));
        }
    }
    Err(DomainStateError::not_found(
        "Thread session is no longer available.",
    ))
}

fn resolve_agent_thread_session(
    repository: &DomainRepository<'_>,
    default_project_id: &str,
    session_id_hint: Option<&str>,
    agent_id: &str,
    agent_session_id: &str,
) -> Result<Option<(String, String)>, DomainStateError> {
    if let Some(session_id_hint) = session_id_hint {
        if let Ok((project_id, session_id)) =
            resolve_thread_session(repository, default_project_id, session_id_hint)
        {
            if let Some(session) = repository.get_session(&project_id, &session_id)? {
                if session_owns_agent_conversation(&session, agent_id, agent_session_id) {
                    return Ok(Some((project_id, session_id)));
                }
            }
        }
    }

    let matches = repository
        .list_sessions(None)?
        .into_iter()
        .filter(|session| session_owns_agent_conversation(session, agent_id, agent_session_id))
        .filter_map(|session| {
            Some((
                required_value_text(&session, "projectId").ok()?,
                required_value_text(&session, "sessionId").ok()?,
            ))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [target] => Ok(Some(target.clone())),
        _ => Err(DomainStateError {
            code: "needsAttention",
            message: format!(
                "Multiple Ghostex sessions own agent conversation {agent_session_id}. Close the duplicate panes before running this automation."
            ),
        }),
    }
}

fn session_owns_agent_conversation(
    session: &Value,
    agent_id: &str,
    agent_session_id: &str,
) -> bool {
    let runtime_settings = session.get("runtimeSettings").and_then(Value::as_object);
    let stored_agent_session_id = runtime_settings
        .and_then(|settings| settings.get("agentSessionId"))
        .and_then(Value::as_str)
        .map(str::trim);
    if stored_agent_session_id != Some(agent_session_id) {
        return false;
    }
    session
        .get("agentId")
        .and_then(Value::as_str)
        .or_else(|| {
            runtime_settings
                .and_then(|settings| settings.get("launchAgentId"))
                .and_then(Value::as_str)
        })
        .is_some_and(|stored_agent_id| stored_agent_id.eq_ignore_ascii_case(agent_id))
}

fn find_run_session_project_id(
    repository: &DomainRepository<'_>,
    run: &AutomationRunRecord,
) -> Option<String> {
    let session_id = run.session_id.as_ref()?;
    if let Some(path) = run.worktree.get("path").and_then(Value::as_str) {
        if let Ok(Some(project)) = find_project_by_path(repository, path) {
            return Some(project.project_id);
        }
    }
    if repository
        .get_session(&run.project_id, session_id)
        .ok()
        .flatten()
        .is_some()
    {
        return Some(run.project_id.clone());
    }
    repository
        .list_projects()
        .ok()?
        .into_iter()
        .filter_map(|project| {
            project
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .find(|project_id| {
            repository
                .get_session(project_id, session_id)
                .ok()
                .flatten()
                .is_some()
        })
}

/*
CDXC:Automations 2026-07-30:
This text is typed into the session, so it is echoed into the same scrollback the
run watcher scans. Two constraints therefore apply at once, and they pull against
each other.

The instructions must never contain a parseable `AUTOMATION_RESULT: <status>`
line, or the echo reports the run's own result before the agent has finished.

They must also leave no doubt about what follows the colon. An earlier revision
satisfied the first constraint with prose right after the marker ("...starts with
AUTOMATION_RESULT: completed by exactly one of these three words..."), and agents
intermittently copied that prose into their answer, emitting
`AUTOMATION_RESULT: completed no_findings`. That parses as nothing, so the run
died at the watcher timeout -- the very failure this module was fixed to remove.

A placeholder template satisfies both: `<status>` is not a valid status, so the
echo cannot parse, while the line still shows the exact shape the agent must
produce.
*/
fn build_automation_prompt(prompt: &str) -> String {
    format!(
        "{}\n\nWhen this automation finishes, end your final message with a line in exactly this form:\n\nAUTOMATION_RESULT: <status>\n\nReplace <status> with exactly one of these three words and nothing else: findings, no_findings, needs_attention. Put a short summary on the lines after that line.",
        prompt.trim()
    )
}

fn compute_next_run_at(schedule: &Value, after: Option<chrono::DateTime<Utc>>) -> Option<String> {
    let now = after.unwrap_or_else(Utc::now);
    match schedule.get("kind").and_then(Value::as_str)? {
        "once" => {
            let run_at = chrono::DateTime::parse_from_rfc3339(
                schedule.get("runAt").and_then(Value::as_str)?,
            )
            .ok()?
            .with_timezone(&Utc);
            (run_at > now).then(|| run_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "interval" => {
            let every_ms = schedule.get("everyMs").and_then(Value::as_i64)?;
            Some(
                (now + chrono::Duration::milliseconds(every_ms))
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            )
        }
        "daily" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            next_daily(now, hour, minute)
        }
        "weekly" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            let days = schedule.get("days").and_then(Value::as_array)?;
            next_weekly(
                now,
                hour,
                minute,
                days.iter().filter_map(Value::as_u64).collect(),
            )
        }
        "cron" => next_basic_cron(now, schedule.get("expression").and_then(Value::as_str)?),
        _ => None,
    }
}

fn is_one_shot_schedule(schedule: &Value) -> bool {
    schedule.get("kind").and_then(Value::as_str) == Some("once")
}

fn next_daily(now: chrono::DateTime<Utc>, hour: u32, minute: u32) -> Option<String> {
    let today = candidate_utc(now.date_naive(), hour, minute)?;
    let next = if today > now {
        today
    } else {
        candidate_utc(now.date_naive().succ_opt()?, hour, minute)?
    };
    Some(next.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

fn next_weekly(
    now: chrono::DateTime<Utc>,
    hour: u32,
    minute: u32,
    days: Vec<u64>,
) -> Option<String> {
    let day_set = days.into_iter().collect::<HashSet<_>>();
    for offset in 0..=7 {
        let date = now
            .date_naive()
            .checked_add_signed(chrono::Duration::days(offset))?;
        let weekday = date.weekday().num_days_from_sunday() as u64;
        if !day_set.contains(&weekday) {
            continue;
        }
        let candidate = candidate_utc(date, hour, minute)?;
        if candidate > now {
            return Some(candidate.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    None
}

fn next_basic_cron(now: chrono::DateTime<Utc>, expression: &str) -> Option<String> {
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 5 {
        return None;
    }
    if let Some(step) = parts[0]
        .strip_prefix("*/")
        .and_then(|value| value.parse::<i64>().ok())
    {
        if parts[1] == "*" && parts[2] == "*" && parts[3] == "*" && parts[4] == "*" && step > 0 {
            let current_minute = now.timestamp() / 60;
            let next_minute = ((current_minute / step) + 1) * step;
            return Utc
                .timestamp_opt(next_minute * 60, 0)
                .single()
                .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    let minute = parts[0].parse::<u32>().ok()?;
    let hour = parts[1].parse::<u32>().ok()?;
    if parts[2] != "*" || parts[3] != "*" {
        return None;
    }
    if parts[4] == "*" {
        return next_daily(now, hour, minute);
    }
    let days = parts[4]
        .split(',')
        .filter_map(|day| day.parse::<u64>().ok())
        .collect::<Vec<_>>();
    next_weekly(now, hour, minute, days)
}

fn candidate_utc(date: NaiveDate, hour: u32, minute: u32) -> Option<chrono::DateTime<Utc>> {
    Utc.from_local_datetime(&date.and_hms_opt(hour, minute, 0)?)
        .single()
}

fn parse_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour <= 23 && minute <= 59).then_some((hour, minute))
}

fn normalize_time(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value.ok_or_else(|| DomainStateError::bad_request("Schedule time is required."))?;
    parse_time(&value)
        .map(|_| value)
        .ok_or_else(|| DomainStateError::bad_request("Schedule time must be HH:mm."))
}

fn normalize_run_at(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value
        .ok_or_else(|| DomainStateError::bad_request("One-time schedule runAt is required."))?;
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|run_at| {
            run_at
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .map_err(|_| {
            DomainStateError::bad_request("One-time schedule runAt must be an ISO 8601 date.")
        })
}

fn is_active_status(status: &str) -> bool {
    status == "queued" || status == "running"
}

fn read_param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_value_text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_value_text(value: &Value, key: &str) -> Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DomainStateError::corrupt_state(format!("Missing {key}.")))
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn parse_json_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({}))
}

fn stringify_json(value: &Value) -> Result<String, DomainStateError> {
    serde_json::to_string(value).map_err(internal_error)
}

fn normalize_path(path: &str) -> String {
    PathBuf::from(path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "run".to_string()
    } else {
        slug
    }
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn internal_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

fn zmx_error(error: ZmxEndpointError) -> DomainStateError {
    match error {
        ZmxEndpointError::Domain(error) => error,
        ZmxEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "internalError",
            message,
        },
    }
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite automation state error: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_input_is_anchored_as_a_one_time_schedule() {
        let before = Utc::now();
        let normalized = normalize_schedule(Some(&json!({
            "kind": "timer",
            "delayMs": 60_000,
        })))
        .expect("timer should normalize");
        let run_at = chrono::DateTime::parse_from_rfc3339(
            normalized
                .get("runAt")
                .and_then(Value::as_str)
                .expect("timer has runAt"),
        )
        .expect("runAt is ISO 8601")
        .with_timezone(&Utc);
        assert_eq!(normalized.get("kind").and_then(Value::as_str), Some("once"));
        assert!(run_at >= before + chrono::Duration::seconds(59));
        assert!(run_at <= Utc::now() + chrono::Duration::seconds(61));
    }

    #[test]
    fn one_time_schedule_has_no_next_run_after_its_deadline() {
        let schedule = json!({
            "kind": "once",
            "runAt": "2026-08-14T09:30:00.000Z",
        });
        let before = Utc.with_ymd_and_hms(2026, 8, 14, 9, 0, 0).single().unwrap();
        let after = Utc
            .with_ymd_and_hms(2026, 8, 14, 10, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            compute_next_run_at(&schedule, Some(before)).as_deref(),
            Some("2026-08-14T09:30:00.000Z")
        );
        assert_eq!(compute_next_run_at(&schedule, Some(after)), None);
    }

    #[test]
    fn prompt_instructions_never_parse_as_a_result() {
        /*
        CDXC:Automations 2026-07-30:
        Regression lock for the pair of bugs that made every delivered run report a
        bogus result: the prompt is echoed into the scanned scrollback, so the
        instruction block must not contain a parseable marker line, and the scan
        must not stop at the first occurrence.
        */
        let prompt = build_automation_prompt("Me fale quem é o NAP");
        assert!(prompt.contains(AUTOMATION_RESULT_PREFIX));
        assert_eq!(parse_automation_result(&prompt), None);
    }

    #[test]
    fn agent_marker_after_echoed_instructions_wins() {
        let session_text = format!(
            "{}\n\nI looked into it.\n\nAUTOMATION_RESULT: needs_attention\nNo NAP context found in the repo.\n",
            build_automation_prompt("Me fale quem é o NAP")
        );
        assert_eq!(
            parse_automation_result(&session_text),
            Some((
                "needs_attention".to_string(),
                Some("No NAP context found in the repo.".to_string())
            ))
        );
    }

    #[test]
    fn last_valid_marker_wins_over_earlier_ones() {
        let session_text = "AUTOMATION_RESULT: findings\nfirst pass\n\nAUTOMATION_RESULT: no_findings\nsecond pass\n";
        assert_eq!(
            parse_automation_result(session_text),
            Some(("no_findings".to_string(), Some("second pass".to_string())))
        );
    }

    #[test]
    fn wrapped_instruction_echo_still_never_parses() {
        /*
        The pane hard-wraps long lines, so the instruction text can break right
        after the marker. An empty or partial trailing line must not parse.
        */
        for wrapped in [
            "AUTOMATION_RESULT:\ncompleted by exactly one of these three words - findings,\n",
            "  AUTOMATION_RESULT: completed by exactly one of these three\n  words - findings, no_findings, or needs_attention.\n",
        ] {
            assert_eq!(parse_automation_result(wrapped), None, "{wrapped}");
        }
    }

    #[test]
    fn prompt_shows_the_marker_alone_on_its_line() {
        /*
        Regression lock for an observed live failure: prose placed right after the
        marker got copied into the agent's answer as
        `AUTOMATION_RESULT: completed no_findings`, which parses as nothing and
        sent the run back to the watcher timeout. The marker must appear only on a
        line of its own, followed by a placeholder and nothing else.
        */
        let prompt = build_automation_prompt("Check the deploy");
        let marker_line = prompt
            .lines()
            .find(|line| line.contains(AUTOMATION_RESULT_PREFIX))
            .expect("prompt names the marker");
        assert_eq!(marker_line.trim(), "AUTOMATION_RESULT: <status>");
        assert_eq!(
            prompt.matches(AUTOMATION_RESULT_PREFIX).count(),
            1,
            "one marker occurrence keeps the echo unambiguous"
        );
    }

    #[test]
    fn status_word_must_stand_alone_after_the_marker() {
        for drifted in [
            "AUTOMATION_RESULT: completed no_findings\nsummary\n",
            "AUTOMATION_RESULT: status findings\nsummary\n",
            "AUTOMATION_RESULT: <status>\nsummary\n",
        ] {
            assert_eq!(parse_automation_result(drifted), None, "{drifted}");
        }
    }

    #[test]
    fn missing_marker_leaves_the_run_pending() {
        assert_eq!(
            parse_automation_result("claude booted and is waiting at an empty composer"),
            None
        );
    }
}
