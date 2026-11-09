//! A GBot chat on its Cloud Box: the chat is made here, in the GBot's project, as a draft that
//! never starts on this computer, and handed to the box the way a new chat's Cloud Box pick hands
//! its draft over (cloud_boxes/for_gbots.rs). The box runs it in its own worktree of the box's
//! project and lists it under the GBot (the box session's home is the GBot's project).
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A box can't reach this computer's gxserver (home pushes to the box; nothing comes back but what home reads), so a chat there can't run `ghostex gbots brief` or save memory. Its first message therefore carries the whole brief, and tells it to end its final message with a "To remember:" list instead of saving memory itself; the user or Chief of Staff keeps what matters. A box only runs code of a project with a repository, so a GBot on a Cloud Box works in that box's project.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::store::BotComputer;
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

use super::super::record::Bot;

/// The first message of a chat on a box: who it is, where it works, its brief, then the task.
fn first_message(
    state: &AppState,
    db: &Connection,
    bot: &Bot,
    computer: &BotComputer,
    task: Option<&str>,
) -> String {
    let team = super::super::brief::Team::load(db).ok();
    let brief = team
        .as_ref()
        .map(|team| super::super::brief::brief_text(&state.paths, bot, team))
        .unwrap_or_default();
    let mut text = format!(
        "You are {}{}, one of the user's GBots, working on the Cloud Box {}. Your brief is below. From the box, `ghostex gbots` commands can't reach the computer that keeps your memory, so don't run them: when something should be remembered, end your final message with a short \"To remember:\" list and the user or Chief of Staff saves it.\n\n{brief}",
        bot.name,
        if bot.title.is_empty() { String::new() } else { format!(", {}", bot.title) },
        computer.label(),
    );
    match task.map(str::trim).filter(|task| !task.is_empty()) {
        Some(task) => {
            text.push_str("\n\n---\n\n");
            text.push_str(task);
        }
        None => text.push_str("\n\nSay in one short line that you're ready and wait."),
    }
    text
}

/// Hands the new chat `project_id`/`session_id` (made in the GBot's project, not started) to the
/// GBot's box `box_id`. Answers the chat and whether the box took it.
pub(crate) fn hand_to_box(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    computer: &BotComputer,
    box_id: &str,
    project_id: &str,
    session_id: &str,
    task: Option<&str>,
) -> Result<(Value, bool), DomainStateError> {
    let row = crate::cloud_boxes::for_gbots::boxes(state)?
        .into_iter()
        .find(|row| row.box_id == box_id)
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "{}'s Cloud Box {} is gone. Give it another computer in GBot Settings > Computer.",
                bot.name,
                computer.label()
            ))
        })?;
    let box_project = row.home_project_id.clone().ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{} belongs to no project here, so {} can't work on it.",
            row.name, bot.name
        ))
    })?;
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| DomainStateError::not_found("The chat is gone."))?;
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert(
        "firstUserMessage".into(),
        json!(first_message(state, db, bot, computer, task)),
    );
    runtime.insert(
        ghostex_gx_protocol::cloud_box::CLOUD_BOX_RUNTIME_SETTINGS_KEY.into(),
        json!({ "pending": true, "provider": row.provider, "reuseBoxId": box_id }),
    );
    // A draft taken by a box never takes another message here, so it is nobody's standing chat.
    if let Some(record) = runtime
        .get_mut(super::super::session::CHAT_KEY)
        .and_then(Value::as_object_mut)
    {
        record.remove("standing");
        record.insert("cloudBoxId".into(), json!(box_id));
    }
    let mut update = Map::new();
    update.insert("projectId".into(), json!(project_id));
    update.insert("sessionId".into(), json!(session_id));
    update.insert("runtimeSettings".into(), Value::Object(runtime));
    repository.update_session(&update)?;
    let arc = super::home::app_state().ok_or_else(|| DomainStateError {
        code: "internalError",
        message: "Ghostex is still starting; try again in a moment.".to_string(),
    })?;
    let draft = ghostex_gx_protocol::cloud_box::CloudBoxDraftRef {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
    };
    let started = tokio::runtime::Handle::current().block_on(
        crate::cloud_boxes::for_gbots::start_draft_on_box(arc, &box_project, draft, box_id),
    );
    let session = repository
        .get_session(project_id, session_id)?
        .unwrap_or(session);
    match started {
        Ok(_) => Ok((session, true)),
        Err(error) => Err(DomainStateError {
            code: error.code,
            message: format!(
                "{}'s Cloud Box didn't take the chat: {}",
                bot.name, error.message
            ),
        }),
    }
}
