/*
CDXC:AnonymousAnalytics 2026-08-26:
The two background tasks, spawned ONLY from the long-running server loop
alongside the other background tasks. A one-shot `ghostex` CLI verb never calls
`telemetry::init`, so it never has a handle, never queues, and never starts a
task — telemetry simply does not exist in that process.

Both tasks follow the shape every other task in `server/src/server/background_tasks.rs`
uses: blocking work off the async worker via `spawn_blocking`, and shutdown
through the shared broadcast rather than a private lifecycle channel. The flush
task differs in one deliberate way — on shutdown it does a FINAL flush before
returning, so the events from the last 30 seconds of a session are not lost
every time someone quits Ghostex.
*/

use std::{sync::Arc, time::Duration};

use tokio::sync::broadcast;

use crate::paths::GxserverPaths;

use super::{client, gate, heartbeat, queue, state};

pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// How long the caller may wait for the shutdown flush before giving up.
/// Bounded, because a hung network must not delay quitting the app.
pub const SHUTDOWN_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

fn flush_interval() -> Duration {
    std::env::var("GHOSTEX_TELEMETRY_FLUSH_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_FLUSH_INTERVAL)
}

/// Drain and post everything currently queued, in batches. Blocking; runs
/// inside `spawn_blocking`.
fn flush_blocking() {
    let Some(telemetry) = queue::handle() else {
        return;
    };
    /*
    The gate is consulted at FLUSH as well as at capture. A user who opts out
    while events are already queued must not have those events sent — so this
    drops the queue rather than draining it.
    */
    if !telemetry.is_enabled() {
        telemetry.drop_queue();
        return;
    }
    while !telemetry.queue_is_empty() {
        let mut batch = telemetry.take_batch();
        if batch.is_empty() {
            break;
        }
        for queued in &mut batch {
            queued.attempts = queued.attempts.saturating_add(1);
        }
        let encoded = telemetry.encode_batch(&batch);
        if client::send_batch(&telemetry.endpoint, encoded) {
            continue;
        }
        /*
        A failed batch goes back at the FRONT and the loop stops: the next
        interval retries it. Continuing would just fail the rest of the queue
        against the same dead network and burn every event's attempt cap at once.
        */
        telemetry.requeue_front(batch);
        break;
    }
}

pub fn spawn_flush_task(mut shutdown_rx: broadcast::Receiver<()>) -> tokio::task::JoinHandle<()> {
    let interval = flush_interval();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(interval) => {
                    let _ = tokio::task::spawn_blocking(flush_blocking).await;
                }
            }
        }
        let _ = tokio::task::spawn_blocking(flush_blocking).await;
    })
}

/// The daily heartbeat. Runs its due-check immediately at start (so an install
/// that is only ever open for twenty minutes a day still reports), then on a
/// 24h interval.
pub fn spawn_heartbeat_task<F>(
    paths: GxserverPaths,
    mut shutdown_rx: broadcast::Receiver<()>,
    collect: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot> + Send + Sync + 'static,
{
    let collect = Arc::new(collect);
    tokio::spawn(async move {
        loop {
            let pass_paths = paths.clone();
            let pass_collect = collect.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_heartbeat_once(&pass_paths, pass_collect.as_ref());
            })
            .await;

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {}
            }
        }
    })
}

fn run_heartbeat_once<F>(paths: &GxserverPaths, collect: &F)
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot>,
{
    if !gate::is_enabled(paths) {
        return;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        super::debug_log("telemetry heartbeat could not open the state database".to_string());
        return;
    };
    if !state::heartbeat_is_due(
        state::read_last_heartbeat_at(&db),
        heartbeat::HEARTBEAT_STALE_HOURS,
    ) {
        return;
    }
    let Some(snapshot) = collect() else {
        return;
    };
    heartbeat::emit(&snapshot);
    state::write_last_heartbeat_at(
        &db,
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    );
}
