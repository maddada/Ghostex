//! The bot's project: an ordinary gxserver project at the bot folder, marked in `launchSettings`
//! (`isBot: true`, `gbotsBotId`), so every chat of the bot is an ordinary session in it and every
//! client, the phone and the `ghostex` CLI reach them with no new session kind.
//!
//! CDXC:GBots 2026-11-08 WHY:
//! The previous GBots marked its bot projects `launchSettings.gbotId` and published `botKind`/`botProfile`; native GBots use a new key (`gbotsBotId`) and a new presentation field (`gbotsBot`), so neither a computer still on an older release nor the one-time clean-up of the previous GBots (retire.rs) ever takes one for the other. `isBot` keeps Ghostex's automatic carry-on messages (Autopilot, self-heal, stall nudges) away from bot chats, as for every bot (bot_projects.rs). A project's `systemKind` can't hold a new value without a table rebuild, so the mark lives in `launchSettings`.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::Bot;
use crate::agents::helpers::object_from_value;
use crate::domain::{normalize_project_root_path, DomainRepository, DomainStateError};
use crate::server::{schedule_presentation_project_delta, AppState};

/// The `launchSettings` key naming the GBot a project stands for.
pub(crate) const BOT_PROJECT_KEY: &str = "gbotsBotId";

/// The GBot a project stands for; `None` for every other project.
pub(crate) fn gbots_bot_id(project: &Value) -> Option<&str> {
    let launch = project.get("launchSettings")?;
    if launch.get("isBot") != Some(&Value::Bool(true)) {
        return None;
    }
    launch
        .get(BOT_PROJECT_KEY)?
        .as_str()
        .filter(|id| id.starts_with("b_"))
}

/// Adds the bot's project at `folder`, named after the bot.
pub(crate) fn add_bot_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    folder: &std::path::Path,
) -> Result<Value, DomainStateError> {
    let path = Value::String(folder.to_string_lossy().to_string());
    let normalized = normalize_project_root_path(Some(&path), "path", false)?;
    let params = object_from_value(json!({
        "path": normalized,
        "name": bot.name,
        "launchSettings": { "isBot": true, BOT_PROJECT_KEY: bot.bot_id },
        "deletedDefaultCommandIds": crate::sidebar_hud::default_sidebar_command_ids()
            .collect::<Vec<_>>(),
    }));
    let project = repository.add_project_path(&params)?;
    let project = crate::workspaces::place_added_project(repository, db, project, &params)?;
    if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
        schedule_presentation_project_delta(state, db, repository, project_id, "projectAdded")?;
    }
    Ok(project)
}

/// Renames the bot's project after the bot.
pub(crate) fn rename_bot_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
) -> Result<(), DomainStateError> {
    let mut params = Map::new();
    params.insert("projectId".into(), json!(bot.project_id));
    params.insert("name".into(), json!(bot.name));
    repository.update_project(&params)?;
    schedule_presentation_project_delta(state, db, repository, &bot.project_id, "projectUpdated")
}

/// The project of a bot: its record's, else the one marked with its id.
pub(crate) fn bot_project(
    repository: &DomainRepository<'_>,
    bot: &Bot,
) -> Result<Option<Value>, DomainStateError> {
    if let Some(project) = repository.get_project(&bot.project_id)? {
        return Ok(Some(project));
    }
    Ok(repository
        .list_projects()?
        .into_iter()
        .find(|project| gbots_bot_id(project) == Some(bot.bot_id.as_str())))
}

/// Removes the bot's project (its chats were closed first, session.rs).
pub(crate) fn remove_bot_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<(), DomainStateError> {
    repository.remove_project(project_id)?;
    crate::session_chat_queue::forget_session_chat_queue(db, project_id, None)?;
    schedule_presentation_project_delta(state, db, repository, project_id, "projectUpdated")
}
