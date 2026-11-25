//! `/api/openShownCommandTerminal { commandId }`: a new plain terminal in Main Hub with one of the
//! commands Ghostex shows (registry.rs) typed at its prompt and not sent, answering
//! `{ projectId, sessionId, title, commandId }` so the caller opens that terminal.
//!
//! CDXC:Updates 2026-11-25 DECISION: User: "can we please create a terminal for them and have them just hit enter there? add button to do this please", then (Q107) in Main Hub, and "we should do the same thing please for all commands like this in the app so we can run them in main hub for the user basically". Run in terminal makes a terminal in Main Hub (the home folder) titled after the command, types the command there and leaves Enter (and the sudo password) to the user, so Ghostex still never runs it. The desktop's card and update dialog, another computer's card and the phone all ask this one endpoint (the phone through `ghostex shown-command open`).
//! SEE-ALSO: apps/desktop/src/app/gx_store/create/shown_command_terminal.rs (the desktop opens what this returns), apps/mobile/app/src/screens/machine-form/AppUpdateSection.tsx (the phone).
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::agents::{apply_created_session_identity, read_agent_settings};
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, restore_parked_project_for_new_session, routed_json,
    schedule_presentation_session_delta, AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;
use crate::zmx::{
    dispatch_zmx_lifecycle_endpoint, dispatch_zmx_session_interaction_endpoint,
    read_zmx_queued_session_write, ZmxEndpointError, ZmxServerContext,
};

pub(crate) const OPEN_SHOWN_COMMAND_TERMINAL_ENDPOINT: &str = "/api/openShownCommandTerminal";

/// How long the command waits for the new shell to draw its prompt before it is typed anyway
/// (text typed before the shell reads its line is echoed once above the prompt too).
const PROMPT_WAIT_POLLS: u32 = 40;
const PROMPT_POLL: Duration = Duration::from_millis(150);

pub(crate) async fn handle_open_shown_command_terminal_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = match read_domain_rpc_params(body) {
        Ok(params) => {
            let id = params
                .get("commandId")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            open(state, &id).await
        }
        Err(error) => Err(error),
    };
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn open(state: &AppState, id: &str) -> Result<Value, DomainStateError> {
    let shown = super::registry::resolve(state, id).await?;
    let worker = state.clone();
    let title = shown.title;
    let (project_id, session_id) =
        tokio::task::spawn_blocking(move || create_started_terminal(&worker, title))
            .await
            .map_err(|error| internal(format!("The terminal could not be made: {error}")))??;
    wait_for_prompt(state, &project_id, &session_id).await;
    type_text(state, &project_id, &session_id, &shown.command).await?;
    Ok(json!({
        "projectId": project_id,
        "sessionId": session_id,
        "title": title,
        "commandId": id,
    }))
}

/// A running plain terminal in Main Hub (the home project, added when missing), its shell started
/// so the command reaches it whichever client opens it.
fn create_started_terminal(
    state: &AppState,
    title: &str,
) -> Result<(String, String), DomainStateError> {
    let db = open_gxserver_database(&state.paths)
        .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let project = crate::chief::home_project(state, &db, &repository)?;
    let project_id = text_of(&project, "projectId")?;
    let mut params = Map::new();
    params.insert("kind".into(), json!("terminal"));
    params.insert("lifecycleState".into(), json!("running"));
    params.insert("projectId".into(), json!(project_id));
    params.insert("surface".into(), json!("workspace"));
    params.insert("title".into(), json!(title));
    let created = repository.create_session(&params, false)?;
    let session = apply_created_session_identity(&repository, &created, &params)?;
    let project_id = text_of(&session, "projectId")?;
    let session_id = text_of(&session, "sessionId")?;
    restore_parked_project_for_new_session(state, &db, &repository, &project_id)?;
    schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)?;
    // A failed start still leaves the terminal, whose first attach starts it again.
    let _ = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/startSessionProvider",
        &ids(&project_id, &session_id),
        &zmx_context(state),
        &read_agent_settings(&db)?,
    );
    let _ = schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
    Ok((project_id, session_id))
}

async fn wait_for_prompt(state: &AppState, project_id: &str, session_id: &str) {
    for _ in 0..PROMPT_WAIT_POLLS {
        let worker = state.clone();
        let params = ids(project_id, session_id);
        let shown = tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&worker.paths).ok()?;
            let repository = DomainRepository::new(&db, &worker.metadata.server_id);
            let read = dispatch_zmx_session_interaction_endpoint(
                &repository,
                "/api/readSessionText",
                &params,
            )
            .ok()?;
            Some(
                read["text"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty()),
            )
        })
        .await
        .ok()
        .flatten();
        if shown == Some(true) {
            return;
        }
        tokio::time::sleep(PROMPT_POLL).await;
    }
}

/// Types `text` at the prompt as it is, without Enter.
async fn type_text(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Result<(), DomainStateError> {
    let queued = {
        let db = open_gxserver_database(&state.paths)
            .map_err(|error| internal(format!("SQLite gxserver state error: {error}")))?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let mut params = ids(project_id, session_id);
        params.insert("text".into(), json!(text));
        read_zmx_queued_session_write(&repository, "/api/sendSessionText", &params)
            .map_err(zmx_error)?
    };
    queued.execute().await.map(|_| ()).map_err(zmx_error)
}

fn ids(project_id: &str, session_id: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params
}

/// How gxserver reaches itself for a zmx lifecycle call.
fn zmx_context(state: &AppState) -> ZmxServerContext {
    ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    }
}

fn zmx_error(error: ZmxEndpointError) -> DomainStateError {
    match error {
        ZmxEndpointError::DependencyUnavailable(message) => internal(message),
        ZmxEndpointError::Domain(error) => error,
    }
}

fn internal(message: String) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message,
    }
}

fn text_of(value: &Value, key: &str) -> Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state(format!("Missing {key}.")))
}
