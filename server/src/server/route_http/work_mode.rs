//! Issues Workflow routes (crate::work_mode): a session's hand-set links, the Linear API key, a
//! status read for the CLI, and the retired per-project switch.

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    expand_added_work_links, linear_api_key_summary, merge_work_links, resolve_work_mode_project,
    store_linear_api_key, verify_linear_api_key, write_work_links, IssuesWorkflowParts,
    LinearKeyScope,
};

use super::super::work_mode_sync::spawn_work_mode_refresh;
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
        // CDXC:WorkMode 2026-10-24 WHY: Issues Workflow has no per-project switch any more (project.rs). Older clients (the phone's Work Mode row, `ghostex work-mode on|off` in old scripts) still call this, so it answers with what the project has now and stores nothing.
        "/api/setProjectWorkMode" => handle_domain_http(
            &state,
            endpoint.path,
            request_id,
            &body_json,
            |repository, _, params, _| {
                let project = resolve_work_mode_project(repository, params)?;
                let parts = crate::work_mode::issues_workflow_parts(&project);
                Ok(json!({
                    "projectId": value_text(&project, "projectId")?,
                    "projectName": project.get("name").cloned().unwrap_or(Value::Null),
                    "workMode": parts.any(),
                    "note": issues_workflow_note(parts),
                }))
            },
        ),
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
                    // A ticket command's links name only the command of the session's workspace,
                    // checked and written under the ticket command store's writer lock and the
                    // placement lock. Read again under them: a move rewrites the project's workspace.
                    let session_workspace = || {
                        let project = repository.get_project(&project_id)?.unwrap_or(Value::Null);
                        Ok(crate::workspaces::project_workspace_id(
                            &crate::workspaces::read_sidebar_workspaces(db)?,
                            &project,
                            &repository.list_projects_without_session_history()?,
                        ))
                    };
                    let (links, added) = crate::work_mode::with_checked_ticket_links(
                        params,
                        session_workspace,
                        db,
                        &state.paths,
                        |params| {
                            let added =
                                expand_added_work_links(&project, &session, existing, params)?;
                            let links = merge_work_links(
                                existing,
                                added.as_ref().map_or(params, |added| &added.request),
                            )?;
                            write_work_links(db, &project_id, &session_id, &links)?;
                            Ok((links, added))
                        },
                    )?;
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
                let gh = crate::session_git_status::gh_cli_is_available();
                let linear_keys = linear_api_key_summary(&worker_state.paths);
                let any_linear_key = linear_keys
                    .as_object()
                    .is_some_and(|keys| keys.values().any(|value| match value {
                        Value::Bool(set) => *set,
                        Value::Array(ids) => !ids.is_empty(),
                        _ => false,
                    }));
                json!({
                    "linearKeys": linear_keys,
                    "gh": gh,
                    "note": issues_workflow_note(IssuesWorkflowParts::from_setup(any_linear_key, gh)),
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
                    DomainStateError::corrupt_state(format!(
                        "Issues Workflow status failed: {error}"
                    )),
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

/// One line for the CLI on what Issues Workflow shows with this setup.
fn issues_workflow_note(parts: IssuesWorkflowParts) -> &'static str {
    match (parts.linear, parts.github) {
        (false, false) if parts.command => "Issues Workflow is on with the workspace's ticket command; add a Linear key (`ghostex issues linear-key`) or sign in with `gh auth login` for Linear tickets or pull requests.",
        (true, true) => "Issues Workflow is on: Linear and GitHub parts both show.",
        (true, false) => "Issues Workflow is on with Linear only; sign in with `gh auth login` for pull requests and GitHub issues.",
        (false, true) => "Issues Workflow is on with GitHub only; add a Linear key (`ghostex issues linear-key`) for Linear tickets.",
        (false, false) => "Issues Workflow is off until you add a Linear key (`ghostex issues linear-key`) or sign in with `gh auth login`.",
    }
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
