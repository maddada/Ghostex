use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::project_session_title_projection;
use crate::zmx::dispatch_zmx_lifecycle_endpoint;
use crate::zmx::ZmxServerContext;

pub(crate) fn fork_session(
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

pub(crate) fn build_agent_fork_plan(
    project: &Value,
    session: &Value,
    settings: &Map<String, Value>,
) -> Value {
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

pub(crate) fn create_agent_fork_session_params(
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

pub(crate) fn request_session_rename(
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
