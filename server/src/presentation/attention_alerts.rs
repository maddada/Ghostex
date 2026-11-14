//! gxserver's verdicts on what alerts, stamped on the presentation so every client agrees: whether
//! an attention alerts (`attention.alerts`), whether Autopilot is healing a stuck session
//! (`stuckHandledBy`), and whether a session is blocked (`blockedSince`, `blockedReason`). The rule
//! is gx-core's `alert_rule::should_alert`; this file only gathers its inputs, the ones only
//! gxserver knows (the coordinator records, Autopilot's choices, account recovery).
//!
//! CDXC:Notifications 2026-11-08 SEE-ALSO: packages/gx-core/src/alert_rule.rs (the decision and the
//! user's words).

use ghostex_gx_core::alert_rule::{
    alert_role, attention_trigger, should_alert, AlertHandling, AlertRole, AlertTrigger,
};
use ghostex_gx_core::AttentionAlerts;
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

/// Whether an attention with this `attention_state` alerts (Alert me for at All; clients narrow it).
pub(crate) fn attention_state_alerts(
    role: AlertRole,
    attention: &Value,
    awaiting_approval: bool,
    project_id: &str,
    session_id: &str,
) -> bool {
    let trigger = attention_trigger(
        attention["kind"].as_str() == Some("finished"),
        attention["handoverNeedsYou"].as_bool() == Some(true),
        awaiting_approval,
    );
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
    let Some(attention) = session
        .get_mut("attention")
        .filter(|value| value.is_object())
    else {
        return;
    };
    let alerts =
        attention_state_alerts(role, attention, awaiting_approval, &project_id, &session_id);
    attention["alerts"] = Value::Bool(alerts);
}

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
        _ => "Blocked: it stopped on an error Ghostex couldn't get past.",
    };
    Some((since.to_string(), reason.to_string()))
}
