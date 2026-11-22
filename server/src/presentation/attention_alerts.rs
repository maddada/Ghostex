//! gxserver's verdicts on what alerts, stamped on the presentation so every client agrees: whether
//! an attention alerts (`attention.alerts`), whether Autopilot is healing a stuck session
//! (`stuckHandledBy`), and whether a session is blocked (`blockedSince`, `blockedReason`). The rule
//! is gx-core's `alert_rule::should_alert`; this file only gathers its inputs, the ones only
//! gxserver knows (the coordinator records, Autopilot's choices, account recovery).
//!
//! CDXC:Notifications 2026-11-08 SEE-ALSO: packages/gx-core/src/alert_rule.rs (the decision and the
//! user's words).

use ghostex_gx_core::AttentionAlerts;
use ghostex_gx_core::alert_rule::{
    AlertHandling, AlertRole, AlertTrigger, alert_role, attention_trigger, should_alert,
};
use rusqlite::Connection;
use serde_json::Value;

/// The session's role from the coordinator records (for a raw row, which has no presentation
/// fields yet).
pub(crate) fn recorded_alert_role(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> AlertRole {
    let thread = crate::coordinators::read_thread(db, project_id, session_id)
        .ok()
        .flatten();
    if let Some(thread) = thread {
        return alert_role(Some("thread"), Some(&thread.coordinator_session_id));
    }
    let coordinator = crate::coordinators::read_coordinator(db, project_id, session_id)
        .ok()
        .flatten()
        .is_some();
    alert_role(coordinator.then_some("coordinator"), None)
}

/// The role the alert rule sees: a coordinator's thread counts as any session while Settings >
/// Notifications > Workers is Sound (CDXC:Notifications 2026-11-14 in server/src/worker_alerts.rs).
pub(crate) fn alert_role_for_rule(role: AlertRole) -> AlertRole {
    if role == AlertRole::Thread
        && crate::worker_alerts::worker_alerts() == crate::worker_alerts::WorkerAlerts::Sound
    {
        AlertRole::Session
    } else {
        role
    }
}

/// Whether an attention with this `attention_state` alerts (Alert me for at All; clients narrow it).
/// `has_question`: the session waits on a question (a question card, an async question or an open
/// form), which in a coordinator's thread is the coordinator's to answer (Q3244 in
/// server/src/worker_alerts.rs) and alerts only while Workers is Sound.
pub(crate) fn attention_state_alerts(
    role: AlertRole,
    attention: &Value,
    awaiting_approval: bool,
    has_question: bool,
    project_id: &str,
    session_id: &str,
) -> bool {
    let role = alert_role_for_rule(role);
    let trigger = attention_trigger(
        attention["kind"].as_str() == Some("finished"),
        attention["handoverNeedsYou"].as_bool() == Some(true),
        awaiting_approval,
    );
    if role == AlertRole::Thread && trigger == AlertTrigger::NeedsInput && has_question {
        return false;
    }
    let autopilot_handles = trigger == AlertTrigger::Approval
        && crate::autopilot::answers_approvals(project_id, session_id);
    should_alert(
        role,
        trigger,
        AlertHandling {
            autopilot_handles,
            healed: false,
        },
        AttentionAlerts::All,
    )
}

/// Stamps `attention.alerts` on one presentation session, once its coordinator fields are in.
pub(crate) fn insert_attention_alerts(session: &mut Value) {
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let role = alert_role(
        text("coordinatorRole").as_deref(),
        text("coordinatorSessionId").as_deref(),
    );
    let (project_id, session_id) = (
        text("projectId").unwrap_or_default(),
        text("sessionId").unwrap_or_default(),
    );
    let awaiting_approval = session.get("awaitingApproval").and_then(Value::as_bool) == Some(true);
    let has_question = session.get("awaitingAnswer").and_then(Value::as_bool) == Some(true)
        || session
            .get("pendingQuestionCount")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0);
    insert_worker_red_dot_stamps(session, role);
    let Some(attention) = session
        .get_mut("attention")
        .filter(|value| value.is_object())
    else {
        return;
    };
    let alerts = attention_state_alerts(
        role,
        attention,
        awaiting_approval,
        has_question,
        &project_id,
        &session_id,
    );
    attention["alerts"] = Value::Bool(alerts);
}

/// Who handles a coordinator's thread's red dot: its coordinator, while Workers is not Sound
/// (CDXC:Notifications 2026-11-14, Q3245 in server/src/worker_alerts.rs). A thread that is not
/// moving (`stuckHandledBy`), or blocked on an error Ghostex could not get past
/// (`blockedAlertedBy`), keeps its red dot without the sound and system notification. Every
/// account out of usage and a sign-in that stopped working still alert: only the user can fix
/// those. A thread Ghostex's restarts gave up on is the user's too (`stuck_handled_by` names
/// nobody for it, and nothing heals it).
fn insert_worker_red_dot_stamps(session: &mut Value, role: AlertRole) {
    if alert_role_for_rule(role) != AlertRole::Thread {
        return;
    }
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let key = (
        text("projectId").unwrap_or_default(),
        text("sessionId").unwrap_or_default(),
    );
    let gave_up = crate::session_self_heal::gave_up_sessions().contains_key(&key);
    let stuck = text("stuckSince").is_some() && text("stuckHandledBy").is_none() && !gave_up;
    let blocked = text("blockedSince").is_some()
        && text("blockedAlertedBy").is_none()
        && text("blockedReason").as_deref() == Some(BLOCKED_ON_ERROR_REASON);
    if stuck {
        session["stuckHandledBy"] = Value::String(WORKER_HANDLED_BY.to_string());
    }
    if blocked {
        session["blockedAlertedBy"] = Value::String(WORKER_HANDLED_BY.to_string());
    }
}

/// `stuckHandledBy` and `blockedAlertedBy` of a coordinator's thread whose red dot its coordinator
/// handles.
pub(crate) const WORKER_HANDLED_BY: &str = "coordinator";

/// The blocked reason of an error Ghostex could not get past (not usage, not a sign-in).
pub(crate) const BLOCKED_ON_ERROR_REASON: &str =
    "Blocked: it stopped on an error Ghostex couldn't get past.";

/// Stamps every session of a presentation snapshot.
pub(crate) fn insert_attention_alerts_presentation_payload(snapshot: &mut Value) {
    if let Some(sessions) = snapshot.get_mut("sessions").and_then(Value::as_array_mut) {
        for session in sessions {
            insert_attention_alerts(session);
        }
    }
}

/// Who heals a stuck session, while someone does: `autopilot` while Autopilot covers it and has
/// not left it for the user. A session Ghostex's restarts gave up on is never "handled": that is
/// already the heal that failed, and Autopilot's one more reload has no report if it fails too.
pub(crate) fn stuck_handled_by(project_id: &str, session_id: &str) -> Option<&'static str> {
    let key = (project_id.to_string(), session_id.to_string());
    if crate::session_self_heal::gave_up_sessions().contains_key(&key) {
        return None;
    }
    crate::autopilot::heals_stuck(project_id, session_id).then_some("autopilot")
}

/// Since when, and why, the session is blocked on something Ghostex could not get past itself:
/// every account out of usage, or an error episode account recovery announced (the failed row's
/// marker, `failureNotifiedAt`). Gone once the session switches, resumes, makes progress or the
/// user starts a new run; never after Stop retrying.
pub(crate) fn blocked_state(session: &Value) -> Option<(String, String)> {
    // A session the memory guard stopped, until it runs again (CDXC:MemoryGuard 2026-11-14 in
    // server/src/memory_guard/card.rs).
    if let Some(blocked) = crate::memory_guard::blocked_by_memory_guard(session) {
        return Some(blocked);
    }
    // A coordinator thread a restart cut off, until it is carried on or dismissed
    // (CDXC:Coordinators 2026-11-14 in server/src/coordinators/stopped_by_restart.rs).
    if let Some(blocked) = crate::coordinators::blocked_by_restart(session) {
        return Some(blocked);
    }
    // A session blocked on a non-account-switched quota limit (Antigravity CLI, Q4461): keeps its
    // red dot until the user clicks Wait for reset or Dismiss, or a real reply clears it.
    if let Some(blocked) = crate::accounts::quota_wait::blocked_state(session) {
        return Some(blocked);
    }
    let runtime = session.get("runtimeSettings")?;
    if runtime["accountRecoverySuppressed"] == true {
        return None;
    }
    if let Some(since) = crate::accounts::limit_wait::blocked_since(session) {
        return Some((
            since,
            "Blocked: every account is out of usage. It carries on when one has room.".to_string(),
        ));
    }
    let recovery = &runtime["accountRecovery"];
    if crate::accounts::limit_wait::limit_gave_up(recovery) {
        let since = recovery["updatedAt"].as_str().unwrap_or_default();
        return Some((
            since.to_string(),
            crate::accounts::limit_wait::LIMIT_GAVE_UP_REASON.to_string(),
        ));
    }
    let since = recovery[crate::accounts::api_errors::FAILURE_NOTIFIED_AT].as_str()?;
    let reason = match recovery["trigger"].as_str() {
        Some("accountDisabled") => "Blocked: its account's sign-in stopped working.",
        _ => BLOCKED_ON_ERROR_REASON,
    };
    Some((since.to_string(), reason.to_string()))
}
