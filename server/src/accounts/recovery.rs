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
use std::{sync::Arc, time::Duration};
const CONTINUE: &str = "Continue the unfinished task from the last confirmed progress after the temporary error or usage limit. Check the current state first and do not repeat completed actions. If the task is already complete, report that and stop.";
/// CDXC:AgentProviders 2026-09-05 DECISION:
/// User requested automatic continuation after errors with increasingly slower retries so unattended work can recover. Temporary errors retry after 5, 10, 20, 40, then 60 minutes; the one-hour cap continues until recovery or Stop.
/// WHY: A durable claim precedes every send. An interrupted or ambiguous send requires attention rather than replaying a possibly accepted prompt. Authentication, approvals, refusals and uncertain delivery are not temporary errors.
pub(crate) fn start(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(Duration::from_secs(30));
        let mut first = true;
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {
                    let scan = state.clone();
                    let restart = first;
                    match tokio::task::spawn_blocking(move || collect(&scan, restart)).await {
                        Ok(Ok(plans)) => {
                            first = false;
                            for plan in plans {
                                let state = state.clone();
                                tokio::spawn(async move { deliver(state, plan).await; });
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
struct Plan {
    project: String,
    session: String,
    claim: String,
    prompt: Option<String>,
}
pub(crate) fn retryable(notice: &SessionChatTerminalNotice) -> bool {
    matches!(
        notice.kind.as_str(),
        "usageLimit" | "streamError" | "agentExited" | "agentError"
    )
}
pub(crate) fn holds_queue(
    repository: &DomainRepository<'_>,
    session: &Value,
    notice: &SessionChatTerminalNotice,
) -> bool {
    if !retryable(notice)
        || session
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
    policy.enabled && (notice.kind == "usageLimit" || policy.retry_errors)
}
fn time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}
fn backoff(attempt: u64) -> chrono::Duration {
    chrono::Duration::minutes((5_i64.saturating_mul(1_i64 << attempt.min(4))).min(60))
}
fn save(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    recovery: Value,
) -> Result<(), DomainStateError> {
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    runtime.insert("accountRecovery".into(), recovery);
    endpoint::update_session(repo, session, runtime)?;
    endpoint::publish(state, repo, session)
}
/// CDXC:AgentProviders 2026-09-11 WHY:
/// One session used to end the whole pass: a `?` or a panic anywhere in the loop skipped every session behind it in the list, silently, for as long as the fault lasted.
/// Each session is now planned on its own, with its errors and panics logged, and the pass outcome is logged too, because three sessions sat at the Fable limit for an hour with nothing in gxserver.jsonl to say why.
fn collect(state: &AppState, restart: bool) -> Result<Vec<Plan>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let targets = repo.list_sessions(None)?;
    if restart {
        let _gate = state
            .accounts
            .mutations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for old in &targets {
            let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str())
            else {
                continue;
            };
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
    if registry.accounts.is_empty()
        && registry.defaults.values().all(|p| !p.enabled)
        && !targets.iter().any(|s| {
            s.pointer("/runtimeSettings/accountPolicyOverride/enabled")
                .and_then(Value::as_bool)
                == Some(true)
                || s.pointer("/runtimeSettings/accountPolicyDefault/enabled")
                    .and_then(Value::as_bool)
                    == Some(true)
        })
    {
        return Ok(vec![]);
    }
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let detector = SessionChatOptionDetector::new(state);
    let mut plans = vec![];
    for old in targets {
        let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str()) else {
            continue;
        };
        let planned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            plan_session(state, &db, &repo, &registry, &snapshot, &detector, &old)
        }));
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
    Ok(plans)
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
        endpoint::update_session(repo, &session, runtime)?;
        session = repo.get_session(pid, sid)?.unwrap_or(session);
    }
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
    }
    Ok(())
}
/// Decides what one session needs this pass: nothing, a state update, an account switch, a restart, or a continuation send (the returned plan).
fn plan_session(
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
            let _gate = state
                .accounts
                .mutations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let _gate = state
        .accounts
        .mutations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    if live_activity || (working && !detection.notice.as_ref().is_some_and(retryable)) {
        if !recovery.is_null() {
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
    let notice = crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid);
    let Some(notice) = notice else {
        return Ok(None);
    };
    if !retryable(&notice) || (notice.kind != "usageLimit" && !policy.retry_errors) {
        if !recovery.is_null() && recovery["status"].as_str() == Some("waiting") {
            recovery["status"] = json!("needsAttention");
            recovery["reason"] = json!(notice.title);
            recovery["updatedAt"] = json!(now.to_rfc3339());
            save(state, repo, &session, recovery)?;
        }
        return Ok(None);
    }
    let attempts = recovery["attempt"].as_u64().unwrap_or(0);
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
    let switch_account = if notice.kind == "usageLimit" && policy.at_limit == LimitAction::Switch {
        ranked(
            &registry,
            snapshot,
            provider,
            current_id.as_deref(),
            &model,
            policy.priority,
        )
        .first()
        .map(|account| account.id.clone())
    } else {
        None
    };
    if recovery.is_null() || recovery["status"].as_str() != Some("waiting") {
        recovery = json!({"status":"waiting","reason":notice.title,"trigger":notice.kind,"attempt":attempts,"nextAttemptAt":(now+backoff(attempts)).to_rfc3339(),"updatedAt":now.to_rfc3339()});
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
            recovery["reason"] = json!(if policy.at_limit == LimitAction::Switch {
                "Waiting for an eligible account to have capacity."
            } else {
                "Waiting for this account's usage to reset."
            });
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
        if session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str)
            .is_none()
        {
            recovery["status"] = json!("needsAttention");
            recovery["reason"] = json!("No saved conversation is available to resume.");
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
    recovery["healthySince"] = Value::Null;
    save(state, repo, &session, recovery)?;
    Ok(Some(Plan {
        project: pid.into(),
        session: sid.into(),
        claim,
        prompt: queue.deliverable_head().map(|p| p.id.clone()),
    }))
}
fn relevant(window: &UsageWindow, model: &str) -> bool {
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
fn has_room(account: &DiscoveredAccount, model: &str, now: DateTime<Utc>) -> bool {
    account.status == "ready"
        && account.usage_error.is_none()
        && account
            .usage_updated_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|t| now - t.with_timezone(&Utc) < chrono::Duration::minutes(5))
        && !account.usage.is_empty()
        && account
            .usage
            .iter()
            .filter(|w| relevant(w, model))
            .all(|w| w.used_percent < 100.)
}
fn ranked<'a>(
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
        .filter(|a| a.provider == provider && a.eligible && Some(a.id.as_str()) != current)
        .filter_map(|a| {
            snapshot
                .accounts
                .iter()
                .find(|d| endpoint::account_id(d) == a.id && has_room(d, model, Utc::now()))
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
        let sender: crate::session_chat_queue::SessionChatQueueSender = Arc::new(move |text| {
            let state = owned.clone();
            let pid = pid.clone();
            let sid = sid.clone();
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
                .map_err(|e| e.message)
            })
        });
        crate::session_chat_queue::deliver_session_chat_queued_prompt(
            &state.paths,
            &state.metadata.server_id,
            &plan.project,
            &plan.session,
            prompt,
            &sender,
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
    let _gate = state
        .accounts
        .mutations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
            {
                recovery["status"] = json!("waiting");
                recovery["nextAttemptAt"] = json!((Utc::now()
                    + backoff(recovery["attempt"].as_u64().unwrap_or(1)))
                .to_rfc3339());
            } else {
                recovery["status"] = json!("needsAttention");
            }
            recovery["reason"] = json!(error.message);
        }
    }
    let _ = save(&state, &repo, &session, recovery);
}
pub(crate) fn user_action(
    state: &AppState,
    project: &str,
    session: &str,
    stop: bool,
) -> Result<(), DomainStateError> {
    let _gate = state.accounts.mutations.lock().map_err(store::error)?;
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
        && !runtime.contains_key("accountPolicyDefault")
        && !runtime.contains_key("accountPolicyOverride")
    {
        return Ok(());
    }
    runtime.remove("accountRecovery");
    runtime.insert("accountRecoverySuppressed".into(), json!(stop));
    endpoint::update_session(&repo, &row, runtime)?;
    endpoint::publish(state, &repo, &row)
}
