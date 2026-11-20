/*
CDXC:Telemetry 2026-08-26:
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

use super::{client, extensions::ExtensionRef, gate, heartbeat, queue, settings_snapshot, state};

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
pub fn spawn_heartbeat_task<F, G>(
    paths: GxserverPaths,
    mut shutdown_rx: broadcast::Receiver<()>,
    collect: F,
    collect_snapshot: G,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot> + Send + Sync + 'static,
    G: Fn() -> settings_snapshot::SnapshotFacts + Send + Sync + 'static,
{
    let collect = Arc::new(collect);
    let collect_snapshot = Arc::new(collect_snapshot);
    tokio::spawn(async move {
        loop {
            let pass_paths = paths.clone();
            let pass_collect = collect.clone();
            let pass_collect_snapshot = collect_snapshot.clone();
            let _ = tokio::task::spawn_blocking(move || {
                run_heartbeat_once(
                    &pass_paths,
                    pass_collect.as_ref(),
                    pass_collect_snapshot.as_ref(),
                );
            })
            .await;

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {}
            }
        }
    })
}

fn run_heartbeat_once<F, G>(paths: &GxserverPaths, collect: &F, collect_snapshot: &G)
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot>,
    G: Fn() -> settings_snapshot::SnapshotFacts,
{
    if !gate::is_enabled(paths) {
        /*
        CDXC:Telemetry 2026-11-20 (privacy review of 591004b37): forget the built-in extension
        list while analytics are off, so switches made then are never reported once they are on
        again; the first snapshot after only records the list.
        */
        if let Ok(db) = crate::storage::open_gxserver_database(paths) {
            state::clear_built_in_extensions(&db);
        }
        return;
    }
    /*
    CDXC:Telemetry 2026-08-27 (addendum v2, §3):
    Collect BEFORE the due-check, and refresh the per-event profile fields from
    the result, because those two things run on different schedules. The
    heartbeat EVENT is due at most once a day; the `project_bucket` /
    `default_agent` values that ride every OTHER event have to be populated from
    the first loop iteration onwards, which is this task's immediate run at
    server start. Folding the refresh into the collect the heartbeat already
    does is what keeps "refresh at startup and once per heartbeat" from needing
    a second DB reader.
    */
    let Some(snapshot) = collect() else {
        return;
    };
    queue::refresh_profile(super::profile::ProfileSnapshot::from_counts(
        snapshot.project_count,
        snapshot.default_agent,
    ));
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
    heartbeat::emit(&snapshot);
    /*
    CDXC:Telemetry 2026-11-20:
    The settings snapshot rides the heartbeat's once-a-day clock rather than
    keeping one of its own, so an install sends both or neither and the two
    always describe the same day. A built-in extension switched on or off since
    the last snapshot becomes an `extension.enabled` / `extension.disabled`
    here: built-in extensions are settings any client can change (the desktop,
    the web build, the phone, `ghostex settings`), and this is the one place
    that sees every one of them. The first snapshot only records the list.
    */
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    settings_snapshot::emit(settings.as_ref(), &collect_snapshot());
    let known = super::extensions::known_built_in_extensions();
    if let Some((previous_known, previous_on)) = state::read_built_in_extensions(&db) {
        for (id, enabled) in super::extensions::built_in_extension_changes(
            &previous_known,
            &previous_on,
            &known,
            &snapshot.built_in_extensions_on,
        ) {
            super::capture::extension_toggled(ExtensionRef::BuiltIn(id), "built_in", enabled);
        }
    }
    state::write_built_in_extensions(&db, &known, &snapshot.built_in_extensions_on);
    state::write_last_heartbeat_at(
        &db,
        &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    );
}
