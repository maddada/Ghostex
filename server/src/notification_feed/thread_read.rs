use std::time::Duration;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::{sql_error, DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;
use crate::storage::open_gxserver_database_with_busy_timeout;

const THREAD_READ_BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// CDXC:Notifications 2026-09-29 DECISION:
/// User: "when i mark a notification as read then we should mark the thread that is related to that notification as read". Marking a row read (one row or Mark All Read) acknowledges its session's attention the way a sidebar click does, so the thread's done/needs-input mark clears on every client. The reverse direction is `observe_agent_activity_result`.
/// Only a session still in attention is acknowledged: the acknowledge transition sets any activity to idle, so sending it to a working session would end its turn's spinner.
/// Runs on the blocking pool after the feed update answered; errors are logged and swallowed.
pub(crate) fn acknowledge_read_notification_sessions(state: &AppState, session_ids: Vec<String>) {
    if session_ids.is_empty() {
        return;
    }
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        for session_id in session_ids {
            if let Err(error) = acknowledge_session_attention(&state, &session_id) {
                let _ = state.logger.log(GxserverLogInput {
                    level: LogLevel::Warn,
                    event: "notificationFeed.threadReadFailed".to_string(),
                    server_id: Some(state.metadata.server_id.clone()),
                    request_id: None,
                    client: None,
                    duration_ms: None,
                    error: Some(error.message.clone()),
                    details: Some(json!({ "sessionId": session_id })),
                });
            }
        }
    });
}

fn acknowledge_session_attention(
    state: &AppState,
    session_id: &str,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database_with_busy_timeout(&state.paths, THREAD_READ_BUSY_TIMEOUT)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    // The row keeps the project it was written under; the session may have moved since.
    let Some(project_id) = db
        .query_row(
            "SELECT projectId FROM sessions WHERE sessionId = ?1 LIMIT 1",
            params![session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
    else {
        return Ok(());
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let in_attention = repository
        .get_session(&project_id, session_id)?
        .and_then(|session| {
            session
                .pointer("/runtimeSettings/agentActivity/activity")
                .and_then(Value::as_str)
                .map(|activity| activity == "attention")
        })
        .unwrap_or(false);
    drop(db);
    if !in_attention {
        return Ok(());
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    params.insert("sessionId".to_string(), json!(session_id));
    params.insert("event".to_string(), json!("acknowledge"));
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        "/api/updateAgentActivity".to_string(),
        format!("notification-read-{}", uuid::Uuid::new_v4()),
        params,
    );
    Ok(())
}
