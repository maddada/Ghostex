//! The session an agent's connector tools serve, and the workspace whose connectors it gets.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

use super::super::workspace::project_workspace;

/// How long a session's workspace is remembered for its tool list (a project moved to another
/// workspace gets the other workspace's tools within this).
const WORKSPACE_FRESH_FOR: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct AgentSession {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
    pub(crate) workspace: String,
    /// A GBot's chat that may only read the user's apps (gbots/apps_access.rs).
    pub(crate) reads_only: bool,
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Connector tools only work inside a Ghostex session (no session was named).",
            )
        })
}

/// The same as [`resolve`], remembered for a minute, for the tool list every helper asks for.
pub(crate) fn resolve_cached(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<AgentSession, DomainStateError> {
    static KNOWN: Mutex<Option<HashMap<(String, String), (AgentSession, Instant)>>> =
        Mutex::new(None);
    let key = (
        param(params, "projectId")?.to_string(),
        param(params, "sessionId")?.to_string(),
    );
    let known = KNOWN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .filter(|(_, at)| at.elapsed() < WORKSPACE_FRESH_FOR)
        .map(|(session, _)| session.clone());
    if let Some(session) = known {
        return Ok(session);
    }
    let session = resolve(state, params)?;
    let mut known = KNOWN.lock().unwrap_or_else(PoisonError::into_inner);
    let map = known.get_or_insert_with(HashMap::new);
    map.retain(|_, (_, at)| at.elapsed() < WORKSPACE_FRESH_FOR);
    map.insert(key, (session.clone(), Instant::now()));
    Ok(session)
}

/// The session `projectId` and `sessionId` name. Blocking.
pub(crate) fn resolve(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<AgentSession, DomainStateError> {
    let project_id = param(params, "projectId")?;
    let session_id = param(params, "sessionId")?;
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        DomainStateError::corrupt_state(format!("SQLite gxserver state error: {error}"))
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found("This session no longer exists."))?;
    let project = repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found("This session's project no longer exists."))?;
    Ok(AgentSession {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        workspace: project_workspace(&db, &repository, &project)?,
        reads_only: crate::gbots::project_reads_apps_only(&db, &project),
    })
}
