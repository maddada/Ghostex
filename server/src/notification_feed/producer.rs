use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;
use crate::session_status::TURN_COMPLETE_ATTENTION_SOURCE;
use crate::storage::open_gxserver_database_with_busy_timeout;

use super::api::broadcast_notification_feed_changed;
use super::body::last_assistant_message_text;
use super::store::{
    bounded_notification_body, insert_notification_feed_row, mark_session_notifications_read,
    notification_agent_icon, NewNotificationFeedRow, NOTIFICATION_FEED_KIND_BELL,
    NOTIFICATION_FEED_KIND_FINISHED, NOTIFICATION_FEED_KIND_NEEDS_INPUT,
};

pub(super) const OBSERVER_BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// Feed rows are written from the one place every attention transition already passes through, after the endpoint's own transaction committed.
/// Entering attention writes a row; leaving attention reads the session's rows, preserving deferrals on automatic transitions.
/// CDXC:Notifications 2026-09-14 DECISION:
/// User: clearing a session's attention from the sidebar or another acknowledgement action also marks its notification read.
/// Explicit acknowledgement and Escape read even deferred rows and rows whose stored activity already settled before the client acknowledged them.
/// Never fails the request: every error is logged and swallowed.
pub(crate) fn observe_agent_activity_result(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if !matches!(
        endpoint_path,
        "/api/ingestAgentHookEvent" | "/api/ingestTerminalTitleEvent" | "/api/updateAgentActivity"
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
    let explicitly_acknowledged = endpoint_path == "/api/updateAgentActivity"
        && matches!(
            params.get("event").and_then(Value::as_str),
            Some("acknowledge" | "escape")
        )
        && next != "attention"
        && activity.get("isAcknowledged").and_then(Value::as_bool) == Some(true);
    if !entered_attention && !left_attention && !explicitly_acknowledged {
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
        record_attention_left(state, session_id, explicitly_acknowledged)
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

/// What a needs-input row says when the attention is an approval card a PermissionRequest hook
/// stored (the sidebar's red dot, CDXC:SessionStatus 2026-10-27 in server/src/session_approval.rs),
/// so the list names what waits for the user instead of quoting the agent's last message.
fn approval_body(activity: &Map<String, Value>) -> Option<String> {
    use crate::session_chat_interactive::{
        parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
    };
    let stored = activity.get("sessionChatPrompt").and_then(Value::as_str)?;
    match parse_stored_session_chat_prompt(stored)? {
        SessionChatInteractivePrompt::Approval { tool, summary, .. } => Some(match summary {
            Some(summary) if !summary.trim().is_empty() => {
                format!("Waiting for your approval: {tool}: {}", summary.trim())
            }
            _ => format!("Waiting for your approval: {tool}"),
        }),
        SessionChatInteractivePrompt::Question { .. } => None,
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
        _ => approval_body(activity)
            .or_else(|| last_assistant_message_text(session))
            .map(|text| bounded_notification_body(&text))
            .unwrap_or_else(|| "Waiting for your input".to_string()),
    };
    let db = open_gxserver_database_with_busy_timeout(&state.paths, OBSERVER_BUSY_TIMEOUT)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (title, subtitle) = session_row_heading(&repository, project_id, session)?;
    let row = insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            kind,
            title,
            subtitle,
            body,
            agent_name: notification_agent_icon(session),
            attention_event_id: activity
                .get("attentionEventId")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
    )?;
    // CDXC:Notifications 2026-11-08: the row is the bell's either way; an attention the rule says
    // does not alert (a thread's finished turn, a coordinator's with nothing for the user, an
    // approval Autopilot answers) is written already read and goes to no phone, so the bell, the
    // phone and the computer's sound agree (packages/gx-core/src/alert_rule.rs).
    if attention_row_alerts(&db, session, project_id, session_id) {
        crate::push::announce_feed_row(state, &repository, session, &row);
    } else if let Some(id) = row.get("id").and_then(Value::as_str) {
        super::store::mark_notification_feed_row_read(&db, id)?;
    }
    Ok(true)
}

/// The same verdict the presentation stamps on this attention (`attention.alerts`), read from the
/// live session the endpoint just stored.
fn attention_row_alerts(
    db: &rusqlite::Connection,
    session: &Value,
    project_id: &str,
    session_id: &str,
) -> bool {
    let now = crate::presentation::now_iso();
    let attention = crate::presentation::attention_state(session, &now);
    let role = crate::presentation::recorded_alert_role(db, project_id, session_id);
    let awaiting_approval = crate::session_approval::session_awaits_approval(session, &now);
    crate::presentation::attention_state_alerts(
        role,
        &attention,
        awaiting_approval,
        project_id,
        session_id,
    )
}

/// A session row's title (the session's title) and subtitle (its project's name).
pub(super) fn session_row_heading(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session: &Value,
) -> Result<(String, String), DomainStateError> {
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
    Ok((title, subtitle))
}

fn record_attention_left(
    state: &AppState,
    session_id: &str,
    include_deferred: bool,
) -> Result<bool, DomainStateError> {
    let db = open_gxserver_database_with_busy_timeout(&state.paths, OBSERVER_BUSY_TIMEOUT)
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let changed = mark_session_notifications_read(&db, session_id, include_deferred)?;
    Ok(changed > 0)
}
