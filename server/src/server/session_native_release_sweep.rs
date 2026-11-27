//! The Native idle release sweep: once a minute this daemon stops the agents of its idle Native
//! chats (the rules: `crate::session_native_release`), whatever the Auto Sleep setting says. The
//! stop is the ordinary sleep (`/api/sleepSession`), which keeps the conversation and its resume
//! id; the next message sent to the chat starts its agent again (session_chat_send_wake.rs).
//!
//! Each stop is marked `sleepTrigger: nativeIdle`, so the daemon's automatic-sleep declines (a
//! queued message, a question card, background work, a coordinator's active threads, a thread that
//! owes work, a Claude chat waiting for idle compaction) still apply; only the keep-awake lease of
//! a client that shows the chat is weighed here instead, as a longer idle time. Background work its
//! transcript shows (`session_details::session_has_running_background_work`: background commands,
//! subagents and jobs, or a tool call still waiting for its result) keeps it running for at most
//! 4 hours after its last activity (`session_native_release::busy_pin_expired`).
//!
//! SEE-ALSO: server/src/session_native_release.rs, server/src/server/session_auto_sleep_sweep.rs.

use super::*;

use crate::session_native_release::{
    busy_pin_expired, idle_since_ms, is_native_chat, release_due, row_allows_release,
};

/// How often the sweep runs.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// No sweep before clients have had one renewal (60 s) to report what they show again
/// (CDXC:SessionSleep 2026-09-25 in session_auto_sleep_sweep.rs).
const STARTUP_GRACE: Duration = Duration::from_secs(90);
/// The pacing between two stops (CDXC:SessionSleep 2026-06-27: gxserver and terminal teardown
/// are not hit concurrently).
const RELEASE_PACING: Duration = Duration::from_millis(350);

pub(crate) fn start_session_native_release_sweep(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        tokio::select! {
            _ = shutdown.recv() => return,
            _ = tokio::time::sleep(STARTUP_GRACE) => {}
        }
        let mut clock = crate::ticks::steady_interval(SWEEP_INTERVAL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = clock.tick() => {}
            }
            // A connected client that does not report what it shows could be looking at any chat
            // (session_auto_sleep.rs `sweep_allowed`).
            if !crate::session_auto_sleep::sweep_allowed(
                state.event_hub.subscriber_count(),
                chrono::Utc::now().timestamp_millis(),
            ) {
                continue;
            }
            run_sweep(&state).await;
        }
    });
}

async fn run_sweep(state: &Arc<AppState>) {
    let reader = state.clone();
    let targets = tokio::task::spawn_blocking(move || select_targets(&reader))
        .await
        .unwrap_or_default();
    for (index, (project_id, session_id)) in targets.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(RELEASE_PACING).await;
        }
        let releaser = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut params = Map::new();
            params.insert("projectId".into(), Value::String(project_id));
            params.insert("reason".into(), json!("nativeIdleRelease"));
            params.insert("sessionId".into(), Value::String(session_id));
            params.insert(
                "sleepTrigger".into(),
                json!(crate::session_keep_awake::NATIVE_IDLE_SLEEP_TRIGGER),
            );
            dispatch_zmx_lifecycle_http_blocking(
                &releaser,
                "/api/sleepSession".to_string(),
                "native-idle-release".to_string(),
                params,
            )
        })
        .await;
    }
}

/// The Native chats to release now, read from the same presentation snapshot every client reads.
fn select_targets(state: &AppState) -> Vec<(String, String)> {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let server_id = state.metadata.server_id.as_str();
    let repository = DomainRepository::new(&db, server_id);
    let Ok(sessions) = repository.list_presentation_sessions() else {
        return Vec::new();
    };
    let Ok(snapshot) = read_presentation_snapshot_in_sequence(state, &db, server_id, sessions)
    else {
        return Vec::new();
    };
    let rows = snapshot
        .get("sessions")
        .and_then(|sessions| sessions.get("rows").or(Some(sessions)))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !rows.iter().any(is_native_chat) {
        return Vec::new();
    }
    let threads = crate::session_auto_sleep::coordinator_thread_activity(&rows);
    let memory_pressure = crate::memory_guard::read_pressure_level()
        .is_some_and(|level| level >= crate::memory_guard::PressureLevel::Warning);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut targets = Vec::new();
    for row in rows.iter().filter(|row| row_allows_release(row)) {
        let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
        let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
            continue;
        };
        let key = (project_id.clone(), session_id.clone());
        let thread_activity = threads.get(&key);
        if thread_activity.is_some_and(|activity| activity.has_active_thread)
            || crate::coordinators::thread_owes_work(&project_id, &session_id)
            || crate::acp::channel::is_switching(&project_id, &session_id)
        {
            continue;
        }
        let Ok(Some(session)) = repository.get_session(&project_id, &session_id) else {
            continue;
        };
        let activity_changed_at = session
            .pointer("/runtimeSettings/agentActivity/lastChangedAt")
            .and_then(Value::as_str);
        let Some(idle_since) = idle_since_ms(
            row,
            activity_changed_at,
            thread_activity.and_then(|activity| activity.last_thread_activity_ms),
        ) else {
            continue;
        };
        let shown = crate::session_keep_awake::is_held_awake(&project_id, &session_id);
        if !release_due(idle_since, shown, memory_pressure, now_ms) {
            continue;
        }
        if !busy_pin_expired(idle_since, now_ms)
            && crate::session_details::session_has_running_background_work(
                state,
                &project_id,
                &session_id,
                &session,
            )
        {
            continue;
        }
        targets.push(key);
    }
    targets
}
