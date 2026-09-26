//! The Auto Sleep sweep: once a minute, and within five seconds of the setting changing, this
//! daemon sleeps its idle agent sessions that no client shows. The decision is
//! `crate::session_auto_sleep`; this file reads its inputs and performs the sleeps.
//!
//! CDXC:SessionSleep 2026-09-25 WHY:
//! Keep-awake leases live in memory, so a daemon that just started knows of none: every client's
//! "I show these sessions" report is gone until it renews (the desktop renews every minute). A
//! sweep in that window would sleep the session the user is looking at, which is the bug
//! CDXC:SessionSleep 2026-08-20 records, so the first sweep waits out one renewal and a margin.
//! The sleeps go out one at a time 350 ms apart (CDXC:SessionSleep 2026-06-27: gxserver and
//! terminal teardown are not hit concurrently), each marked `sleepTrigger: automatic` so the
//! daemon's own declines (a lease taken meanwhile, queued chat prompts, never active, background
//! work) still apply.
//!
//! SEE-ALSO: server/src/session_auto_sleep.rs, server/src/session_keep_awake.rs.

use super::*;

use crate::session_auto_sleep::{sessions_to_sleep, sweep_allowed, AutoSleepSettings};

/// How often the sweep runs (the runtime's `GPUI_AUTO_SLEEP_MONITOR_INTERVAL_MS`).
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// How often the settings file is read, so a changed setting sweeps promptly, as the runtime's
/// `onRuntimeSettingsChanged` did.
const SETTINGS_POLL: Duration = Duration::from_secs(5);
/// No sweep before clients have had one renewal (60 s) to report what they show again.
const STARTUP_GRACE: Duration = Duration::from_secs(90);
/// The pacing between two sleeps (`GPUI_SIDEBAR_BULK_SLEEP_INTERVAL_MS`).
const SLEEP_PACING: Duration = Duration::from_millis(350);

pub(crate) fn start_session_auto_sleep_sweep(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let started = Instant::now();
        let mut last_sweep: Option<Instant> = None;
        let mut last_settings: Option<AutoSleepSettings> = None;
        let mut poll = tokio::time::interval(SETTINGS_POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            if started.elapsed() < STARTUP_GRACE {
                continue;
            }
            let settings = read_auto_sleep_settings(&state.paths);
            let changed = last_settings != Some(settings);
            last_settings = Some(settings);
            let due = last_sweep.is_none_or(|at| at.elapsed() >= SWEEP_INTERVAL);
            if !due && !changed {
                continue;
            }
            last_sweep = Some(Instant::now());
            if settings.idle_minutes == 0
                || !sweep_allowed(
                    state.event_hub.subscriber_count(),
                    chrono::Utc::now().timestamp_millis(),
                )
            {
                continue;
            }
            run_sweep(&state, settings).await;
        }
    });
}

fn read_auto_sleep_settings(paths: &GxserverPaths) -> AutoSleepSettings {
    let settings = fs::read_to_string(paths.app_config_dir.join("native-sidebar-settings.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    AutoSleepSettings::from_settings(settings.as_ref())
}

async fn run_sweep(state: &Arc<AppState>, settings: AutoSleepSettings) {
    let reader = state.clone();
    let targets = tokio::task::spawn_blocking(move || select_targets(&reader, settings))
        .await
        .unwrap_or_default();
    for (index, (project_id, session_id)) in targets.into_iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(SLEEP_PACING).await;
        }
        let sleeper = state.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut params = Map::new();
            params.insert("projectId".into(), Value::String(project_id));
            params.insert("reason".into(), json!("autoSleep"));
            params.insert("sessionId".into(), Value::String(session_id));
            params.insert("sleepTrigger".into(), json!("automatic"));
            dispatch_zmx_lifecycle_http_blocking(
                &sleeper,
                "/api/sleepSession".to_string(),
                "auto-sleep-sweep".to_string(),
                params,
            )
        })
        .await;
    }
}

/// The sessions to sleep now, read from the same presentation snapshot every client reads.
fn select_targets(state: &AppState, settings: AutoSleepSettings) -> Vec<(String, String)> {
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
    sessions_to_sleep(
        &snapshot,
        &|project_id, session_id| session_keep_awake::is_held_awake(project_id, session_id),
        settings,
        chrono::Utc::now().timestamp_millis(),
    )
}
