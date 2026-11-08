//! `/api/autopilot`: Settings > Autopilot, the chat pills, the sidebar's project menu and
//! `ghostex autopilot` read and change Autopilot here.
//!
//! - `op: "read"` (default): the choices, the projects and the sessions set one by one, the
//!   summary since the switch went on, and the newest log lines (`limit`, 200 by default).
//! - `op: "set"`: `patch` holds the choices to change (`enabled`, `scope`, `tier`, `limits`,
//!   `answerQuestions`, `answerAfterMinutes`, `answerApprovals`, `supervisor`, `sweepSeconds`).
//! - `op: "setProject"`: `projectId` and `on`; a worktree's main project is changed instead.
//! - `op: "setSession"`: `projectId`, `sessionId` and `on` (`toggle: true` flips it).
//! - `op: "decide"`: the supervisor's decision for one session (`ghostex autopilot decide`):
//!   `projectId`, `sessionId`, and `answers` (one pick per open question, in order), `message`
//!   (sent to it as the user's) or `leave` (a note: left for the user). Ghostex applies it and logs
//!   it; the supervisor never answers a prompt or types into a session itself.
//!
//! Every change republishes the projects (the sidebar's Advanced > Autopilot row) and the open
//! chats (their pill).

use super::*;

use crate::autopilot::{self, AutopilotConfig};

pub(crate) const AUTOPILOT_ENDPOINTS: &[&str] = &["/api/autopilot"];

pub(crate) async fn handle_autopilot_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", error.message, Some(request_id)),
            );
        }
    };
    let result = if params.get("op").and_then(Value::as_str) == Some("decide") {
        decide(state, &params).await
    } else {
        handle(state, &params)
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            if error.code == "badRequest" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn saved(result: std::io::Result<AutopilotConfig>) -> Result<AutopilotConfig, DomainStateError> {
    result.map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Ghostex could not save Autopilot's settings: {error}"),
    })
}

fn handle(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let op = text(params, "op").unwrap_or_else(|| "read".to_string());
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: error.to_string(),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let before = autopilot::read_config(&state.paths);
    let after = match op.as_str() {
        "read" => None,
        "set" => {
            let patch = params
                .get("patch")
                .cloned()
                .ok_or_else(|| DomainStateError::bad_request("patch is required."))?;
            Some(saved(autopilot::update_config(&state.paths, |config| {
                autopilot::apply_patch(config, &patch)
            }))?)
        }
        "setProject" => {
            let project_id = text(params, "projectId")
                .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
            let on = params
                .get("on")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("on is required (true or false)."))?;
            let project = repository
                .get_project(&project_id)?
                .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
            let main = autopilot::main_project_of(&project).unwrap_or(project_id);
            Some(saved(autopilot::update_config(&state.paths, |config| {
                config.set_project(&main, on)
            }))?)
        }
        "setSession" => {
            let project_id = text(params, "projectId")
                .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
            let session_id = text(params, "sessionId")
                .ok_or_else(|| DomainStateError::bad_request("sessionId is required."))?;
            let main = repository
                .get_project(&project_id)?
                .as_ref()
                .and_then(autopilot::main_project_of);
            let toggle = params.get("toggle").and_then(Value::as_bool) == Some(true);
            let on = match params.get("on").and_then(Value::as_bool) {
                Some(on) => on,
                None if toggle => !before.covers(&project_id, main.as_deref(), &session_id),
                None => {
                    return Err(DomainStateError::bad_request(
                        "on is required (true or false), or toggle: true.",
                    ))
                }
            };
            Some(saved(autopilot::update_config(&state.paths, |config| {
                config.set_session(&project_id, main.as_deref(), &session_id, on)
            }))?)
        }
        other => {
            return Err(DomainStateError::bad_request(format!(
                "Unknown op \"{other}\": use read, set, setProject or setSession."
            )))
        }
    };
    if let Some(after) = &after {
        if *after != before {
            if let Some(summary) = autopilot_widening(&before, after) {
                crate::audit::record(
                    crate::audit::Action::new("autopilot.widen", summary),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Done,
                    None,
                );
            }
            publish_change(state, &db, &repository, &before, after);
        }
    }
    let config = after.unwrap_or(before);
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .min(2_000) as usize;
    Ok(read_view(state, &repository, &config, limit))
}

/// The security log's sentence for a change that lets Autopilot do more (turned on, answers
/// approvals, a wider tier, a hard limit it may now cross, more projects or sessions); `None` when
/// the change only narrows it.
fn autopilot_widening(before: &AutopilotConfig, after: &AutopilotConfig) -> Option<String> {
    let mut changes = Vec::new();
    if after.enabled && !before.enabled {
        changes.push("turned on Autopilot".to_string());
    }
    if after.answer_approvals && !before.answer_approvals {
        changes.push("Autopilot may answer approvals".to_string());
    }
    if after.tier != before.tier {
        changes.push(format!("Autopilot's level set to {:?}", after.tier));
    }
    let limits = |config: &AutopilotConfig| {
        serde_json::to_value(&config.limits)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    };
    let old_limits = limits(before);
    for (limit, allowed) in limits(after) {
        if allowed == Value::Bool(true) && old_limits.get(&limit) != Some(&Value::Bool(true)) {
            changes.push(format!("Autopilot may now {limit}"));
        }
    }
    let added = after
        .projects
        .keys()
        .chain(after.sessions.keys())
        .filter(|key| !before.projects.contains_key(*key) && !before.sessions.contains_key(*key))
        .count();
    if added > 0 {
        changes.push(format!(
            "Autopilot covers {added} more project(s) or session(s)"
        ));
    }
    let mut summary = changes.join(", ");
    let first = summary.chars().next()?.to_uppercase().to_string();
    summary.replace_range(..1, &first);
    Some(summary)
}

/// What Settings > Autopilot and `ghostex autopilot status` show.
fn read_view(
    state: &AppState,
    repository: &DomainRepository<'_>,
    config: &AutopilotConfig,
    limit: usize,
) -> Value {
    let projects = repository.list_projects().unwrap_or_default();
    let name_of = |project_id: &str| {
        projects
            .iter()
            .find(|project| project["projectId"].as_str() == Some(project_id))
            .and_then(|project| project["name"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    let project_rows: Vec<Value> = projects
        .iter()
        .filter(|project| autopilot::main_project_of(project).is_none())
        .filter(|project| project["isRecentProject"].as_bool() != Some(true))
        .filter_map(|project| {
            let project_id = project["projectId"].as_str()?;
            Some(json!({
                "projectId": project_id,
                "name": project["name"],
                "path": project["path"],
                "in": config.project_in(project_id, None),
            }))
        })
        .collect();
    let session_rows: Vec<Value> = config
        .sessions
        .iter()
        .filter_map(|(key, membership)| {
            let (project_id, session_id) = key.split_once(':')?;
            let session = repository.get_session(project_id, session_id).ok()??;
            Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "title": session["title"],
                "project": name_of(project_id),
                "on": *membership == autopilot::Membership::On,
            }))
        })
        .collect();
    let since_ms = config
        .enabled_at
        .as_deref()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000);
    let mut config_value = serde_json::to_value(config).unwrap_or_default();
    if let Some(object) = config_value.as_object_mut() {
        object.remove("projects");
        object.remove("sessions");
    }
    json!({
        "config": config_value,
        "projects": project_rows,
        "sessions": session_rows,
        "summary": autopilot::summary(&state.paths, since_ms),
        "log": autopilot::read_log(&state.paths, limit),
        "watching": super::autopilot_runtime::autopilot_runtime_watching(),
        "supervisor": super::autopilot_judge::supervisor_view(&state.paths),
    })
}

/// Republishes what shows Autopilot: every project row (the sidebar menu's row comes and goes
/// with the switch and the scope) and every running session's open chat (its pill).
fn publish_change(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    before: &AutopilotConfig,
    after: &AutopilotConfig,
) {
    let projects_changed = before.enabled != after.enabled
        || before.scope != after.scope
        || before.projects != after.projects;
    if projects_changed {
        for project in repository.list_projects().unwrap_or_default() {
            if let Some(project_id) = project["projectId"].as_str() {
                let _ = schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    project_id,
                    "projectUpdated",
                );
            }
        }
    }
    republish_open_chats(state, repository);
}

/// Re-sends every running session's open chat, so its Autopilot pill follows.
pub(super) fn republish_open_chats(state: &AppState, repository: &DomainRepository<'_>) {
    if let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") {
        for session in sessions {
            if let (Some(project_id), Some(session_id)) =
                (session["projectId"].as_str(), session["sessionId"].as_str())
            {
                super::coordinator_runtime::republish_session_chat(state, project_id, session_id);
            }
        }
    }
}

/// Applies the supervisor's decision for one session and logs it.
async fn decide(
    state: &Arc<AppState>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId")
        .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
    let session_id = text(params, "sessionId")
        .ok_or_else(|| DomainStateError::bad_request("sessionId is required."))?;
    let key = (project_id.clone(), session_id.clone());
    let answers: Vec<String> = params
        .get("answers")
        .and_then(Value::as_array)
        .map(|answers| {
            answers
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|answer| !answer.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let message = text(params, "message");
    let leave = text(params, "leave");
    let (title, project) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| DomainStateError::not_found("Session not found."))?;
        let project = repository
            .get_project(&project_id)?
            .and_then(|project| project["name"].as_str().map(str::to_string))
            .unwrap_or_default();
        (
            session["title"].as_str().unwrap_or_default().to_string(),
            project,
        )
    };
    let failed = |message: String| DomainStateError {
        code: "invalidState",
        message,
    };
    let did = if !answers.is_empty() {
        super::autopilot_runtime::answer_with_picks(state, &key, &answers)
            .await
            .map_err(failed)?
    } else if let Some(message) = message {
        super::autopilot_runtime::send_text(
            state,
            &key,
            &format!("Ghostex: [Autopilot] {message}"),
        )
        .await
        .map_err(failed)?;
        format!("Told it: \"{message}\"")
    } else if let Some(note) = leave.clone() {
        format!("Left it for you: {note}")
    } else {
        return Err(DomainStateError::bad_request(
            "Give --answer (once per question), --message or --leave.",
        ));
    };
    autopilot::append_log(
        &state.paths,
        &autopilot::LogEntry {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            project_id,
            session_id,
            title,
            project,
            saw: "The supervisor looked at it.".to_string(),
            did: did.clone(),
            rule: "supervisor".to_string(),
            outcome: if leave.is_some() && answers.is_empty() {
                autopilot::Outcome::Parked
            } else {
                autopilot::Outcome::Done
            },
        },
    );
    super::autopilot_judge::forget_session(&key);
    Ok(json!({ "did": did }))
}
