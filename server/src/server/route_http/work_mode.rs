//! Work mode routes (crate::work_mode): the per-project switch, a session's hand-set links, the
//! Linear API key, and a status read for the CLI.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    expand_added_work_links, linear_api_key_summary, merge_work_links, resolve_work_mode_project,
    set_project_work_mode, store_linear_api_key, verify_linear_api_key, write_work_links,
    LinearKeyScope,
};

use super::super::work_mode_sync::{publish_project_work_mode_change, spawn_work_mode_refresh};
use super::*;

pub(super) async fn route_work_mode_http(
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
        "/api/setProjectWorkMode" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let enabled =
                        params
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .ok_or_else(|| {
                                DomainStateError::bad_request("Pass enabled: true or false.")
                            })?;
                    let project = resolve_work_mode_project(repository, params)?;
                    let project_id = value_text(&project, "projectId")?;
                    let project = if crate::work_mode::stored_project_work_mode(&project) == enabled
                        && crate::workspaces::project_work_mode_set_by_hand(&project)
                    {
                        project
                    } else {
                        set_project_work_mode(repository, &project, enabled)?
                    };
                    publish_project_work_mode_change(&state, db, repository, &project_id)?;
                    Ok(json!({
                        "projectId": project_id,
                        "projectName": project.get("name").cloned().unwrap_or(Value::Null),
                        "workMode": enabled,
                    }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        "/api/setSessionWorkLinks" => {
            let response = handle_domain_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
                |repository, db, params, _| {
                    let project_id = required_text(params, "projectId")?;
                    let session_id = required_text(params, "sessionId")?;
                    let session = repository
                        .get_session(&project_id, &session_id)?
                        .ok_or_else(|| DomainStateError::bad_request("No such session."))?;
                    let existing = session
                        .get("runtimeSettings")
                        .and_then(|settings| settings.get("workLinks"))
                        .and_then(Value::as_object);
                    // `add…` keys (the Work page's Link to current session) join the session's
                    // effective links and answer with the request that takes them back.
                    let project = repository.get_project(&project_id)?.unwrap_or(Value::Null);
                    let added = expand_added_work_links(&project, &session, existing, params)?;
                    let links = merge_work_links(
                        existing,
                        added.as_ref().map_or(params, |added| &added.request),
                    )?;
                    write_work_links(db, &project_id, &session_id, &links)?;
                    schedule_presentation_session_delta(
                        &state,
                        db,
                        repository,
                        &project_id,
                        &session_id,
                    )?;
                    Ok(json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "workLinks": Value::Object(links),
                        "undo": added.map(|added| Value::Object(added.undo)),
                    }))
                },
            );
            spawn_work_mode_refresh(&state);
            response
        }
        // Off the async runtime: checking a key is a network call to Linear.
        "/api/setLinearApiKey" => {
            // A workspace's (or the shared) key is what a team member's own Linear user and own
            // key come from; a project's override is not.
            let project_scoped = body_json
                .pointer("/params/projectId")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.trim().is_empty());
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            let response = tokio::task::spawn_blocking(move || {
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |_, _, params, _| set_linear_api_key(&worker_state, params),
                )
            })
            .await;
            spawn_work_mode_refresh(&state);
            if !project_scoped {
                crate::team_sync::spawn_member_linear_key_sync(&state, None);
            }
            match response {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!(
                        "Saving the Linear key failed: {error}"
                    )),
                ),
            }
        }
        "/api/readWorkModeStatus" => {
            let worker_state = state.clone();
            match tokio::task::spawn_blocking(move || {
                json!({
                    "linearKeys": linear_api_key_summary(&worker_state.paths),
                    "gh": crate::session_git_status::gh_cli_is_available(),
                })
            })
            .await
            {
                Ok(status) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, status),
                ),
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Work mode status failed: {error}")),
                ),
            }
        }
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

/// Checks a key with Linear before storing it, so a typo fails here instead of as empty cards.
/// An empty or missing `apiKey` removes the key. `projectId` sets a project's override,
/// `workspaceId` a workspace's key, neither the shared key.
fn set_linear_api_key(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
    };
    let project_id = text("projectId");
    let workspace_id = text("workspaceId");
    let scope = match (project_id, workspace_id) {
        (Some(project_id), _) => LinearKeyScope::Project(project_id),
        (None, Some(workspace_id)) => LinearKeyScope::Workspace(workspace_id),
        (None, None) => LinearKeyScope::Shared,
    };
    let api_key = params
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty());
    let account = match api_key {
        Some(key) => Some(verify_linear_api_key(key).map_err(DomainStateError::bad_request)?),
        None => None,
    };
    store_linear_api_key(&state.paths, scope, api_key).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the Linear key: {error}"),
    })?;
    Ok(json!({
        "configured": api_key.is_some(),
        "projectId": project_id,
        "workspaceId": workspace_id,
        "account": account,
    }))
}

fn required_text(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("Pass {key}.")))
}
