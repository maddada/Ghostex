//! Where a chat's work items come from: a coordinator's thread list (`coordinatorThreads.work`),
//! or an ordinary session's own (`sessionWork`, while the Work items extension is on). Every reader
//! of the Work panel, its dock rows and the chat's item cards goes through here, so an ordinary
//! session gets them exactly as a coordinator does; `coordinator_threads::is_coordinator` still
//! reads only the thread list, so nothing else of a coordinator's reaches an ordinary session.
//!
//! CDXC:Coordinators 2026-11-18 SEE-ALSO: server/src/coordinators/session_work.rs (the user's
//! decision and the field), packages/gx-chat-core/src/extras/coordinator_board.rs (the panel).

use serde_json::Value;

use crate::state::ChatState;

/// An ordinary session's own work, while the extension is on.
pub fn own_work(state: &ChatState) -> Option<&Value> {
    state
        .session
        .session_work
        .as_ref()
        .filter(|work| work["enabled"] == true)
}

/// The value holding `work`: the coordinator's thread list, else the session's own.
pub fn work_holder(state: &ChatState) -> Option<&Value> {
    state
        .session
        .coordinator_threads
        .as_deref()
        .or_else(|| own_work(state))
}

/// The chat's work items value (`{items, asks, counts, topic?}`), if it keeps any.
pub fn work_of(state: &ChatState) -> Option<&Value> {
    work_holder(state).and_then(|holder| holder.get("work"))
}

/// The line at the top of the Work panel (`{text, byUser}`), `Value::Null` for none.
pub fn topic(state: &ChatState) -> Value {
    work_of(state)
        .and_then(|work| work.get("topic"))
        .filter(|topic| topic["text"].as_str().is_some_and(|text| !text.is_empty()))
        .cloned()
        .unwrap_or(Value::Null)
}
