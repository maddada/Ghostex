//! A row's Delayed Send item (`sessionAction: delayedSend`): open the Delayed Send dialog seeded
//! with what the row already shows.
//!
//! CDXC:DelayedSend 2026-09-21 WHY:
//! Followed to its last function, the old path was ONE app-modal-host message and nothing else:
//! `runNativeSessionAction` (sidebar-page-frozen/session-actions.ts) called `openAppModal` with eleven
//! fields read off the sidebar store's session, and `openAppModal` was `postAppModalHostMessage`,
//! whose `open` arm is `open_app_modal_from_bridge`. Unlike Rename and Note there is NO close
//! first, and the dialog forwards this payload VERBATIM (the Delayed Send kind is not on the
//! bridge's flat-field allowlist), so every field, including the booleans and the specific-agent
//! reference object, reaches the dialog exactly as built here. The arming the user then does comes
//! back as its own `scheduleDelayedSend` message and never passes through the sidebar's funnel.
//!
//! The seeds are the row's: the title Rename would show, the daemon's Delayed Send when it
//! published one and the host's own timer otherwise (the projection's `serverDelayedSend ??
//! resolveDelayedSend` precedence, reproduced in `sidebar_view/rows.rs`), and the host's Close
//! After Done. The two `supports…` flags were constants in the TypeScript and are here too; the
//! bridge's own enrichment recomputes the project-scope one for a local pane after this.
//!
//! Ported from the sidebar page's `runNativeSessionAction` (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/session-actions.ts`; see git history).
//!
//! SEE-ALSO: apps/desktop/src/app/remote_conn/app_modal_bridge.rs (`open_app_modal_from_bridge`),
//! apps/desktop/src/app/gx_store/sidebar_state_actions.rs.

use serde_json::{Map, Value};

use crate::core::Core;
use crate::keys::SessionKey;
use crate::sidebar_view::{DelayedSendView, SidebarView};

use super::modals::rename_seed_title;
use super::plan::{ActionEffect, SidebarActionPlan};
use super::resolve::{drawn_row, text_field};

/// Whether this renderer command is the Delayed Send item, without resolving the row.
pub fn owns_delayed_send_command(command: &Value) -> bool {
    text_field(command, "type") == Some("sessionAction")
        && text_field(command, "action") == Some("delayedSend")
}

/// The dialog the item opens, or `None` when the row is not drawn.
///
/// `None` is a hand-off, not an answer: the TypeScript reads the FULL sidebar store and the store
/// reads the drawn list, so a row the list is not drawing goes to the old runtime, the same
/// decline Rename and Note make (a context menu can only be opened on a drawn row).
pub fn plan_delayed_send_action(view: &SidebarView, command: &Value) -> Option<SidebarActionPlan> {
    if !owns_delayed_send_command(command) {
        return None;
    }
    let sidebar_session_id = text_field(command, "sessionId")?;
    let row = drawn_row(view, sidebar_session_id)?;
    let title = rename_seed_title(
        row.menu_facts.primary_title.as_deref(),
        row.menu_facts.terminal_title.as_deref(),
        &row.alias,
    );
    let delayed = row.delayed_send.as_ref();
    let mut open = Map::new();
    let mut text = |key: &str, value: &str| {
        open.insert(key.to_string(), Value::String(value.to_string()));
    };
    text("type", "open");
    text("modal", "delayedSend");
    text("title", &title);
    text("sessionId", sidebar_session_id);
    // Every optional field below is ABSENT when the TypeScript's value is `undefined`, because
    // `JSON.stringify` drops the key and the dialog tells an absent key from a null one.
    if let Some(agent_icon) = &row.agent_icon {
        text("agentIcon", agent_icon);
    }
    insert_delayed_send_seed(&mut open, delayed);
    let mut flag = |key: &str, value: bool| {
        open.insert(key.to_string(), Value::Bool(value));
    };
    flag(
        "closeAfterDoneActive",
        row.close_after_done
            .as_ref()
            .is_some_and(|close| close.armed),
    );
    flag("supportsSendWhenAgentStops", true);
    flag("supportsSendWhenAllProjectSessionsStop", true);
    Some(SidebarActionPlan::one(ActionEffect::OpenAppModal {
        payload: Value::Object(open),
    }))
}

/// The armed Delayed Send of one session, as the dialog's seed fields: what a Delayed Send opened
/// by the hotkey or a session's own bar adds to its open message, so it shows the countdown or the
/// armed trigger the row's menu item shows. It reads the drawn row, which is where the menu item
/// reads it, and the session's daemon state when the row is not drawn (a collapsed group). Empty
/// when neither has one armed; the host's own timers are the host's to add.
///
/// CDXC:DelayedSend 2026-09-25 WHY:
/// The hotkey's open message names the session by its shell id, which no sidebar row matches, so it carried no daemon trigger and an armed send opened on "After a delay". Both entry points now seed from the same row, so they show the same state.
pub fn delayed_send_seed(
    view: &SidebarView,
    core: &Core,
    session: &SessionKey,
) -> Map<String, Value> {
    let mut open = Map::new();
    let delayed = match drawn_row(view, &session.to_sidebar_session_id()) {
        Some(row) => row.delayed_send.clone(),
        None => core
            .presentation()
            .session(session)
            .and_then(|session| crate::sidebar_view::rows::delayed_send(&session, None)),
    };
    insert_delayed_send_seed(&mut open, delayed.as_ref());
    open
}

/// The five Delayed Send fields of the dialog's open message. Every optional one is ABSENT when
/// the TypeScript's value was `undefined`, because the dialog tells an absent key from a null one.
fn insert_delayed_send_seed(open: &mut Map<String, Value>, delayed: Option<&DelayedSendView>) {
    if let Some(deadline_at) = delayed.and_then(|delayed| delayed.deadline_at.as_deref()) {
        open.insert(
            "delayedSendDeadlineAt".to_string(),
            Value::String(deadline_at.to_string()),
        );
    }
    if let Some(label) = delayed.and_then(|delayed| delayed.remaining_label.as_deref()) {
        open.insert(
            "delayedSendRemainingLabel".to_string(),
            Value::String(label.to_string()),
        );
    }
    open.insert(
        "sendWhenAllProjectSessionsStopActive".to_string(),
        Value::Bool(
            delayed.is_some_and(|delayed| delayed.send_when_all_project_sessions_stop_active),
        ),
    );
    open.insert(
        "sendWhenAgentStopsActive".to_string(),
        Value::Bool(delayed.is_some_and(|delayed| delayed.send_when_agent_stops_active)),
    );
    // Passed on as the daemon sent it. A daemon `null` is the one shape that differs: the protocol
    // reads it as absent while the TypeScript copies `null` through (declared difference 40).
    if let Some(agent) =
        delayed.and_then(|delayed| delayed.send_when_specific_agent_finishes.as_ref())
    {
        open.insert("sendWhenSpecificAgentFinishes".to_string(), agent.clone());
    }
}
