use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::presentation::read_runtime_text;
use crate::server::AppState;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;
use crate::storage::open_gxserver_database_with_busy_timeout;

use super::api::broadcast_notification_feed_changed;
use super::body::last_assistant_message_text;
use super::store::{
    bounded_notification_body, insert_notification_feed_row, mark_session_notifications_read,
    NewNotificationFeedRow, NOTIFICATION_FEED_KIND_BELL, NOTIFICATION_FEED_KIND_FINISHED,
    NOTIFICATION_FEED_KIND_NEEDS_INPUT,
};

const OBSERVER_BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// Feed rows are written from the one place every attention transition already passes through, after the endpoint's own transaction committed.
/// Entering attention writes a row; leaving attention by any route (acknowledge, escape, a new turn, exit) reads the session's rows, so the bell count tracks the sidebar's blue dots without a second acknowledge path.
/// Never fails the request: every error is logged and swallowed.
pub(crate) fn observe_agent_activity_result(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if !matches!(
        endpoint_path,
        "/api/ingestAgentHookEvent"
            | "/api/ingestSessionStateEvent"
            | "/api/ingestTerminalTitleEvent"
            | "/api/updateAgentActivity"
    ) {
        return;
    }
    let Some(previous) = result.get("previousActivity").and_then(Value::as_str) else {
        return;
    };
    let Some(activity) = result.get("activity").and_then(Value::as_object) else {
        return;
    };
    let next = activity
        .get("activity")
        .and_then(Value::as_str)
        .unwrap_or("idle");
    let entered_attention = result.get("enteredAttention").and_then(Value::as_bool) == Some(true);
    let left_attention = previous == "attention" && next != "attention";
    if !entered_attention && !left_attention {
        return;
    }
    let Some(session) = result.get("session") else {
        return;
    };
    let Some(session_id) = session
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| params.get("sessionId").and_then(Value::as_str))
    else {
        return;
    };
    let Some(project_id) = session
        .get("projectId")
        .and_then(Value::as_str)
        .or_else(|| params.get("projectId").and_then(Value::as_str))
    else {
        return;
    };
    let outcome = if entered_attention {
        record_attention_row(state, session, project_id, session_id, activity, params)
    } else {
        record_attention_left(state, session_id)
    };
    match outcome {
        Ok(changed) => {
            if changed {
                broadcast_notification_feed_changed(state);
            }
        }
        Err(error) => {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "notificationFeed.observeFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message.clone()),
                details: Some(json!({
                    "enteredAttention": entered_attention,
                    "sessionId": session_id,
                })),
            });
        }
    }
}

fn record_attention_row(
    state: &AppState,
    session: &Value,
    project_id: &str,
    session_id: &str,
    activity: &Map<String, Value>,
    params: &Map<String, Value>,
) -> Result<bool, DomainStateError> {
    let attention_source = activity.get("attentionSource").and_then(Value::as_str);
    let event = params.get("event").and_then(Value::as_str);
    let kind = if attention_source == Some(TURN_COMPLETE_ATTENTION_SOURCE) {
        NOTIFICATION_FEED_KIND_FINISHED
    } else if event == Some("bell") {
        NOTIFICATION_FEED_KIND_BELL
    } else {
        NOTIFICATION_FEED_KIND_NEEDS_INPUT
    };
    let body = match kind {
        NOTIFICATION_FEED_KIND_BELL => "Terminal bell".to_string(),
        NOTIFICATION_FEED_KIND_FINISHED => last_assistant_message_text(session)
            .map(|text| bounded_notification_body(&text))
            .unwrap_or_else(|| "Finished a turn".to_string()),
        _ => last_assistant_message_text(session)
            .map(|text| bounded_notification_body(&text))
            .unwrap_or_else(|| "Waiting for your input".to_string()),
    };
    let db = open_gxserver_database_with_busy_timeout(&state.paths, OBSERVER_BUSY_TIMEOUT)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let subtitle = repository
        .get_project(project_id)?
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let title = session
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Session")
        .to_string();
    insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            kind,
            title,
            subtitle,
            body,
            agent_name: read_runtime_text(session, "agentName"),
            attention_event_id: activity
                .get("attentionEventId")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
    )?;
    Ok(true)
}

fn record_attention_left(state: &AppState, session_id: &str) -> Result<bool, DomainStateError> {
    let db = open_gxserver_database_with_busy_timeout(&state.paths, OBSERVER_BUSY_TIMEOUT)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let changed = mark_session_notifications_read(&db, session_id, false)?;
    Ok(changed > 0)
}
