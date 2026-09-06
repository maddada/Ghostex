use super::{endpoint, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
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
            tokio::select! {_=shutdown.recv()=>break,_=clock.tick()=>{
                let scan=state.clone();let restart=first;
                let result=tokio::task::spawn_blocking(move||collect(&scan,restart)).await;
                if let Ok(Ok(plans))=result{first=false;for plan in plans{let state=state.clone();tokio::spawn(async move{deliver(state,plan).await;});}}
            }}
        }
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
fn collect(state: &AppState, restart: bool) -> Result<Vec<Plan>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let targets = repo.list_sessions(None)?;
    if restart {
        let _gate = state.accounts.mutations.lock().map_err(store::error)?;
        for old in &targets {
            let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str())
            else {
                continue;
            };
            let Some(session) = repo.get_session(pid, sid)? else {
                continue;
            };
            if session
                .pointer("/runtimeSettings/accountRecovery/status")
                .and_then(Value::as_str)
                == Some("retrying")
            {
                let mut recovery = session["runtimeSettings"]["accountRecovery"].clone();
                recovery["status"] = json!("needsAttention");
                recovery["reason"] = json!("Ghostex restarted during a recovery attempt. Check the conversation before continuing.");
                recovery["updatedAt"] = json!(Utc::now().to_rfc3339());
                save(state, &repo, &session, recovery)?;
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
        let Some(pid) = old["projectId"].as_str() else {
            continue;
        };
        let Some(sid) = old["sessionId"].as_str() else {
            continue;
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
                let _gate = state.accounts.mutations.lock().map_err(store::error)?;
                let Some(row) = repo.get_session(pid, sid)? else {
                    continue;
                };
                if row["lifecycleState"].as_str() != Some("sleeping")
                    || row
                        .pointer("/runtimeSettings/accountRecovery/restartRequired")
                        .and_then(Value::as_bool)
                        != Some(true)
                {
                    continue;
                }
                let mut recovery = row["runtimeSettings"]["accountRecovery"].clone();
                match endpoint::cycle(state, &repo, &row, "/api/wakeSession") {
                    Ok(()) => recovery["restartRequired"] = json!(false),
                    Err(e) => {
                        let attempt = recovery["attempt"].as_u64().unwrap_or(0) + 1;
                        recovery["attempt"] = json!(attempt);
                        recovery["reason"] = json!(e.message);
                        recovery["nextAttemptAt"] =
                            json!((Utc::now() + backoff(attempt)).to_rfc3339());
                    }
                }
                let row = repo.get_session(pid, sid)?.unwrap_or(row);
                save(state, &repo, &row, recovery)?;
            }
            continue;
        }
        let Some(project) = repo.get_project(pid)? else {
            continue;
        };
        let Some(provider) = launch::provider(&project, &old) else {
            continue;
        };
        let policy = launch::effective_policy(&registry, provider, &old);
        if !policy.enabled
            || old
                .pointer("/runtimeSettings/accountRecoverySuppressed")
                .and_then(Value::as_bool)
                == Some(true)
        {
            continue;
        }
        let detection = detector.detect_blocking(pid, sid, Some(provider.id()), false);
        if !detection.captured {
            continue;
        }
        let _gate = state.accounts.mutations.lock().map_err(store::error)?;
        let Some(session) = repo.get_session(pid, sid)? else {
            continue;
        };
        if session["lifecycleState"].as_str() != Some("running")
            || session
                .pointer("/runtimeSettings/accountRecoverySuppressed")
                .and_then(Value::as_bool)
                == Some(true)
        {
            continue;
        }
        let registry = store::read(&db)?;
        let policy = launch::effective_policy(&registry, provider, &session);
        if !policy.enabled {
            continue;
        }
        let mut recovery = session
            .pointer("/runtimeSettings/accountRecovery")
            .cloned()
            .unwrap_or(Value::Null);
        let now = Utc::now();
        if recovery["status"].as_str() == Some("needsAttention") {
            continue;
        }
        if recovery["status"].as_str() == Some("retrying") {
            continue;
        }
        if detection.activity.is_some()
            || (crate::presentation::presentation_activity(&session, &now.to_rfc3339())
                == "working"
                && !detection.notice.as_ref().is_some_and(retryable))
        {
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
                save(state, &repo, &session, recovery)?;
            }
            continue;
        }
        if detection.prompt.is_some()
            || crate::session_chat_send::transcript_pending_question_prompt(&session).is_some()
        {
            continue;
        }
        let notice =
            crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid);
        let Some(notice) = notice else { continue };
        if !retryable(&notice) || (notice.kind != "usageLimit" && !policy.retry_errors) {
            if !recovery.is_null() && recovery["status"].as_str() == Some("waiting") {
                recovery["status"] = json!("needsAttention");
                recovery["reason"] = json!(notice.title);
                recovery["updatedAt"] = json!(now.to_rfc3339());
                save(state, &repo, &session, recovery)?;
            }
            continue;
        }
        let attempts = recovery["attempt"].as_u64().unwrap_or(0);
        if recovery.is_null() || recovery["status"].as_str() != Some("waiting") {
            recovery = json!({"status":"waiting","reason":notice.title,"trigger":notice.kind,"attempt":attempts,"nextAttemptAt":(now+backoff(attempts)).to_rfc3339(),"updatedAt":now.to_rfc3339()});
            if notice.kind == "usageLimit"
                && session
                    .pointer("/runtimeSettings/accountId")
                    .and_then(Value::as_str)
                    .is_some()
                && attempts == 0
            {
                recovery["nextAttemptAt"] = json!(now.to_rfc3339());
            }
            save(state, &repo, &session, recovery.clone())?;
        }
        if time(recovery.get("nextAttemptAt")).is_some_and(|t| t > now) {
            continue;
        }
        let mut session = session;
        if notice.kind == "usageLimit" {
            let current_id = session
                .pointer("/runtimeSettings/accountId")
                .and_then(Value::as_str);
            let current = snapshot
                .accounts
                .iter()
                .find(|a| Some(endpoint::account_id(a).as_str()) == current_id);
            let model = detection
                .options
                .as_ref()
                .and_then(|o| o.selection.model.as_ref())
                .map(|m| m.value.clone())
                .unwrap_or_default();
            let candidates = ranked(
                &registry,
                &snapshot,
                provider,
                current_id,
                &model,
                policy.priority,
            );
            if policy.at_limit == LimitAction::Switch && !candidates.is_empty() {
                let id = &candidates[0].id;
                if let Err(error) = endpoint::select(
                    state,
                    &repo,
                    &registry,
                    &snapshot,
                    &project,
                    &session,
                    Some(id),
                ) {
                    recovery["reason"] = json!(error.message);
                    recovery["nextAttemptAt"] = json!((now + backoff(attempts)).to_rfc3339());
                    recovery["attempt"] = json!(attempts + 1);
                    session = repo.get_session(pid, sid)?.unwrap_or(session);
                    recovery["restartRequired"] =
                        json!(session["lifecycleState"].as_str() == Some("sleeping"));
                    save(state, &repo, &session, recovery)?;
                    continue;
                }
                session = repo.get_session(pid, sid)?.unwrap_or(session);
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
                    save(state, &repo, &session, recovery)?;
                    continue;
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
                save(state, &repo, &session, recovery)?;
                continue;
            }
            recovery["restartRequired"] = json!(true);
            save(state, &repo, &session, recovery.clone())?;
            if let Err(e) = endpoint::cycle(state, &repo, &session, "/api/sleepSession")
                .and_then(|_| endpoint::cycle(state, &repo, &session, "/api/wakeSession"))
            {
                session = repo.get_session(pid, sid)?.unwrap_or(session);
                recovery["reason"] = json!(e.message);
                recovery["attempt"] = json!(attempts + 1);
                recovery["nextAttemptAt"] = json!((now + backoff(attempts + 1)).to_rfc3339());
                save(state, &repo, &session, recovery)?;
                continue;
            }
            session = repo.get_session(pid, sid)?.unwrap_or(session);
        }
        let queue = crate::session_chat_queue::read_session_chat_queue_snapshot_with(&db, pid, sid);
        if queue.queue.iter().any(|p| p.state != "queued") {
            recovery["status"] = json!("needsAttention");
            recovery["reason"] =
                json!("A queued message needs review before recovery can continue.");
            save(state, &repo, &session, recovery)?;
            continue;
        }
        let claim = uuid::Uuid::new_v4().to_string();
        recovery["restartRequired"] = json!(false);
        recovery["status"] = json!("retrying");
        recovery["claim"] = json!(claim);
        recovery["attempt"] = json!(attempts + 1);
        recovery["updatedAt"] = json!(now.to_rfc3339());
        recovery["healthySince"] = Value::Null;
        save(state, &repo, &session, recovery)?;
        plans.push(Plan {
            project: pid.into(),
            session: sid.into(),
            claim,
            prompt: queue.deliverable_head().map(|p| p.id.clone()),
        });
    }
    Ok(plans)
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
