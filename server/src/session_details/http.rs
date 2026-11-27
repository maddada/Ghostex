//! `/api/readSessionDetails` and `/api/stopSessionBackgroundTask`.

use std::path::PathBuf;
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;

use super::async_cache::{current_revision, wait_for_revision_change, SectionLoad};
use super::probe::{ProbeMode, ProbeSnapshot, ProbeTarget};
use super::scan::TranscriptFacts;

/// Maximum time a progressive `afterRevision` request waits for any background section to finish.
const PROGRESSIVE_WAIT_CAP: Duration = Duration::from_millis(1500);

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
/// `progressive` (`true`) returns cached/instant sections immediately (never blocking one section
/// on another) with `pendingSections` and `revision`; passing `afterRevision` waits up to
/// [`PROGRESSIVE_WAIT_CAP`] for any pending background section to finish before responding.
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
        let mode = match text_param(&params, "breakdown").as_str() {
            "refresh" => ProbeMode::Refresh,
            _ => ProbeMode::Auto,
        };
        let progressive = params
            .get("progressive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let after_revision = params.get("afterRevision").and_then(Value::as_u64);
        if progressive {
            if let Some(after) = after_revision {
                wait_for_revision_change(after, PROGRESSIVE_WAIT_CAP).await;
            }
        }
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            read_details(&state, &project_id, &session_id, mode, progressive)
        })
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
    progressive: bool,
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
    let is_local_active = crate::agentbox::session_agentbox(&session).is_none()
        && !super::agents::session_is_inactive(&session);
    let zmx_name = is_local_active
        .then(|| crate::zmx::provider_zmx_session_name(&session).ok())
        .flatten();

    if progressive {
        return read_details_progressive(
            state,
            project_id,
            session_id,
            &session,
            agent,
            claude,
            folder,
            home,
            environment,
            zmx_name,
            mode,
        );
    }

    // Legacy / CLI callers: run RAM, Changes and agent/Claude sections in parallel so total
    // latency is max(probes) rather than sum(probes).
    let (memory_bytes, changes, agent_or_claude) = std::thread::scope(|scope| {
        let mem_handle = scope.spawn(|| {
            zmx_name
                .as_deref()
                .and_then(super::memory::read_session_memory_bytes)
        });
        let changes_handle =
            scope.spawn(|| folder.as_deref().and_then(super::changes::read_changes));
        let body_handle = scope.spawn(|| -> Result<Value, DomainStateError> {
            let mut extra = json!({});
            if !claude {
                let mut ignored_pending = Vec::new();
                super::agents::read_agent_details(
                    state,
                    project_id,
                    session_id,
                    &session,
                    folder.as_deref(),
                    agent.as_deref().unwrap_or_default(),
                    false,
                    &mut extra,
                    &mut ignored_pending,
                );
                return Ok(extra);
            }
            read_claude_sections(
                state,
                project_id,
                session_id,
                &session,
                folder.as_deref(),
                &home,
                mode,
                false,
                &mut extra,
                &mut Vec::new(),
            )?;
            Ok(extra)
        });
        (
            mem_handle.join().ok().flatten(),
            changes_handle.join().ok().flatten(),
            body_handle.join().unwrap_or_else(|_| {
                Err(failure("internalError", "session details worker panicked"))
            }),
        )
    });
    if is_local_active {
        environment["memoryBytes"] = json!(memory_bytes);
    }
    let mut result = json!({
        "agent": agent,
        "supported": claude,
        "environment": environment,
        "changes": changes,
        "updatedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    });
    if let Value::Object(extra_map) = agent_or_claude? {
        if let Some(target) = result.as_object_mut() {
            for (k, v) in extra_map {
                target.insert(k, v);
            }
        }
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn read_details_progressive(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    agent: Option<String>,
    claude: bool,
    folder: Option<String>,
    home: PathBuf,
    mut environment: Value,
    zmx_name: Option<String>,
    mode: ProbeMode,
) -> Result<Value, DomainStateError> {
    let mut pending: Vec<&'static str> = Vec::new();
    if let Some(name) = zmx_name.as_deref() {
        environment["memoryBytes"] =
            json!(super::memory::read_session_memory_bytes_progressive(name));
    }
    let changes = match folder.as_deref() {
        Some(folder) => match super::changes::read_changes_progressive(folder) {
            SectionLoad::Ready(value) => value,
            SectionLoad::Pending => {
                pending.push("changes");
                None
            }
        },
        None => None,
    };
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
            session,
            folder.as_deref(),
            agent.as_deref().unwrap_or_default(),
            true,
            &mut result,
            &mut pending,
        );
    } else {
        read_claude_sections(
            state,
            project_id,
            session_id,
            session,
            folder.as_deref(),
            &home,
            mode,
            true,
            &mut result,
            &mut pending,
        )?;
    }
    result["revision"] = json!(current_revision());
    result["pendingSections"] = json!(pending);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn read_claude_sections(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
    folder: Option<&str>,
    home: &std::path::Path,
    mode: ProbeMode,
    progressive: bool,
    result: &mut Value,
    pending: &mut Vec<&'static str>,
) -> Result<(), DomainStateError> {
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
    let agent_session_id = read_runtime_text(session, "agentSessionId");
    let hook_state =
        crate::agent_hooks::config::HookPaths::from_paths(&state.paths).hook_state_directory;
    let statusline = agent_session_id.as_deref().and_then(|id| {
        crate::agent_hooks::statusline::read_claude_statusline_payload(&hook_state, id)
    });
    let payload = statusline.as_ref().map(|stored| &stored.payload);
    let live = payload
        .and_then(|payload| payload.get("context_window"))
        .map(|window| {
            // Not measured since Claude started or compacted: no size yet, not 0.
            let measured = crate::session_chat_options::claude_context_measured(Some(window));
            let used = |key: &str| {
                if measured {
                    window[key].clone()
                } else {
                    Value::Null
                }
            };
            json!({
                "usedTokens": used("total_input_tokens"),
                "windowSize": window["context_window_size"],
                "usedPercentage": used("used_percentage"),
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
        .or_else(|| folder.map(PathBuf::from));
    let probe = match agent_session_id.clone() {
        Some(agent_session_id) => super::probe::request(
            ProbeTarget {
                agent_session_id,
                cwd,
                zmx_name: crate::zmx::provider_zmx_session_name(session).ok(),
                model: model.clone(),
                transcript_len,
                resumable: facts
                    .as_ref()
                    .is_some_and(|facts| facts.conversation_started),
                working: resolution.working(),
                home: home.to_path_buf(),
            },
            mode,
        ),
        None => ProbeSnapshot::default(),
    };
    let (connectors, connectors_pending) = if progressive {
        super::connectors::read_connectors_progressive(
            state,
            project_id,
            session_id,
            facts.as_ref(),
            probe.mcp_status.as_ref(),
        )
    } else {
        (
            super::connectors::read_connectors(
                state,
                project_id,
                session_id,
                facts.as_ref(),
                probe.mcp_status.as_ref(),
            ),
            false,
        )
    };
    result["context"] = json!({
        "model": model,
        "live": live,
        "breakdown": probe.breakdown,
        "breakdownAt": probe.taken_at.map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        "breakdownRunning": probe.running,
        "breakdownError": probe.error,
        "refreshAt": probe.refresh_at.map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    });
    if !connectors_pending
        || connectors["servers"]
            .as_array()
            .is_some_and(|servers| !servers.is_empty())
        || !connectors["workspace"].is_null()
    {
        result["connectors"] = connectors;
    }
    if connectors_pending && !pending.contains(&"connectors") {
        pending.push("connectors");
    }
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
        let sources = if facts.skills.is_empty() {
            Some(std::collections::HashMap::new())
        } else if progressive {
            match super::skills::skill_sources_progressive(
                home,
                folder.map(std::path::Path::new),
                facts.skills.iter().map(|(name, _)| name.as_str()),
            ) {
                SectionLoad::Ready(sources) => Some(sources),
                SectionLoad::Pending => {
                    pending.push("skills");
                    None
                }
            }
        } else {
            Some(super::skills::skill_sources(
                home,
                folder.map(std::path::Path::new),
                facts.skills.iter().map(|(name, _)| name.as_str()),
            ))
        };
        if let Some(sources) = sources {
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
        }
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
        result["background"] = json!( {
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
    Ok(())
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
