//! How long a gxserver asked to stop may take: the cap on draining in-flight HTTP requests, the hard exit deadline, and the work stopped the moment the stop arrives.

use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;

use crate::{
    logging::{GxserverLogInput, GxserverLogger, LogLevel},
    session_chat_follower::stop_all_session_chat_followers,
};

use super::{stop_all_zmx_title_observers, AppState};

/// How long requests already in flight when the stop arrives may keep the listener's drain going.
pub(super) const HTTP_DRAIN_LIMIT: Duration = Duration::from_secs(3);

/// CDXC:ServerDaemon 2026-10-17 WHY:
/// A stop on 2026-10-17 left gxserver alive with its listener closed until it was killed 90 s later, and `cargo xtask start-server` gave up without starting the new build: axum's graceful shutdown waits for every in-flight HTTP request, `/api/waitConnectorAgentTools` held one for 4 minutes per session (it now answers at the stop), any other slow request (a Connectors provider call, a client that never finishes its body) can do the same, and the 10 s exit guard added on 2026-10-12 (daemons left alive overnight by blocking work stuck on a hung Windows drive) only started once that drain was over. The deadline now starts at the stop itself, on a plain thread so a wedged runtime cannot hold it, and is 8 s: the desktop (`gpui_wait_for_local_gxserver_process_exit`) and `start-server` wait 15 s for the old process, and 8 s still covers the 3 s request drain plus the telemetry flush (at most 5 s), which begins at the stop signal.
pub(super) const SHUTDOWN_EXIT_DEADLINE: Duration = Duration::from_secs(8);

/// Ends the process `SHUTDOWN_EXIT_DEADLINE` after the first stop signal, whatever is still running.
pub(super) fn arm_shutdown_exit_deadline(
    mut shutdown_rx: broadcast::Receiver<()>,
    logger: Arc<GxserverLogger>,
    server_id: String,
) {
    let _ = std::thread::Builder::new()
        .name("gxserver-exit-deadline".into())
        .spawn(move || {
            if let Err(broadcast::error::RecvError::Closed) = shutdown_rx.blocking_recv() {
                return;
            }
            std::thread::sleep(SHUTDOWN_EXIT_DEADLINE);
            let _ = logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "serverShutdownDeadlineExit".to_string(),
                server_id: Some(server_id),
                request_id: None,
                client: None,
                duration_ms: Some(SHUTDOWN_EXIT_DEADLINE.as_millis()),
                error: None,
                details: None,
            });
            std::process::exit(0);
        });
}

/// Stops the per-session chat followers and title observers as soon as the stop arrives instead of after the drain: they read screens, transcripts and the database every few hundred milliseconds and keep blocking-pool work queued, which only delays the exit. The push sender drops what it has queued and gets at most its request timeout to finish the request in flight, inside the exit deadline.
pub(super) fn stop_session_watchers_on_shutdown(state: &Arc<AppState>) {
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(broadcast::error::RecvError::Closed) = shutdown_rx.recv().await {
            return;
        }
        stop_all_zmx_title_observers(&state);
        stop_all_session_chat_followers(&state);
        let push = state.push.clone();
        let _ =
            tokio::task::spawn_blocking(move || push.stop(crate::push::expo::HTTP_TIMEOUT)).await;
    });
}

/// Waits for the stop signal and then `HTTP_DRAIN_LIMIT`; the listener's drain is abandoned when this finishes first.
pub(super) async fn http_drain_limit_elapsed(mut shutdown_rx: broadcast::Receiver<()>) {
    if let Err(broadcast::error::RecvError::Closed) = shutdown_rx.recv().await {
        return std::future::pending().await;
    }
    tokio::time::sleep(HTTP_DRAIN_LIMIT).await;
}

pub(super) fn log_http_drain_abandoned(state: &AppState) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "serverHttpDrainAbandoned".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: Some(HTTP_DRAIN_LIMIT.as_millis()),
        error: None,
        details: Some(json!({ "reason": "requests still in flight after the stop" })),
    });
}
