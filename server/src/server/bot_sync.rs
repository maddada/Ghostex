//! Running the bot sync (crate::bot_projects): `/api/syncBotProjects`, which a client sends on the
//! way into Bots mode, and one pass when gxserver starts, so a profile made while Ghostex was
//! closed is there without a mode switch. Every added project is published like any other. Also
//! the pass that keeps each bot row's gateway dot and today's run count current.

use std::sync::Arc;

use serde_json::{json, Value};

use super::{schedule_presentation_project_delta, value_text, AppState};
use crate::bot_feed::{bot_automations_enabled, refresh_published_bot_runs_today};
use crate::bot_projects::{
    bot_profile, bots_enabled, discover_bot_profiles, refresh_published_bot_gateways,
    sync_bot_projects,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::session_chat_hermes::hermes_home;
use crate::storage::open_gxserver_database;

/// Adds the missing bot projects and publishes each one. Nothing while Bots is switched off.
pub(crate) fn sync_and_publish_bot_projects(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    if !bots_enabled(&state.paths) {
        return Ok(json!({ "enabled": false, "added": [] }));
    }
    let added = sync_bot_projects(repository, &discover_bot_profiles(&hermes_home()))?;
    if !added.is_empty() {
        crate::external_sessions::rescan_on_next_discovery(&state.paths);
    }
    for project in &added {
        let project_id = value_text(project, "projectId")?;
        schedule_presentation_project_delta(state, db, repository, &project_id, "projectAdded")?;
    }
    Ok(json!({ "enabled": true, "added": added }))
}

/// The startup pass, off the async runtime because it reads the database and the disk.
pub(crate) fn start_bot_project_sync(state: Arc<AppState>) {
    tokio::task::spawn_blocking(move || {
        let synced = match open_gxserver_database(&state.paths) {
            Ok(db) => {
                let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
                sync_and_publish_bot_projects(&state, &db, &repository)
                    .map(drop)
                    .map_err(|error| error.message)
            }
            Err(error) => Err(error.to_string()),
        };
        if let Err(message) = synced {
            log_bot_warning(&state, "botProjectSyncFailed", &message);
        }
    });
}

/// Republishes each bot whose gateway started or stopped, or whose count of runs delivered today
/// changed, since the last pass, so its row's dot and the Automations row's count follow without a
/// reload. It rides the 60s project refresh wake-up in `background_tasks.rs`: a pass reads one or
/// two small files per bot, plus today's cron output while Bot automations is on, and spawns
/// nothing.
pub(crate) fn run_bot_refresh_once(state: &Arc<AppState>) -> Result<(), DomainStateError> {
    if !bots_enabled(&state.paths) {
        return Ok(());
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let bots: Vec<(&str, &str)> = projects
        .iter()
        .filter(|project| crate::presentation::should_include_presentation_project(project))
        .filter_map(|project| {
            Some((
                project.get("projectId").and_then(Value::as_str)?,
                bot_profile(project)?,
            ))
        })
        .collect();
    let profiles = || bots.iter().map(|&(_, profile)| profile);
    let mut changed = refresh_published_bot_gateways(&hermes_home(), profiles());
    if bot_automations_enabled(&state.paths) {
        changed.extend(refresh_published_bot_runs_today(&hermes_home(), profiles()));
    }
    for (project_id, profile) in bots {
        if changed.iter().any(|changed| changed == profile) {
            schedule_presentation_project_delta(
                state,
                &db,
                &repository,
                project_id,
                "projectUpdated",
            )?;
        }
    }
    Ok(())
}

pub(crate) fn log_bot_warning(state: &Arc<AppState>, event: &str, message: &str) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: None,
    });
}
