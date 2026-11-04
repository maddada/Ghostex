//! Follows the Workspaces built-in extension's switch (`crate::workspaces::workspaces_feature_enabled`)
//! so turning it on or off reaches every connected client within seconds: the team subscriptions
//! start or stop, a work-mode pass runs when it comes on, and every client gets one fresh
//! presentation snapshot (with or without the workspaces document and project workspace ids).
//! It also follows the two Issues Workflow settings (`crate::work_mode::issues_workflow_settings`):
//! a flip runs a work pass, which republishes the open sessions whose title or Clean up offer
//! changed.
//!
//! CDXC:Workspaces 2026-10-09 WHY:
//! The switch lives in the settings file, which the desktop's Settings page and `ghostex settings`
//! write directly, so gxserver never hears of the write; it is read every few seconds like the
//! Spaces switch (`sidebar_spaces_switch.rs`). One full snapshot is sent instead of per-row deltas
//! because the switch changes every project row, every work-mode session and a side document at
//! once, and a snapshot is the one frame every client already applies wholesale.

use super::*;

/// How often the settings file is read.
const SETTINGS_POLL: Duration = Duration::from_secs(5);

pub(crate) fn start_workspaces_switch_watch(state: Arc<AppState>) {
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut published = crate::workspaces::read_workspaces_feature_enabled(&state.paths);
        crate::workspaces::remember_workspaces_feature_enabled(published);
        let mut issues_settings = crate::work_mode::read_issues_workflow_settings(&state.paths);
        crate::work_mode::remember_issues_workflow_settings(issues_settings);
        let mut poll = crate::ticks::steady_interval(SETTINGS_POLL);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = poll.tick() => {}
            }
            let next_issues_settings =
                crate::work_mode::read_issues_workflow_settings(&state.paths);
            if next_issues_settings != issues_settings {
                issues_settings = next_issues_settings;
                crate::work_mode::remember_issues_workflow_settings(issues_settings);
                super::work_mode_sync::spawn_work_mode_refresh(&state);
            }
            let enabled = crate::workspaces::read_workspaces_feature_enabled(&state.paths);
            if enabled == published {
                continue;
            }
            crate::workspaces::remember_workspaces_feature_enabled(enabled);
            crate::team_sync::reload_team_sync();
            let publish_state = state.clone();
            let sent =
                tokio::task::spawn_blocking(move || publish_presentation_snapshot(&publish_state))
                    .await;
            // A pass that could not publish tries again on the next read.
            if matches!(sent, Ok(Ok(()))) {
                published = enabled;
                if enabled {
                    super::work_mode_sync::spawn_work_mode_refresh(&state);
                }
            }
        }
    });
}

fn publish_presentation_snapshot(state: &AppState) -> std::result::Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let sessions = repository.list_presentation_sessions()?;
    let snapshot =
        read_presentation_snapshot_in_sequence(state, &db, &state.metadata.server_id, sessions)?;
    let revision = snapshot
        .get("revision")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "revision": revision,
        "serverId": state.metadata.server_id.clone(),
        "snapshot": snapshot,
        "type": "presentationSnapshot",
    }));
    Ok(())
}
