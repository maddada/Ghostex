//! A coordinator's turn that ends while its threads still work is stored as already seen: no
//! dot, no Notifications row, no sound, banner or push, on any client. Decided once, where the
//! turn end is stored, so a later thread finishing never raises it after the fact.
//!
//! CDXC:Notifications 2026-11-14 DECISION:
//! User: "please don't show the blue dot on any coordinator while it has workers still working. ur
//! spamming me with notifications this is annoying af. only 1 notification when u send me when all
//! done and no running workers. dont put blue dot on sidebar if not all workers done under the
//! coordinator". So:
//! - A coordinator's turn that ends while any of its threads (on this computer or another) is
//!   working stays quiet: it enters no attention (the acknowledged idle a sidebar click
//!   leaves), so no blue dot and no notification. When the last thread finishes later, nothing is
//!   raised then either: the thread's report wakes the coordinator, and its next turn end decides.
//! - A coordinator's turn that ends with no thread working gets the dot and one notification (the
//!   alert rule's `Coordinator` finished turn, packages/gx-core/src/alert_rule.rs, under Alert me
//!   for). Supersedes the part of the 2026-11-13 decision there that kept every plain finished
//!   coordinator turn quiet.
//! - What needs the user still alerts while threads work: a new `item ask --holds` from that turn
//!   (the turn keeps its dot and alert), a question form it asked (open questions raise it,
//!   session_chat_open_questions.rs), and any approval, question card or red dot, which are not
//!   finished turns.
//! SEE-ALSO: server/src/agents/activity.rs and server/src/agents/session_state_ingest.rs (where it
//! is applied), packages/gx-core/src/alert_rule.rs (the alert rule).

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::live::classify_thread_live;
use super::records::{list_threads_for, read_coordinator, SessionKey};
use super::remote_threads::list_remote_threads_for;
use super::state::{ThreadProgress, ThreadState};
use super::work::{list_work_items, WorkItem};
use crate::domain::DomainRepository;
use crate::session_status::{
    settle_entered_turn_end_as_seen, update_enters_turn_complete_attention, ActivityUpdate,
};

/// Stores the coordinator's finished turn `update` enters as already seen while its threads still
/// work and the turn put no holding ask on its board. Returns whether it did, so the caller can
/// tell the turn-end observers (`quietTurnEnd` on the endpoint's result).
pub(crate) fn quiet_coordinator_turn_end(
    repository: &DomainRepository<'_>,
    session: &Value,
    update: &mut ActivityUpdate,
) -> bool {
    if !update_enters_turn_complete_attention(update)
        || update.activity.get("sessionChatPrompt").is_some()
        || crate::agents::session_chat_prompt_setting(session).is_some()
    {
        return false;
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or_default();
    let key: SessionKey = (text("projectId").to_string(), text("sessionId").to_string());
    let db = repository.connection();
    if !read_coordinator(db, &key.0, &key.1).is_ok_and(|record| record.is_some()) {
        return false;
    }
    if !coordinator_threads_still_working(repository, &key) {
        return false;
    }
    let turn_started = update
        .activity
        .get("workingStartedAt")
        .and_then(Value::as_str)
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc));
    if let Some(since) = turn_started {
        if list_work_items(db, &key).is_ok_and(|items| raised_holding_ask_since(&items, since)) {
            return false;
        }
    }
    settle_entered_turn_end_as_seen(update);
    true
}

/// Whether any open thread of the coordinator is working: one here as the sidebar classifies it
/// (`coordinatorThreadState` `working`, a thread whose brief has not run yet included), or one on
/// another computer last seen working. A waiting thread does not count: one that waits on the user
/// alerts by itself, and one stuck waiting must not keep its coordinator quiet for good.
fn coordinator_threads_still_working(repository: &DomainRepository<'_>, key: &SessionKey) -> bool {
    let db = repository.connection();
    let now = crate::presentation::now_iso();
    let local = list_threads_for(db, &key.0, &key.1).is_ok_and(|threads| {
        threads
            .iter()
            .filter(|thread| !thread.is_resolved() && thread.key() != *key)
            .any(|thread| {
                let session = repository
                    .get_session(&thread.project_id, &thread.session_id)
                    .ok()
                    .flatten();
                classify_thread_live(session.as_ref(), ThreadProgress::of(thread), &now)
                    == ThreadState::Working
            })
    });
    local
        || list_remote_threads_for(db, key).is_ok_and(|threads| {
            threads.iter().any(|thread| {
                thread.resolved_at.is_none()
                    && thread.last_state.as_deref() == Some(ThreadState::Working.as_str())
            })
        })
}

/// Whether an open ask that holds the work up (`item ask --holds`) was opened since `since`, on an
/// item still open.
fn raised_holding_ask_since(items: &[WorkItem], since: DateTime<Utc>) -> bool {
    items
        .iter()
        .filter(|item| item.finished_at.is_none() && item.dropped_at.is_none())
        .flat_map(|item| item.asks.iter())
        .any(|ask| {
            ask.is_open()
                && ask.holds
                && !ask.withdrawn
                && DateTime::parse_from_rfc3339(&ask.asked_at)
                    .is_ok_and(|at| at.with_timezone(&Utc) >= since)
        })
}
