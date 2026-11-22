//! The "Keep this machine up to date" switch: installs an available update on its own while no agent here is working.
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

use super::http::start_install;
use super::record::{age_ms, now_iso, read_settled_record, write_record, AutoAttempt};
use super::status::{ask_app, AppAnswer};
use crate::domain::DomainRepository;
use crate::presentation::presentation_activity;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

const FIRST_PASS_DELAY: Duration = Duration::from_secs(3 * 60);
const PASS_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// A version the switch already tried is tried again only after this long.
const RETRY_SAME_VERSION_AFTER_MS: i64 = 6 * 60 * 60 * 1000;

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);

pub(super) fn wake_auto_update() {
    WAKE.notify_one();
}

pub(crate) fn spawn_app_auto_update_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut wait = FIRST_PASS_DELAY;
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(wait) => {}
                _ = WAKE.notified() => {}
            }
            wait = PASS_INTERVAL;
            run_pass(&state).await;
        }
    })
}

/// CDXC:RemoteMachines 2026-10-16 WHY:
/// The update restarts the app and gxserver but not the zmx sessions, so agents keep running through it; the switch still waits for a moment when no agent is working, because the app's windows close and reopen under the user and a turn that is streaming would lose its live view for those seconds. A version that failed is retried after six hours, not every pass.
async fn run_pass(state: &Arc<AppState>) {
    let record = read_settled_record(&state.paths);
    if !record.auto_update || record.pending_install.is_some() {
        return;
    }
    let AppAnswer::Report(report) = ask_app(state, "status").await else {
        return;
    };
    let supported = report.get("supported").and_then(Value::as_bool) == Some(true);
    let phase = report
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(version) = report
        .get("availableVersion")
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
    else {
        return;
    };
    if !supported || phase != "available" {
        return;
    }
    if record.last_auto_attempt.as_ref().is_some_and(|attempt| {
        attempt.version == version
            && age_ms(&attempt.at).is_some_and(|age| age < RETRY_SAME_VERSION_AFTER_MS)
    }) {
        return;
    }
    let busy_state = state.clone();
    let busy = tokio::task::spawn_blocking(move || any_agent_working(&busy_state))
        .await
        .unwrap_or(true);
    if busy {
        return;
    }
    let mut record = read_settled_record(&state.paths);
    record.last_auto_attempt = Some(AutoAttempt {
        version,
        at: now_iso(),
    });
    write_record(&state.paths, &record);
    let _ = start_install(state, "auto").await;
}

/// A running session whose agent is working (one waiting on the user is not: its question survives the restart).
fn any_agent_working(state: &AppState) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return true;
    };
    let now = now_iso();
    sessions
        .iter()
        .any(|session| presentation_activity(session, &now) == "working")
}
