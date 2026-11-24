/*
CDXC:Telemetry 2026-08-26:
The two background tasks, spawned ONLY from the long-running server loop
alongside the other background tasks. A one-shot `ghostex` CLI verb never calls
`telemetry::init`, so it never has a handle, never queues, and never starts a
task: telemetry simply does not exist in that process.

Both tasks follow the shape every other task in `server/src/server/background_tasks.rs`
uses: blocking work off the async worker via `spawn_blocking`, and shutdown
through the shared broadcast rather than a private lifecycle channel. The flush
task differs in one deliberate way: on shutdown it does a FINAL flush before
returning (unless held during unfinished first-run onboarding or opted out), so
the events from the last 30 seconds of a session are not lost every time someone
quits Ghostex.

CDXC:Telemetry 2026-11-24 (privacy overhaul, Q19/Q20/Q22):
- Q19: Zero bytes leave the machine while first-run onboarding is unfinished.
  Events buffer in memory and are immediately dropped + wiped if the user opts
  out during onboarding.
- Q20: Daily `heartbeat`, `session_mode.daily_rollup`, and `settings.snapshot`
  only emit when a Ghostex client (desktop, web, mobile) or agent session was
  actively used since the previous heartbeat. An idle daemon never phones home.
- Q22: Turning analytics off immediately drops the in-memory queue, deletes the
  entire `analytics.state.v1` row from SQLite, and removes `analytics-install-id`.
*/

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use tokio::sync::{broadcast, Notify};

use crate::paths::GxserverPaths;

use super::{
    client, extensions::ExtensionRef, gate, heartbeat, identity, queue, settings_snapshot, state,
};

pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const HEARTBEAT_ACTIVITY_COALESCE: Duration = Duration::from_secs(2);
const HEARTBEAT_ONBOARDING_POLL: Duration = Duration::from_secs(30);
const HEARTBEAT_DISABLED_POLL: Duration = Duration::from_secs(5 * 60);
const HEARTBEAT_MAX_WAIT: Duration = Duration::from_secs(60 * 60);
/// Brief non-blocking defer for `settings.snapshot` on daemon startup when no
/// cached or live account snapshot exists yet, giving the desktop app's initial
/// account refresh a window to populate Claude/Codex plans.
const SETTINGS_SNAPSHOT_ACCOUNTS_WAIT: Duration = Duration::from_secs(20);

type SnapshotCollector = Arc<dyn Fn() -> settings_snapshot::SnapshotFacts + Send + Sync>;

static SNAPSHOT_COLLECTOR: OnceLock<SnapshotCollector> = OnceLock::new();
static SNAPSHOT_NEEDS_ACCOUNTS: AtomicBool = AtomicBool::new(false);
static OPT_OUT_WIPED: AtomicBool = AtomicBool::new(false);
static ACTIVITY_NOTIFY: OnceLock<Notify> = OnceLock::new();

/// How long the caller may wait for the shutdown flush before giving up.
/// Bounded, because a hung network must not delay quitting the app.
pub const SHUTDOWN_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

fn activity_notify() -> &'static Notify {
    ACTIVITY_NOTIFY.get_or_init(Notify::new)
}

/// Wake the heartbeat task when client or session activity arrives or first-run
/// onboarding completes/skips.
pub(super) fn notify_activity() {
    if let Some(notify) = ACTIVITY_NOTIFY.get() {
        notify.notify_one();
    }
}

/// Immediately drop the in-memory queue, clear cached identity and counters,
/// delete the entire `analytics.state.v1` row from SQLite, and remove
/// `analytics-install-id` from disk (Q22).
pub fn purge_on_opt_out(paths: &GxserverPaths) {
    if let Some(telemetry) = queue::handle() {
        telemetry.drop_queue();
        telemetry.clear_cached_identity();
    }
    SNAPSHOT_NEEDS_ACCOUNTS.store(false, Ordering::SeqCst);
    super::capture::reset_mobile_hello_cache();
    state::clear_pending_switches();
    state::clear_pending_activity();
    identity::delete_install_id(paths);
    if let Ok(db) = crate::storage::open_gxserver_database(paths) {
        state::wipe_all_state(&db);
        OPT_OUT_WIPED.store(true, Ordering::SeqCst);
    }
}

/// Same as [`purge_on_opt_out`], guarded by an atomic so repeated dropped
/// captures while already opted out do not re-open SQLite on every call.
pub(super) fn purge_on_opt_out_if_needed(paths: &GxserverPaths) {
    if !OPT_OUT_WIPED.load(Ordering::SeqCst) {
        purge_on_opt_out(paths);
    }
}

pub(super) fn reset_opt_out_wiped() {
    OPT_OUT_WIPED.store(false, Ordering::SeqCst);
}

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
    while events are already queued must not have those events sent, and all
    local telemetry state is wiped immediately.
    */
    if !telemetry.is_enabled() {
        purge_on_opt_out(telemetry.paths());
        return;
    }
    /*
    Q19: Hold all PostHog flushes while first-run onboarding is unfinished so
    zero bytes leave the machine before the user sees the disclosure/toggle.
    */
    if gate::is_onboarding_held(telemetry.paths()) {
        if super::debug_payload_logging_enabled() && !telemetry.queue_is_empty() {
            super::debug_log(format!(
                "holding {} buffered event(s) until first-run onboarding completes",
                telemetry.queued_len()
            ));
        }
        return;
    }
    if let Ok(db) = crate::storage::open_gxserver_database(telemetry.paths()) {
        state::persist_pending_switches(&db);
        state::persist_pending_activity(&db);
    }
    if telemetry.queue_is_empty() {
        return;
    }
    let distinct_id = telemetry.ensure_distinct_id(true);
    while !telemetry.queue_is_empty() {
        let mut batch = telemetry.take_batch();
        if batch.is_empty() {
            break;
        }
        for queued in &mut batch {
            queued.attempts = queued.attempts.saturating_add(1);
        }
        let encoded = telemetry.encode_batch(&batch, &distinct_id);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeartbeatRunOutcome {
    Emitted,
    NotDue(Duration),
    WaitingForActivity,
    OnboardingHeld,
    Disabled,
    Skipped,
}

/// The activity-gated daily heartbeat. Runs only when a Ghostex client or
/// agent session has been active since the previous heartbeat and onboarding
/// is finished, then sleeps until the next due window.
pub fn spawn_heartbeat_task<F, G, H>(
    paths: GxserverPaths,
    mut shutdown_rx: broadcast::Receiver<()>,
    collect: F,
    collect_snapshot: G,
    collect_live_modes: H,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot> + Send + Sync + 'static,
    G: Fn() -> settings_snapshot::SnapshotFacts + Send + Sync + 'static,
    H: Fn() -> super::capture::LiveSessionModeCounts + Send + Sync + 'static,
{
    let collect = Arc::new(collect);
    let collect_snapshot: SnapshotCollector = Arc::new(collect_snapshot);
    let _ = SNAPSHOT_COLLECTOR.set(collect_snapshot.clone());
    let collect_live_modes = Arc::new(collect_live_modes);
    let notify = activity_notify();
    tokio::spawn(async move {
        loop {
            let pass_paths = paths.clone();
            let pass_collect = collect.clone();
            let pass_collect_live_modes = collect_live_modes.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                run_heartbeat_once(
                    &pass_paths,
                    pass_collect.as_ref(),
                    pass_collect_live_modes.as_ref(),
                )
            })
            .await
            .unwrap_or(HeartbeatRunOutcome::Skipped);

            if outcome == HeartbeatRunOutcome::Emitted {
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
                        if !gate::is_enabled(&pass_paths) || gate::is_onboarding_held(&pass_paths) {
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

            match outcome {
                HeartbeatRunOutcome::Emitted => {
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = tokio::time::sleep(HEARTBEAT_MAX_WAIT) => {}
                    }
                }
                HeartbeatRunOutcome::NotDue(remaining) => {
                    let wait = remaining.clamp(Duration::from_secs(60), HEARTBEAT_MAX_WAIT);
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = tokio::time::sleep(wait) => {}
                    }
                }
                HeartbeatRunOutcome::WaitingForActivity => {
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = notify.notified() => {
                            tokio::select! {
                                _ = shutdown_rx.recv() => break,
                                _ = tokio::time::sleep(HEARTBEAT_ACTIVITY_COALESCE) => {}
                            }
                        }
                    }
                }
                HeartbeatRunOutcome::OnboardingHeld => {
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = notify.notified() => {
                            tokio::select! {
                                _ = shutdown_rx.recv() => break,
                                _ = tokio::time::sleep(HEARTBEAT_ACTIVITY_COALESCE) => {}
                            }
                        }
                        _ = tokio::time::sleep(HEARTBEAT_ONBOARDING_POLL) => {}
                    }
                }
                HeartbeatRunOutcome::Disabled | HeartbeatRunOutcome::Skipped => {
                    tokio::select! {
                        _ = shutdown_rx.recv() => break,
                        _ = notify.notified() => {}
                        _ = tokio::time::sleep(HEARTBEAT_DISABLED_POLL) => {}
                    }
                }
            }
        }
    })
}

/// Called after `accounts::runtime::AccountRuntime::discover_all` populates
/// account plans. Persists the anonymous counts/plan enums for future startup
/// snapshots and emits a follow-up `settings.snapshot` once if today's active
/// snapshot went out before any accounts were discovered.
pub(crate) fn note_accounts_discovered(discovered: &crate::accounts::model::Snapshot) {
    let Some(telemetry) = queue::handle() else {
        return;
    };
    if !telemetry.is_enabled() || gate::is_onboarding_held(telemetry.paths()) {
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

fn run_heartbeat_once<F, H>(
    paths: &GxserverPaths,
    collect: &F,
    collect_live_modes: &H,
) -> HeartbeatRunOutcome
where
    F: Fn() -> Option<heartbeat::HeartbeatSnapshot>,
    H: Fn() -> super::capture::LiveSessionModeCounts,
{
    if !gate::is_enabled(paths) {
        purge_on_opt_out(paths);
        return HeartbeatRunOutcome::Disabled;
    }
    if gate::is_onboarding_held(paths) {
        return HeartbeatRunOutcome::OnboardingHeld;
    }
    /*
    CDXC:Telemetry 2026-08-27 (addendum v2, §3):
    Collect BEFORE the due-check, and refresh the per-event profile fields from
    the result, because those two things run on different schedules. The
    heartbeat EVENT is due at most once a day when active; the `project_bucket`
    and `default_agent` values that ride every OTHER event stay populated from
    startup onwards.
    */
    let Some(snapshot) = collect() else {
        return HeartbeatRunOutcome::Skipped;
    };
    queue::refresh_profile(super::profile::ProfileSnapshot::from_counts(
        snapshot.project_count,
        snapshot.default_agent,
    ));
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        super::debug_log("telemetry heartbeat could not open the state database".to_string());
        return HeartbeatRunOutcome::Skipped;
    };
    state::persist_pending_activity(&db);
    let last_heartbeat_at = state::read_last_heartbeat_at(&db);
    if !state::heartbeat_is_due(last_heartbeat_at, heartbeat::HEARTBEAT_STALE_HOURS) {
        let remaining = last_heartbeat_at
            .and_then(|last| {
                let due_at = last + chrono::Duration::hours(heartbeat::HEARTBEAT_STALE_HOURS);
                due_at
                    .signed_duration_since(chrono::Utc::now())
                    .to_std()
                    .ok()
            })
            .unwrap_or(HEARTBEAT_MAX_WAIT);
        return HeartbeatRunOutcome::NotDue(remaining);
    }
    let last_activity_at = state::read_last_activity_at(&db);
    if !state::has_recent_activity_since_heartbeat(
        chrono::Utc::now(),
        last_activity_at,
        last_heartbeat_at,
        state::HEARTBEAT_ACTIVITY_RECENCY_HOURS,
    ) {
        return HeartbeatRunOutcome::WaitingForActivity;
    }
    heartbeat::emit(&snapshot);
    let settings = crate::shared_settings_file::read_shared_settings(paths);
    let install_modes = super::capture::resolve_install_session_modes(settings.as_ref());
    let live_modes = collect_live_modes();
    let switches = state::take_daily_switches(&db);
    super::capture::session_mode_daily_rollup(install_modes, live_modes, switches);
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
    HeartbeatRunOutcome::Emitted
}
