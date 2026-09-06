//! CDXC:SessionChat 2026-09-05 DECISION:
//! User: model selection is always available, including during a turn; if delivery cannot succeed, queue it for the next opportunity.
//! User: try the change immediately, even while working; only retain it when the terminal cannot accept it. This supersedes waiting for an idle turn boundary.
//! One durable desired selection per session coalesces repeated choices, survives disconnects and restarts, and remains pending until the terminal confirms it.

use crate::{domain::DomainStateError, server::AppState, storage::open_gxserver_database};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{future::Future, pin::Pin, sync::Arc};

static SELECTION_REQUESTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub(crate) async fn selection_requested() {
    SELECTION_REQUESTED.notified().await;
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingModelSelection {
    pub id: String,
    pub model: String,
    pub effort: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip)]
    pub retry_at: i64,
}

pub type ModelSelectionSender = Arc<
    dyn Fn(String, String, PendingModelSelection) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

fn storage_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

pub fn read_pending(
    db: &Connection,
    project: &str,
    session: &str,
) -> Option<PendingModelSelection> {
    db.query_row(
        "SELECT selectionId, model, effort, state, errorMessage, retryAt FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2",
        params![project, session],
        |row| Ok(PendingModelSelection { id: row.get(0)?, model: row.get(1)?, effort: row.get(2)?, state: row.get(3)?, error_message: row.get(4)?, retry_at: row.get(5)? }),
    ).optional().ok().flatten()
}

/// The published catalog can grow without rebuilding gxserver; the terminal is the authority on availability.
pub(crate) fn validate_selection(
    provider: &str,
    model: &str,
    effort: &str,
) -> Result<(), DomainStateError> {
    let token = |value: &str| {
        !value.is_empty()
            && value.len() <= 160
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-._[]():/".contains(&byte))
    };
    if !matches!(provider, "codex" | "claude")
        || !token(model)
        || (!effort.is_empty() && !token(effort))
    {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A valid model and effort are required for this agent.".into(),
        });
    }
    Ok(())
}

pub(crate) fn enqueue(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project = params
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let session_id = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if project.is_empty() || session_id.is_empty() {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A project and session are required.".into(),
        });
    }
    let db = open_gxserver_database(&state.paths).map_err(storage_error)?;
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let session = repository
        .get_session(project, session_id)?
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The session no longer exists.".into(),
        })?;
    let provider =
        crate::session_chat_follower::session_chat_agent_for_session(&session).unwrap_or_default();
    let model = params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let effort = params
        .get("effort")
        .and_then(Value::as_str)
        .unwrap_or_default();
    validate_selection(&provider, model, effort)?;
    let id = uuid::Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO session_chat_model_selections (projectId, sessionId, selectionId, model, effort, state, retryAt) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0) ON CONFLICT(projectId, sessionId) DO UPDATE SET selectionId = excluded.selectionId, model = excluded.model, effort = excluded.effort, state = 'queued', errorMessage = NULL, retryAt = 0",
        params![project, session_id, id, model, effort],
    ).map_err(storage_error)?;
    let pending = read_pending(&db, &project, &session_id);
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state,
        &project,
        &session_id,
    );
    SELECTION_REQUESTED.notify_one();
    Ok(
        json!({ "ok": true, "model": model, "effort": effort, "queued": true, "pendingModelSelection": pending }),
    )
}

pub(crate) fn sender(state: &Arc<AppState>) -> ModelSelectionSender {
    let state = state.clone();
    Arc::new(move |project, session, pending| {
        let state = state.clone();
        Box::pin(async move {
            let claimed = open_gxserver_database(&state.paths).ok().and_then(|db| db.execute(
                "UPDATE session_chat_model_selections SET state = 'applying', errorMessage = NULL WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3",
                params![project, session, pending.id],
            ).ok()).is_some_and(|count| count > 0);
            if !claimed {
                return;
            }
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state, &project, &session,
            );
            let params = json!({ "projectId": project, "sessionId": session, "model": pending.model, "effort": pending.effort });
            let result = crate::session_chat_codex_picker::select_session_chat_model(
                &state,
                params.as_object().unwrap(),
            )
            .await;
            if let Ok(db) = open_gxserver_database(&state.paths) {
                match result {
                    Ok(_) => {
                        let _ = db.execute("DELETE FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id]);
                    }
                    Err(error) => {
                        let _ = db.execute("UPDATE session_chat_model_selections SET state = 'queued', errorMessage = ?4, retryAt = ?5 WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id, error.message, chrono::Utc::now().timestamp_millis() + 5_000]);
                    }
                }
            }
            crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state, &project, &session,
            );
        })
    })
}
