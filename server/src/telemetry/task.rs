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

use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::broadcast;

use crate::paths::GxserverPaths;

use super::{client, extensions::ExtensionRef, gate, heartbeat, queue, settings_snapshot, state};

pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Brief non-blocking defer for `settings.snapshot` on daemon startup when no
/// cached or live account snapshot exists yet, giving the desktop app's initial
/// account refresh a window to populate Claude/Codex plans.
const SETTINGS_SNAPSHOT_ACCOUNTS_WAIT: Duration = Duration::from_secs(20);

type SnapshotCollector = Arc<dyn Fn() -> settings_snapshot::SnapshotFacts + Send + Sync>;

static SNAPSHOT_COLLECTOR: OnceLock<SnapshotCollector> = OnceLock::new();
static SNAPSHOT_NEEDS_ACCOUNTS: AtomicBool = AtomicBool::new(false);

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
    let collect_snapshot: SnapshotCollector = Arc::new(collect_snapshot);
    let _ = SNAPSHOT_COLLECTOR.set(collect_snapshot.clone());
    tokio::spawn(async move {
        loop {
            let pass_paths = paths.clone();
            let pass_collect = collect.clone();
            let due = tokio::task::spawn_blocking(move || {
                run_heartbeat_once(&pass_paths, pass_collect.as_ref())
            })
            .await
            .unwrap_or(false);

            if due {
                let pass_snapshot = collect_snapshot.clone();
                let mut facts = tokio::task::spawn_blocking(move || pass_snapshot())
                    .await
                    .ok();
                if facts.as_ref().is_some_and(|facts| facts.accounts.is_none()) {
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = tokio::time::sleep(SETTINGS_SNAPSHOT_ACCOUNTS_WAIT) => {}
                    }
                    let pass_snapshot = collect_snapshot.clone();
                    facts = tokio::task::spawn_blocking(move || pass_snapshot())
                        .await
                        .ok();
                }
                if let Some(facts) = facts {
                    let pass_paths = paths.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        if !gate::is_enabled(&pass_paths) {
                            return;
                        }
                        let settings =
                            crate::shared_settings_file::read_shared_settings(&pass_paths);
                        settings_snapshot::emit(settings.as_ref(), &facts);
                        SNAPSHOT_NEEDS_ACCOUNTS.store(facts.accounts.is_none(), Ordering::SeqCst);
                    })
                    .await;
                }
            }

            tokio::select! {
                _ = shutdown_rx.recv() => break,
                _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {}
            }
        }
    })
}

/// Called after `accounts::runtime::AccountRuntime::discover_all` populates
/// account plans. Persists the anonymous counts/plan enums for future startup
/// snapshots and emits a follow-up `settings.snapshot` once if today's startup
/// snapshot went out before any accounts were discovered.
pub(crate) fn note_accounts_discovered(discovered: &crate::accounts::model::Snapshot) {
    let Some(telemetry) = queue::handle() else {
        return;
    };
    if !telemetry.is_enabled() {
        return;
    }
    let Some(report) = crate::telemetry::settings_snapshot::accounts_report_from_discovered(
        telemetry.paths(),
        discovered,
    ) else {
        return;
    };
    if let Ok(db) = crate::storage::open_gxserver_database(telemetry.paths()) {
        state::write_cached_accounts(&db, &report);
    }
    if SNAPSHOT_NEEDS_ACCOUNTS.swap(false, Ordering::SeqCst) {
        if let Some(collect_snapshot) = SNAPSHOT_COLLECTOR.get() {
            let settings = crate::shared_settings_file::read_shared_settings(telemetry.paths());
            let mut facts = collect_snapshot();
            if facts.accounts.is_none() {
                facts.accounts = Some(report);
            }
            settings_snapshot::emit(settings.as_ref(), &facts);
        }
    }
}

fn run_heartbeat_once<F>(paths: &GxserverPaths, collect: &F) -> bool
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot>,
{
    if !gate::is_enabled(paths) {
        /*
        CDXC:Telemetry 2026-11-20 (privacy review of 591004b37): forget the built-in extension
        list while analytics are off, so switches made then are never reported once they are on
        again; the first snapshot after only records the list.
        */
        SNAPSHOT_NEEDS_ACCOUNTS.store(false, Ordering::SeqCst);
        super::capture::reset_mobile_hello_cache();
        if let Ok(db) = crate::storage::open_gxserver_database(paths) {
            state::clear_built_in_extensions(&db);
            state::clear_cached_accounts(&db);
            state::clear_mobile_hello(&db);
        }
        return false;
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
        return false;
    };
    queue::refresh_profile(super::profile::ProfileSnapshot::from_counts(
        snapshot.project_count,
        snapshot.default_agent,
    ));
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        super::debug_log("telemetry heartbeat could not open the state database".to_string());
        return false;
    };
    if !state::heartbeat_is_due(
        state::read_last_heartbeat_at(&db),
        heartbeat::HEARTBEAT_STALE_HOURS,
    ) {
        return false;
    }
    heartbeat::emit(&snapshot);
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
    true
}
