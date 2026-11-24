//! Queued Terminal-to-Native switches (`Q4771`): when a user asks to switch a running Terminal
//! session to Native mode while the agent is mid-reply (`activity == "working"`), the switch is
//! held here instead of interrupting the turn, and runs automatically as soon as the turn finishes.
//!
//! CDXC:AgentProviders 2026-11-24 DECISION:
//! User (Q4771, `queue-when-done`): clicking "Switch to Native" mid-reply queues the switch until
//! the reply finishes so no in-flight work is lost, shows a cancelable pending state in the chat
//! pill strip, and switches automatically once the agent is no longer working.
//! SEE-ALSO: server/src/acp/channel.rs (`switch_to_acp`, `cancel_switch_to_acp`),
//! server/src/session_chat_queue.rs (`switch_to_native_queued`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::domain::DomainRepository;
use crate::logging::LogLevel;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

const TICK: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
pub(crate) struct PendingSwitchEntry {
    pub(crate) caller_is_user_device: bool,
    pub(crate) queued_at_ms: i64,
    pub(crate) telemetry_source: &'static str,
    pub(crate) telemetry_client: &'static str,
}

static PENDING_SWITCHES: LazyLock<Mutex<HashMap<(String, String), PendingSwitchEntry>>> =
    LazyLock::new(Mutex::default);
static IN_FLIGHT: LazyLock<Mutex<HashSet<(String, String)>>> = LazyLock::new(Mutex::default);

pub(crate) fn is_in_flight(project_id: &str, session_id: &str) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    IN_FLIGHT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&key)
}

pub(crate) fn is_pending(project_id: &str, session_id: &str) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    let pending = PENDING_SWITCHES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&key);
    if pending {
        return true;
    }
    is_in_flight(project_id, session_id)
}

pub(crate) fn queue(
    project_id: &str,
    session_id: &str,
    caller_is_user_device: bool,
    telemetry_source: &'static str,
    telemetry_client: &'static str,
) {
    let key = (project_id.to_string(), session_id.to_string());
    let entry = PendingSwitchEntry {
        caller_is_user_device,
        queued_at_ms: chrono::Utc::now().timestamp_millis(),
        telemetry_source,
        telemetry_client,
    };
    PENDING_SWITCHES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, entry);
}

pub(crate) fn cancel(project_id: &str, session_id: &str) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    PENDING_SWITCHES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key)
        .is_some()
}

pub(crate) fn start_pending_switch_runtime(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut clock = crate::ticks::steady_interval(TICK);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            tick(&state).await;
        }
    });
}

enum PollOutcome {
    StillWorking,
    Drop,
    Ready,
}

async fn tick(state: &Arc<AppState>) {
    let entries: Vec<((String, String), PendingSwitchEntry)> = {
        let map = PENDING_SWITCHES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if map.is_empty() {
            return;
        }
        map.iter().map(|(k, v)| (k.clone(), *v)).collect()
    };

    for ((project_id, session_id), entry) in entries {
        let key = (project_id.clone(), session_id.clone());
        let already_running = IN_FLIGHT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&key);
        if already_running {
            continue;
        }

        let paths = state.paths.clone();
        let server_id = state.metadata.server_id.clone();
        let pid = project_id.clone();
        let sid = session_id.clone();
        let session_opt = tokio::task::spawn_blocking(move || {
            let Ok(db) = open_gxserver_database(&paths) else {
                return Err(());
            };
            let repo = DomainRepository::new(&db, server_id.as_str());
            Ok(repo.get_session(&pid, &sid).ok().flatten())
        })
        .await
        .ok()
        .and_then(Result::ok);

        let outcome = match session_opt {
            None => PollOutcome::StillWorking,
            Some(None) => PollOutcome::Drop,
            Some(Some(session)) => {
                if super::session_is_acp(&session)
                    || super::channel::session_native_switch_family(&session).is_err()
                    || crate::agents::session_is_draft(&session)
                    || crate::presentation::effective_lifecycle_state(&session) != "running"
                {
                    PollOutcome::Drop
                } else {
                    match super::channel::check_session_safe_to_switch_now(
                        state,
                        &project_id,
                        &session_id,
                        &session,
                    )
                    .await
                    {
                        super::channel::SwitchSafety::Safe => PollOutcome::Ready,
                        super::channel::SwitchSafety::Busy(_) => PollOutcome::StillWorking,
                        super::channel::SwitchSafety::CannotResume(_) => PollOutcome::Drop,
                    }
                }
            }
        };

        match outcome {
            PollOutcome::StillWorking => {}
            PollOutcome::Drop => {
                if cancel(&project_id, &session_id) {
                    broadcast_state(state, &project_id, &session_id).await;
                }
            }
            PollOutcome::Ready => {
                // Claim atomically so cancellation or another tick never double-runs this switch.
                let claimed = {
                    let mut pending = PENDING_SWITCHES
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if pending.remove(&key).is_some() {
                        IN_FLIGHT
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .insert(key.clone());
                        true
                    } else {
                        false
                    }
                };
                if !claimed {
                    continue;
                }
                let state_for_task = state.clone();
                let pid = project_id.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    let result = super::channel::execute_switch_to_acp(
                        &state_for_task,
                        &pid,
                        &sid,
                        entry.caller_is_user_device,
                        super::channel::SwitchOrigin::Queued,
                        entry.telemetry_source,
                        entry.telemetry_client,
                    )
                    .await;
                    IN_FLIGHT
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&(pid.clone(), sid.clone()));
                    if let Err(error) = result {
                        if error.code == "sessionBusy" {
                            // A send or queued prompt grabbed the per-session lock right before us:
                            // put the queued switch back so it waits for the newly started turn.
                            queue(
                                &pid,
                                &sid,
                                entry.caller_is_user_device,
                                entry.telemetry_source,
                                entry.telemetry_client,
                            );
                        } else {
                            super::channel::log(
                                LogLevel::Warn,
                                "acp.queuedSwitchToAcpFailed",
                                json!({
                                    "projectId": pid,
                                    "sessionId": sid,
                                    "queuedAtMs": entry.queued_at_ms,
                                    "code": error.code,
                                    "message": error.message,
                                }),
                            );
                        }
                    }
                    // Refresh queue + presentation state after IN_FLIGHT clears so clients stay in sync.
                    broadcast_state(&state_for_task, &pid, &sid).await;
                });
            }
        }
    }
}

pub(crate) async fn broadcast_state(state: &AppState, project_id: &str, session_id: &str) {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    if let Ok(Ok(Some(updated_session))) = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths)?;
        let repo = DomainRepository::new(&db, server_id.as_str());
        Ok::<_, anyhow::Error>(repo.get_session(&pid, &sid).ok().flatten())
    })
    .await
    {
        state.event_hub.broadcast(json!({
            "type": "sessionUpdated",
            "projectId": project_id,
            "session": updated_session,
        }));
    }
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );
}
