//! `ghostex slack post --session <ref> "<text>"` (`/api/postSlackWorkingThread`): an agent posts a
//! milestone to its ticket's Slack working thread through the team's Convex project; `--final`
//! also posts the text once in the threads the request came from.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;
use crate::work_mode::{work_targets, WorkSourceId};
use crate::workspaces::{project_workspace_id, read_sidebar_workspaces};

use super::connections::read_team_connection;
use super::convex_http::ConvexCallKind;
use super::operations::member_call;

/// `{ projectId, sessionId, text, final? }`.
pub(crate) fn post_slack_working_thread(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let field = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let project_id =
        field("projectId").ok_or_else(|| DomainStateError::bad_request("Pass projectId."))?;
    let session_id =
        field("sessionId").ok_or_else(|| DomainStateError::bad_request("Pass sessionId."))?;
    let text =
        field("text").ok_or_else(|| DomainStateError::bad_request("Pass the text to post."))?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let project = projects
        .iter()
        .find(|project| {
            project.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
        })
        .cloned()
        .ok_or_else(|| DomainStateError::bad_request(format!("No project {project_id}.")))?;
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::bad_request(format!("No session {session_id}.")))?;
    let workspaces = read_sidebar_workspaces(&db)?;
    let workspace_id = project_workspace_id(&workspaces, &project, &projects);
    let connection = read_team_connection(&state.paths, &workspace_id).ok_or_else(|| {
        DomainStateError::bad_request(
            "This session's workspace is not connected to a team, so it has no Slack working thread.",
        )
    })?;
    let mut args = Map::new();
    args.insert("sessionId".to_string(), json!(session_id));
    args.insert(
        "tickets".to_string(),
        json!(work_targets(&project, &session).ids_of(WorkSourceId::Linear)),
    );
    args.insert("text".to_string(), json!(text));
    args.insert(
        "final".to_string(),
        json!(params
            .get("final")
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    member_call(
        &connection,
        ConvexCallKind::Action,
        "slackPost:postForSession",
        args,
    )
    .map_err(DomainStateError::bad_request)
}
