//! The storage cleanup sweep: a few minutes after gxserver starts and then once a day, it applies
//! every age limit in `crate::storage_cleanup` and its owners' (Codex mirror, visual pages), on
//! every platform and on remote computers alike, and logs one line saying what it deleted.
//!
//! CDXC:ServerDaemon 2026-11-03 DECISION:
//! User: Ghostex's data folders had reached 5.4 GB with nothing ever deleted. The age limits are a gxserver feature for every user ("gxserver feature for everyone"), applied silently ("None, just do it quietly"; no Settings page, settings or CLI verb) and from the first run ("Nothing, just run it"; no notice or prompt). The limits themselves sit beside each rule.
//!
//! SEE-ALSO: server/src/storage_cleanup/, server/src/codex_rollout_archive.rs (`prune_unread_mirrors`), server/src/visual_pages/store.rs (`prune_old_pages`), server/src/connectors/executor/process.rs (`discard_previous_traces`).

use super::*;

use crate::storage_cleanup::{expired_closed_sessions, Tally, CLOSED_SESSION_MAX_IDLE};

/// Let startup work (wake-on-open, transcript scans, clients reconnecting) finish first.
const STARTUP_DELAY: Duration = Duration::from_secs(3 * 60);
const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Closed sessions removed in one sweep; a backlog bigger than this finishes on later days.
const MAX_SESSIONS_PER_SWEEP: usize = 5_000;
/// The pause between two session removals, so the presentation deltas they broadcast reach
/// clients as a trickle rather than a burst.
const SESSION_REMOVAL_PACING: Duration = Duration::from_millis(20);

pub(crate) fn start_storage_cleanup_sweep(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut wait = STARTUP_DELAY;
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(wait) => {}
            }
            wait = SWEEP_INTERVAL;
            let sweep_state = state.clone();
            let _ = tokio::task::spawn_blocking(move || run_sweep(&sweep_state)).await;
        }
    });
}

fn run_sweep(state: &AppState) {
    let now = std::time::SystemTime::now();
    let paths = &state.paths;
    let files = [
        (
            "images",
            crate::storage_cleanup::prune_images(&paths.app_data_dir, now),
        ),
        (
            "codexMirror",
            crate::codex_rollout_archive::prune_unread_mirrors(now),
        ),
        (
            "visualPages",
            crate::visual_pages::prune_old_pages(&paths.root_dir, now),
        ),
        (
            "logs",
            crate::storage_cleanup::prune_logs(&paths.logs_dir, now),
        ),
        (
            "backups",
            crate::storage_cleanup::prune_backups(&paths.root_dir, &paths.app_state_dir, now),
        ),
    ];
    let sessions = remove_expired_closed_sessions(state);
    let mut total = Tally::default();
    let mut details = Map::new();
    for (name, tally) in files {
        total.add(tally);
        if tally.files > 0 {
            details.insert(
                name.to_string(),
                json!({ "files": tally.files, "bytes": tally.bytes }),
            );
        }
    }
    if total.files == 0 && sessions == 0 {
        return;
    }
    details.insert("files".to_string(), json!(total.files));
    details.insert("bytes".to_string(), json!(total.bytes));
    details.insert("closedSessions".to_string(), json!(sessions));
    // Deleting the user's files is rare (daily, and only past an age limit) and must be traceable,
    // so the summary is persisted without a diagnostic area, like the empty-session closes.
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "storageCleanup".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(Value::Object(details)),
    });
}

/// Removes the closed sessions past [`CLOSED_SESSION_MAX_IDLE`] and returns how many went.
fn remove_expired_closed_sessions(state: &AppState) -> usize {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return 0;
    };
    let cutoff = chrono::Utc::now()
        - chrono::Duration::from_std(CLOSED_SESSION_MAX_IDLE).unwrap_or_default();
    let cutoff = cutoff.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Ok(expired) = expired_closed_sessions(&db, &cutoff, MAX_SESSIONS_PER_SWEEP) else {
        return 0;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut removed = 0;
    for (project_id, session_id) in expired {
        let mut params = Map::new();
        params.insert("projectId".into(), json!(project_id));
        params.insert("sessionId".into(), json!(session_id));
        params.insert("reason".into(), json!("storageCleanup"));
        if remove_session_row(state, &db, &repository, &params).is_ok() {
            removed += 1;
        }
        std::thread::sleep(SESSION_REMOVAL_PACING);
    }
    removed
}
