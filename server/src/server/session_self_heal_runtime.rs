//! Restarts agent sessions whose terminal daemon died under a running agent. Every few seconds it
//! reads which daemons are alive, hands the sightings to the book in session_self_heal.rs, and runs
//! the restarts it asks for: the wake half of `ghostex agents reload` (the sleep half only writes
//! `sleeping` for a daemon that is already gone), then "carry on" for a coordinator's thread or a
//! coordinator, and a report for the thread's coordinator.
//!
//! SEE-ALSO: server/src/session_self_heal.rs (the rules and the CDXC:SessionSleep decision),
//! server/src/session_chat_agent_restart.rs (`reload`).

use super::*;

use crate::coordinators::{self, QuietOwner, SessionKey, SessionQuiet};
use crate::session_self_heal::{
    self as heal, GiveUpReason, HealDue, HealOutcome, HealReportKind, Sighting,
};

const TICK: Duration = Duration::from_secs(3);

pub(crate) fn start_session_self_heal_runtime(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    heal::note_server_stopping();
                    break;
                }
                _ = clock.tick() => {}
            }
            if heal::server_stopping() {
                break;
            }
            let ticking = state.clone();
            let due = tokio::task::spawn_blocking(move || tick(&ticking))
                .await
                .unwrap_or_default();
            for due in due {
                tokio::spawn(heal_session(state.clone(), due));
            }
        }
    });
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn text_field(session: &Value, key: &str) -> Option<String> {
    session
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A running agent session in a zmx daemon of this computer that a restart can bring back; drafts
/// (nothing to resume, and an agent switch kills their daemon on purpose) and box sessions are not.
fn heal_candidate(session: &Value) -> Option<(SessionKey, String)> {
    if session.get("kind").and_then(Value::as_str) != Some("agent")
        || session_state_sync::read_session_persistence_provider(session).as_deref() != Some("zmx")
        || crate::agents::session_is_draft(session)
        || crate::agentbox::is_agentbox_session(session)
    {
        return None;
    }
    let project_id = text_field(session, "projectId")?;
    let session_id = text_field(session, "sessionId")?;
    if crate::agents::draft_agent_switch_in_progress(&project_id, &session_id) {
        return None;
    }
    let zmx_name = crate::zmx::provider_zmx_session_name(session).ok()?;
    Some(((project_id, session_id), zmx_name))
}

/// BLOCKING (one `zmx list`, a process scan every `AGENT_CHECK_EVERY_MS`, SQLite).
fn tick(state: &AppState) -> Vec<HealDue> {
    if heal::server_stopping() {
        return Vec::new();
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return Vec::new();
    };
    // No reading, no judgment: a failed `zmx list` must never read as every daemon gone.
    let Ok(live) = read_cached_zmx_existing_session_names() else {
        return Vec::new();
    };
    let now = now_ms();
    let now_iso = crate::presentation::now_iso();
    let candidates = sessions
        .iter()
        .filter_map(|session| {
            let (key, zmx_name) = heal_candidate(session)?;
            let working = matches!(
                crate::presentation::presentation_activity(session, &now_iso).as_str(),
                "working" | "attention"
            );
            Some((key, zmx_name, working))
        })
        .collect::<Vec<_>>();
    let alive = candidates
        .iter()
        .filter(|(_, zmx_name, _)| live.contains(zmx_name))
        .map(|(key, _, _)| key.clone())
        .collect::<Vec<_>>();
    let check = heal::agent_check_due(&alive, now);
    let agents = if check.is_empty() {
        None
    } else {
        let names = candidates
            .iter()
            .filter(|(key, _, _)| check.contains(key))
            .map(|(_, zmx_name, _)| zmx_name.clone())
            .collect::<Vec<_>>();
        read_cached_zmx_session_process_identities(&names, &state.paths.home_dir).ok()
    };
    probe_working_sessions_without_agent(
        state,
        &sessions,
        &candidates,
        &live,
        &check,
        agents.as_ref(),
    );
    let sightings = candidates
        .into_iter()
        .map(|(key, zmx_name, working)| Sighting {
            daemon_alive: live.contains(&zmx_name),
            agent_running: agents
                .as_ref()
                .filter(|_| check.contains(&key))
                .map(|agents| agents.contains_key(&zmx_name)),
            working,
            key,
        })
        .collect();
    let outcome = heal::observe(sightings, now);
    for (key, reason) in outcome.gave_up {
        gave_up(state, &db, &key, &reason);
    }
    publish_attention(state, &db, &repository);
    outcome.heals
}

/// Sessions with a probe from [`probe_working_sessions_without_agent`] running, so a slow capture
/// is never stacked.
static GONE_PROBES_IN_FLIGHT: std::sync::LazyLock<Mutex<std::collections::HashSet<SessionKey>>> =
    std::sync::LazyLock::new(Default::default);

/// A session's place in [`GONE_PROBES_IN_FLIGHT`], given back when the probe ends, panics included.
struct GoneProbeInFlight(SessionKey);

impl Drop for GoneProbeInFlight {
    fn drop(&mut self) {
        GONE_PROBES_IN_FLIGHT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.0);
    }
}

/// A session that reads working while this tick's process check found no agent in its live daemon
/// gets a fresh look through the chat detection funnel, so the gone-agent reconcile there
/// (session_chat_agent_gone_activity.rs) also reaches a session nobody has open. Working only:
/// the tick's own `working` counts attention too, and an attention is left alone. The funnel's
/// classifier decides whether the agent is gone; this only asks it to look.
fn probe_working_sessions_without_agent(
    state: &AppState,
    sessions: &[Value],
    candidates: &[(SessionKey, String, bool)],
    live: &std::collections::HashSet<String>,
    checked: &[SessionKey],
    agents: Option<&HashMap<String, crate::zmx::ZmxProcessIdentity>>,
) {
    let Some(agents) = agents else {
        return;
    };
    for (key, zmx_name, _) in candidates {
        if !checked.contains(key) || !live.contains(zmx_name) || agents.contains_key(zmx_name) {
            continue;
        }
        let Some(session) = sessions.iter().find(|session| {
            text_field(session, "projectId").as_deref() == Some(key.0.as_str())
                && text_field(session, "sessionId").as_deref() == Some(key.1.as_str())
        }) else {
            continue;
        };
        if !crate::agents::stored_activity_is_working(session) {
            continue;
        }
        if !GONE_PROBES_IN_FLIGHT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key.clone())
        {
            continue;
        }
        let slot = GoneProbeInFlight(key.clone());
        let detector = crate::session_chat_options::SessionChatOptionDetector::new(state);
        let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
        tokio::task::spawn_blocking(move || {
            let slot = slot;
            detector.detect_blocking(&slot.0 .0, &slot.0 .1, agent.as_deref(), true);
        });
    }
}

/// Shows every session Ghostex gave up on as needing attention: the red dot and one notification
/// (`publish_stuck_changes`), cleared once it runs again.
fn publish_attention(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
) {
    let changes = coordinators::replace_session_quiet(
        QuietOwner::Heal,
        heal::gave_up_sessions()
            .into_iter()
            .map(|(key, at)| {
                (
                    key,
                    SessionQuiet {
                        last_output_at: None,
                        quiet_since: at,
                        stalled: true,
                        stuck: true,
                    },
                )
            })
            .collect(),
    );
    super::session_stall_runtime::publish_stuck_changes(
        state,
        db,
        repository,
        QuietOwner::Heal,
        changes,
    );
}

/// Whether the session is an open coordinator's thread, and whether it is a coordinator. A bot's
/// conversation is neither: it is never told to carry on (CDXC:Bots 2026-11-08, bot_projects.rs).
fn roles(db: &rusqlite::Connection, server_id: &str, key: &SessionKey) -> (bool, bool) {
    if crate::bot_projects::is_bot_conversation(&DomainRepository::new(db, server_id), &key.0) {
        return (false, false);
    }
    let thread = coordinators::read_thread(db, &key.0, &key.1)
        .ok()
        .flatten()
        .is_some_and(|thread| !thread.is_resolved());
    let coordinator = coordinators::read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()
        .is_some();
    (thread, coordinator)
}

fn gave_up(state: &AppState, db: &rusqlite::Connection, key: &SessionKey, reason: &GiveUpReason) {
    if roles(db, state.metadata.server_id.as_str(), key).0 {
        heal::queue_report(
            key,
            HealReportKind::GaveUp {
                reason: reason.clone(),
            },
        );
    }
    log(
        state,
        "sessionSelfHealGaveUp",
        LogLevel::Warn,
        key,
        json!({ "reason": format!("{reason:?}") }),
    );
}

fn log(state: &AppState, event: &str, level: LogLevel, key: &SessionKey, details: Value) {
    let mut details = details;
    details["projectId"] = json!(key.0);
    details["sessionId"] = json!(key.1);
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// The row as it is now, if it still says running with its daemon gone.
fn still_dead(state: &AppState, key: &SessionKey) -> Option<Value> {
    if heal::server_stopping() {
        return None;
    }
    let db = open_gxserver_database(&state.paths).ok()?;
    let session = DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_session(&key.0, &key.1)
        .ok()
        .flatten()?;
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return None;
    }
    let (_, zmx_name) = heal_candidate(&session)?;
    // The authoritative list, not the cached one: this decides a restart.
    let live = crate::zmx::read_zmx_existing_session_names().ok()?;
    (!live.contains(&zmx_name)).then_some(session)
}

/// One restart: read the row again, wake it (resume command, same account or the wake's account
/// rules), then tell a thread or coordinator to carry on and queue the thread's report.
async fn heal_session(state: Arc<AppState>, due: HealDue) {
    let key = due.key.clone();
    let outcome = run_heal(&state, &due).await;
    let restarted = matches!(outcome, HealOutcome::Restarted);
    let failure = match &outcome {
        HealOutcome::Failed(error) => Some(error.clone()),
        _ => None,
    };
    let cancelled = matches!(outcome, HealOutcome::Cancelled);
    if let Some(reason) = heal::finish_heal(&key, now_ms(), outcome) {
        let gave_state = state.clone();
        let gave_key = key.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(db) = open_gxserver_database(&gave_state.paths) {
                gave_up(&gave_state, &db, &gave_key, &reason);
            }
        })
        .await;
    }
    if restarted {
        log(
            &state,
            "sessionSelfHealed",
            LogLevel::Warn,
            &key,
            json!({ "attempt": due.attempt }),
        );
    } else if let Some(error) = failure {
        log(
            &state,
            "sessionSelfHealFailed",
            LogLevel::Warn,
            &key,
            json!({ "attempt": due.attempt, "reason": error }),
        );
    } else if cancelled {
        log(
            &state,
            "sessionSelfHealCancelled",
            LogLevel::Info,
            &key,
            json!({}),
        );
    }
}

async fn run_heal(state: &Arc<AppState>, due: &HealDue) -> HealOutcome {
    let key = &due.key;
    let read_state = state.clone();
    let read_key = key.clone();
    let Some(session) = tokio::task::spawn_blocking(move || still_dead(&read_state, &read_key))
        .await
        .ok()
        .flatten()
    else {
        return HealOutcome::Cancelled;
    };
    log(
        state,
        "sessionDaemonLost",
        LogLevel::Warn,
        key,
        json!({ "attempt": due.attempt, "wasWorking": due.was_working }),
    );
    if !crate::session_chat_queue_runtime::can_start_on_its_conversation(state, &session).await {
        return HealOutcome::Hopeless(GiveUpReason::NothingToResume);
    }
    let body = json!({ "params": { "projectId": key.0, "sessionId": key.1 } });
    let routed = super::zmx_http::handle_zmx_lifecycle_http(
        state,
        "/api/wakeSession".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    let ok = routed.response.status().is_success();
    let bytes = axum::body::to_bytes(routed.response.into_body(), 256 * 1024)
        .await
        .unwrap_or_default();
    let answer = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
    crate::session_chat_options::forget_session_chat_options(state, &key.0, &key.1);
    if !ok {
        // An error answer carries its reason at the top level (`domain_error_response`), beside the `error` code.
        let message = answer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the session could not be started again")
            .chars()
            .take(300)
            .collect();
        return HealOutcome::Failed(message);
    }
    if answer.pointer("/result/attach/restoreBlocked").is_some() {
        return HealOutcome::Hopeless(GiveUpReason::RestartFailed(
            "its working folder no longer exists".to_string(),
        ));
    }
    crate::session_chat_agent_restart::note_agent_restarted(state, &key.0, &key.1);
    crate::session_chat_send_diagnostics::record_send_recovery(
        state,
        "sessionAgentSelfHealed",
        &key.0,
        &key.1,
        "The session's terminal ended under its running agent; restarted the agent on its own conversation.",
        &[],
    );
    let role_state = state.clone();
    let role_key = key.clone();
    let (thread, coordinator) = tokio::task::spawn_blocking(move || {
        open_gxserver_database(&role_state.paths)
            .map(|db| roles(&db, role_state.metadata.server_id.as_str(), &role_key))
            .unwrap_or((false, false))
    })
    .await
    .unwrap_or((false, false));
    if thread || coordinator {
        let carry_on = super::coordinator_runtime::send_to_coordinator(
            state,
            key,
            heal::CARRY_ON_MESSAGE,
            &Uuid::new_v4().to_string(),
        )
        .await
        .err()
        .map(|error| error.chars().take(200).collect::<String>());
        if thread {
            // Before the hold lifts (`finish_heal`): a resumed thread that sits idle goes to the
            // idle watch (its cut-off turn) instead of reading as a finished turn.
            let clear_state = state.clone();
            let clear_key = key.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(db) = open_gxserver_database(&clear_state.paths) {
                    let _ = coordinators::clear_thread_observed_working(
                        &db,
                        &clear_key.0,
                        &clear_key.1,
                    );
                }
            })
            .await;
            heal::queue_report(
                key,
                HealReportKind::Restarted {
                    attempt: due.attempt,
                    carry_on_error: carry_on,
                },
            );
        }
    } else if due.was_working {
        // Nobody supervises it: tell the user its task was cut off.
        let notify_state = state.clone();
        let notify_key = key.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let Ok(db) = open_gxserver_database(&notify_state.paths) else {
                return;
            };
            let repository = DomainRepository::new(&db, notify_state.metadata.server_id.as_str());
            let mut params = Map::new();
            params.insert("projectId".to_string(), json!(notify_key.0));
            params.insert("sessionId".to_string(), json!(notify_key.1));
            params.insert(
                "body".to_string(),
                json!("Restarted after it stopped: it ended unexpectedly while working, so its last task was cut off. Tell it to carry on."),
            );
            let _ = crate::notification_feed::create_notification_endpoint(
                &notify_state,
                &repository,
                &db,
                &params,
            );
        })
        .await;
    }
    HealOutcome::Restarted
}
