//! The armed Delayed Send and Close After Done labels the chat's working row draws, for EVERY
//! session rather than only the rows the sidebar list shows.
//!
//! CDXC:SessionChat 2026-09-21 SEE-ALSO:
//! apps/desktop/src/app/native_chat/working_strip.rs draws the labels. Ported from
//! packages/shared/session-chat-presentation/armed-actions.ts (deleted 2026-09-25).
//!
//! CDXC:SessionChat 2026-09-21 WHY:
//! Drawn from every machine's presentation rather than from the sidebar view, because the view is
//! the DRAWN list: a session the machine filter, the Space, a tag filter or Show Hidden leaves out
//! still has an armed timer and still has a chat open on it. The labels are formatted against the
//! host's clock on its own tick, so a countdown moves every second without the list being rebuilt.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::core::Core;
use crate::keys::SessionKey;

use super::inputs::{CloseAfterDoneInput, SidebarHostInputs};
use super::rows::delayed_send;
use super::session_text::deadline_countdown;
use super::view::DelayedSendView;

/// `SessionChatArmedActionId`, the two ids the working row knows.
pub const ARMED_ACTION_DELAYED_SEND: &str = "delayedSend";
pub const ARMED_ACTION_CLOSE_AFTER_DONE: &str = "closeAfterDone";

/// One indicator on the chat's working row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmedAction {
    /// [`ARMED_ACTION_DELAYED_SEND`] or [`ARMED_ACTION_CLOSE_AFTER_DONE`].
    pub id: &'static str,
    pub label: String,
}

/// Every session that has an armed timer right now, by its sidebar session id. A session with
/// neither is absent rather than present with an empty list.
pub fn armed_actions_by_session(
    core: &Core,
    host: &SidebarHostInputs,
    now_ms: u64,
) -> BTreeMap<String, Vec<ArmedAction>> {
    let mut armed: BTreeMap<String, Vec<ArmedAction>> = BTreeMap::new();
    for (machine, presentation) in core.presentation().machines() {
        let Some(loaded) = presentation.loaded() else {
            continue;
        };
        for session in loaded.server_sessions() {
            let key = SessionKey {
                machine: machine.clone(),
                project_id: session.project_id.clone(),
                session_id: session.session_id.clone(),
            };
            let sidebar_session_id = key.to_sidebar_session_id();
            let close = CloseAfterDoneInput::from_session(session);
            let close = close.as_ref();
            let local = host.local_delayed_sends.get(&sidebar_session_id);
            // Nothing to say about this row at all: the common case, and the one that keeps a tick
            // over a workspace with hundreds of sessions to one map lookup each.
            if close.is_none()
                && local.is_none()
                && session.delayed_send_deadline_at.is_none()
                && session.delayed_send_remaining_label.is_none()
                && session.delayed_send_remaining_ms.is_none()
                && session.send_when_all_project_sessions_stop_active != Some(true)
                && session.send_when_agent_stops_active != Some(true)
            {
                continue;
            }
            // The row as the sidebar would build it: a locally hidden session has none.
            let Some(effective) =
                presentation.effective_session(&session.project_id, &session.session_id)
            else {
                continue;
            };
            let delayed = delayed_send(&effective, local);
            let actions = session_armed_actions(delayed.as_ref(), close, now_ms);
            if !actions.is_empty() {
                armed.insert(sidebar_session_id, actions);
            }
        }
    }
    armed
}

/// `sessionChatArmedActions`.
fn session_armed_actions(
    delayed: Option<&DelayedSendView>,
    close: Option<&CloseAfterDoneInput>,
    now_ms: u64,
) -> Vec<ArmedAction> {
    let mut actions = Vec::new();
    // JavaScript truthiness: `delayedSendDeadlineAt || delayedSendRemainingLabel`, so an empty
    // string arms nothing.
    if let Some(delayed) = delayed.filter(|delayed| {
        text(delayed.deadline_at.as_deref()).is_some()
            || text(delayed.remaining_label.as_deref()).is_some()
    }) {
        actions.push(ArmedAction {
            id: ARMED_ACTION_DELAYED_SEND,
            label: delayed_send_label(delayed, now_ms),
        });
    }
    if let Some(close) = close.filter(|close| {
        close.armed
            || text(close.deadline_at.as_deref()).is_some()
            || text(close.remaining_label.as_deref()).is_some()
    }) {
        let remaining = text(close.deadline_at.as_deref())
            .and_then(|deadline| deadline_countdown(deadline, now_ms))
            .or_else(|| text(close.remaining_label.as_deref()).map(str::to_string));
        actions.push(ArmedAction {
            id: ARMED_ACTION_CLOSE_AFTER_DONE,
            label: match remaining {
                Some(remaining) => format!("Close After Done in {remaining}"),
                None => "Close After Done armed".to_string(),
            },
        });
    }
    actions
}

/// `delayedSendLabel`.
fn delayed_send_label(delayed: &DelayedSendView, now_ms: u64) -> String {
    let countdown = text(delayed.deadline_at.as_deref())
        .and_then(|deadline| deadline_countdown(deadline, now_ms));
    if let Some(countdown) = countdown {
        return format!("Delayed Send in {countdown}");
    }
    let remaining = text(delayed.remaining_label.as_deref());
    match remaining {
        Some("Waiting for agent") => {
            if is_truthy(delayed.send_when_specific_agent_finishes.as_ref()) {
                "Delayed Send when the chosen agent finishes".to_string()
            } else {
                "Delayed Send when the agent finishes".to_string()
            }
        }
        Some("Waiting for agents") => "Delayed Send when all agents finish".to_string(),
        Some(remaining) => format!("Delayed Send in {remaining}"),
        None => "Delayed Send armed".to_string(),
    }
}

/// A string that is truthy in JavaScript.
fn text(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// `sendWhenSpecificAgentFinishes` under JavaScript truthiness: the daemon publishes it as an
/// object or as an agent id, and only a missing, null, false, empty or zero value is "no".
fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Number(number)) => number.as_f64().is_some_and(|number| number != 0.0),
        Some(_) => true,
    }
}
