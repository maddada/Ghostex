//! The "Link to" picker's suggestions and the merged-PR cleanup answer (crate::work_mode).
//!
//! SEE-ALSO: server/src/work_mode/candidates.rs and cleanup.rs (the rules), work_mode.rs in this
//! folder (`/api/setSessionWorkLinks`, which a pick in the picker writes through).

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    list_work_link_candidates, offers_work_cleanup, presentation_session_work, project_work_mode,
    work_cleanup_pull_request_key, work_cleanup_worktree_path, write_work_cleanup_answer,
    WorkLinkKind,
};

use super::*;

pub(super) async fn route_work_links_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    Ok(match endpoint.path.as_str() {
        // Off the async runtime: `gh` and Linear are network calls.
        "/api/listWorkLinkCandidates" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            let response = tokio::task::spawn_blocking(move || {
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |repository, _, params, _| {
                        let (project, session) = work_mode_session(repository, params)?;
                        let kind = params
                            .get("kind")
                            .and_then(Value::as_str)
                            .and_then(WorkLinkKind::parse)
                            .ok_or_else(|| {
                                DomainStateError::bad_request(
                                    "kind must be pullRequest, linearIssue, linearProject or githubIssue.",
                                )
                            })?;
                        let query = params
                            .get("query")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        Ok(list_work_link_candidates(
                            &worker_state.paths,
                            &project,
                            &session,
                            kind,
                            query,
                        ))
                    },
                )
            })
            .await;
            match response {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Listing suggestions failed: {error}")),
                ),
            }
        }
        "/api/answerWorkCleanup" => match answer_work_cleanup(&state, &body_json).await {
            Ok(result) => routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => {
                project_worktree_operation_error_response(endpoint.path, request_id, error)
            }
        },
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
            })
        }
    })
}

/// The project and session a request names; the project must have work mode on.
fn work_mode_session(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<(Value, Value), DomainStateError> {
    let project_id = required_param(params, "projectId")?;
    let session_id = required_param(params, "sessionId")?;
    let project = repository
        .get_project(&project_id)?
        .ok_or_else(|| DomainStateError::bad_request("No such project."))?;
    if !project_work_mode(&project) {
        return Err(DomainStateError::bad_request(
            "Work mode is off for this project.",
        ));
    }
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::bad_request("No such session."))?;
    Ok((project, session))
}

fn required_param(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}

fn open_repository_db(state: &AppState) -> Result<rusqlite::Connection, DomainStateError> {
    open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })
}

/// `{ projectId, sessionId, answer: "cleanUp" | "keep" }`. "Clean up" removes the session's
/// worktree (`remove_session_worktree`, what `/api/removeSessionWorktree` runs) and parks the
/// session the way the Park action does (`isParked`, then a sleep when "Sleep session when
/// parking" is on). A worktree with uncommitted changes is kept and the session still parks, so
/// nothing is lost. Either answer is recorded for the PR so the offer never comes back for it.
async fn answer_work_cleanup(
    state: &Arc<AppState>,
    body: &Value,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let params = read_domain_rpc_params(body)?;
    let answer = params
        .get("answer")
        .and_then(Value::as_str)
        .filter(|answer| matches!(*answer, "cleanUp" | "keep"))
        .ok_or_else(|| DomainStateError::bad_request("answer must be cleanUp or keep."))?
        .to_string();
    let (project_id, session_id, pull_request_key, worktree_path) = {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let (project, session) = work_mode_session(&repository, &params)?;
        let pull_request = presentation_session_work(&project, &session)
            .and_then(|work| work.get("pullRequest").cloned())
            .filter(|pull_request| {
                pull_request.get("state").and_then(Value::as_str) == Some("merged")
            })
            .ok_or_else(|| DomainStateError::bad_request("This session's PR is not merged."))?;
        let key = work_cleanup_pull_request_key(
            pull_request
                .get("number")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            pull_request.get("url").and_then(Value::as_str),
        );
        if !offers_work_cleanup(&session, &key) {
            return Ok(json!({ "answered": false }));
        }
        (
            value_text(&project, "projectId")?,
            value_text(&session, "sessionId")?,
            key,
            work_cleanup_worktree_path(&session),
        )
    };

    let mut removed_worktree = false;
    let mut kept_dirty_worktree = false;
    let mut warnings: Vec<Value> = Vec::new();
    // CDXC:WorkMode 2026-10-10 WHY: The agent still running in the worktree holds it as its
    // working folder, and Windows will not delete a folder in use: `git worktree remove` emptied
    // the checkout, failed on the folder itself, and that error ended the answer before the session
    // was parked, so Clean up left an empty folder, a running session and the offer. The session is
    // put to sleep before its worktree goes (it cannot run on in a deleted folder anyway), and a
    // removal that still fails is a warning: the session is parked either way.
    let mut slept = false;
    if answer == "cleanUp" {
        if let Some(worktree_path) = worktree_path {
            sleep_work_cleanup_session(state, &project_id, &session_id).await;
            slept = true;
            let mut remove_params = Map::new();
            remove_params.insert("projectId".to_string(), json!(project_id));
            remove_params.insert("worktreePath".to_string(), json!(worktree_path));
            match remove_session_worktree(state, &remove_params).await {
                Ok(removed) => {
                    removed_worktree =
                        removed.get("removed").and_then(Value::as_bool) == Some(true);
                    kept_dirty_worktree = !removed_worktree
                        && removed.get("dirty").and_then(Value::as_bool) == Some(true);
                    if let Some(more) = removed.get("warnings").and_then(Value::as_array) {
                        warnings.extend(more.iter().cloned());
                    }
                }
                Err(error) => warnings.push(json!(format!(
                    "The worktree could not be removed: {}",
                    worktree_error_text(&error)
                ))),
            }
        }
    }

    let parked = answer == "cleanUp";
    {
        let db = open_repository_db(state)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        if parked {
            let mut update = Map::new();
            update.insert("projectId".to_string(), json!(project_id));
            update.insert("sessionId".to_string(), json!(session_id));
            update.insert("isParked".to_string(), Value::Bool(true));
            repository.update_session(&update)?;
        }
        write_work_cleanup_answer(
            &db,
            &project_id,
            &session_id,
            &pull_request_key,
            &answer,
            &now_iso(),
        )?;
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
    }
    if parked && !slept && sleep_session_when_parking(state) {
        sleep_work_cleanup_session(state, &project_id, &session_id).await;
    }
    Ok(json!({
        "answered": true,
        "parked": parked,
        "removedWorktree": removed_worktree,
        "keptDirtyWorktree": kept_dirty_worktree,
        "warnings": warnings,
    }))
}

async fn sleep_work_cleanup_session(state: &Arc<AppState>, project_id: &str, session_id: &str) {
    let sleeper = state.clone();
    let (sleep_project_id, sleep_session_id) = (project_id.to_string(), session_id.to_string());
    let _ = tokio::task::spawn_blocking(move || {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(sleep_project_id));
        params.insert("sessionId".to_string(), json!(sleep_session_id));
        dispatch_zmx_lifecycle_http_blocking(
            &sleeper,
            "/api/sleepSession".to_string(),
            "work-cleanup".to_string(),
            params,
        )
    })
    .await;
}

fn worktree_error_text(error: &ProjectWorktreeOperationError) -> String {
    match error {
        ProjectWorktreeOperationError::Domain(error) => error.message.clone(),
        ProjectWorktreeOperationError::Typed(error) => error.message.clone(),
        ProjectWorktreeOperationError::ProjectPath(_) => {
            "the folder could not be read.".to_string()
        }
    }
}

/// The "Sleep session when parking" setting, which the sidebar's Park action honours too.
fn sleep_session_when_parking(state: &AppState) -> bool {
    std::fs::read_to_string(
        state
            .paths
            .app_config_dir
            .join("native-sidebar-settings.json"),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    .and_then(|settings| {
        settings
            .get("sleepSessionWhenParking")
            .and_then(Value::as_bool)
    })
    .unwrap_or(false)
}
