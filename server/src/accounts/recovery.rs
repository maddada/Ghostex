use super::{endpoint, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    session_chat_notice::SessionChatTerminalNotice,
    session_chat_options::SessionChatOptionDetector,
    session_chat_queue_runtime::{send_session_chat_message_internal, SessionChatMessageSource},
    storage::open_gxserver_database,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Duration,
};
type SessionKey = (String, String);
#[derive(Default)]
struct PendingRecovery {
    observed: BTreeMap<SessionKey, String>,
    pending: BTreeSet<SessionKey>,
}
/// CDXC:AgentProviders 2026-09-22 WHY:
/// A limit already visible in chat used to wait for the 30-second recovery sweep. Fresh visible notices now wake a targeted pass; remembering each instance prevents the pass's own screen probe from scheduling itself forever.
#[derive(Default)]
pub(crate) struct RecoverySignals {
    changed: tokio::sync::Notify,
    state: Mutex<PendingRecovery>,
}
impl RecoverySignals {
    pub(crate) fn observe(
        &self,
        project: &str,
        session: &str,
        notice: Option<&SessionChatTerminalNotice>,
    ) {
        let key = (project.to_string(), session.to_string());
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(notice) = notice.filter(|notice| retryable(notice)) else {
            state.observed.remove(&key);
            return;
        };
        let instance = format!("{}\n{}", notice.identity(), notice.detected_at);
        if state.observed.get(&key) == Some(&instance) {
            return;
        }
        state.observed.insert(key.clone(), instance);
        state.pending.insert(key);
        self.changed.notify_one();
    }
    /// Runs a targeted pass for one session now (api_errors.rs: a failed turn, a quick try due).
    pub(crate) fn wake(&self, project: &str, session: &str) {
        let key = (project.to_string(), session.to_string());
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .insert(key);
        self.changed.notify_one();
    }
    fn take(&self) -> BTreeSet<SessionKey> {
        std::mem::take(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()).pending)
    }
}
const CONTINUE: &str = "Continue the unfinished task from the last confirmed progress after the temporary error or usage limit. Check the current state first and do not repeat completed actions. If the task is already complete, report that and stop.";
/// CDXC:AgentProviders 2026-10-16 DECISION:
/// User (2026-10-16) picked "Don't count blocked tries": when something on the agent's screen blocks a quick retry (a Claude popup, for example), that try must not use up one of the five quick tries; only a try that actually reached the agent counts. A blocked try waits and tries again once the screen is clear, the card's countdown and try number stay true, and a screen that stays blocked is shown on the card by what blocks it (`screen_block` in api_errors.rs, from what the chat already knows) instead of looping unseen. This extends the 2026-10-15 decision below.
/// User (2026-10-15): "also i noticed you didn't retry when i got this error please fix! should've retried!", then chose "fast first then the existing schedule: wait for the network, retry after 5s, 15s, 1m, 2m, 5m; if all fail, fall back to the 5/10/20/40/60-minute schedule until it works or Stop; report to the orchestrator once the fast stage gives up", and "keep the one existing 'Retry after temporary errors' switch". This extends the 2026-09-05 decision ("automatic continuation after errors with increasingly slower retries so unattended work can recover. Temporary errors retry after 5, 10, 20, 40, then 60 minutes; the one-hour cap continues until recovery or Stop"): a connection or server error (`streamError`) now gets the five quick tries first, each one sent only once the provider's API host answers again, and other temporary errors keep the slow schedule alone. Schedule and detection: api_errors.rs.
/// WHY: A durable claim precedes every send. An interrupted or ambiguous send requires attention rather than replaying a possibly accepted prompt. Authentication, approvals, refusals and uncertain delivery are not temporary errors.
pub(crate) fn start(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(Duration::from_secs(30));
        let mut first = true;
        loop {
            let targets = tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => None,
                _ = state.accounts.recovery.changed.notified() => {
                    Some(state.accounts.recovery.take())
                }
            };
            let scan = state.clone();
            let restart = first;
            match tokio::task::spawn_blocking(move || collect(&scan, restart, targets)).await {
                Ok(Ok(plans)) => {
                    first = false;
                    for plan in plans {
                        let state = state.clone();
                        tokio::spawn(async move {
                            deliver(state, plan).await;
                        });
                    }
                }
                Ok(Err(error)) => log(
                    &state,
                    LogLevel::Warn,
                    "accountRecoveryPassFailed",
                    Some(error.message),
                    json!({ "restart": restart }),
                ),
                Err(join) => log(
                    &state,
                    LogLevel::Error,
                    "accountRecoveryPassPanicked",
                    Some(join.to_string()),
                    json!({ "restart": restart }),
                ),
            }
        }
    });
}
fn log(state: &AppState, level: LogLevel, event: &str, error: Option<String>, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(details),
    });
}
pub(crate) struct Plan {
    project: String,
    session: String,
    claim: String,
    prompt: Option<String>,
}
/// A disabled account counts: another account recovers it (disabled.rs), although retrying on the same one never does.
pub(crate) fn retryable(notice: &SessionChatTerminalNotice) -> bool {
    matches!(
        notice.kind.as_str(),
        "usageLimit" | "streamError" | "agentExited" | "agentError"
    ) || notice.account_disabled()
}
pub(crate) fn holds_queue(
    repository: &DomainRepository<'_>,
    session: &Value,
    notice: &SessionChatTerminalNotice,
) -> bool {
    retryable(notice)
        && recovery_retries(
            repository,
            session,
            notice.kind == "usageLimit" || notice.account_disabled(),
        )
}
/// Whether this pass would retry the session's failed turn by itself: its recovery is on and not
/// stopped, and for anything but a usage limit (or a disabled account) retries after temporary
/// errors are on too.
pub(crate) fn recovery_retries(
    repository: &DomainRepository<'_>,
    session: &Value,
    usage_limit: bool,
) -> bool {
    if session
        .pointer("/runtimeSettings/accountRecoverySuppressed")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return false;
    }
    let Some(pid) = session["projectId"].as_str() else {
        return false;
    };
    let Ok(Some(project)) = repository.get_project(pid) else {
        return false;
    };
    let Some(provider) = launch::provider(&project, session) else {
        return false;
    };
    let Ok(registry) = store::read(repository.db) else {
        return false;
    };
    let policy = launch::effective_policy(&registry, provider, session);
    policy.enabled && (usage_limit || policy.retry_errors)
}
fn time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}
pub(super) fn backoff(attempt: u64) -> chrono::Duration {
    chrono::Duration::minutes((5_i64.saturating_mul(1_i64 << attempt.min(4))).min(60))
}
pub(crate) fn save(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    recovery: Value,
) -> Result<(), DomainStateError> {
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if recovery["status"] == "needsAttention"
        && runtime.get("accountSwitch").is_some_and(|progress| {
            matches!(
                progress["phase"].as_str(),
                Some("switching" | "resuming" | "continuing")
            )
        })
    {
        super::switch_progress::set_phase(&mut runtime, "failed", recovery["reason"].as_str());
    }
    let previous = runtime
        .insert("accountRecovery".into(), recovery)
        .unwrap_or(Value::Null);
    // And the episode's failed row when due (CDXC:Notifications 2026-10-31 in api_errors.rs).
    super::api_errors::store_recovery(state, repo, session, &previous, runtime)?;
    endpoint::publish(state, repo, session)
}
/// CDXC:AgentProviders 2026-09-11 WHY:
/// One session used to end the whole pass: a `?` or a panic anywhere in the loop skipped every session behind it in the list, silently, for as long as the fault lasted.
/// Each session is now planned on its own, with its errors and panics logged, and the pass outcome is logged too, because three sessions sat at the Fable limit for an hour with nothing in gxserver.jsonl to say why.
pub(crate) fn collect(
    state: &AppState,
    restart: bool,
    selected: Option<BTreeSet<SessionKey>>,
) -> Result<Vec<Plan>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let _ = super::login_expired::heal(state, &db);
    let registry = store::read(&db)?;
    let selected = selected.filter(|_| !restart);
    // CDXC:AgentProviders 2026-10-23 WHY:
    // The 30-second pass and the startup pass hydrated every session row (over ten thousand on a long-lived registry, about 148 MB, at startup alongside the other startup passes) although `plan_session` acts only on running and sleeping rows. Both now read the rows that are not stopped; what a stopped row still took part in is kept exactly: the policy-override test below runs over every row in SQL, a stopped session's usage-limit wait is still settled after the loop, and the startup repair picks its rows, closed ones included, from the few fields it acts on (`restore_candidates`).
    // True for every pass that reads only the rows that are not stopped (all but a targeted one).
    let narrowed = selected.is_none();
    let repairs = if restart {
        restore_candidates(&db, &registry)?
    } else {
        Vec::new()
    };
    let targets = if let Some(selected) = selected {
        selected
            .into_iter()
            .map(|(pid, sid)| repo.get_session(&pid, &sid))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect()
    } else {
        repo.list_sessions_excluding_stopped(None)?
    };
    if restart {
        let _gate = state.accounts.mutation_gate();
        for (pid, sid) in &repairs {
            let (pid, sid) = (pid.as_str(), sid.as_str());
            if let Err(error) = restore_session(state, &repo, &registry, pid, sid) {
                log(
                    state,
                    LogLevel::Warn,
                    "accountRecoveryRestoreFailed",
                    Some(error.message),
                    json!({ "projectId": pid, "sessionId": sid }),
                );
            }
        }
    }
    if narrowed {
        super::api_errors::retain_continuations(&targets);
    }
    // A failed row an episode owes cannot wait for what follows, which may skip the session.
    for session in &targets {
        super::api_errors::settle_failure_notice(state, &repo, session);
    }
    // A provider with nothing saved follows `Policy::default`, which is on.
    if registry.accounts.is_empty()
        && [Provider::Claude, Provider::Codex]
            .into_iter()
            .all(|provider| {
                !registry
                    .defaults
                    .get(&provider)
                    .cloned()
                    .unwrap_or_default()
                    .enabled
            })
        && !(if narrowed {
            any_session_enables_policy_override(&db)?
        } else {
            targets.iter().any(|s| {
                s.pointer("/runtimeSettings/accountPolicyOverride/enabled")
                    .and_then(Value::as_bool)
                    == Some(true)
            })
        })
    {
        return Ok(vec![]);
    }
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let detector = SessionChatOptionDetector::new(state);
    let mut plans = vec![];
    let mut settled = BTreeSet::new();
    for old in targets {
        let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str()) else {
            continue;
        };
        let planned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plan_session(state, &db, &repo, &registry, &snapshot, &detector, &old)
        }));
        super::limit_wait::settle(state, &repo, pid, sid);
        if narrowed {
            settled.insert((pid.to_string(), sid.to_string()));
        }
        match planned {
            Ok(Ok(Some(plan))) => plans.push(plan),
            Ok(Ok(None)) => {}
            Ok(Err(error)) => log(
                state,
                LogLevel::Warn,
                "accountRecoverySessionFailed",
                Some(error.message),
                json!({ "projectId": pid, "sessionId": sid }),
            ),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "panic".to_string());
                log(
                    state,
                    LogLevel::Error,
                    "accountRecoverySessionPanicked",
                    Some(message),
                    json!({ "projectId": pid, "sessionId": sid }),
                );
            }
        }
    }
    if narrowed {
        // The stopped rows this pass no longer reads: a wait that is still open for one of them
        // ends here, as it did when the pass walked every row.
        for (pid, sid) in super::limit_wait::waiting_sessions() {
            if !settled.contains(&(pid.clone(), sid.clone())) {
                super::limit_wait::settle(state, &repo, &pid, &sid);
            }
        }
    }
    Ok(plans)
}
/// Whether any session row, stopped ones included, turns its account policy on by itself (`accountPolicyOverride.enabled` is JSON `true`).
fn any_session_enables_policy_override(
    db: &rusqlite::Connection,
) -> Result<bool, DomainStateError> {
    db.query_row(
        "SELECT EXISTS (SELECT 1 FROM sessions WHERE json_type(runtimeSettingsJson, '$.accountPolicyOverride.enabled') = 'true')",
        [],
        |row| row.get::<_, bool>(0),
    )
    .map_err(store::error)
}
/// The rows the startup repair may change, closed ones included, in `list_sessions` order: those
/// `restore_may_apply` accepts, judged from the few runtime fields it reads, which SQLite extracts
/// so no row is hydrated.
fn restore_candidates(
    db: &rusqlite::Connection,
    registry: &Registry,
) -> Result<Vec<SessionKey>, DomainStateError> {
    let mut statement = db
        .prepare(
            "SELECT projectId, sessionId,
                    CASE WHEN json_type(runtimeSettingsJson, '$.accountId') = 'text'
                         THEN json_extract(runtimeSettingsJson, '$.accountId') END,
                    json_type(runtimeSettingsJson, '$.accountSlot'),
                    CASE WHEN json_type(runtimeSettingsJson, '$.accountSlot') = 'text'
                         THEN json_extract(runtimeSettingsJson, '$.accountSlot') END,
                    json_type(runtimeSettingsJson, '$.accountColor') IS NOT NULL,
                    json_type(runtimeSettingsJson, '$.accountRecovery.accountWait') = 'object',
                    json_type(runtimeSettingsJson, '$.accountSuppressedUsageNoticeAt') IS NOT NULL,
                    CASE WHEN json_type(runtimeSettingsJson, '$.accountRecovery.status') = 'text'
                         THEN json_extract(runtimeSettingsJson, '$.accountRecovery.status') END,
                    CASE WHEN json_type(runtimeSettingsJson, '$.accountSwitch.phase') = 'text'
                         THEN json_extract(runtimeSettingsJson, '$.accountSwitch.phase') END
             FROM sessions
             ORDER BY updatedAt DESC, projectId ASC, sessionId ASC",
        )
        .map_err(store::error)?;
    let rows = statement
        .query_map([], |row| {
            // The same fields of a hydrated row, so `restore_may_apply` decides exactly as before.
            let mut runtime = serde_json::Map::new();
            if let Some(id) = row.get::<_, Option<String>>(2)? {
                runtime.insert("accountId".into(), json!(id));
            }
            if row.get::<_, Option<String>>(3)?.is_some() {
                runtime.insert(
                    "accountSlot".into(),
                    row.get::<_, Option<String>>(4)?
                        .map_or(Value::Null, Value::String),
                );
            }
            if row.get::<_, bool>(5)? {
                runtime.insert("accountColor".into(), Value::Null);
            }
            let mut recovery = serde_json::Map::new();
            if row.get::<_, Option<bool>>(6)? == Some(true) {
                recovery.insert("accountWait".into(), json!({}));
            }
            if let Some(status) = row.get::<_, Option<String>>(8)? {
                recovery.insert("status".into(), json!(status));
            }
            runtime.insert("accountRecovery".into(), Value::Object(recovery));
            if row.get::<_, bool>(7)? {
                runtime.insert("accountSuppressedUsageNoticeAt".into(), Value::Null);
            }
            if let Some(phase) = row.get::<_, Option<String>>(9)? {
                runtime.insert("accountSwitch".into(), json!({ "phase": phase }));
            }
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                json!({ "runtimeSettings": runtime }),
            ))
        })
        .map_err(store::error)?;
    let mut candidates = Vec::new();
    for row in rows {
        let (pid, sid, session) = row.map_err(store::error)?;
        if restore_may_apply(registry, &session) {
            candidates.push((pid, sid));
        }
    }
    Ok(candidates)
}
/// Whether `restore_session` can do anything for this row: each of its steps starts from one of these fields.
///
/// CDXC:AgentProviders 2026-10-23 WHY:
/// The startup pass re-read every session row (10,900 on 2026-10-23) one at a time while holding `accounts.mutations`, so account requests waited out the whole walk after each start, though only about a hundred rows carry anything to repair. Rows without these fields are skipped; the rest are still re-read under the lock before anything is written, as before.
fn restore_may_apply(registry: &Registry, session: &Value) -> bool {
    let runtime = &session["runtimeSettings"];
    let stale_slot = registry
        .accounts
        .iter()
        .find(|account| runtime["accountId"].as_str() == Some(account.id.as_str()))
        .is_some_and(|account| {
            runtime["accountSlot"] != json!(account.selector)
                || runtime.get("accountColor").is_some()
        });
    stale_slot
        || runtime["accountRecovery"]["accountWait"].is_object()
        || runtime.get("accountSuppressedUsageNoticeAt").is_some()
        || runtime["accountRecovery"]["status"] == "retrying"
        || matches!(
            runtime["accountSwitch"]["phase"].as_str(),
            Some("switching" | "resuming" | "continuing")
        )
}
/// Startup repair for one session: slot migration, the suppressed usage notice, and a recovery interrupted by the restart.
fn restore_session(
    state: &AppState,
    repo: &DomainRepository<'_>,
    registry: &Registry,
    pid: &str,
    sid: &str,
) -> Result<(), DomainStateError> {
    let Some(mut session) = repo.get_session(pid, sid)? else {
        return Ok(());
    };
    if let Some(account) = registry.accounts.iter().find(|account| {
        session
            .pointer("/runtimeSettings/accountId")
            .and_then(Value::as_str)
            == Some(account.id.as_str())
    }) {
        let mut runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        runtime.insert("accountSlot".into(), json!(account.selector));
        runtime.remove("accountColor");
        // CDXC:AgentProviders 2026-10-23 WHY:
        // The slot migration rewrote every session bound to a saved account on every gxserver start, closed ones included (1,818 rows on 2026-10-23, all already migrated), which made each start write the whole set and stamp all of them with the start time as `updatedAt`. Only a row the migration actually changes is written now.
        if session["runtimeSettings"].as_object() != Some(&runtime) {
            endpoint::update_session(repo, &session, runtime)?;
            session = repo.get_session(pid, sid)?.unwrap_or(session);
        }
    }
    super::limit_wait::restore(&session);
    if let Some(since) = time(session.pointer("/runtimeSettings/accountSuppressedUsageNoticeAt")) {
        crate::session_chat_notice::suppress_account_usage_notice(pid, sid, since);
    }
    if session
        .pointer("/runtimeSettings/accountRecovery/status")
        .and_then(Value::as_str)
        == Some("retrying")
    {
        let mut recovery = session["runtimeSettings"]["accountRecovery"].clone();
        recovery["status"] = json!("needsAttention");
        recovery["reason"] = json!("Ghostex restarted during a recovery attempt. Check the conversation before continuing.");
        recovery["updatedAt"] = json!(Utc::now().to_rfc3339());
        save(state, repo, &session, recovery)?;
    } else if matches!(
        session
            .pointer("/runtimeSettings/accountSwitch/phase")
            .and_then(Value::as_str),
        Some("switching" | "resuming" | "continuing")
    ) {
        super::switch_progress::fail(
            state,
            repo,
            &session,
            "Ghostex restarted during the account switch. Retry when you are ready.",
        );
    }
    Ok(())
}
/// Decides what one session needs this pass: nothing, a state update, an account switch, a restart, or a continuation send (the returned plan).
pub(crate) fn plan_session(
    state: &AppState,
    db: &rusqlite::Connection,
    repo: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    detector: &SessionChatOptionDetector,
    old: &Value,
) -> Result<Option<Plan>, DomainStateError> {
    let Some(pid) = old["projectId"].as_str() else {
        return Ok(None);
    };
    let Some(sid) = old["sessionId"].as_str() else {
        return Ok(None);
    };
    if old["lifecycleState"].as_str() != Some("running") {
        if old["lifecycleState"].as_str() == Some("sleeping")
            && old
                .pointer("/runtimeSettings/accountRecovery/restartRequired")
                .and_then(Value::as_bool)
                == Some(true)
            && old
                .pointer("/runtimeSettings/accountRecovery/status")
                .and_then(Value::as_str)
                == Some("waiting")
            && time(old.pointer("/runtimeSettings/accountRecovery/nextAttemptAt"))
                .is_none_or(|t| t <= Utc::now())
        {
            let _gate = state.accounts.mutation_gate();
            let Some(row) = repo.get_session(pid, sid)? else {
                return Ok(None);
            };
            if row["lifecycleState"].as_str() != Some("sleeping")
                || row
                    .pointer("/runtimeSettings/accountRecovery/restartRequired")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                return Ok(None);
            }
            let mut recovery = row["runtimeSettings"]["accountRecovery"].clone();
            match endpoint::cycle(state, repo, &row, "/api/wakeSession") {
                Ok(()) => recovery["restartRequired"] = json!(false),
                Err(e) => {
                    let attempt = recovery["attempt"].as_u64().unwrap_or(0) + 1;
                    recovery["attempt"] = json!(attempt);
                    recovery["reason"] = json!(e.message);
                    recovery["nextAttemptAt"] = json!((Utc::now() + backoff(attempt)).to_rfc3339());
                }
            }
            let row = repo.get_session(pid, sid)?.unwrap_or(row);
            save(state, repo, &row, recovery)?;
        }
        return Ok(None);
    }
    let Some(project) = repo.get_project(pid)? else {
        return Ok(None);
    };
    let Some(provider) = launch::provider(&project, old) else {
        return Ok(None);
    };
    // Ahead of the Continue automatically check too: a login a session reported expired marks its account Needs sign-in whatever this session's policy says.
    let _ = super::login_expired::observe(state, db, old, provider);
    // Ahead of the Continue automatically check: a disabled login is marked out of automatic switching whatever this session's policy says.
    if super::disabled::recover(state, db, repo, snapshot, &project, old, provider)? {
        return Ok(None);
    }
    let policy = launch::effective_policy(registry, provider, old);
    if !policy.enabled
        || old
            .pointer("/runtimeSettings/accountRecoverySuppressed")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Ok(None);
    }
    let detection = detector.detect_blocking(pid, sid, Some(provider.id()), false);
    if !detection.captured {
        return Ok(None);
    }
    let _gate = state.accounts.mutation_gate();
    let Some(session) = repo.get_session(pid, sid)? else {
        return Ok(None);
    };
    if session["lifecycleState"].as_str() != Some("running")
        || session
            .pointer("/runtimeSettings/accountRecoverySuppressed")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Ok(None);
    }
    let registry = store::read(db)?;
    let policy = launch::effective_policy(&registry, provider, &session);
    if !policy.enabled {
        return Ok(None);
    }
    let mut recovery = session
        .pointer("/runtimeSettings/accountRecovery")
        .cloned()
        .unwrap_or(Value::Null);
    let now = Utc::now();
    if recovery["status"].as_str() == Some("needsAttention") {
        return Ok(None);
    }
    if recovery["status"].as_str() == Some("retrying") {
        return Ok(None);
    }
    if recovery["status"].as_str() == Some("waiting")
        && crate::session_chat_notice_progress::has_cleared_error(state, pid, sid)
        && crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid)
            .is_none()
    {
        recovery["status"] = json!("resumed");
        recovery["reason"] = json!("The agent is responding again.");
        recovery["nextAttemptAt"] = Value::Null;
        recovery["updatedAt"] = json!(now.to_rfc3339());
        save(state, repo, &session, recovery.clone())?;
    }
    // CDXC:AgentProviders 2026-09-11 WHY:
    // The screen keeps the finished turn's rows, and the activity detector still classifies them: the last `⏺` message, the tool gutter that holds the limit text itself, a "done" status row.
    // The chat hides those behind the hook-derived working flag; this pass read the raw value, took the idle screen for progress and never recovered a session whose limit arrived mid-turn.
    // Activity counts as progress only while the session is working, or while Claude compacts, which the hooks do not report.
    let working =
        crate::presentation::presentation_activity(&session, &now.to_rfc3339()) == "working";
    let live_activity = detection.activity.as_ref().is_some_and(|activity| {
        working
            || activity.kind
                == crate::session_chat_terminal_activity::SESSION_CHAT_ACTIVITY_COMPACTING
    });
    // A visible limit has already passed transcript-progress and account-switch
    // suppression. Leftover tool rows and a working hook cannot overrule it.
    let usage_limit = detection.notice.as_ref().is_some_and(|notice| {
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT
    });
    if !usage_limit
        && (live_activity || (working && !detection.notice.as_ref().is_some_and(retryable)))
    {
        // A failed-episode marker alone is no recovery to report progress on.
        if recovery["status"].is_string() {
            if let Some(since) = time(recovery.get("healthySince")) {
                if now - since >= chrono::Duration::minutes(1) {
                    recovery["attempt"] = json!(0);
                    recovery["status"] = json!("resumed");
                    recovery["reason"] = json!("The agent is making progress again.");
                    recovery["nextAttemptAt"] = Value::Null;
                }
            } else {
                recovery["healthySince"] = json!(now.to_rfc3339());
            }
            save(state, repo, &session, recovery)?;
        }
        return Ok(None);
    }
    if detection.prompt.is_some()
        || crate::session_chat_send::transcript_pending_question_prompt(&session).is_some()
    {
        return Ok(None);
    }
    // Claude records a failed call itself, which the screen list can miss; a screen notice that
    // is retryable or blocks the input still comes first (api_errors.rs).
    let transcript_failure = (provider == Provider::Claude)
        .then(|| super::api_errors::claude_open_failure(&session))
        .flatten();
    let notice =
        match crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid) {
            Some(notice) if retryable(&notice) || notice.blocks_input() => Some(notice),
            other => transcript_failure
                .as_ref()
                .map(super::api_errors::OpenFailure::notice)
                .or(other),
        };
    let Some(notice) = notice else {
        return Ok(None);
    };
    // A disabled account that disabled.rs did not act on is not retried here: the same login refuses every continuation.
    if !retryable(&notice)
        || notice.account_disabled()
        || (notice.kind != "usageLimit" && !policy.retry_errors)
    {
        if !recovery.is_null() && recovery["status"].as_str() == Some("waiting") {
            recovery["status"] = json!("needsAttention");
            recovery["reason"] = json!(notice.title);
            recovery["updatedAt"] = json!(now.to_rfc3339());
            save(state, repo, &session, recovery)?;
        } else if retryable(&notice)
            && !notice.account_disabled()
            && matches!(
                notice.kind.as_str(),
                crate::session_chat_notice::SESSION_CHAT_NOTICE_STREAM_ERROR | "agentError"
            )
        {
            // An error recovery never retries because retries are off, whatever an earlier
            // recovery that ended left behind: announced once.
            super::api_errors::announce_episode(state, repo, &session)?;
        }
        return Ok(None);
    }
    let connection_error =
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_STREAM_ERROR;
    // The last continuation was answered: what still looks like a failure is the old one.
    if connection_error
        && recovery["status"].as_str() == Some("resumed")
        && super::api_errors::last_try_answered(pid, sid, time(recovery.get("lastAttemptAt")))
    {
        return Ok(None);
    }
    // The API answered after the last try: that try worked, so this is a new failure and the quick tries start over.
    let answered_since_last_try = recovery["status"].as_str() == Some("resumed")
        && transcript_failure.as_ref().is_some_and(|failure| {
            failure.replied_at_ms.is_some_and(|replied| {
                time(recovery.get("lastAttemptAt"))
                    .is_none_or(|tried| replied > tried.timestamp_millis())
            })
        });
    let attempts = if answered_since_last_try {
        0
    } else {
        recovery["attempt"].as_u64().unwrap_or(0)
    };
    let delay = if connection_error {
        super::api_errors::error_backoff(attempts)
    } else {
        backoff(attempts)
    };
    // CDXC:AgentProviders 2026-09-11 WHY:
    // Restored sessions can display a saved login through its configured home without a persisted accountId. Checking only that field delayed their first usage-limit switch by five minutes even when another account was ready.
    // Use the same identity as the account panel, exclude it from candidates, and let an available replacement release that initial wait. Failed switch attempts still keep their retry backoff.
    let current_id = super::session_identity::display_account_id(
        &registry,
        provider,
        &session,
        &state.paths.home_dir,
    )
    .map(str::to_string);
    let model = detection
        .options
        .as_ref()
        .and_then(|o| o.selection.model.as_ref())
        .map(|m| m.value.clone())
        .unwrap_or_default();
    // A session on its own login keeps waiting for its limit to reset instead of moving to an account (CDXC:AgentProviders 2026-10-09 in launch.rs).
    let target = if notice.kind == "usageLimit"
        && policy.at_limit == LimitAction::Switch
        && !launch::session_uses_own_login(provider, &session)
    {
        switch_target(
            &registry,
            snapshot,
            provider,
            current_id.as_deref(),
            &model,
            policy.priority,
            &session,
        )
    } else {
        SwitchTarget::None
    };
    let switch_account = match &target {
        SwitchTarget::Move(id) => Some(id.clone()),
        _ => None,
    };
    if recovery.is_null() || recovery["status"].as_str() != Some("waiting") {
        let last_attempt_at = recovery["lastAttemptAt"].clone();
        recovery = json!({"status":"waiting","reason":notice.title,"trigger":notice.kind,"attempt":attempts,"nextAttemptAt":(now+delay).to_rfc3339(),"updatedAt":now.to_rfc3339(),"lastAttemptAt":last_attempt_at,"errorText":notice.detail});
        if connection_error {
            super::api_errors::wake_at(state, pid, sid, now + delay);
        }
        if notice.kind == "usageLimit"
            && (current_id.is_some() || switch_account.is_some())
            && attempts == 0
        {
            recovery["nextAttemptAt"] = json!(now.to_rfc3339());
        }
        save(state, repo, &session, recovery.clone())?;
    }
    if time(recovery.get("nextAttemptAt")).is_some_and(|t| t > now)
        && !(attempts == 0 && switch_account.is_some())
    {
        return Ok(None);
    }
    // A try the agent's screen would refuse (a popup, a dialog, a question) never reaches it, so
    // a connection error waits here without spending a try, looking again every few seconds.
    if connection_error {
        if let Some(block) = super::api_errors::screen_block(&detection, &session) {
            if recovery["blockedBy"].as_str() != Some(block.as_str()) {
                super::api_errors::mark_blocked(&mut recovery, &block, now);
                save(state, repo, &session, recovery)?;
            }
            super::api_errors::wake_at(
                state,
                pid,
                sid,
                now + chrono::Duration::from_std(super::api_errors::BLOCKED_RECHECK)
                    .unwrap_or_default(),
            );
            return Ok(None);
        }
        super::api_errors::clear_blocked(&mut recovery);
    }
    // A try sent while the API host cannot be reached fails the same way and spends a try for
    // nothing, so a connection error waits here, looking again every few seconds.
    if connection_error {
        if !super::api_errors::api_reachable(provider) {
            if recovery["waitingForNetwork"] != true {
                recovery["waitingForNetwork"] = json!(true);
                recovery["updatedAt"] = json!(now.to_rfc3339());
                save(state, repo, &session, recovery)?;
            }
            super::api_errors::wake_at(
                state,
                pid,
                sid,
                now + chrono::Duration::from_std(super::api_errors::NETWORK_RECHECK)
                    .unwrap_or_default(),
            );
            return Ok(None);
        }
        if let Some(fields) = recovery.as_object_mut() {
            fields.remove("waitingForNetwork");
        }
    }
    let mut session = session;
    if notice.kind == "usageLimit" {
        let current = snapshot
            .accounts
            .iter()
            .find(|a| Some(endpoint::account_id(a).as_str()) == current_id.as_deref());
        if let Some(id) = switch_account {
            if let Err(error) = endpoint::select(
                state,
                repo,
                &registry,
                snapshot,
                &project,
                &session,
                Some(&id),
                super::continuation::SwitchSource::Automatic,
            ) {
                recovery["reason"] = json!(error.message);
                recovery["nextAttemptAt"] = json!((now + backoff(attempts)).to_rfc3339());
                recovery["attempt"] = json!(attempts + 1);
                session = repo.get_session(pid, sid)?.unwrap_or(session);
                recovery["restartRequired"] =
                    json!(session["lifecycleState"].as_str() == Some("sleeping"));
                save(state, repo, &session, recovery)?;
                return Ok(None);
            }
            // The switch owns its one-shot dot; do not also send the generic recovery prompt.
            endpoint::publish(state, repo, &session)?;
            return Ok(None);
        } else if !current.is_some_and(|a| has_room(a, &model, now)) {
            let reset = current.and_then(|a| {
                a.usage
                    .iter()
                    .filter(|w| relevant(w, &model) && w.used_percent >= 100.)
                    .filter_map(|w| {
                        time(
                            w.resets_at
                                .as_ref()
                                .map(|s| Value::String(s.clone()))
                                .as_ref(),
                        )
                    })
                    .max()
            });
            let held_back = match &target {
                SwitchTarget::HeldBack { name, left_at, .. } => Some(format!(
                    "Waiting for a new usage reading of {name} before moving back to it: this session left it at {}.",
                    left_at.with_timezone(&chrono::Local).format("%-I:%M %p")
                )),
                _ => None,
            };
            let reason = if let Some(held_back) = held_back.as_deref() {
                held_back
            } else if policy.at_limit == LimitAction::Switch {
                super::limit_wait::NO_ROOM
            } else {
                "Waiting for this account's usage to reset."
            };
            // Once per wait: the 30-second passes re-save the same reason. The 2026-10-11 G9ow7 delay left nothing in gxserver.jsonl to say which account readings held the switch back.
            if policy.at_limit == LimitAction::Switch && recovery["reason"] != reason {
                let candidates: Vec<Value> = registry
                    .accounts
                    .iter()
                    .filter(|a| a.provider == provider && a.auto_switchable())
                    .map(|a| {
                        let found = snapshot
                            .accounts
                            .iter()
                            .find(|d| endpoint::account_id(d) == a.id);
                        json!({
                            "slot": a.selector,
                            "current": Some(a.id.as_str()) == current_id.as_deref(),
                            "status": found.map(|d| d.status.as_str()),
                            "usageError": found.and_then(|d| d.usage_error.as_deref()),
                            "usageUpdatedAt": found.and_then(|d| d.usage_updated_at.as_deref()),
                            "maxUsedPercent": found.map(|d| d.usage.iter().filter(|w| relevant(w, &model)).map(|w| w.used_percent).fold(0f64, f64::max)),
                            "heldBackUntilNewReading": matches!(&target, SwitchTarget::HeldBack { id, .. } if *id == a.id),
                        })
                    })
                    .collect();
                log(
                    state,
                    LogLevel::Warn,
                    if held_back.is_some() {
                        "accountRecoveryWaitingForReading"
                    } else {
                        "accountRecoveryNoAccountWithRoom"
                    },
                    None,
                    json!({ "projectId": pid, "sessionId": sid, "model": model, "leftAt": left_account(&session).map(|(_, at)| at.to_rfc3339()), "accounts": candidates }),
                );
            }
            recovery["reason"] = json!(reason);
            recovery["nextAttemptAt"] = json!(reset
                .filter(|t| *t > now)
                .map(|t| if policy.at_limit == LimitAction::Switch {
                    t.min(now + chrono::Duration::minutes(2))
                } else {
                    t
                })
                .unwrap_or(now + backoff(attempts))
                .to_rfc3339());
            // An untracked default login has no trustworthy quota reading. Retry it conservatively rather than inventing an available percentage.
            if current_id.is_some() {
                // Not the every-account-full card: an account has room, it only needs a newer reading.
                if policy.at_limit == LimitAction::Switch && held_back.is_none() {
                    let wait = super::limit_wait::describe(
                        &registry,
                        snapshot,
                        provider,
                        current_id.as_deref(),
                        &model,
                        now,
                    );
                    super::limit_wait::enter(state, repo, &session, &mut recovery, wait);
                }
                save(state, repo, &session, recovery)?;
                return Ok(None);
            }
        }
    }
    if notice.kind == "agentExited"
        || (notice.kind == "usageLimit"
            && notice.blocks_input()
            && session["runtimeSettings"]["accountId"] == old["runtimeSettings"]["accountId"])
    {
        // CDXC:AgentProviders 2026-10-31: a session id is not a conversation (GitHub #366); see recovery_conversation.rs.
        if super::recovery_conversation::withhold_continue(
            &mut recovery,
            &project,
            &session,
            &crate::agents::read_agent_settings(db)?,
        ) {
            save(state, repo, &session, recovery)?;
            return Ok(None);
        }
        recovery["restartRequired"] = json!(true);
        save(state, repo, &session, recovery.clone())?;
        if let Err(e) = endpoint::cycle(state, repo, &session, "/api/sleepSession")
            .and_then(|_| endpoint::cycle(state, repo, &session, "/api/wakeSession"))
        {
            session = repo.get_session(pid, sid)?.unwrap_or(session);
            recovery["reason"] = json!(e.message);
            recovery["attempt"] = json!(attempts + 1);
            recovery["nextAttemptAt"] = json!((now + backoff(attempts + 1)).to_rfc3339());
            save(state, repo, &session, recovery)?;
            return Ok(None);
        }
        session = repo.get_session(pid, sid)?.unwrap_or(session);
    }
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(db, pid, sid);
    if queue.queue.iter().any(|p| p.state != "queued") {
        recovery["status"] = json!("needsAttention");
        recovery["reason"] = json!("A queued message needs review before recovery can continue.");
        save(state, repo, &session, recovery)?;
        return Ok(None);
    }
    let claim = uuid::Uuid::new_v4().to_string();
    recovery["restartRequired"] = json!(false);
    recovery["status"] = json!("retrying");
    recovery["claim"] = json!(claim);
    recovery["attempt"] = json!(attempts + 1);
    recovery["updatedAt"] = json!(now.to_rfc3339());
    // Kept so a try the screen turns away can give its slot back (api_errors::give_back_blocked_try).
    recovery["previousAttemptAt"] = recovery["lastAttemptAt"].clone();
    recovery["lastAttemptAt"] = json!(now.to_rfc3339());
    recovery["healthySince"] = Value::Null;
    save(state, repo, &session, recovery)?;
    Ok(Some(Plan {
        project: pid.into(),
        session: sid.into(),
        claim,
        prompt: queue.deliverable_head().map(|p| p.id.clone()),
    }))
}
pub(super) fn relevant(window: &UsageWindow, model: &str) -> bool {
    window.model.as_ref().is_none_or(|m| {
        !model.contains("claude")
            && !model.contains("gpt")
            && !model.contains("opus")
            && !model.contains("sonnet")
            && !model.contains("haiku")
            || model.to_lowercase().contains(&m.to_lowercase())
            || m.to_lowercase().contains(&model.to_lowercase())
    })
}
fn reading_time(account: &DiscoveredAccount) -> Option<DateTime<Utc>> {
    account
        .usage_updated_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}
/// CDXC:AgentProviders 2026-10-11 WHY:
/// A switch target used to also need a usage reading under 5 minutes old, but cswap polls an idle account only every 5 to 10 minutes (`poll_policy.py`, kept under the usage endpoint's hourly budget), so a session at its limit sat on "Waiting for an eligible account to have capacity" until cswap next polled the target: G9ow7 waited 2m10s for slot 2's 11:54:15 reading although slot 2 was at 11% and 3%.
/// cswap already judges whether a reading is trustworthy enough for a switch and reports `unavailable` (which sets `usage_error`) when it is not, so a candidate goes on that judgment alone. The current account keeps the 5-minute rule in `has_room`, because the limit notice is newer evidence than any older reading of it.
fn trusted_room(account: &DiscoveredAccount, model: &str) -> bool {
    account.status == "ready"
        && account.usage_error.is_none()
        && reading_time(account).is_some()
        && !account.usage.is_empty()
        && account
            .usage
            .iter()
            .filter(|w| relevant(w, model))
            .all(|w| w.used_percent < 100.)
}
fn has_room(account: &DiscoveredAccount, model: &str, now: DateTime<Utc>) -> bool {
    trusted_room(account, model)
        && reading_time(account).is_some_and(|t| now - t < chrono::Duration::minutes(5))
}
/// Where an automatic switch at a usage limit takes the session.
#[derive(Debug, PartialEq)]
pub(super) enum SwitchTarget {
    /// The best account with room.
    Move(String),
    /// The only accounts with room include the one this session automatically left, whose last reading predates leaving: it may still show the limit the session left it for.
    HeldBack {
        id: String,
        name: String,
        left_at: DateTime<Utc>,
    },
    /// No account has room.
    None,
}
/// The account this session last left by an automatic switch, and when.
///
/// CDXC:AgentProviders 2026-11-08 WHY:
/// The coordinator G9jt7 hit account 4's weekly limit at 08:27 (Dubai) and logged "no account with room" although account 5 was ready at 42%: it had moved from 5 to 4 automatically on 6 Nov, and this read the leave time from `accountSwitch.updatedAt`, which every later phase change re-stamps. A message sent from the composer marks the old switch `cancelled` (`user_action`), so the coordinator's reports at 08:25 and 08:27 moved "left at" past account 5's 08:24 reading, and account 5 was held back as if just left. The switch now stamps `leftAt` once; a record saved before that falls back to `updatedAt` only while no later `cancelled` has overwritten it.
pub(super) fn left_account(session: &Value) -> Option<(&str, DateTime<Utc>)> {
    let progress = session
        .pointer("/runtimeSettings/accountSwitch")
        .filter(|progress| progress["source"] == "automatic")?;
    let at = time(progress.get("leftAt")).or_else(|| {
        (progress["phase"] != "cancelled")
            .then(|| time(progress.get("updatedAt")))
            .flatten()
    })?;
    Some((progress["fromAccountId"].as_str()?, at))
}
/// The account a usage-limit switch moves `session` to. An account this session automatically left counts again only once a reading taken after it left says so: candidates no longer need a fresh reading (see `trusted_room`), and an older one would bounce the session straight back to the account whose limit it just hit.
pub(super) fn switch_target(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: Option<&str>,
    model: &str,
    priority: Priority,
    session: &Value,
) -> SwitchTarget {
    let left = left_account(session);
    let mut held_back = SwitchTarget::None;
    for account in ranked(registry, snapshot, provider, current, model, priority) {
        let Some((from, at)) = left.filter(|(from, _)| *from == account.id) else {
            return SwitchTarget::Move(account.id.clone());
        };
        let read_since = snapshot
            .accounts
            .iter()
            .find(|d| endpoint::account_id(d) == from)
            .and_then(reading_time)
            .is_some_and(|t| t > at);
        if read_since {
            return SwitchTarget::Move(account.id.clone());
        }
        held_back = SwitchTarget::HeldBack {
            id: account.id.clone(),
            name: account.name.clone(),
            left_at: at,
        };
    }
    held_back
}
pub(super) fn ranked<'a>(
    registry: &'a Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: Option<&str>,
    model: &str,
    priority: Priority,
) -> Vec<&'a SavedAccount> {
    let mut rows: Vec<_> = registry
        .accounts
        .iter()
        .filter(|a| a.provider == provider && a.auto_switchable() && Some(a.id.as_str()) != current)
        .filter_map(|a| {
            snapshot
                .accounts
                .iter()
                .find(|d| endpoint::account_id(d) == a.id && trusted_room(d, model))
                .map(|d| (a, d))
        })
        .collect();
    rows.sort_by(|(a, ua), (b, ub)| {
        let score = |u: &DiscoveredAccount| {
            u.usage
                .iter()
                .filter(|w| relevant(w, model))
                .map(|w| w.used_percent)
                .fold(0f64, f64::max)
        };
        let reset = |u: &DiscoveredAccount| {
            u.usage
                .iter()
                .filter(|w| relevant(w, model))
                .filter_map(|w| {
                    w.resets_at
                        .as_ref()
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                })
                .map(|t| t.timestamp())
                .min()
        };
        let order = match priority {
            Priority::LeastUsed => score(ua).total_cmp(&score(ub)),
            Priority::MostUsed => score(ub).total_cmp(&score(ua)),
            Priority::SoonestReset | Priority::LatestReset => match (reset(ua), reset(ub)) {
                (Some(a), Some(b)) => {
                    if matches!(priority, Priority::LatestReset) {
                        b.cmp(&a)
                    } else {
                        a.cmp(&b)
                    }
                }
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
        };
        order.then_with(|| a.id.cmp(&b.id))
    });
    rows.into_iter().map(|(a, _)| a).collect()
}
async fn deliver(state: Arc<AppState>, plan: Plan) {
    // A newly resumed CLI needs time to paint its composer. Waiting here has not sent any bytes.
    let detector = SessionChatOptionDetector::new(&state);
    // What the screen showed while the composer was not ready, for a try it turns away.
    let mut screen_block = None;
    for _ in 0..30 {
        let row = {
            let Ok(db) = open_gxserver_database(&state.paths) else {
                return;
            };
            let repo = DomainRepository::new(&db, &state.metadata.server_id);
            let Ok(Some(row)) = repo.get_session(&plan.project, &plan.session) else {
                return;
            };
            row
        };
        if row
            .pointer("/runtimeSettings/accountRecovery/claim")
            .and_then(Value::as_str)
            != Some(&plan.claim)
        {
            return;
        }
        let agent = crate::session_chat_composer::session_chat_composer_agent_id(&row);
        let detection = detector
            .detect(&plan.project, &plan.session, agent.as_deref(), true)
            .await;
        if detection.captured
            && detection.composer.state
                == crate::session_chat_composer::SessionChatComposerState::Ready
        {
            break;
        }
        screen_block = super::api_errors::screen_block(&detection, &row);
        if detection
            .notice
            .as_ref()
            .is_some_and(|n| n.blocks_queued_delivery() && !retryable(n))
        {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let result = if let Some(prompt) = &plan.prompt {
        let owned = state.clone();
        let pid = plan.project.clone();
        let sid = plan.session.clone();
        let claim = plan.claim.clone();
        let sender: crate::session_chat_queue::SessionChatQueueSender =
            Arc::new(move |text, _images| {
                let state = owned.clone();
                let (pid, sid) = (pid.clone(), sid.clone());
                // Its prompt hook is this continuation (api_errors::end_failure_episode).
                super::api_errors::note_continuation(&pid, &sid, &claim, &text);
                Box::pin(async move {
                    send_session_chat_message_internal(
                        &state,
                        &pid,
                        &sid,
                        &text,
                        &[],
                        SessionChatMessageSource::AutomaticRecovery,
                    )
                    .await
                    .map(|_| ())
                })
            });
        crate::session_chat_queue::deliver_session_chat_queued_prompt(
            &state.paths,
            &state.metadata.server_id,
            &plan.project,
            &plan.session,
            prompt,
            &sender,
            false,
        )
        .await
        .and_then(|d| {
            if d.sent {
                Ok(())
            } else {
                Err(store::error(d.error_message.unwrap_or_default()))
            }
        })
    } else {
        super::api_errors::note_continuation(&plan.project, &plan.session, &plan.claim, CONTINUE);
        send_session_chat_message_internal(
            &state,
            &plan.project,
            &plan.session,
            CONTINUE,
            &[],
            SessionChatMessageSource::AutomaticRecovery,
        )
        .await
        .map(|_| ())
    };
    let _gate = state.accounts.mutation_gate();
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(session)) = repo.get_session(&plan.project, &plan.session) else {
        return;
    };
    let mut recovery = session
        .pointer("/runtimeSettings/accountRecovery")
        .cloned()
        .unwrap_or(Value::Null);
    if recovery["claim"].as_str() != Some(&plan.claim) {
        return;
    }
    recovery["updatedAt"] = json!(Utc::now().to_rfc3339());
    match result {
        Ok(()) => {
            recovery["nextAttemptAt"] = Value::Null;
            recovery["status"] = json!("resumed");
            recovery["reason"] =
                json!("Continuation sent. Ghostex is watching for further errors.");
        }
        Err(error) => {
            if matches!(error.code, "composerNotReady" | "accountRecoveryNotReady")
                && plan.prompt.is_none()
                && super::api_errors::is_connection_trigger(&recovery)
            {
                // Nothing reached the agent: the try does not count.
                super::api_errors::give_back_blocked_try(
                    &mut recovery,
                    screen_block.as_deref().unwrap_or(&error.message),
                    Utc::now(),
                );
                if let Some(next) = time(recovery.get("nextAttemptAt")) {
                    super::api_errors::wake_at(&state, &plan.project, &plan.session, next);
                }
            } else if matches!(error.code, "composerNotReady" | "accountRecoveryNotReady")
                && plan.prompt.is_none()
            {
                let attempt = recovery["attempt"].as_u64().unwrap_or(1);
                let next = Utc::now() + backoff(attempt);
                recovery["status"] = json!("waiting");
                recovery["nextAttemptAt"] = json!(next.to_rfc3339());
                super::api_errors::wake_at(&state, &plan.project, &plan.session, next);
            } else {
                recovery["status"] = json!("needsAttention");
            }
            recovery["reason"] = json!(error.message);
        }
    }
    let _ = save(&state, &repo, &session, recovery);
}
/// Retry now on an API error card: resume the turn at once, whatever the schedule, the network
/// wait, Stop retrying, or the Continue automatically switch say, because the user asked. The
/// caller holds `accounts.mutations`.
pub(crate) fn retry_now(
    state: &AppState,
    repo: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    session: &Value,
) -> Result<(), DomainStateError> {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return Ok(());
    };
    if session["lifecycleState"].as_str() != Some("running") {
        return Err(DomainStateError::bad_request(
            "Wake the session before retrying.",
        ));
    }
    if crate::presentation::presentation_activity(session, &Utc::now().to_rfc3339()) == "working" {
        return Err(DomainStateError::bad_request(
            "The agent is already working.",
        ));
    }
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let previous = runtime
        .get("accountRecovery")
        .cloned()
        .unwrap_or(Value::Null);
    let error_text = previous["errorText"]
        .as_str()
        .map(str::to_string)
        .or_else(|| super::api_errors::claude_open_failure(session).map(|failure| failure.text));
    let claim = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    runtime.insert("accountRecoverySuppressed".into(), json!(false));
    let mut recovery = json!({
        "status": "retrying",
        "trigger": crate::session_chat_notice::SESSION_CHAT_NOTICE_STREAM_ERROR,
        "claim": claim,
        "attempt": previous["attempt"].as_u64().unwrap_or(0),
        "reason": "Retrying now.",
        "errorText": error_text,
        "previousAttemptAt": previous["lastAttemptAt"],
        "lastAttemptAt": now,
        "updatedAt": now,
    });
    // A retry the user asked for continues the episode: no second failed notification.
    super::api_errors::reconcile_failure_notice(&previous, &mut recovery);
    runtime.insert("accountRecovery".into(), recovery);
    let members = &["accountRecoverySuppressed", "accountRecovery"];
    endpoint::update_session_members(repo, session, &runtime, members)?;
    let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(db, pid, sid);
    let plan = Plan {
        project: pid.into(),
        session: sid.into(),
        claim,
        prompt: queue.deliverable_head().map(|p| p.id.clone()),
    };
    let state = Arc::new(state.clone());
    tokio::spawn(async move { deliver(state, plan).await });
    Ok(())
}
/// Whether a session without its own Customize settings is covered by an enabled provider default, so Stop and the next send still record the recovery suppression flag.
fn follows_enabled_defaults(
    repo: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: &str,
    session: &Value,
) -> Result<bool, DomainStateError> {
    let Some(project) = repo.get_project(project)? else {
        return Ok(false);
    };
    let Some(provider) = launch::provider(&project, session) else {
        return Ok(false);
    };
    Ok(launch::effective_policy(&store::read(db)?, provider, session).enabled)
}
pub(crate) fn user_action(
    state: &AppState,
    project: &str,
    session: &str,
    stop: bool,
) -> Result<(), DomainStateError> {
    let _gate = state.accounts.mutation_gate();
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Some(row) = repo.get_session(project, session)? else {
        return Ok(());
    };
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if !runtime.contains_key("accountRecovery")
        && !runtime.contains_key("accountRecoverySuppressed")
        && !runtime.contains_key("accountPolicyOverride")
        && !follows_enabled_defaults(&repo, &db, project, &row)?
    {
        return Ok(());
    }
    runtime.remove("accountRecovery");
    super::api_errors::forget_continuation(project, session);
    super::switch_progress::set_phase(&mut runtime, "cancelled", None);
    runtime.insert("accountRecoverySuppressed".into(), json!(stop));
    endpoint::update_session(&repo, &row, runtime)?;
    endpoint::publish(state, &repo, &row)
}
