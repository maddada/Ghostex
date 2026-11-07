//! The observers' poll loop and the comparison summary.
//!
//! CDXC:AgentHooks 2026-11-01 DECISION:
//! User: "yes let's do side by side for claude/codex/open code. keep hooks for others for now, will work on removing those later". Ghostex tracks Claude, Codex and OpenCode activity through hooks it installs into each agent; these observers read the state the agents now write themselves (Claude's session records, Codex's rollout, OpenCode's service events) and run beside the hooks, which keep driving every state. Each observer event is paired with the hook event for the same session and what only one side saw is recorded, to prove parity before a later step stops installing those three agents' hooks. The comparison runs only while the Diagnostic logs area "Hook-free activity comparison" is on; `ghostex hookless-activity` reads the summary. Claude's observer also runs while Track Claude without its hooks is on (drive.rs).
//! SEE-ALSO: server/src/hookless_activity/comparison.rs (the pairing), server/src/server/agent_http.rs (where the hook side is recorded), packages/settings-catalog/src/data/diagnostic_logging.rs (`gxserver.hooklessActivity`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::claude::{ClaudeObserver, SessionRecords};
use super::codex::CodexObserver;
use super::comparison::{self, Side};
use super::drive::{self, ClaudeDriver};
use super::plan_mode::PlanModeWatch;
use super::process_starts::ProcessStartRecorder;
use super::surfaces::SurfaceRecords;
use crate::domain::DomainRepository;
use crate::logging::DiagnosticLogScenario;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

const POLL: Duration = Duration::from_millis(500);
const SESSIONS_REFRESH: Duration = Duration::from_secs(2);

/// What runs whether or not the observers do: Claude's session records and the process starts
/// read off them (process_starts.rs).
#[derive(Default)]
struct AlwaysOn {
    records: SessionRecords,
    starts: ProcessStartRecorder,
}

#[derive(Default)]
struct Observers {
    always: AlwaysOn,
    claude: ClaudeObserver,
    codex: CodexObserver,
    driver: ClaudeDriver,
    plan_mode: PlanModeWatch,
    surfaces: SurfaceRecords,
    sessions: Vec<Value>,
    sessions_read_at: Option<Instant>,
    /// Whether the last pass ran the comparison (Codex and OpenCode run only for it).
    compared: bool,
}

pub(super) fn comparison_enabled(state: &AppState) -> bool {
    state
        .logger
        .routine_logging_enabled(DiagnosticLogScenario::HooklessActivityComparison)
}

pub(crate) fn spawn_hookless_activity_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut observers = Some(Observers::default());
        let mut running = false;
        loop {
            let compare = comparison_enabled(&state);
            let drive = drive::claude_drive_enabled(&state.paths);
            let mut current = observers.take().unwrap_or_default();
            if !(compare || drive) && running {
                // Off: forget every baseline so turning it back on starts clean.
                super::opencode::refresh(&[], false);
                drive::set_tracked([].iter());
                current = Observers {
                    always: current.always,
                    ..Observers::default()
                };
            }
            running = compare || drive;
            let state = state.clone();
            observers = tokio::task::spawn_blocking(move || {
                poll(&state, &mut current, compare, drive);
                current
            })
            .await
            .ok();
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(POLL) => {}
            }
        }
    })
}

fn poll(state: &AppState, observers: &mut Observers, compare: bool, drive: bool) {
    let observing = compare || drive;
    let live = observers
        .always
        .records
        .read(&state.paths.home_dir, observing);
    observers
        .always
        .starts
        .record(&state.paths.app_state_dir, &live);
    if !observing {
        return;
    }
    if !compare && observers.compared {
        // The comparison turned off while the switch stays on: Codex and OpenCode start over.
        super::opencode::refresh(&[], false);
        observers.codex = CodexObserver::default();
        observers.sessions_read_at = None;
    }
    observers.compared = compare;
    if compare
        && observers
            .sessions_read_at
            .is_none_or(|at| at.elapsed() >= SESSIONS_REFRESH)
    {
        if let Ok(db) = open_gxserver_database(&state.paths) {
            let repository = DomainRepository::new(&db, &state.metadata.server_id);
            if let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") {
                observers.sessions = sessions;
            }
        }
        observers.sessions_read_at = Some(Instant::now());
        super::opencode::refresh(&observers.sessions, true);
    }
    let mut events = observers.claude.poll(&live);
    if drive {
        drive::set_tracked(observers.claude.panes());
        observers.surfaces.poll(&state.paths, &observers.claude);
        for event in &events {
            observers.driver.drive(state, event);
        }
        observers.driver.retry_unread_waits(state);
        observers.plan_mode.poll(state, &observers.claude);
    } else {
        drive::set_tracked([].iter());
    }
    if !compare {
        return;
    }
    events.extend(observers.codex.poll(&observers.sessions));
    events.extend(super::opencode::drain());
    for event in events {
        comparison::record(&state.logger, Side::Observer, event);
    }
    comparison::expire(&state.logger);
}

/// `/api/readHooklessActivityComparison`: the summary, or a reset when `reset` is true.
pub(crate) fn read_comparison(state: &AppState, params: &serde_json::Map<String, Value>) -> Value {
    if params.get("reset").and_then(Value::as_bool) == Some(true) {
        comparison::reset();
        super::opencode::reset();
    }
    let mut summary = comparison::summary(comparison_enabled(state));
    if let Some(object) = summary.as_object_mut() {
        object.insert(
            "opencodeDroppedStreamEvents".to_string(),
            super::opencode::dropped_counts(),
        );
        object.insert(
            "opencodeStreamConnected".to_string(),
            json!(crate::session_chat_opencode::events::connected()),
        );
        // Whether Claude's sessions are driven without their hooks here, and why.
        object.insert(
            "claudeDrive".to_string(),
            json!(drive::claude_drive_reason(&state.paths).map(drive::DriveReason::as_str)),
        );
    }
    summary
}
