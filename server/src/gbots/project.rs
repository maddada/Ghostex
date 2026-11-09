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

/// The one sentence a create that would land in a GBot's project is refused with.
pub(crate) const BOT_PROJECT_REFUSAL: &str = "A GBot's own folder is only for its chats, so start this in a project the user can see: pass --path <folder> (Ghostex adds the folder as a project) or --project-id <id>.";

/// A session create's `projectPath` (with no `projectId`) added as a project the way Add Project
/// adds one (a folder that doesn't exist yet is created), so the session lands in a project the
/// sidebar shows; `params` names that project from here on.
///
/// CDXC:GBots 2026-11-09 WHY:
/// Issue #519: a bot asked to "create a project in a folder and a coordinator for it" ran `ghostex coordinator create` from its chat, the create defaulted to the chat's own project, and the coordinator was invisible in every sidebar and in `ghostex coordinator list`. `--path <folder>` on `coordinator create`, `coordinator start-thread` and `agents create` sends `projectPath`; adding it here (not in the CLI) keeps it working through `--computer`, where `/api/addProjectPath` is not open to the other computer.
pub(crate) fn add_create_project_path(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &mut Map<String, Value>,
) -> Result<(), DomainStateError> {
    let named = params
        .get("projectId")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty());
    let Some(path) = params
        .get("projectPath")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty() && !named)
        .map(str::to_string)
    else {
        return Ok(());
    };
    let path = normalize_project_root_path(Some(&Value::String(path)), "path", true)?;
    let project = match repository.find_project_by_path(&path)? {
        Some(project) => project,
        None => {
            let add = object_from_value(json!({ "path": path }));
            let project = repository.add_project_path(&add)?;
            let project = crate::workspaces::place_added_project(repository, db, project, &add)?;
            if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
                schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    project_id,
                    "projectAdded",
                )?;
            }
            project
        }
    };
    if let Some(project_id) = project.get("projectId").and_then(Value::as_str) {
        params.insert("projectId".into(), json!(project_id));
        params.remove("projectPath");
    }
    Ok(())
}

/// Refuses a create through the session routes into a GBot's project: only the bot's own chats
/// live there, and they are opened through `/api/gbots` (session.rs `create_bot_chat_with`), never
/// through these routes. Reopening one of its closed chats is the exception.
///
/// CDXC:GBots 2026-11-09 WHY:
/// A GBot's project is hidden from every project list, so a coordinator, thread or session created in it by a call that defaulted to the calling chat's project (issue #519) showed nowhere but `ghostex sessions`. The rule sits on `/api/createSession`, `/api/createAgentSession` and `/api/createWorktreeSession`, so `ghostex coordinator create`, `agents create`, `coordinator start-thread`, `create-agent` and every app obey it alike; threads a bot hands to another bot through `ghostex gbots ask` are bot chats and are not affected.
pub(crate) fn refuse_bot_project_create(
    project: &Value,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let reopening = ["restoredFromSessionId", "restoredFromHistoryId"]
        .iter()
        .any(|key| params.get(*key).is_some_and(|value| !value.is_null()));
    if gbots_bot_id(project).is_some() && !reopening {
        return Err(DomainStateError::bad_request(BOT_PROJECT_REFUSAL));
    }
    Ok(())
}
