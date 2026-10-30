//! Work tracker routes (crate::work_mode::tracker): which tracker a workspace or project uses and
//! changing it, creating a GitHub issue for a GitHub workspace, and closing a work-mode notice.
//! They answer with Workspaces off too: Issues Workflow does not need it (a project then uses the
//! workspace it is stored in, Personal for most).

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::work_mode::{
    dismiss_work_notice, github_projects_status_json, refresh_github_projects_access,
    remember_team_work_tracker, resolve_work_mode_project, stored_workspace_work_tracker,
    work_notice_dismissed, work_source, workspace_work_tracker, TicketDraft, TicketScope,
    WorkSourceId,
};
use crate::workspaces::{
    find_workspace_id, project_workspace_id, read_sidebar_workspaces, update_workspace_in,
    write_sidebar_workspaces,
};

use super::super::work_mode_sync::spawn_work_mode_refresh;
use super::*;

const TRACKER_ROUTES: &[&str] = &[
    "/api/readWorkTracker",
    "/api/setWorkTracker",
    "/api/createGithubIssue",
    "/api/dismissWorkNotice",
];

pub(super) async fn route_work_tracker_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if !TRACKER_ROUTES.contains(&request.endpoint.path.as_str()) {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    let path = endpoint.path.clone();
    let worker_state = state.clone();
    let worker_request_id = request_id.clone();
    // Off the async runtime: Convex and `gh` are network calls.
    let response = tokio::task::spawn_blocking(move || {
        let route = path.clone();
        handle_domain_http(
            &worker_state,
            path,
            worker_request_id,
            &body_json,
            |repository, db, params, _| match route.as_str() {
                "/api/readWorkTracker" => read_work_tracker(&worker_state, repository, db, params),
                "/api/setWorkTracker" => set_work_tracker(&worker_state, db, params),
                "/api/createGithubIssue" => create_issue(repository, params),
                _ => {
                    let notice = params
                        .get("notice")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    dismiss_work_notice(db, notice)?;
                    Ok(json!({ "notice": notice, "dismissed": true }))
                }
            },
        )
    })
    .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint.path.clone(),
            request_id,
            DomainStateError::corrupt_state(format!("Work tracker request failed: {error}")),
        ),
    };
    if endpoint.path == "/api/setWorkTracker" {
        // The pass republishes the projects whose tracker changed (work_mode_sync.rs).
        spawn_work_mode_refresh(&state);
    }
    Ok(response)
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `{ workspaceId } | { projectId | path }`, `force?` → `{ workspaceId, projectId, repo, tracker,
/// source, team, canEdit, githubProjects: { access, command, noticeDismissed } }`. A team workspace's choice is
/// read from Convex right away (and remembered), so Settings shows what the team picked.
fn read_work_tracker(
    state: &AppState,
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspaces = read_sidebar_workspaces(db)?;
    let (workspace_id, project_id, repo) = match text(params, "workspaceId") {
        Some(reference) => (
            find_workspace_id(&workspaces, &reference).ok_or_else(|| {
                DomainStateError::bad_request(format!("No workspace matched \"{reference}\"."))
            })?,
            None,
            None,
        ),
        None => {
            let project = resolve_work_mode_project(repository, params)?;
            let projects = repository.list_projects()?;
            (
                project_workspace_id(&workspaces, &project, &projects),
                project
                    .get("projectId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // Where a GitHub issue for this project goes (the Create ticket dialog says so).
                project
                    .get("path")
                    .and_then(Value::as_str)
                    .and_then(crate::work_mode::work_repo_of),
            )
        }
    };
    let force = params.get("force").and_then(Value::as_bool) == Some(true);
    let mut can_edit = true;
    let team = crate::team_sync::workspace_has_team(&state.paths, &workspace_id);
    if team {
        match crate::team_sync::read_team_flow(
            &state.paths,
            &crate::work_mode::workspace_params(&workspace_id),
        ) {
            Ok(flow) => {
                remember_team_work_tracker(
                    &workspace_id,
                    WorkSourceId::from_wire(flow.get("tracker")),
                );
                can_edit = flow.get("canEdit").and_then(Value::as_bool) != Some(false);
            }
            // Unreachable team: show what this computer knows and do not offer the switch.
            Err(_) => can_edit = false,
        }
    }
    let (tracker, source) = workspace_work_tracker(&state.paths, &workspaces, &workspace_id);
    let mut github_projects = github_projects_status_json();
    if tracker == WorkSourceId::Github && crate::session_git_status::gh_cli_is_available() {
        refresh_github_projects_access(force);
        github_projects = github_projects_status_json();
    }
    github_projects["noticeDismissed"] = json!(work_notice_dismissed(db, "githubProjectsScope"));
    Ok(json!({
        "workspaceId": workspace_id,
        "projectId": project_id,
        "repo": repo,
        "tracker": tracker.as_wire(),
        "source": source.as_wire(),
        "team": team,
        "canEdit": can_edit,
        "githubProjects": github_projects,
    }))
}

/// `{ workspaceId, tracker: "linear" | "github" }`. A team workspace saves it in the team's flow
/// settings (owners only; Convex refuses everyone else), and every workspace keeps it in this
/// computer's workspaces document too, so it still applies after leaving the team.
fn set_work_tracker(
    state: &AppState,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let current = read_sidebar_workspaces(db)?;
    let reference = text(params, "workspaceId")
        .ok_or_else(|| DomainStateError::bad_request("Pass workspaceId."))?;
    let workspace_id = find_workspace_id(&current, &reference).ok_or_else(|| {
        DomainStateError::bad_request(format!("No workspace matched \"{reference}\"."))
    })?;
    let tracker = WorkSourceId::from_wire(params.get("tracker")).ok_or_else(|| {
        DomainStateError::bad_request("tracker must be \"linear\" or \"github\".")
    })?;
    if crate::team_sync::workspace_has_team(&state.paths, &workspace_id) {
        let mut team_params = crate::work_mode::workspace_params(&workspace_id);
        team_params.insert("tracker".to_string(), json!(tracker.as_wire()));
        crate::team_sync::set_team_flow(&state.paths, &team_params)
            .map_err(DomainStateError::bad_request)?;
        remember_team_work_tracker(&workspace_id, Some(tracker));
    }
    if stored_workspace_work_tracker(&current, &workspace_id) != Some(tracker) {
        let mut patch = Map::new();
        patch.insert("tracker".to_string(), json!(tracker.as_wire()));
        let next = update_workspace_in(&current, &workspace_id, &patch)?;
        write_sidebar_workspaces(db, &next)?;
    }
    Ok(json!({ "workspaceId": workspace_id, "tracker": tracker.as_wire() }))
}

/// `{ projectId | path, title, description?, assignToMe? }` → `{ number, url, repo }`: a GitHub
/// issue in the project's repo, assigned to the person unless `assignToMe` is false.
fn create_issue(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project = resolve_work_mode_project(repository, params)?;
    let cwd = project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("This project has no folder."))?;
    let title = text(params, "title")
        .ok_or_else(|| DomainStateError::bad_request("An issue needs a title."))?;
    let body = text(params, "description");
    let created = work_source(WorkSourceId::Github)
        .create(
            &TicketScope {
                project_id: project
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                workspace_id: None,
                cwd: Some(cwd),
                credentials: None,
            },
            &TicketDraft {
                title: &title,
                description: body.as_deref(),
                assign_to_me: params.get("assignToMe").and_then(Value::as_bool) != Some(false),
                extra: &Map::new(),
            },
        )
        .map_err(DomainStateError::bad_request)?;
    Ok(json!({
        "number": created.ticket.id.parse::<u64>().ok(),
        "url": created.url,
        "repo": created.repo,
    }))
}
