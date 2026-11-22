//! `/api/claudeCloud {action, …}`: every Claude cloud session operation a client or the session's
//! own terminal client asks for.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

/// Runs one action. The slow ones (a cloud start, a follow-up, `gh`) run off the async runtime.
pub(crate) async fn claude_cloud_action(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if action == "bringBack" {
        return super::bring_back::bring_cloud_session_back(&state, &params).await;
    }
    if action == "copy" {
        // The copy's row exists at once; its start (up to a few minutes) runs on behind it, and its
        // terminal shows the start like any cloud session's.
        let worker = state.clone();
        let (project_id, session_id) =
            tokio::task::spawn_blocking(move || super::copy::create_cloud_copy(&worker, &params))
                .await
                .unwrap_or_else(|error| {
                    Err(DomainStateError::corrupt_state(format!(
                        "Claude cloud copy failed: {error}"
                    )))
                })?;
        let starter = state.clone();
        let mut start = Map::new();
        start.insert("projectId".to_string(), Value::String(project_id.clone()));
        start.insert("sessionId".to_string(), Value::String(session_id.clone()));
        tokio::task::spawn_blocking(move || {
            let _ = super::start::start_cloud_session(&starter, &start);
        });
        return Ok(serde_json::json!({ "projectId": project_id, "sessionId": session_id }));
    }
    let blocking = match action.as_str() {
        "get" => super::start::read_cloud_session,
        "start" => super::start::start_cloud_session,
        "send" => super::start::send_to_cloud_session,
        "conversation" => super::chat::read_conversation,
        "pageStatus" => super::page_status::apply_page_status,
        _ => {
            return Err(DomainStateError::bad_request(
                "action must be get, start, send, conversation, pageStatus, bringBack or copy.",
            ))
        }
    };
    tokio::task::spawn_blocking(move || blocking(&state, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError::corrupt_state(format!(
                "Claude cloud action failed: {error}"
            )))
        })
}
