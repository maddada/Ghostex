//! CDXC:SessionChat 2026-09-05 DECISION:
//! User: model selection is always available, including during a turn; if delivery cannot succeed, queue it for the next opportunity.
//! User: try the change immediately, even while working; only retain it when the terminal cannot accept it. This supersedes waiting for an idle turn boundary.
//! One durable desired selection per session coalesces repeated choices, survives disconnects and restarts, and remains pending until the terminal confirms it.
//! CDXC:SessionChat 2026-09-08 DECISION:
//! User: effort, Plan mode, Fast mode and Claude permissions are always changeable from chat too; attempt immediately and retain the desired state for retry if delivery cannot succeed.

use crate::{domain::DomainStateError, server::AppState, storage::open_gxserver_database};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{future::Future, pin::Pin, sync::Arc};

static SELECTION_REQUESTED: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<String>,
}

impl SelectionOptions {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.fast_mode.is_none()
    }
}

pub(crate) fn read_options(
    provider: &str,
    params: &Map<String, Value>,
) -> Result<SelectionOptions, DomainStateError> {
    let options: SelectionOptions =
        serde_json::from_value(params.get("options").cloned().unwrap_or_else(|| json!({})))
            .map_err(|_| DomainStateError {
                code: "invalidParams",
                message: "Invalid chat option selection.".into(),
            })?;
    let valid_mode = options.mode.as_deref().is_none_or(|mode| match provider {
        "opencode" => !mode.trim().is_empty() && mode.len() <= 160,
        "codex" => matches!(mode, "plan" | "default"),
        "claude" => matches!(mode, "bypass" | "auto" | "manual" | "accept-edits" | "plan"),
        _ => false,
    });
    if (options.fast_mode.is_some() && !matches!(provider, "codex" | "claude"))
        || !valid_mode
        || !options
            .fast_mode
            .as_deref()
            .is_none_or(|value| matches!(value, "on" | "off"))
    {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "That option is not available for this agent.".into(),
        });
    }
    Ok(options)
}

pub(crate) async fn selection_requested() {
    SELECTION_REQUESTED.notified().await;
}

pub(crate) const SCOPE_SESSION: &str = "session";
pub(crate) const SCOPE_DEFAULT: &str = "default";

/// CDXC:SessionChat 2026-09-18 DECISION:
/// User: a chat model pick can apply to this session alone instead of changing the agent's saved default.
/// Only Claude can honour it: its `/model` list answers `s` with "for this session only", while confirming Codex's picker rewrites `model` and `model_reasoning_effort` in `~/.codex/config.toml` with no opt-out (verified live 2026-09-18).
/// An absent scope stays `default`, so a client that predates the field keeps its old behaviour.
/// SEE-ALSO: server/src/session_chat_codex_picker.rs drives both scopes, packages/shared/session-chat.ts carries the wire type.
pub(crate) fn read_scope(
    provider: &str,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    let scope = params
        .get("scope")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(SCOPE_DEFAULT);
    if provider == "opencode" && matches!(scope, SCOPE_DEFAULT | SCOPE_SESSION) {
        return Ok(scope.to_string());
    }
    if scope == SCOPE_DEFAULT {
        return Ok(SCOPE_DEFAULT.to_string());
    }
    if scope != SCOPE_SESSION {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "A model selection scope is either session or default.".into(),
        });
    }
    if provider != "claude" {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "This agent's model picker always saves the choice as its default.".into(),
        });
    }
    Ok(SCOPE_SESSION.to_string())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingModelSelection {
    pub id: String,
    pub model: String,
    pub effort: String,
    pub options: SelectionOptions,
    /// Always serialized so a client can tell a scope-aware daemon from one that dropped the field.
    pub scope: String,
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
        "SELECT selectionId, model, effort, state, errorMessage, retryAt, options, scope FROM session_chat_model_selections WHERE projectId = ?1 AND sessionId = ?2",
        params![project, session],
        |row| Ok(PendingModelSelection { id: row.get(0)?, model: row.get(1)?, effort: row.get(2)?, state: row.get(3)?, error_message: row.get(4)?, retry_at: row.get(5)?, options: serde_json::from_str(&row.get::<_, String>(6)?).map_err(|error| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error)))?, scope: row.get(7)? }),
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
    if !matches!(
        provider,
        "codex" | "claude" | "cursor" | "grok" | "antigravity" | "opencode"
    ) || !token(model)
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
    let incoming_options = read_options(&provider, params)?;
    let scope = read_scope(&provider, params)?;
    if !model.is_empty() || incoming_options.is_empty() {
        validate_selection(&provider, model, effort)?;
    } else if !matches!(provider.as_str(), "codex" | "claude" | "opencode") || !effort.is_empty() {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "An effort change requires its model.".into(),
        });
    }
    // Merge under the write lock so independent controls cannot overwrite each other's pending choice.
    let transaction =
        rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
            .map_err(storage_error)?;
    let previous = read_pending(&transaction, project, session_id);
    let model = if model.is_empty() {
        previous
            .as_ref()
            .map_or("", |pending| pending.model.as_str())
    } else {
        model
    };
    let effort = if params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty()
    {
        previous
            .as_ref()
            .map_or("", |pending| pending.effort.as_str())
    } else {
        effort
    };
    let mut options = previous
        .as_ref()
        .map(|pending| pending.options.clone())
        .unwrap_or_default();
    if incoming_options.mode.is_some() {
        options.mode = incoming_options.mode;
    }
    if incoming_options.fast_mode.is_some() {
        options.fast_mode = incoming_options.fast_mode;
    }
    // CDXC:SessionChat 2026-09-19 WHY:
    // Only a request that names a model carries a scope of its own; an absent one means `default`
    // even while a session-only pick is pending, so an older client keeps its old behaviour. An
    // options-only change keeps the pending model choice's scope so it cannot turn a session-only
    // pick into a default one, and with no pending choice it is `default`: a session-only row without
    // a model would be refused by the driver and retried every five seconds forever.
    let replaces_model = !params
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .is_empty();
    let scope = if replaces_model {
        scope
    } else {
        previous
            .as_ref()
            .map_or(SCOPE_DEFAULT, |pending| pending.scope.as_str())
            .to_string()
    };
    let id = uuid::Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO session_chat_model_selections (projectId, sessionId, selectionId, model, effort, state, retryAt, options, scope) VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, ?6, ?7) ON CONFLICT(projectId, sessionId) DO UPDATE SET selectionId = excluded.selectionId, model = excluded.model, effort = excluded.effort, options = excluded.options, scope = excluded.scope, state = 'queued', errorMessage = NULL, retryAt = 0",
        params![project, session_id, id, model, effort, serde_json::to_string(&options).map_err(storage_error)?, scope],
    ).map_err(storage_error)?;
    transaction.commit().map_err(storage_error)?;
    let pending = read_pending(&db, &project, &session_id);
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state,
        &project,
        &session_id,
    );
    SELECTION_REQUESTED.notify_one();
    Ok(
        json!({ "ok": true, "model": model, "effort": effort, "scope": scope, "queued": true, "pendingModelSelection": pending }),
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
            let params = json!({ "projectId": project, "sessionId": session, "model": pending.model, "effort": pending.effort, "options": pending.options, "scope": pending.scope });
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
                    // CDXC:SessionChat 2026-09-18 WHY:
                    // A selection the terminal can never accept stops here instead of retrying:
                    // one model the CLI does not list for an account retyped `/model` into the
                    // user's pane 181 times. The row stays only to carry its reason to the chat,
                    // and `failed` keeps it out of delivery, wake-keeping and the desired model,
                    // the same way a failed queued prompt is treated.
                    Err(error)
                        if error.code
                            == crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE =>
                    {
                        let _ = db.execute("UPDATE session_chat_model_selections SET state = 'failed', errorMessage = ?4, retryAt = 0 WHERE projectId = ?1 AND sessionId = ?2 AND selectionId = ?3", params![project, session, pending.id, error.message]);
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
