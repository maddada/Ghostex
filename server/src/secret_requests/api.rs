//! The secret request routes: the CLI creates a request and waits for it (`createSecretRequest`,
//! `waitSecretRequest`, `cancelSecretRequest`), a chat client answers it (`answerSecretRequest`),
//! and every chat frame of the session carries its open requests (`secretRequests`).

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
    time::Duration,
};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::store::{self, NewSecretRequest, SecretAnswer, SecretRequestError, SecretRequestStatus};
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::{rpc_error, rpc_success};
use crate::security::{Capability, Principal};
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

pub(crate) const SECRET_REQUEST_ENDPOINTS: &[&str] = &[
    "/api/createSecretRequest",
    "/api/waitSecretRequest",
    "/api/cancelSecretRequest",
    "/api/answerSecretRequest",
];

/// One `waitSecretRequest` holds the HTTP call at most this long; the CLI asks again until the
/// user answers, which is also how gxserver knows the agent is still waiting.
const MAX_WAIT: Duration = Duration::from_secs(25);
/// How often a request's watcher looks for a timeout or a CLI that went away.
const WATCH_EVERY: Duration = Duration::from_secs(10);

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

fn required(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    text(params, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

fn request_error(error: SecretRequestError) -> DomainStateError {
    match error {
        SecretRequestError::NotFound => DomainStateError::not_found(error.message()),
        SecretRequestError::StoreFailed => DomainStateError::corrupt_state(error.message()),
        other => DomainStateError::bad_request(other.message()),
    }
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

/*
CDXC:Secrets 2026-11-08 DECISION:
Only the session that asks for a secret makes its card and waits for the answer: the caller must be that session's own `gxs_` token (`Principal::Session` with the same project and session). Anyone else (another session, the token file, the owner's devices, connected computers) is refused, so no one can put a card in another session's chat or collect the file its answer was saved to.
*/
/// Refuses a caller that is not session `project_id`/`session_id` itself.
fn require_own_session(
    caller: &Principal,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    match caller {
        Principal::Session {
            project_id: own_project,
            session_id: own_session,
            ..
        } if own_project == project_id && own_session == session_id => Ok(()),
        _ => Err(forbidden(
            "A secret request can only be made and waited for by the agent asking, from inside its own session.",
        )),
    }
}

fn status_text(status: SecretRequestStatus) -> Value {
    serde_json::to_value(status).unwrap_or(Value::Null)
}

/// Adds `secretRequests` (the session's open and just-answered requests, never a value) to a chat
/// frame or read result. Always present, so an empty list clears a card on every client.
pub(crate) fn insert_secret_requests(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let views = store::list_for_session(project_id, session_id);
    frame.insert(
        "secretRequests".to_string(),
        serde_json::to_value(views).unwrap_or_else(|_| json!([])),
    );
}

/// Ids and states only, in memory: wakes an SSH-only long-poller when a card opens or closes.
pub(crate) fn secret_requests_identity(project_id: &str, session_id: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for view in store::list_for_session(project_id, session_id) {
        view.id.hash(&mut hasher);
        status_text(view.status).to_string().hash(&mut hasher);
    }
    hasher.finish()
}

/// Deletes saved files older than a day; gxserver runs it when it starts.
pub(crate) fn sweep_saved_files(paths: &crate::paths::GxserverPaths) -> usize {
    store::sweep_expired_files(&store::files_dir(paths))
}

fn republish(state: &AppState, project_id: &str, session_id: &str) {
    crate::server::republish_session_chat(state, project_id, session_id);
}

/// Closes the card when the request times out or its CLI stops asking, without waiting for some
/// other frame to carry the change.
fn watch(state: Arc<AppState>, id: String, project_id: String, session_id: String) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(WATCH_EVERY).await;
            let still_open = store::list_for_session(&project_id, &session_id)
                .iter()
                .any(|view| view.id == id && view.status == SecretRequestStatus::Pending);
            if !still_open {
                republish(&state, &project_id, &session_id);
                // One more pass after the answered card's linger, so it leaves every client.
                tokio::time::sleep(Duration::from_secs(35)).await;
                republish(&state, &project_id, &session_id);
                return;
            }
        }
    });
}

fn create(
    state: &Arc<AppState>,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = required(params, "projectId")?;
    let session_id = required(params, "sessionId")?;
    require_own_session(caller, &project_id, &session_id)?;
    let timeout = params
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(store::DEFAULT_TIMEOUT);
    let id = store::create(NewSecretRequest {
        project_id: project_id.clone(),
        session_id: session_id.clone(),
        label: text(params, "label").unwrap_or_default(),
        reason: text(params, "reason").unwrap_or_default(),
        placeholder: text(params, "placeholder"),
        accepts_reference: crate::secret_providers::has_one_password(&state.paths),
        timeout,
    })
    .map_err(request_error)?;
    sweep_saved_files(&state.paths);
    republish(state, &project_id, &session_id);
    watch(state.clone(), id.clone(), project_id, session_id);
    Ok(json!({ "requestId": id, "status": "pending" }))
}

async fn wait(
    state: &Arc<AppState>,
    caller: &Principal,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let id = required(&params, "requestId")?;
    let Some((project_id, session_id)) = store::session_of(&id) else {
        return Err(request_error(SecretRequestError::NotFound));
    };
    require_own_session(caller, &project_id, &session_id)?;
    let wait_for = params
        .get("waitMs")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(MAX_WAIT)
        .min(MAX_WAIT);
    let waiting_id = id.clone();
    let outcome = tokio::task::spawn_blocking(move || store::wait(&waiting_id, wait_for))
        .await
        .map_err(|_| DomainStateError::corrupt_state("The secret request stopped unexpectedly."))?
        .map_err(request_error)?;
    if outcome.status != SecretRequestStatus::Pending {
        republish(state, &project_id, &session_id);
    }
    let mut result = json!({ "requestId": id, "status": status_text(outcome.status) });
    if let Some(file) = outcome.file {
        result["file"] = json!(file.to_string_lossy());
    }
    Ok(result)
}

fn cancel(state: &Arc<AppState>, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let id = required(params, "requestId")?;
    let session = store::session_of(&id);
    let cancelled = store::cancel(&id);
    if let (true, Some((project_id, session_id))) = (cancelled, session) {
        republish(state, &project_id, &session_id);
    }
    Ok(json!({ "requestId": id, "cancelled": cancelled }))
}

/// The user's answer. The value is only ever moved: into the private file, or (for a 1Password
/// reference) into 1Password's read and then the file. Errors never repeat it.
///
/// Only the owner's devices answer (`Capability::OwnerSecrets`); the route gate refuses everyone
/// else already, and this check keeps a reference from ever being read for them.
async fn answer(
    state: &Arc<AppState>,
    caller: &Principal,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if !caller.has(Capability::OwnerSecrets) {
        return Err(forbidden(
            "Only you can answer a secret request, from Ghostex on your computer or phone.",
        ));
    }
    let project_id = required(&params, "projectId")?;
    let session_id = required(&params, "sessionId")?;
    let id = required(&params, "requestId")?;
    let answer = params
        .get("answer")
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("answer is required."))?;
    let reply = match answer.get("type").and_then(Value::as_str) {
        Some("decline") => SecretAnswer::Decline,
        Some("save") => SecretAnswer::Save(
            answer
                .get("secret")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        _ => {
            return Err(DomainStateError::bad_request(
                "answer.type must be save or decline.",
            ))
        }
    };
    let paths = state.paths.clone();
    let (answer_project, answer_session, answer_id) =
        (project_id.clone(), session_id.clone(), id.clone());
    tokio::task::spawn_blocking(move || {
        let reply = match reply {
            SecretAnswer::Save(value) => {
                match crate::secret_providers::resolve_typed_reference(&paths, &value) {
                    Some(Ok(read)) => SecretAnswer::Save(read),
                    Some(Err(message)) => return Err(DomainStateError::bad_request(message)),
                    None => SecretAnswer::Save(value),
                }
            }
            decline => decline,
        };
        store::answer(&paths, &answer_project, &answer_session, &answer_id, reply)
            .map_err(request_error)
    })
    .await
    .map_err(|_| DomainStateError::corrupt_state("The secret request stopped unexpectedly."))??;
    republish(state, &project_id, &session_id);
    Ok(json!({ "requestId": id, "ok": true }))
}

pub(crate) async fn handle_secret_request_http(
    state: &Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/createSecretRequest" => create(state, caller, &params),
        "/api/waitSecretRequest" => wait(state, caller, params).await,
        "/api/cancelSecretRequest" => cancel(state, &params),
        "/api/answerSecretRequest" => answer(state, caller, params).await,
        _ => Err(DomainStateError::not_found(
            "Unknown secret request endpoint.",
        )),
    };
    match result {
        Ok(value) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) if error.code == "forbidden" => routed_json(
            Some(endpoint_path),
            StatusCode::FORBIDDEN,
            rpc_error(error.code, error.message, Some(request_id)),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
