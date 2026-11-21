//! Non-account-switched usage-limit waits (Antigravity CLI, and any agent without saved accounts
//! whose screen reports a `usageLimit` notice with a reset timer).
//!
//! CDXC:AgentProviders 2026-11-22 DECISION:
//! User answered the Antigravity quota Live page (Q4460-Q4463 = all A, with exact UX on Q4461):
//! - While quota-blocked (before user chooses): keep the red dot in the sidebar so the user sees
//!   the session or thread is blocked, alongside the reset countdown badge; stop quiet-monitor
//!   nudges, repeated coordinator spam reports, and Autopilot `TurnCutOff` retries immediately;
//!   and send one deduplicated Grok Bot notification when a coordinator itself hits quota.
//! - User can click **Dismiss** on the card to dismiss the red dot without arming auto-resume, or
//!   click **Wait for reset** to dismiss the red dot, keep a calm waiting/countdown state, and arm
//!   Ghostex to continue the session once automatically right after `resetsAt` (+ 20s buffer).
//! - User can click **Switch model** right on the card to pick another model without Ghostex
//!   silently downgrading.
//! - When a real assistant reply arrives (or user sends/switches model after reset), clear the
//!   quota state automatically.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

use super::{endpoint, launch, recovery, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    session_chat_notice::{
        parse_notice_reset_duration, SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
        SessionChatTerminalNoticeActionKind, SESSION_CHAT_NOTICE_USAGE_LIMIT,
    },
    storage::open_gxserver_database,
};

/// Runtime settings key storing this session's non-account-switched quota wait state.
pub(crate) const RUNTIME_KEY: &str = "quotaLimit";
/// Card action id for "Wait for reset" (`restartAgent` lane so v11.2.1+ phones can click it too).
pub(crate) const ACTION_WAIT_FOR_RESET: &str = "waitForQuotaReset";
/// Card action id for "Dismiss" / "Stop" on a quota-blocked or waiting session.
pub(crate) const ACTION_DISMISS_USAGE_LIMIT: &str = "dismissUsageLimit";
/// Margin after the parsed `resetsAt` before sending the single automatic continuation (15-30s).
const RESUME_BUFFER_SECS: i64 = 20;
/// Fallback delay when the user clicks "Wait for reset" on a quota banner that named no timer.
const UNKNOWN_RESET_FALLBACK_MINUTES: i64 = 5;

fn states() -> &'static Mutex<HashMap<(String, String), Value>> {
    static STATES: OnceLock<Mutex<HashMap<(String, String), Value>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(project_id: &str, session_id: &str) -> (String, String) {
    (project_id.to_string(), session_id.to_string())
}

fn parse_rfc3339(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn current_cached(project_id: &str, session_id: &str) -> Option<Value> {
    states()
        .lock()
        .ok()
        .and_then(|map| map.get(&key(project_id, session_id)).cloned())
}

fn set_cached(project_id: &str, session_id: &str, value: Option<Value>) {
    if let Ok(mut map) = states().lock() {
        let k = key(project_id, session_id);
        match value {
            Some(v) if v.is_object() => {
                map.insert(k, v);
            }
            _ => {
                map.remove(&k);
            }
        }
    }
}

/// Persists `runtimeSettings.quotaLimit` and notifies presentation + chat followers.
fn save_state(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    quota_limit: Option<Value>,
) -> Result<(), DomainStateError> {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return Ok(());
    };
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    match &quota_limit {
        Some(value) if value.is_object() => {
            runtime.insert(RUNTIME_KEY.to_string(), value.clone());
        }
        _ => {
            runtime.remove(RUNTIME_KEY);
        }
    }
    endpoint::update_session_members(repo, session, &runtime, &[RUNTIME_KEY])?;
    set_cached(pid, sid, quota_limit);
    endpoint::publish(state, repo, session)?;
    Ok(())
}

/// Whether `session` is managed by `quota_wait` (agents without Claude/Codex account switching,
/// such as Antigravity CLI).
fn applies_to_session(project: &Value, session: &Value) -> bool {
    launch::provider(project, session).is_none()
}

/// Observes the latest terminal detection for a session: records a new quota-blocked episode,
/// sends a deduplicated Grok Bot alert for coordinators, or clears the episode when a real reply
/// clears the notice.
pub(crate) fn observe(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    notice: Option<&SessionChatTerminalNotice>,
    model_label: Option<&str>,
) {
    let usage_limit_notice = notice.filter(|n| n.kind == SESSION_CHAT_NOTICE_USAGE_LIMIT);
    let cached = current_cached(project_id, session_id);
    // Fast path: nothing cached and no usage-limit notice on screen.
    if usage_limit_notice.is_none() && cached.is_none() {
        return;
    }
    // Fast path: same notice episode already tracked.
    if let (Some(n), Some(existing)) = (usage_limit_notice, cached.as_ref()) {
        if existing["detectedAt"].as_str() == Some(n.detected_at.as_str()) {
            return;
        }
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(session)) = repo.get_session(project_id, session_id) else {
        set_cached(project_id, session_id, None);
        return;
    };
    let Ok(Some(project)) = repo.get_project(project_id) else {
        return;
    };
    if !applies_to_session(&project, &session) {
        return;
    }
    let existing = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())
        .cloned();
    let Some(n) = usage_limit_notice else {
        // Notice is no longer visible (screen clean or cleared by an assistant reply).
        let Some(existing) = existing else {
            set_cached(project_id, session_id, None);
            return;
        };
        let status = existing["status"].as_str().unwrap_or_default();
        let cleared_by_reply =
            crate::session_chat_notice_progress::has_cleared_error(state, project_id, session_id);
        let now = Utc::now();
        let wait_still_future = status == "waiting"
            && parse_rfc3339(existing.get("resumeAt")).is_some_and(|at| at > now);
        // Keep an armed Wait-for-reset alive if no assistant reply has arrived yet and the reset
        // timer is still in the future (a terminal redraw might momentarily scroll the banner out).
        if wait_still_future && !cleared_by_reply {
            return;
        }
        let _ = save_state(state, &repo, &session, None);
        return;
    };
    if let Some(existing) = existing.as_ref() {
        if existing["detectedAt"].as_str() == Some(n.detected_at.as_str()) {
            set_cached(project_id, session_id, Some(existing.clone()));
            return;
        }
        // If the user already dismissed or armed Wait-for-reset for an active episode whose reset
        // has not yet passed and the banner text is the same notice kind, do not re-raise the red
        // dot unless the previous episode was already resumed.
        let status = existing["status"].as_str().unwrap_or_default();
        if matches!(status, "waiting" | "dismissed")
            && existing["evidence"].as_str() == n.details.as_deref()
        {
            set_cached(project_id, session_id, Some(existing.clone()));
            return;
        }
    }
    let now = Utc::now();
    let anchor = DateTime::parse_from_rfc3339(&n.detected_at)
        .ok()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now);
    let duration = parse_notice_reset_duration(n);
    let resets_at = duration.map(|d| anchor + d);
    let resume_at = resets_at.map(|t| t + chrono::Duration::seconds(RESUME_BUFFER_SECS));
    let mut record = json!({
        "status": "blocked",
        "detectedAt": n.detected_at,
        "since": now.to_rfc3339(),
        "resetsAt": resets_at.map(|t| t.to_rfc3339()),
        "resumeAt": resume_at.map(|t| t.to_rfc3339()),
        "model": model_label.filter(|m| !m.trim().is_empty()),
        "evidence": n.details,
    });
    // Q4460: When a coordinator session itself hits quota, send a single Grok Bot alert
    // (deduplicated per episode via `grokbotNotifiedAt`).
    let runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let is_coordinator = crate::agents::session_is_coordinator(&runtime)
        || crate::coordinators::read_coordinator(&db, project_id, session_id)
            .ok()
            .flatten()
            .is_some();
    if is_coordinator {
        let title = session["title"]
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Coordinator");
        let agent_name = crate::presentation::session_agent_name(Some(&project), &session)
            .unwrap_or_else(|| "Antigravity".to_string());
        let project_name = project["name"].as_str();
        let global_ref = crate::ids::create_global_session_ref(
            state.metadata.server_id.as_str(),
            project_id,
            session_id,
        );
        let text = match resets_at {
            Some(at) => format!(
                "Coordinator \"{title}\" hit {agent_name} quota (resets in {}, {}). Paused until reset or model switch.",
                short_countdown(at - now),
                local_clock(at),
            ),
            None => format!(
                "Coordinator \"{title}\" hit {agent_name} quota. Paused until reset or model switch."
            ),
        };
        if crate::grokbot::notify_internal(
            &state.paths,
            "blocked",
            &text,
            project_name,
            Some(&agent_name),
            Some(&global_ref),
        ) {
            record["grokbotNotifiedAt"] = json!(now.to_rfc3339());
        }
    }
    let _ = save_state(state, &repo, &session, Some(record));
}

/// Clears any stored quota wait when the user switches model, sends a message after reset, or a
/// real assistant reply arrives.
pub(crate) fn clear_for_session(state: &AppState, project_id: &str, session_id: &str) {
    if current_cached(project_id, session_id).is_none() {
        return;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        set_cached(project_id, session_id, None);
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(session)) = repo.get_session(project_id, session_id) else {
        set_cached(project_id, session_id, None);
        return;
    };
    let _ = save_state(state, &repo, &session, None);
}

/// Card action: user clicked "Wait for reset" on the quota card (Q4461).
/// Dismisses the sidebar red dot and arms one automatic continuation after `resetsAt` (+ 20s).
pub(crate) async fn answer_wait_for_reset(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let field = |k: &str| {
        params
            .get(k)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("{k} is required.")))
    };
    let (pid, sid) = (field("projectId")?, field("sessionId")?);
    let state_bg = state.clone();
    let (pid_bg, sid_bg) = (pid.clone(), sid.clone());
    tokio::task::spawn_blocking(move || -> Result<(), DomainStateError> {
        let _gate = state_bg.accounts.mutation_gate();
        let db = open_gxserver_database(&state_bg.paths).map_err(store::error)?;
        let repo = DomainRepository::new(&db, &state_bg.metadata.server_id);
        let session = repo
            .get_session(&pid_bg, &sid_bg)?
            .ok_or_else(|| DomainStateError::not_found("This session no longer exists."))?;
        let now = Utc::now();
        let mut record = session
            .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| {
                let notice = crate::session_chat_options::cached_session_chat_terminal_notice(
                    &state_bg, &pid_bg, &sid_bg,
                );
                let duration = notice.as_ref().and_then(parse_notice_reset_duration);
                let resets_at = duration.map(|d| now + d);
                json!({
                    "detectedAt": notice.as_ref().map(|n| n.detected_at.clone()).unwrap_or_else(|| now.to_rfc3339()),
                    "since": now.to_rfc3339(),
                    "resetsAt": resets_at.map(|t| t.to_rfc3339()),
                })
            });
        let resets_at = parse_rfc3339(record.get("resetsAt"));
        let resume_at = resets_at
            .map(|t| t + chrono::Duration::seconds(RESUME_BUFFER_SECS))
            .unwrap_or_else(|| now + chrono::Duration::minutes(UNKNOWN_RESET_FALLBACK_MINUTES));
        record["status"] = json!("waiting");
        record["resumeAt"] = json!(resume_at.to_rfc3339());
        record["updatedAt"] = json!(now.to_rfc3339());
        save_state(&state_bg, &repo, &session, Some(record))?;
        super::api_errors::wake_at(&state_bg, &pid_bg, &sid_bg, resume_at);
        let detector = crate::session_chat_options::SessionChatOptionDetector::new(&state_bg);
        detector.publish_screen_state(&pid_bg, &sid_bg);
        Ok(())
    })
    .await
    .map_err(|_| DomainStateError::bad_request("Could not arm wait for reset."))?
}

/// Card action: user clicked "Dismiss" (on blocked card) or "Stop" (on waiting card) (Q4461).
/// Dismisses the sidebar red dot and cancels any armed auto-resume.
pub(crate) async fn answer_dismiss(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let field = |k: &str| {
        params
            .get(k)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("{k} is required.")))
    };
    let (pid, sid) = (field("projectId")?, field("sessionId")?);
    let state_bg = state.clone();
    let (pid_bg, sid_bg) = (pid.clone(), sid.clone());
    tokio::task::spawn_blocking(move || -> Result<(), DomainStateError> {
        let _gate = state_bg.accounts.mutation_gate();
        let db = open_gxserver_database(&state_bg.paths).map_err(store::error)?;
        let repo = DomainRepository::new(&db, &state_bg.metadata.server_id);
        let Some(session) = repo.get_session(&pid_bg, &sid_bg)? else {
            set_cached(&pid_bg, &sid_bg, None);
            return Ok(());
        };
        let now = Utc::now();
        let mut record = session
            .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        record["status"] = json!("dismissed");
        record["resumeAt"] = Value::Null;
        record["updatedAt"] = json!(now.to_rfc3339());
        save_state(&state_bg, &repo, &session, Some(record))?;
        let detector = crate::session_chat_options::SessionChatOptionDetector::new(&state_bg);
        detector.publish_screen_state(&pid_bg, &sid_bg);
        Ok(())
    })
    .await
    .map_err(|_| DomainStateError::bad_request("Could not dismiss quota alert."))?
}

/// Attaches `quotaWait` (and a backwards-compatible `Wait for reset` action for v11.2.1+ phones)
/// to a `usageLimit` notice when this session has an active quota-wait state.
pub(crate) fn attach_to_notice(
    project_id: &str,
    session_id: &str,
    notice: &mut SessionChatTerminalNotice,
) {
    if notice.kind != SESSION_CHAT_NOTICE_USAGE_LIMIT {
        return;
    }
    let Some(record) = current_cached(project_id, session_id) else {
        return;
    };
    let mode = match record["status"].as_str() {
        Some("blocked") => "quotaBlocked",
        Some("waiting") => "quotaWaiting",
        _ => return,
    };
    notice.quota_wait = Some(json!({
        "mode": mode,
        "resetsAt": record.get("resetsAt").cloned().unwrap_or(Value::Null),
        "resumeAt": record.get("resumeAt").cloned().unwrap_or(Value::Null),
        "model": record.get("model").cloned().unwrap_or(Value::Null),
    }));
    // Include Wait for reset as a standard `restartAgent` action on the notice when blocked, so
    // even older v11.2.1+ phones that ignore `quotaWait` can arm automatic continuation.
    if mode == "quotaBlocked"
        && !notice
            .actions
            .iter()
            .any(|a| a.id == ACTION_WAIT_FOR_RESET)
    {
        notice.actions.insert(
            0,
            SessionChatTerminalNoticeAction {
                id: ACTION_WAIT_FOR_RESET.to_string(),
                label: "Wait for reset".to_string(),
                kind: SessionChatTerminalNoticeActionKind::RestartAgent,
                send: None,
            },
        );
    }
}

/// Sidebar red dot state (`blockedSince`, `blockedReason`) for a quota-blocked session.
/// Returns `None` once the user clicks "Wait for reset" or "Dismiss" (Q4461).
pub(crate) fn blocked_state(session: &Value) -> Option<(String, String)> {
    let record = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())?;
    if record["status"].as_str() != Some("blocked") {
        return None;
    }
    let since = record["since"]
        .as_str()
        .or_else(|| record["detectedAt"].as_str())
        .unwrap_or_default()
        .to_string();
    let reason = match parse_rfc3339(record.get("resetsAt")) {
        Some(at) if at > Utc::now() => format!(
            "Blocked: quota reached (resets in {}). Open to wait for reset, switch model, or dismiss.",
            short_countdown(at - Utc::now())
        ),
        _ => "Blocked: quota reached. Open to wait for reset, switch model, or dismiss.".to_string(),
    };
    Some((since, reason))
}

/// Additive presentation fields `(quotaResetsAt, quotaWaitState)` for the sidebar countdown badge.
pub(crate) fn presentation_fields(session: &Value) -> Option<(Option<String>, &'static str)> {
    let record = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())?;
    let state = match record["status"].as_str() {
        Some("blocked") => "blocked",
        Some("waiting") => "waiting",
        _ => return None,
    };
    let resets_at = record["resetsAt"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((resets_at, state))
}

/// Whether this session is currently paused on a non-account-switched quota limit (`blocked` or
/// `waiting`), used to suppress quiet nudges, stall dots, coordinator report delivery, and
/// Autopilot `TurnCutOff` retries.
pub(crate) fn is_quota_paused(session: &Value) -> bool {
    session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/status"))
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "blocked" | "waiting"))
}

/// Restores saved `quotaLimit` state into memory on gxserver startup and re-arms any pending
/// wait-for-reset wake timer.
pub(crate) fn restore(state: &AppState, repo: &DomainRepository<'_>, session: &Value) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    let Some(mut record) = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())
        .cloned()
    else {
        return;
    };
    match record["status"].as_str() {
        Some("resuming") => {
            // Restart interrupted an in-flight continuation: return to blocked so the user sees it
            // rather than double-sending.
            record["status"] = json!("blocked");
            record["claim"] = Value::Null;
            record["updatedAt"] = json!(Utc::now().to_rfc3339());
            let _ = save_state(state, repo, session, Some(record));
        }
        Some("waiting") => {
            set_cached(pid, sid, Some(record.clone()));
            if let Some(resume_at) = parse_rfc3339(record.get("resumeAt")) {
                super::api_errors::wake_at(state, pid, sid, resume_at);
            }
        }
        Some("blocked" | "dismissed") => {
            set_cached(pid, sid, Some(record));
        }
        _ => {
            set_cached(pid, sid, None);
        }
    }
}

/// Called from `recovery::collect` for every running session: if this session is armed in
/// `quotaLimit.status == "waiting"` and `resumeAt <= now`, claims a one-shot continuation plan.
pub(crate) fn plan_due_resume(
    state: &AppState,
    db: &rusqlite::Connection,
    repo: &DomainRepository<'_>,
    session: &Value,
) -> Result<Option<recovery::Plan>, DomainStateError> {
    if session["lifecycleState"].as_str() != Some("running") {
        return Ok(None);
    }
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return Ok(None);
    };
    let Some(record) = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())
    else {
        return Ok(None);
    };
    if record["status"].as_str() != Some("waiting") {
        return Ok(None);
    }
    let now = Utc::now();
    // If an assistant reply already cleared the error, clear the wait without sending anything.
    if crate::session_chat_notice_progress::has_cleared_error(state, pid, sid) {
        save_state(state, repo, session, None)?;
        return Ok(None);
    }
    if let Some(resume_at) = parse_rfc3339(record.get("resumeAt")) {
        if resume_at > now {
            super::api_errors::wake_at(state, pid, sid, resume_at);
            return Ok(None);
        }
    }
    if crate::session_chat_send::transcript_pending_question_prompt(session).is_some() {
        return Ok(None);
    }
    let _gate = state.accounts.mutation_gate();
    let Some(fresh) = repo.get_session(pid, sid)? else {
        return Ok(None);
    };
    if fresh
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}/status"))
        .and_then(Value::as_str)
        != Some("waiting")
    {
        return Ok(None);
    }
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(db, pid, sid);
    if queue.queue.iter().any(|p| p.state != "queued") {
        return Ok(None);
    }
    let claim = uuid::Uuid::new_v4().to_string();
    let mut updated = fresh
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    updated["status"] = json!("resuming");
    updated["claim"] = json!(claim);
    updated["resumedAt"] = json!(now.to_rfc3339());
    updated["updatedAt"] = json!(now.to_rfc3339());
    save_state(state, repo, &fresh, Some(updated))?;
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Info,
        event: "quotaWaitResuming".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "projectId": pid, "sessionId": sid })),
    });
    Ok(Some(recovery::Plan::for_quota_wait(
        pid.into(),
        sid.into(),
        claim,
        queue.deliverable_head().map(|p| p.id.clone()),
    )))
}

/// Settles a `quota_wait` continuation claim after `recovery::deliver` finishes sending.
pub(crate) fn finish_delivery(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    claim: &str,
    result: Result<(), DomainStateError>,
) {
    let _gate = state.accounts.mutation_gate();
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(session)) = repo.get_session(project_id, session_id) else {
        return;
    };
    let Some(mut record) = session
        .pointer(&format!("/runtimeSettings/{RUNTIME_KEY}"))
        .filter(|v| v.is_object())
        .cloned()
    else {
        return;
    };
    if record["claim"].as_str() != Some(claim) {
        return;
    }
    match result {
        Ok(()) => {
            // Clear the wait cleanly once the single continuation was delivered.
            let _ = save_state(state, &repo, &session, None);
        }
        Err(error) => {
            let now = Utc::now();
            if matches!(
                error.code,
                "composerNotReady" | "promptWaiting" | "accountRecoveryNotReady"
            ) {
                let next = now + chrono::Duration::minutes(2);
                record["status"] = json!("waiting");
                record["claim"] = Value::Null;
                record["resumeAt"] = json!(next.to_rfc3339());
                record["updatedAt"] = json!(now.to_rfc3339());
                let _ = save_state(state, &repo, &session, Some(record));
                super::api_errors::wake_at(state, project_id, session_id, next);
            } else {
                record["status"] = json!("blocked");
                record["claim"] = Value::Null;
                record["updatedAt"] = json!(now.to_rfc3339());
                let _ = save_state(state, &repo, &session, Some(record));
            }
        }
    }
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(state);
    detector.publish_screen_state(project_id, session_id);
}

fn short_countdown(remaining: chrono::Duration) -> String {
    let secs = remaining.num_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", (secs + 59) / 60)
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {mins}m")
        }
    }
}

fn local_clock(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let clock = local.format("%-I:%M %p");
    if local.date_naive() == today {
        format!("at {clock}")
    } else if Some(local.date_naive()) == today.succ_opt() {
        format!("tomorrow at {clock}")
    } else {
        format!("on {} at {clock}", local.format("%b %-d"))
    }
}
