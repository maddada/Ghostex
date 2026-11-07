//! "Update automatically" (Q1365): every half hour, each item whose switch is on and that has an
//! update installs it while it isn't in use: an agent while none of its sessions is working, the
//! other tools while no agent is working. The Ghostex card's switch is the app's own "Keep this
//! machine up to date" (app_update/auto.rs), which runs on its own.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::{install, items, store};
use crate::server::AppState;

const FIRST_PASS_DELAY: Duration = Duration::from_secs(5 * 60);
const PASS_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// A version that was already tried is tried again only after this long.
const RETRY_SAME_VERSION_AFTER: Duration = Duration::from_secs(6 * 60 * 60);

/// Item id to the version last tried and when.
static ATTEMPTS: LazyLock<Mutex<HashMap<String, (String, Instant)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn spawn_updates_auto_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut wait = FIRST_PASS_DELAY;
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(wait) => {}
            }
            wait = PASS_INTERVAL;
            keep_mise_versions(&state).await;
            run_pass(&state).await;
        }
    })
}

/// Every pass, whatever the switches say: names the mise versions running sessions use so no
/// `mise prune` deletes them, and uninstalls versions an update replaced once nothing uses them
/// (agent_cli/mise_keep.rs).
async fn keep_mise_versions(state: &AppState) {
    let home = state.paths.agent_config_home_dir().to_path_buf();
    for problem in crate::agent_cli::mise_keep::pass(&home).await {
        let _ = state.logger.log(crate::logging::GxserverLogInput {
            level: crate::logging::LogLevel::Warn,
            event: "miseKeepVersions".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: Some(problem),
            details: None,
        });
    }
}

async fn run_pass(state: &AppState) {
    let switched_on = store::auto_ids(&state.paths);
    if switched_on.is_empty() {
        return;
    }
    let mut due = Vec::new();
    for item in items::list(state, false).await {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        if id == "app"
            || !switched_on.contains(&id)
            || item["state"] != "available"
            || item["canUpdate"] != true
            || item["inUse"] == true
        {
            continue;
        }
        let version = item["latest"]
            .as_str()
            .or(item["summary"].as_str())
            .unwrap_or_default()
            .to_string();
        let mut attempts = ATTEMPTS.lock().unwrap_or_else(|e| e.into_inner());
        if attempts
            .get(&id)
            .is_some_and(|(tried, at)| *tried == version && at.elapsed() < RETRY_SAME_VERSION_AFTER)
        {
            continue;
        }
        attempts.insert(id.clone(), (version, Instant::now()));
        due.push(id);
    }
    if !due.is_empty() {
        install::start(state, due, "auto");
    }
}
