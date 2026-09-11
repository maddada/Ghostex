use serde_json::{json, Map, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::domain::{
    optional_trimmed_string_param, read_unvalidated_project_lookup_id,
    read_unvalidated_session_lookup_id, DomainRepository, DomainResult, DomainStateError,
};
use crate::server::AppState;

use super::store::{
    bounded_notification_body, clear_notification_feed, defer_session_notification,
    dismiss_notification, insert_notification_feed_row, mark_all_notifications_read,
    mark_notification_read, mark_notification_unread, mark_session_notifications_read,
    notification_agent_icon, notification_exists, read_notification_feed_state,
    NewNotificationFeedRow, NOTIFICATION_FEED_KIND_CUSTOM,
};

pub(crate) const NOTIFICATION_FEED_READ_ENDPOINT: &str = "/api/readNotificationFeed";
pub(crate) const NOTIFICATION_FEED_UPDATE_ENDPOINT: &str = "/api/updateNotificationFeed";
pub(crate) const NOTIFICATION_FEED_CREATE_ENDPOINT: &str = "/api/createNotification";
const NOTIFICATION_FEED_CHANGED_EVENT_TYPE: &str = "notificationFeedChanged";

/// The change announcement carries no rows: `/api/readNotificationFeed` stays the single projection, so every client refetches and sees the same list.
pub(crate) fn broadcast_notification_feed_changed(state: &AppState) {
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": NOTIFICATION_FEED_CHANGED_EVENT_TYPE,
    }));
}

pub(crate) fn read_notification_feed_endpoint(db: &rusqlite::Connection) -> DomainResult<Value> {
    read_notification_feed_state(db)
}

pub(crate) fn update_notification_feed_endpoint(
    state: &AppState,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> DomainResult<Value> {
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    match action {
        "markRead" | "markUnread" | "dismiss" => {
            let id = required_text(params, "notificationId")?;
            if !notification_exists(db, &id)? {
                return Err(DomainStateError::not_found(format!(
                    "Notification \"{id}\" was not found."
                )));
            }
            match action {
                "markRead" => mark_notification_read(db, &id)?,
                "markUnread" => mark_notification_unread(db, &id)?,
                _ => dismiss_notification(db, &id)?,
            };
        }
        "markSessionRead" => {
            let session_id = required_text(params, "sessionId")?;
            mark_session_notifications_read(db, &session_id, true)?;
        }
        "deferUnread" => {
            let session_id = required_text(params, "sessionId")?;
            defer_session_notification(db, &session_id)?;
        }
        "markAllRead" => {
            mark_all_notifications_read(db)?;
        }
        "clearAll" => {
            clear_notification_feed(db)?;
        }
        "" => {
            return Err(DomainStateError::bad_request(
                "action is required.".to_string(),
            ))
        }
        other => {
            return Err(DomainStateError::bad_request(format!(
                "Unknown notification feed action \"{other}\"."
            )))
        }
    }
    let feed = read_notification_feed_state(db)?;
    broadcast_notification_feed_changed(state);
    Ok(feed)
}

/// `ghostex notify` and hooks post their own rows here; they ride the same supersede, prune, and change announcement as attention rows.
pub(crate) fn create_notification_endpoint(
    state: &AppState,
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
) -> DomainResult<Value> {
    let project_id = read_unvalidated_project_lookup_id(params);
    let session_id = read_unvalidated_session_lookup_id(params);
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session \"{session_id}\" was not found in project \"{project_id}\"."
            ))
        })?;
    let canonical_project_id = session
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(project_id);
    let canonical_session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(session_id);
    let title = optional_trimmed_string_param(params, "title")?.unwrap_or_else(|| {
        session
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Session")
            .to_string()
    });
    let body = optional_trimmed_string_param(params, "body")?
        .map(|body| bounded_notification_body(&body))
        .unwrap_or_default();
    let subtitle = repository
        .get_project(&canonical_project_id)?
        .and_then(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let item = insert_notification_feed_row(
        db,
        NewNotificationFeedRow {
            project_id: canonical_project_id,
            session_id: canonical_session_id,
            kind: NOTIFICATION_FEED_KIND_CUSTOM,
            title,
            subtitle,
            body,
            agent_name: notification_agent_icon(&session),
            attention_event_id: None,
        },
    )?;
    let feed = read_notification_feed_state(db)?;
    broadcast_notification_feed_changed(state);
    Ok(json!({
        "feed": feed,
        "notification": item,
    }))
}

fn required_text(params: &Map<String, Value>, key: &str) -> DomainResult<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}
