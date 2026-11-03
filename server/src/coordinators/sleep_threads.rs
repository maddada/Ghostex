//! Which of an orchestrator's threads its Sleep Inactive Threads puts to sleep.

use serde_json::Value;

use super::records::SessionKey;

/// The project menu's Sleep Inactive test over one presentation session: awake, neither working
/// nor waiting on the user, no unanswered question card, no background shell or monitor running.
///
/// CDXC:Coordinators 2026-10-13 DECISION:
/// User: "right click on orchestrator and sleep", "Should just be sleep inactive not park"; asked whether the phone gets the item too: yes, with the same rule as the computer. `ghostex orchestrator sleep-inactive-threads` (which the phone's orchestrator menu runs) sleeps only the orchestrator's own threads that pass this test; the orchestrator, its threads' own threads and every other session are left alone, and nothing is parked.
/// SEE-ALSO: `is_inactive` and `coordinator_thread_keys` in packages/gx-core/src/sidebar_actions/bulk.rs (the desktop's twin over the same presentation fields; the two must keep the same test), apps/mobile/app/src/screens/sessions-screen/sidebar-menus.ts (the phone's row).
pub fn is_sleep_inactive_session(session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    let pending_questions = session
        .get("pendingQuestionCount")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|count| count as u64))
        })
        .unwrap_or(0);
    text("lifecycleState") == Some("running")
        && !matches!(text("activity"), Some("working" | "attention"))
        && !ghostex_gx_protocol::session_has_question(
            pending_questions,
            session.get("awaitingAnswer").and_then(Value::as_bool) == Some(true),
        )
        && session
            .get("backgroundWorkDetectedAt")
            .is_none_or(Value::is_null)
        // A thread gxserver reads as working by its transcript, though its hooks say it stopped
        // (live.rs).
        && text("coordinatorThreadState") != Some("working")
}

/// Whether a session is one of this orchestrator's own threads (not a thread of one of its
/// threads, and never the orchestrator itself).
fn is_thread_of(session: &Value, coordinator: &SessionKey) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    text("coordinatorProjectId") == Some(coordinator.0.as_str())
        && text("coordinatorSessionId") == Some(coordinator.1.as_str())
        && !(text("projectId") == Some(coordinator.0.as_str())
            && text("sessionId") == Some(coordinator.1.as_str()))
}

/// The orchestrator's threads Sleep Inactive Threads sleeps, in the snapshot's session order.
pub fn inactive_thread_sessions(sessions: &[Value], coordinator: &SessionKey) -> Vec<SessionKey> {
    sessions
        .iter()
        .filter(|session| is_thread_of(session, coordinator) && is_sleep_inactive_session(session))
        .filter_map(|session| {
            Some((
                session.get("projectId")?.as_str()?.to_string(),
                session.get("sessionId")?.as_str()?.to_string(),
            ))
        })
        .collect()
}
