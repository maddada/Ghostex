//! The Work page's routes (crate::work_mode): its list, one ticket's details, and the team flow
//! the details draw as a tracker.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::team_sync::{
    apply_team_ticket_summaries, read_team_flow_steps, refresh_team_ticket_summaries,
    store_team_flow_steps, team_tickets_by_workspace, TeamFlowSteps,
};
use crate::work_mode::{
    build_work_list, default_team_flow_steps, load_work_projects, read_work_item,
    refresh_work_feeds, resolve_team_flow, share_work_item_with_sidebar, store_team_flow,
    team_flow_rule_catalog, validate_team_flow_steps, work_feed_plan, work_feeds_stale,
    TeamFlowScope, WorkItemRef,
};

use super::super::work_mode_sync::spawn_work_mode_refresh;

use super::*;

/// How long the list waits for Linear and `gh`. Past it, the page gets what the caches hold and
/// `refreshing: true`, and asks again a moment later while the fetch finishes on its own.
const WORK_LIST_WAIT: Duration = Duration::from_secs(8);
/// How long one ticket's details may take before the page gets an error it can retry.
const WORK_ITEM_WAIT: Duration = Duration::from_secs(20);
/// How long the list waits for a Work workspace's team to count each row's Slack threads.
const TEAM_SUMMARY_WAIT: Duration = Duration::from_secs(3);

pub(super) async fn route_work_items_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
    } = request;
    if !matches!(
        endpoint.path.as_str(),
        "/api/listWorkItems" | "/api/readWorkItem" | "/api/readTeamFlow" | "/api/updateTeamFlow"
    ) {
        return Err(RouteHttpRequest {
            state,
            endpoint,
            request_id,
            body_json,
            token_extension_id,
        });
    }
    let params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
    };
    let result = match endpoint.path.as_str() {
        "/api/listWorkItems" => list_work_items(state.clone(), params).await,
        "/api/readWorkItem" => read_work_item_route(state.clone(), params).await,
        "/api/readTeamFlow" => run_blocking(state.clone(), params, read_team_flow).await,
        _ => run_blocking(state.clone(), params, update_team_flow).await,
    };
    Ok(match result {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}

fn project_ids(params: &Map<String, Value>) -> Option<Vec<String>> {
    params
        .get("projectIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn task_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError::corrupt_state(format!("Issues view request failed: {error}"))
}

async fn run_blocking(
    state: Arc<AppState>,
    params: Map<String, Value>,
    work: fn(&AppState, &Map<String, Value>) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    tokio::task::spawn_blocking(move || work(&state, &params))
        .await
        .map_err(task_error)?
}

/// `{ projectIds?, force? }` → the list (see `build_work_list`) plus `refreshing`.
async fn list_work_items(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let wanted = project_ids(&params);
    let force = params
        .get("force")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let load_state = state.clone();
    let (projects, plan) = tokio::task::spawn_blocking(move || {
        let projects = load_work_projects(&load_state, wanted.as_deref())?;
        let plan = work_feed_plan(&projects);
        Ok::<_, DomainStateError>((projects, plan))
    })
    .await
    .map_err(task_error)??;
    let mut refreshing = false;
    if force || work_feeds_stale(&plan) {
        let refresh_plan = plan.clone();
        let refresh = tokio::task::spawn_blocking(move || {
            refresh_work_feeds(&refresh_plan, force);
        });
        refreshing = tokio::time::timeout(WORK_LIST_WAIT, refresh).await.is_err();
    }
    let mut list = {
        let projects = projects.clone();
        tokio::task::spawn_blocking(move || build_work_list(&projects, &plan))
            .await
            .map_err(task_error)?
    };
    // A Work workspace's team adds each row's Slack-thread count; a slow team only delays the
    // counts, which the next read picks up from the cache.
    let groups = team_tickets_by_workspace(&projects, &list);
    if !groups.is_empty() {
        let paths = state.paths.clone();
        let refresh = tokio::task::spawn_blocking(move || {
            refresh_team_ticket_summaries(&paths, &groups, force);
        });
        refreshing |= tokio::time::timeout(TEAM_SUMMARY_WAIT, refresh)
            .await
            .is_err();
        let paths = state.paths.clone();
        list = tokio::task::spawn_blocking(move || {
            apply_team_ticket_summaries(&paths, &projects, &mut list);
            list
        })
        .await
        .map_err(task_error)?;
    }
    list["refreshing"] = json!(refreshing);
    // GitHub Projects need the `read:project` scope; a GitHub workspace's page shows a closable
    // notice with the command while it is missing (crate::work_mode::github_projects).
    let paths = state.paths.clone();
    list["githubProjects"] = tokio::task::spawn_blocking(move || {
        let mut status = crate::work_mode::github_projects_status_json();
        status["noticeDismissed"] = json!(crate::storage::open_gxserver_database(&paths)
            .ok()
            .is_some_and(|db| crate::work_mode::work_notice_dismissed(&db, "githubProjectsScope")));
        status
    })
    .await
    .map_err(task_error)?;
    Ok(list)
}

/// `{ projectId?, projectIds?, source + ticketId | pullRequest, force? }`.
async fn read_work_item_route(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let item_ref = WorkItemRef::from_params(&params).map_err(DomainStateError::bad_request)?;
    let refresh_state = state.clone();
    let task = tokio::task::spawn_blocking(move || {
        let project_id = params
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let mut wanted = project_ids(&params);
        if let (Some(wanted), Some(project_id)) = (wanted.as_mut(), project_id.as_ref()) {
            if !wanted.contains(project_id) {
                wanted.push(project_id.clone());
            }
        }
        let force = params
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let projects = load_work_projects(&state, wanted.as_deref())?;
        let plan = work_feed_plan(&projects);
        let details = read_work_item(
            &state.paths,
            &projects,
            &plan,
            project_id.as_deref(),
            &item_ref,
            force,
        );
        let shared = share_work_item_with_sidebar(&projects, &details);
        Ok::<_, DomainStateError>((details, shared))
    });
    match tokio::time::timeout(WORK_ITEM_WAIT, task).await {
        Ok(result) => {
            let (details, shared) = result.map_err(task_error)??;
            // The cards linking this ticket catch up with what the details just showed.
            if shared {
                spawn_work_mode_refresh(&refresh_state);
            }
            Ok(details)
        }
        Err(_) => Err(DomainStateError::bad_request(
            "Linear or GitHub took too long to answer. Try again.",
        )),
    }
}

/// The workspace a scope's steps come from: the workspace itself, or the project's (a worktree
/// project follows its parent checkout).
fn scope_workspace_id(
    state: &AppState,
    scope: &TeamFlowScope,
) -> Result<Option<String>, DomainStateError> {
    Ok(match scope {
        TeamFlowScope::Workspace(id) => Some(id.clone()),
        TeamFlowScope::Project(id) => load_work_projects(state, Some(std::slice::from_ref(id)))?
            .into_iter()
            .find(|project| &project.project_id == id)
            .map(|project| project.workspace_id),
        TeamFlowScope::Default => None,
    })
}

/// The answer for a workspace connected to a team: the team's steps (the default flow while it
/// has none) and whether this member may change them.
fn team_answer(steps: TeamFlowSteps) -> Value {
    let source = if steps.steps.is_some() {
        "team"
    } else {
        "builtIn"
    };
    json!({
        "steps": steps.steps.unwrap_or_else(default_team_flow_steps),
        "source": source,
        "team": true,
        "canEdit": steps.can_edit,
        "rules": team_flow_rule_catalog(),
    })
}

/// `{ projectId? | workspaceId? | scope: "default" }` → the steps that scope uses, where they come
/// from, and the rules a step can use. In a workspace connected to a team: the team's steps,
/// `team: true` and `canEdit`.
fn read_team_flow(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let scope = TeamFlowScope::from_params(params);
    let workspace_id = scope_workspace_id(state, &scope)?;
    if let Some(team) = workspace_id
        .as_deref()
        .and_then(|id| read_team_flow_steps(&state.paths, id))
    {
        return team.map(team_answer).map_err(DomainStateError::bad_request);
    }
    let project_id = match &scope {
        TeamFlowScope::Project(id) => Some(id.as_str()),
        _ => None,
    };
    let (steps, source) = resolve_team_flow(&state.paths, project_id, workspace_id.as_deref());
    Ok(json!({
        "steps": steps,
        "source": source,
        "team": false,
        "canEdit": true,
        "rules": team_flow_rule_catalog(),
    }))
}

/// `{ projectId? | workspaceId? | scope: "default", steps }` saves; `reset: true` removes the
/// scope's own steps so it uses the next one up again. In a workspace connected to a team it saves
/// the team's steps, which only owners may change.
fn update_team_flow(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let scope = TeamFlowScope::from_params(params);
    let steps = if params.get("reset").and_then(Value::as_bool) == Some(true) {
        None
    } else {
        Some(validate_team_flow_steps(params.get("steps").ok_or_else(
            || DomainStateError::bad_request("Pass steps, or reset: true."),
        )?)?)
    };
    if let Some(saved) = scope_workspace_id(state, &scope)?
        .as_deref()
        .and_then(|id| store_team_flow_steps(&state.paths, id, steps.clone()))
    {
        return saved
            .map(team_answer)
            .map_err(DomainStateError::bad_request);
    }
    store_team_flow(&state.paths, &scope, steps).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save the team flow: {error}"),
    })?;
    read_team_flow(state, params)
}
