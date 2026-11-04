//! `/api/readSessionDetails` and `/api/stopSessionBackgroundTask`.

use std::path::PathBuf;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;

use super::probe::{ProbeMode, ProbeSnapshot, ProbeTarget};
use super::scan::TranscriptFacts;

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

fn text_param(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn require_enabled(state: &AppState) -> Result<(), DomainStateError> {
    if super::session_details_enabled(&state.paths) {
        return Ok(());
    }
    Err(failure(
        "disabled",
        ghostex_settings_catalog::built_in_extensions::turned_off_message(
            ghostex_settings_catalog::built_in_extensions::SESSION_DETAILS,
        ),
    ))
}

fn read_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<(Value, Option<Value>), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        failure(
            "internalError",
            format!("SQLite gxserver state error: {error}"),
        )
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| failure("notFound", "The session no longer exists."))?;
    let project = repository.get_project(project_id)?;
    Ok((session, project))
}

/// `/api/readSessionDetails`: everything the Session details panel shows for one session.
/// `breakdown` is `"auto"` (the panel's own reads) or `"refresh"` (its refresh button).
pub(crate) async fn handle_read_session_details_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = async {
        let params = read_domain_rpc_params(body)?;
        let (project_id, session_id) = (
            text_param(&params, "projectId"),
            text_param(&params, "sessionId"),
        );
        if project_id.is_empty() || session_id.is_empty() {
            return Err(failure(
                "invalidParams",
                "readSessionDetails requires projectId and sessionId.",
            ));
        }
        require_enabled(state)?;
        let mode = match text_param(&params, "breakdown").as_str() {
            "refresh" => ProbeMode::Refresh,
            _ => ProbeMode::Auto,
        };
        let state = state.clone();
        tokio::task::spawn_blocking(move || read_details(&state, &project_id, &session_id, mode))
            .await
            .map_err(|error| failure("internalError", error.to_string()))?
    }
    .await;
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn read_details(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    mode: ProbeMode,
) -> Result<Value, DomainStateError> {
    let (session, project) = read_session(state, project_id, session_id)?;
    let agent = crate::session_chat_follower::session_chat_display_agent_for_session(&session);
    let claude = agent.as_deref() == Some("claude");
    let folder = crate::session_git_status::effective_session_git_cwd(&session, project.as_ref());
    let home = state.paths.home_dir.clone();
    let mut environment = super::environment::read_environment(
        &session,
        project.as_ref(),
        folder.as_deref(),
        &home,
        agent.as_deref(),
    );
    // A Cloud Box session's processes run on the box, out of this computer's reach.
    if crate::agentbox::session_agentbox(&session).is_none() {
        environment["memoryBytes"] = json!(crate::zmx::provider_zmx_session_name(&session)
            .ok()
            .and_then(|name| super::memory::read_session_memory_bytes(&name)));
    }
    let changes = folder.as_deref().and_then(super::changes::read_changes);
    let mut result = json!({
        "agent": agent,
        "supported": claude,
        "environment": environment,
        "changes": changes,
        "updatedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });
    if !claude {
        super::agents::read_agent_details(
            state,
            project_id,
            session_id,
            &session,
            folder.as_deref(),
            agent.as_deref().unwrap_or_default(),
            &mut result,
        );
        return Ok(result);
    }
    let resolution = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    )?;
    let (_, transcript) = resolution.transcript();
    let transcript = transcript.map(PathBuf::from);
    let facts: Option<TranscriptFacts> = transcript
        .as_deref()
        .and_then(|path| super::scan::scan_claude_transcript(path).ok());
    let transcript_len = transcript
        .as_deref()
        .and_then(|path| std::fs::metadata(path).ok())
        .map_or(0, |metadata| metadata.len());
    let agent_session_id = read_runtime_text(&session, "agentSessionId");
    let hook_state =
        crate::agent_hooks::config::HookPaths::from_paths(&state.paths).hook_state_directory;
    let statusline = agent_session_id.as_deref().and_then(|id| {
        crate::agent_hooks::statusline::read_claude_statusline_payload(&hook_state, id)
    });
    let payload = statusline.as_ref().map(|stored| &stored.payload);
    let live = payload
        .and_then(|payload| payload.get("context_window"))
        .map(|window| {
            json!({
                "usedTokens": window["total_input_tokens"],
                "windowSize": window["context_window_size"],
                "usedPercentage": window["used_percentage"],
                "updatedAt": statusline.as_ref().map(|stored| stored.updated_at.clone()),
            })
        });
    let model = payload
        .and_then(|payload| payload.get("model"))
        .and_then(|model| model.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let cwd = payload
        .and_then(|payload| {
            payload.get("cwd").or_else(|| {
                payload
                    .get("workspace")
                    .and_then(|workspace| workspace.get("current_dir"))
            })
        })
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| folder.as_deref().map(PathBuf::from));
    let probe = match agent_session_id.clone() {
        Some(agent_session_id) => super::probe::request(
            ProbeTarget {
                agent_session_id,
                cwd,
                zmx_name: crate::zmx::provider_zmx_session_name(&session).ok(),
                model: model.clone(),
                transcript_len,
                resumable: facts
                    .as_ref()
                    .is_some_and(|facts| facts.conversation_started),
                working: resolution.working(),
                home: home.clone(),
            },
            mode,
        ),
        None => ProbeSnapshot::default(),
    };
    let connectors = super::connectors::read_connectors(
        state,
        project_id,
        session_id,
        facts.as_ref(),
        probe.mcp_status.as_ref(),
    );
    result["context"] = json!({
        "model": model,
        "live": live,
        "breakdown": probe.breakdown,
        "breakdownAt": probe.taken_at.map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        "breakdownRunning": probe.running,
        "breakdownError": probe.error,
        "refreshAt": probe.refresh_at.map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    });
    result["connectors"] = connectors;
    let skill_tokens = |name: &str| {
        probe
            .breakdown
            .as_ref()
            .and_then(|breakdown| breakdown["skills"]["items"].as_array())
            .and_then(|items| items.iter().find(|item| item["name"] == name))
            .map(|item| item["tokens"].clone())
            .unwrap_or(Value::Null)
    };
    if let Some(facts) = facts.as_ref() {
        let sources = super::skills::skill_sources(
            &home,
            folder.as_deref().map(std::path::Path::new),
            facts.skills.iter().map(|(name, _)| name.as_str()),
        );
        result["skills"] = json!({
            "used": facts.invoked_skills,
            "available": facts.skills.iter().map(|(name, description)| json!({
                "name": name,
                "description": description,
                "tokens": skill_tokens(name),
                "scope": sources.get(name).map(|source| source.scope),
                "path": sources.get(name).and_then(|source| source.path.clone()),
            })).collect::<Vec<_>>(),
        });
        let file_tokens = |path: &str| {
            probe
                .breakdown
                .as_ref()
                .and_then(|breakdown| breakdown["memoryFiles"].as_array())
                .and_then(|files| files.iter().find(|file| file["path"] == path))
                .map(|file| file["tokens"].clone())
                .unwrap_or(Value::Null)
        };
        result["memoryFiles"] = json!(facts
            .memory_files
            .iter()
            .map(|file| json!({
                "path": file.path,
                "kind": file.kind,
                "tokens": file_tokens(&file.path),
            }))
            .collect::<Vec<_>>());
        result["background"] = json!({
            "finished": facts.finished_background().into_iter().map(|run| json!({
                "id": run.id,
                "kind": run.kind,
                "title": run.title,
                "command": run.command,
                "status": run.status,
                "summary": run.summary,
                "startedAt": chrono::DateTime::from_timestamp_millis(run.started_at_ms)
                    .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
                "endedAt": run.ended_at_ms.and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            })).collect::<Vec<_>>(),
        });
    }
    Ok(result)
}

/// `/api/stopSessionBackgroundTask`: ends one background command of a Claude or OpenCode session.
pub(crate) async fn handle_stop_session_background_task_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = async {
        let params = read_domain_rpc_params(body)?;
        let (project_id, session_id, task_id) = (
            text_param(&params, "projectId"),
            text_param(&params, "sessionId"),
            text_param(&params, "taskId"),
        );
        if project_id.is_empty() || session_id.is_empty() || task_id.is_empty() {
            return Err(failure(
                "invalidParams",
                "stopSessionBackgroundTask requires projectId, sessionId and taskId.",
            ));
        }
        // Claude's task ids are short and plain; anything else is no task of Claude's.
        if !task_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }) {
            return Err(failure(
                "invalidParams",
                "That isn't a background command id.",
            ));
        }
        require_enabled(state)?;
        let (session, project) = read_session(state, &project_id, &session_id)?;
        if crate::session_chat_follower::session_chat_display_agent_for_session(&session).as_deref()
            == Some("opencode")
        {
            let folder =
                crate::session_git_status::effective_session_git_cwd(&session, project.as_ref());
            tokio::task::spawn_blocking(move || {
                super::opencode::stop_opencode_shell(&session, folder.as_deref(), &task_id)
            })
            .await
            .map_err(|error| failure("internalError", error.to_string()))?
            .map_err(|message| failure("failed", message))?;
            return Ok(json!({"stopped": true}));
        }
        let zmx_name = crate::zmx::provider_zmx_session_name(&session)?;
        let home = state.paths.home_dir.clone();
        tokio::task::spawn_blocking(move || {
            super::background::stop_background_task(&home, &zmx_name, &task_id)
        })
        .await
        .map_err(|error| failure("internalError", error.to_string()))?
        .map_err(|message| failure("failed", message))?;
        Ok(json!({"stopped": true}))
    }
    .await;
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
