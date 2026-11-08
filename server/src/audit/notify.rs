//! The notification for each sensitive action: a row in the notification bell of every client of
//! this computer and a push to its paired phones.

use std::sync::{Arc, OnceLock};

use crate::logging::{GxserverLogInput, LogLevel};
use crate::notification_feed::store::{
    insert_notification_feed_row, NewNotificationFeedRow, NOTIFICATION_FEED_KIND_SECURITY,
    SECURITY_NOTIFICATION_PROJECT_ID,
};
use crate::server::AppState;

use super::chain::StoredEntry;
use super::types::Action;

static STATE: OnceLock<Arc<AppState>> = OnceLock::new();

/// Called once at gxserver start, so entries written from then on notify.
pub(crate) fn install_notifier(state: Arc<AppState>) {
    if let Err(error) = super::live_log().prime() {
        log(&state, "securityLog.readFailed", format!("{error:#}"));
    }
    let _ = STATE.set(state);
}

/// The feed row's `sessionId`: one row per entry, never superseded by the next one.
pub(crate) fn security_notification_session_id(seq: u64) -> String {
    format!(
        "{}{seq}",
        crate::notification_feed::store::SECURITY_NOTIFICATION_SESSION_PREFIX
    )
}

/// The bell row's title, subtitle and body for an entry.
pub(crate) fn notification_text(entry: &StoredEntry) -> (String, String, String) {
    let outcome = match entry.outcome.as_str() {
        "pending" => "Waiting for your approval",
        "approved" => "Approved by you",
        "denied" => "You said no",
        "refused" => "Refused",
        "failed" => "Failed",
        _ => "Done",
    };
    let mut body = format!("{outcome}. Asked by {}.", entry.actor.label);
    if let Some(reason) = &entry.reason {
        body.push(' ');
        body.push_str(reason);
    }
    let subtitle = entry
        .target
        .clone()
        .unwrap_or_else(|| "Security".to_string());
    (entry.summary.clone(), subtitle, body)
}

pub(super) fn announce(entry: &StoredEntry) {
    let Some(state) = STATE.get() else {
        return;
    };
    let (title, subtitle, body) = notification_text(entry);
    let session_id = security_notification_session_id(entry.seq);
    let row = crate::storage::open_gxserver_database(&state.paths)
        .map_err(|error| error.to_string())
        .and_then(|db| {
            insert_notification_feed_row(
                &db,
                NewNotificationFeedRow {
                    project_id: SECURITY_NOTIFICATION_PROJECT_ID.into(),
                    session_id: session_id.clone(),
                    kind: NOTIFICATION_FEED_KIND_SECURITY,
                    title: title.clone(),
                    subtitle,
                    body: crate::notification_feed::store::bounded_notification_body(&body),
                    agent_name: None,
                    attention_event_id: None,
                },
            )
            .map_err(|error| error.message)
        });
    match row {
        Ok(row) => {
            crate::notification_feed::broadcast_notification_feed_changed(state);
            let id = row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            state.push.enqueue_alert(crate::push::message::AlertEvent {
                kind: crate::push::message::AlertKind::Security,
                project_id: SECURITY_NOTIFICATION_PROJECT_ID.into(),
                session_id,
                notification_id: id,
                agent_name: "Ghostex".to_string(),
                project_name: title,
                occurred_at_ms: entry.at_ms,
            });
        }
        Err(error) => log(state, "securityLog.notifyFailed", error),
    }
}

pub(super) fn report_write_failure(action: &Action, error: &anyhow::Error) {
    if let Some(state) = STATE.get() {
        log(
            state,
            "securityLog.writeFailed",
            format!("{}: {error:#}", action.kind),
        );
    }
}

fn log(state: &AppState, event: &str, error: String) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Error,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error),
        details: None,
    });
}
