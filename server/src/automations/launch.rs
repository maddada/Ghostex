use std::{path::Path, process::Command};

use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};

use crate::{
    agents::{
        apply_created_session_identity, build_agent_resume_plan,
        create_agent_session_params_for_project, read_agent_settings,
    },
    domain::{DomainRepository, DomainStateError},
    zmx::{dispatch_zmx_lifecycle_endpoint, ZmxServerContext},
};

use super::*;

pub(super) fn queue_automation_run(
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
            runtime.routine_run_settled(&run.project_id, &run.id);
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
    if let Some(bot_id) = automation.owner_bot_id.as_deref() {
        return launch_bot_automation(runtime, repository, db, automation, bot_id, &prompt);
    }
    match execution_kind {
        "thread" => launch_thread_automation(runtime, repository, db, automation, &prompt, run),
        "worktree" => launch_worktree_automation(runtime, repository, db, automation, &prompt, run),
        _ => launch_local_automation(runtime, repository, db, automation, &prompt),
    }
}

/// A GBot's routine runs as a chat of that GBot (gbots/routine_runs.rs), which queues the prompt
/// itself, so the watcher has nothing left to deliver.
fn launch_bot_automation(
    runtime: &AutomationRuntime,
    repository: &DomainRepository<'_>,
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    bot_id: &str,
    prompt: &str,
) -> Result<AutomationLaunch, DomainStateError> {
    let app = runtime.app_state().ok_or_else(|| DomainStateError {
        code: "needsAttention",
        message: "Ghostex was still starting, so the routine's chat could not open.".to_string(),
    })?;
    let (session_project_id, session_id) = crate::gbots::start_routine_run(
        &app,
        db,
        repository,
        bot_id,
        &automation.id,
        &automation.name,
        prompt,
    )?;
    Ok(AutomationLaunch {
        pending_prompt: None,
        session_id,
        session_project_id,
        worktree: None,
    })
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
    let output = crate::platform::process::background_command("git")
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

pub(super) fn archive_run(
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
            let _ = crate::platform::process::background_command("git")
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
pub(super) fn build_automation_prompt(prompt: &str) -> String {
    format!(
        "{}\n\nWhen this automation finishes, end your final message with a line in exactly this form:\n\nAUTOMATION_RESULT: <status>\n\nReplace <status> with exactly one of these three words and nothing else: findings, no_findings, needs_attention. Put a short summary on the lines after that line.",
        prompt.trim()
    )
}
