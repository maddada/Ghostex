//! Work mode ticket routes (crate::work_mode): start work on a Linear or GitHub ticket, create a
//! Linear ticket, and the teams and Linear projects the Create Linear ticket dialog offers.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    default_linear_team_id, list_linear_projects, list_linear_teams, project_linear_identifiers,
    resolve_work_mode_project, start_work_on_ticket, work_source, TicketDraft, TicketScope,
    WorkCredentials, WorkSourceId,
};

use super::super::work_mode_sync::spawn_work_mode_refresh;
use super::*;

pub(super) async fn route_work_tickets_http(
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
        "/api/startWorkOnTicket" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            let result = start_work_on_ticket(&state, &params).await;
            spawn_work_mode_refresh(&state);
            match result {
                Ok(result) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => {
                    project_worktree_operation_error_response(endpoint.path, request_id, error)
                }
            }
        }
        // Off the async runtime: each of these is a network call to Linear.
        "/api/createLinearIssue" | "/api/listLinearTeams" | "/api/listLinearProjects" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            let response = tokio::task::spawn_blocking(move || {
                let path = worker_endpoint.clone();
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |_, _, params, _| match path.as_str() {
                        "/api/createLinearIssue" => create_linear_ticket(&worker_state, params),
                        "/api/listLinearTeams" => linear_teams(&worker_state, params),
                        _ => linear_projects(&worker_state, params),
                    },
                )
            })
            .await;
            match response {
                Ok(response) => response,
                Err(error) => domain_error_response(
                    endpoint.path,
                    request_id,
                    DomainStateError::corrupt_state(format!("Linear request failed: {error}")),
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

/// The project a request names, its Linear key, and the credentials that key was read from, so
/// one request uses one key throughout.
fn project_linear_key(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(Value, String, String, WorkCredentials), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let project = resolve_work_mode_project(
        &DomainRepository::new(&db, state.metadata.server_id.as_str()),
        params,
    )?;
    let project_id = value_text(&project, "projectId")?;
    let credentials = WorkCredentials::new(&state.paths);
    let api_key = credentials
        .linear_api_key(
            project.get("projectId").and_then(Value::as_str),
            crate::workspaces::stored_project_workspace_id(&project),
        )
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Set a Linear API key first (Settings, or ghostex issues linear-key).",
            )
        })?;
    Ok((project, project_id, api_key, credentials))
}

/// `{ teams: [{ id, key, name }], defaultTeamId }`.
fn linear_teams(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let (_, project_id, api_key, _) = project_linear_key(state, params)?;
    let teams = list_linear_teams(&api_key).map_err(DomainStateError::bad_request)?;
    let used = project_linear_identifiers(state, &project_id)?;
    Ok(json!({
        "teams": teams,
        "defaultTeamId": default_linear_team_id(&teams, &used),
    }))
}

/// `{ projects: [{ id, name }] }`: open Linear projects, of one team when `teamId` is given.
fn linear_projects(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (_, _, api_key, _) = project_linear_key(state, params)?;
    let team_id = text(params, "teamId");
    let projects = list_linear_projects(&api_key, team_id.as_deref())
        .map_err(DomainStateError::bad_request)?;
    Ok(json!({ "projects": projects }))
}

/// `{ identifier, url, branchName }`. With no `teamId` the ticket goes to the team this
/// project's sessions use most.
fn create_linear_ticket(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (project, project_id, api_key, credentials) = project_linear_key(state, params)?;
    let title = text(params, "title")
        .ok_or_else(|| DomainStateError::bad_request("A ticket needs a title."))?;
    let team_id = match text(params, "teamId") {
        Some(team_id) => team_id,
        None => {
            let teams = list_linear_teams(&api_key).map_err(DomainStateError::bad_request)?;
            let used = project_linear_identifiers(state, &project_id)?;
            default_linear_team_id(&teams, &used).ok_or_else(|| {
                DomainStateError::bad_request("This Linear workspace has no teams.")
            })?
        }
    };
    let description = text(params, "description");
    let mut extra = Map::new();
    extra.insert("teamId".to_string(), json!(team_id));
    if let Some(linear_project_id) = text(params, "linearProjectId") {
        extra.insert("linearProjectId".to_string(), json!(linear_project_id));
    }
    let created = work_source(WorkSourceId::Linear)
        .create(
            &TicketScope {
                project_id: &project_id,
                workspace_id: crate::workspaces::stored_project_workspace_id(&project),
                cwd: None,
                credentials: Some(&credentials),
            },
            &TicketDraft {
                title: &title,
                description: description.as_deref(),
                assign_to_me: params.get("assignToMe").and_then(Value::as_bool) != Some(false),
                extra: &extra,
            },
        )
        .map_err(DomainStateError::bad_request)?;
    Ok(json!({
        "identifier": created.ticket.id,
        "url": created.url,
        "branchName": created.branch_name,
    }))
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
