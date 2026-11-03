//! A workspace's ticket command (crate::work_mode::command): reading what is set, setting or
//! clearing its path, and storing its credentials. Each may run the command's `info`, so they run
//! off the async runtime. They answer with Workspaces off too, like the tracker routes.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::work_mode::{
    clear_work_source, log_work_source_failure, proposed_values_mask, read_work_sources,
    referenced_source_ids, set_work_source, set_work_source_credentials, work_source_summary,
    CheckFailure, CommandEnv, CommandFailure, SourcePath, WorkSourceChange, WorkSourceError,
    WorkspaceCommand,
};
use crate::workspaces::{
    find_workspace_id, read_sidebar_workspaces, DEFAULT_WORKSPACE_ID, NO_SUCH_WORKSPACE,
};

use super::*;

const WORK_SOURCE_ROUTES: &[&str] = &[
    "/api/readWorkSource",
    "/api/setWorkSource",
    "/api/setWorkSourceCredentials",
];

pub(super) async fn route_work_source_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if !WORK_SOURCE_ROUTES.contains(&request.endpoint.path.as_str()) {
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
    let worker_request_id = request_id.clone();
    // Off the async runtime: setting a command or its credentials runs its `info` (10 s at most).
    let response = tokio::task::spawn_blocking(move || {
        let route = path.clone();
        handle_domain_http(
            &state,
            path,
            worker_request_id,
            &body_json,
            |repository, db, params, _| match route.as_str() {
                "/api/readWorkSource" => read_work_source(&state, db, params),
                "/api/setWorkSource" => set_command(&state, repository, db, params),
                _ => set_credentials(&state, db, params),
            },
        )
    })
    .await;
    Ok(match response {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint.path.clone(),
            request_id,
            DomainStateError::corrupt_state(format!("Ticket command request failed: {error}")),
        ),
    })
}

/// The workspace a request names (`workspaceId`, an id or a name), else the default workspace. Its
/// errors hold nothing the request sent.
fn workspace_of(
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    let workspaces = read_sidebar_workspaces(db)?;
    match params
        .get("workspaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        Some(reference) => find_workspace_id(&workspaces, reference)
            .ok_or_else(|| DomainStateError::bad_request(NO_SUCH_WORKSPACE)),
        None => Ok(DEFAULT_WORKSPACE_ID.to_string()),
    }
}

fn in_team(state: &AppState, workspace_id: &str) -> bool {
    crate::team_sync::workspace_has_team(&state.paths, workspace_id)
}

/// `{ workspaceId? }` → `{ workspaceId, team, command }` (`work_source_summary`).
fn read_work_source(
    state: &AppState,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace_id = workspace_of(db, params)?;
    summary(state, db, &workspace_id)
}

fn summary(
    state: &AppState,
    db: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<Value, DomainStateError> {
    let sources = read_work_sources(db)?;
    Ok(work_source_summary(
        &state.paths,
        workspace_id,
        sources.workspaces.get(workspace_id),
        in_team(state, workspace_id),
    ))
}

/// `{ workspaceId?, path: string | null, switch? }`: sets the command, or with `null` clears it,
/// keeping its credentials. `{ workspaceId?, check: true, switch? }` checks it again with the path
/// it has, read in the store under its writer lock, so a path another client sets in between is
/// never put back. A team workspace may clear a command it had before joining, never set one (the
/// store refuses it, under its writer lock).
///
/// Each change's errors leave through the store's one mask (`WorkSourceChange::mask`), and the
/// route words its own errors after it with the same one, like a credential save.
fn set_command(
    state: &AppState,
    repository: &DomainRepository,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let workspace_id = workspace_of(db, params)?;
    let references = || referenced_source_ids(repository, &read_sidebar_workspaces(db)?);
    let switch = params.get("switch").and_then(Value::as_bool) == Some(true);
    let check = params.get("check").and_then(Value::as_bool) == Some(true);
    let set = |path| set_work_source(db, &state.paths, &workspace_id, path, switch, &references);
    let (failed, mask) = match (params.get("path"), check) {
        (None, true) => checked(set(SourcePath::Stored))?,
        (Some(Value::String(path)), false) => checked(set(SourcePath::Typed(path)))?,
        (Some(Value::Null), false) => {
            let cleared = clear_work_source(db, &state.paths, &workspace_id, &references);
            cleared.result.map_err(refusal)?;
            (None, cleared.mask)
        }
        _ => {
            return Err(DomainStateError::bad_request(
                "Pass path: the ticket command's path, or null to clear it; or check: true to check it again.",
            ))
        }
    };
    let answer = summary(state, db, &workspace_id).map_err(|error| masked(&mask, error))?;
    with_check(state, answer, failed)
}

/// A set's failed check, which the answer carries, and its mask; any other error is the request's.
fn checked(
    change: WorkSourceChange<WorkspaceCommand>,
) -> Result<(Option<CheckFailure>, CommandEnv), DomainStateError> {
    match change.result {
        Ok(_) => Ok((None, change.mask)),
        Err(WorkSourceError::Failed(failure)) => Ok((Some(failure), change.mask)),
        Err(error) => Err(refusal(error)),
    }
}

fn masked(mask: &CommandEnv, mut error: DomainStateError) -> DomainStateError {
    error.message = mask.shown(&error.message);
    error
}

/// `{ workspaceId?, values: { <field>: string | null } }`: stores the values, then checks `info`
/// with them. A failing check keeps the values and shows in the answer's `check` and `lastError`.
/// Its own errors leave through the save's one mask (`CredentialsSaved::mask`); one worded before
/// a workspace is known, through the proposed values alone (`proposed_values_mask`). That one is
/// fixed wording, masked anyway so that nothing a credential save answers holds a proposed value
/// of 3 or more characters, even as one of Ghostex's own words. A shorter one is never secret, so
/// it isn't masked.
fn set_credentials(
    state: &AppState,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let values = params
        .get("values")
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("Pass values: { <field>: text or null }."))?;
    let workspace_id =
        workspace_of(db, params).map_err(|error| masked(&proposed_values_mask(values), error))?;
    let saved =
        set_work_source_credentials(db, &state.paths, &workspace_id, values).map_err(refusal)?;
    let answer = summary(state, db, &workspace_id).map_err(|error| masked(&saved.mask, error))?;
    with_check(state, answer, saved.failure)
}

/// The answer, with `check: { error, detail }` when the command ran and failed. A failed check is
/// an answer, not a request error: a request error's message is written to gxserver's request log
/// (`http_dispatch.rs`), and this one can quote the command's own error text. The log gets the
/// metadata-only line instead (`log_work_source_failure`).
fn with_check(
    state: &AppState,
    mut answer: Value,
    failure: Option<CheckFailure>,
) -> Result<Value, DomainStateError> {
    if let Some(failure) = failure {
        log_work_source_failure(&state.logger, &failure);
        answer["check"] = json!({
            "error": failure.message,
            "detail": match &failure.failure {
                CommandFailure::Unreadable { stdout, .. } => Some(stdout.clone()),
                _ => None,
            },
        });
    }
    Ok(answer)
}

/// A refusal Ghostex words itself, never the command's output, or a storage error.
fn refusal(error: WorkSourceError) -> DomainStateError {
    match error {
        WorkSourceError::Refused(text) => DomainStateError::bad_request(text),
        // Only `info` runs fail, and both routes answer those with `with_check`; this message
        // stays generic so no command text can reach the request log.
        WorkSourceError::Failed(_) => {
            DomainStateError::bad_request("The ticket command failed; check it again for details.")
        }
        WorkSourceError::Storage(error) => error,
    }
}
