//! Work mode ticket routes (crate::work_mode): start work on a Linear or GitHub ticket, create a
//! Linear ticket or a ticket command's ticket, and the teams and Linear projects the Create Linear
//! ticket dialog offers.

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{
    default_linear_team_id, list_linear_projects, list_linear_teams, project_linear_identifiers,
    project_work_tracker, resolve_work_mode_project, start_work_on_ticket, work_repo_of,
    work_source, CommandProject, CommandVerb, CreateOutcome, TicketDraft, TicketScope,
    WorkCredentials, WorkSourceId, CREATE_WRITER_WAIT,
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
        // Off the async runtime: each of these is a network call to Linear, or runs a ticket
        // command's `create`.
        "/api/createLinearIssue"
        | "/api/listLinearTeams"
        | "/api/listLinearProjects"
        | "/api/createWorkTicket" => {
            let worker_state = state.clone();
            let worker_endpoint = endpoint.path.clone();
            let worker_request_id = request_id.clone();
            // Before the request waits for a blocking worker: a create may start only within
            // `CREATE_WRITER_WAIT` of its arrival.
            let admit_by = std::time::Instant::now() + CREATE_WRITER_WAIT;
            let response = tokio::task::spawn_blocking(move || {
                let path = worker_endpoint.clone();
                handle_domain_http(
                    &worker_state,
                    worker_endpoint,
                    worker_request_id,
                    &body_json,
                    |_, _, params, _| match path.as_str() {
                        "/api/createLinearIssue" => create_linear_ticket(&worker_state, params),
                        "/api/createWorkTicket" => create_command_ticket(
                            &worker_state,
                            params,
                            CommandVerb::Create.timeout(),
                            admit_by,
                        ),
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

/// A project whose workspace's tracker is a ticket command makes that command's tickets, which
/// `/api/createWorkTicket` creates: the Create ticket window, which knows only Linear and GitHub,
/// still sends it here, and is refused.
fn refuse_command_tracker(
    state: &AppState,
    project: &Value,
    project_id: &str,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let workspaces = crate::workspaces::read_sidebar_workspaces(&db)?;
    let projects = DomainRepository::new(&db, state.metadata.server_id.as_str()).list_projects()?;
    let tracker = project_work_tracker(&state.paths, &workspaces, project, &projects);
    if !matches!(tracker, WorkSourceId::Command(_)) {
        return Ok(());
    }
    let workspace_id = crate::workspaces::project_workspace_id(&workspaces, project, &projects);
    let scope = TicketScope {
        project_id,
        effective_workspace_id: Some(&workspace_id),
        ..TicketScope::default()
    };
    Err(DomainStateError::bad_request(format!(
        "Ghostex can't create {} tickets yet.",
        work_source(tracker).name(&scope)
    )))
}

/// The project a request names, its Linear key, and the credentials that key was read from, so
/// one request uses one key throughout.
fn project_linear_key(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(Value, String, String, WorkCredentials), DomainStateError> {
    let (project, project_id) = request_project(state, params)?;
    let (api_key, credentials) = linear_key_of(state, &project)?;
    Ok((project, project_id, api_key, credentials))
}

/// The project a request names, and its id.
fn request_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(Value, String), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let project = resolve_work_mode_project(
        &DomainRepository::new(&db, state.metadata.server_id.as_str()),
        params,
    )?;
    let project_id = value_text(&project, "projectId")?;
    Ok((project, project_id))
}

/// The project's Linear key, and the credentials it was read from.
fn linear_key_of(
    state: &AppState,
    project: &Value,
) -> Result<(String, WorkCredentials), DomainStateError> {
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
    Ok((api_key, credentials))
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
    // Before the Linear key: a command tracker's workspace often has none, and asking for one
    // would send the person to set up the wrong tracker.
    let (project, project_id) = request_project(state, params)?;
    refuse_command_tracker(state, &project, &project_id)?;
    let (api_key, credentials) = linear_key_of(state, &project)?;
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
                effective_workspace_id: None,
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

/// `{ projectId | path, source, title, description?, assignToMe? }` → `{ source, id, label, url,
/// branchName }` for the ticket the workspace's ticket command `source` created, or `{ source,
/// error, detail, mayHaveBeenCreated }` when it ran and failed, which the log gets as one line
/// without its words: an answer, not a request error, since the command's text would reach the
/// request log. A request that runs nothing is refused in Ghostex's own words, as is one that can't
/// start by `admit_by` (`run_create`).
pub(crate) fn create_command_ticket(
    state: &AppState,
    params: &Map<String, Value>,
    timeout: std::time::Duration,
    admit_by: std::time::Instant,
) -> Result<Value, DomainStateError> {
    let Some(WorkSourceId::Command(source)) = WorkSourceId::from_wire(params.get("source")) else {
        return Err(DomainStateError::bad_request(
            "Pass source: the id of the workspace's ticket command. Linear tickets and GitHub issues have their own routes.",
        ));
    };
    let title = text(params, "title")
        .ok_or_else(|| DomainStateError::bad_request("A ticket needs a title."))?;
    let description = text(params, "description");
    let (project, project_id) = request_project(state, params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let workspace_id = crate::workspaces::project_workspace_id(
        &crate::workspaces::read_sidebar_workspaces(&db)?,
        &project,
        &DomainRepository::new(&db, state.metadata.server_id.as_str()).list_projects()?,
    );
    let field = |key: &str| project.get(key).and_then(Value::as_str).map(str::to_string);
    let path = field("path");
    let credentials = WorkCredentials::new(&state.paths);
    let scope = TicketScope {
        project_id: &project_id,
        workspace_id: crate::workspaces::stored_project_workspace_id(&project),
        effective_workspace_id: Some(&workspace_id),
        cwd: path.as_deref(),
        credentials: Some(&credentials),
    };
    let command_project = CommandProject {
        id: project_id.clone(),
        name: field("name").unwrap_or_default(),
        repo: path
            .as_deref()
            .and_then(work_repo_of)
            .map(|repo| repo.to_ascii_lowercase()),
        path: path.clone(),
    };
    let extra = Map::new();
    let draft = TicketDraft {
        title: &title,
        description: description.as_deref(),
        assign_to_me: params.get("assignToMe").and_then(Value::as_bool) != Some(false),
        extra: &extra,
    };
    Ok(
        match source
            .source()
            .create_ticket(&scope, &draft, &command_project, timeout, admit_by)
            .map_err(DomainStateError::bad_request)?
        {
            CreateOutcome::Created(created) => json!({
                "source": source.as_str(),
                "label": created.id,
                "id": created.id,
                "url": created.url,
                "branchName": created.branch_name,
            }),
            CreateOutcome::Failed {
                error,
                detail,
                may_have_been_created,
            } => json!({
                "source": source.as_str(),
                "error": error,
                "detail": detail,
                "mayHaveBeenCreated": may_have_been_created,
            }),
        },
    )
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
