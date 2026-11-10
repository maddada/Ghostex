//! A computer per GBot: where a GBot's chats run (this computer, a connected computer, or one of
//! the user's Cloud Boxes) and whether it may use that computer's screen and apps.
//!
//! - [`store`]: `gbots_bots.computer` and its columns, and `gbots_remote_chats`.
//! - [`ops`]: the `computer` and `computerSet` ops (GBot Settings > Computer, `ghostex gbots computer`).
//! - [`remote_chat`]: a chat started on a connected computer; [`link`]: the calls to it.
//! - [`cloud_box`]: a chat handed to the GBot's Cloud Box.
//! - [`home`]: a chat elsewhere reading its brief and keeping its memory on the home computer,
//!   and the brief's "Where you work" and computer-use parts.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Coordinator brief for M12 (plan piece 15): "a GBot can be given its own computer to work in: a Cloud Box or another connected computer of the user's, and optionally computer use on it. The GBot's chats then run there." Decided in the plan's spirit: the bot folder, its record, its memory and its routines stay on the home computer. Chats the user, Chief of Staff and other GBots open with it start on its computer and keep their memory at home through gxserver; routine runs (and their command checks, which run in the bot folder) and GBot API chats stay on the home computer, where the routine scheduler and the API watch their chat. Changing the computer moves no open chat: new chats start on the new one.

mod cloud_box;
mod home;
mod link;
mod ops;
mod remote_chat;
mod store;

use rusqlite::Connection;
use serde_json::Value;

use super::record::Bot;
use super::session::ChatOpening;
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

pub(crate) use home::{
    brief_section, forward_home, handle_gbots_remote_http, remember_state, GBOTS_REMOTE_ENDPOINT,
};
pub(crate) use ops::{computer_status, handles, run, ComputerCaller};
pub(crate) use remote_chat::remote_chat_summary;
pub(crate) use store::{list_remote_chats, Place};

/// Whether `bot_id` may use the screen and apps of the computer its chats run on.
pub(crate) fn computer_use(db: &Connection, bot_id: &str) -> bool {
    store::read(db, bot_id).is_ok_and(|computer| computer.computer_use)
}

/// Chats that stay on the home computer whatever the GBot's computer: routine runs and GBot API
/// chats.
fn stays_home(opening: &ChatOpening<'_>) -> bool {
    opening.opened_by.starts_with("routine:") || opening.opened_by.starts_with("api:")
}

/// Opens the chat on the GBot's connected computer when it has one; `None` when the chat is made
/// here (this computer, or a Cloud Box, which takes a chat made here: [`box_for_chat`]).
pub(crate) fn start_elsewhere(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    opening: &ChatOpening<'_>,
) -> Result<Option<(Value, bool)>, DomainStateError> {
    if stays_home(opening) {
        return Ok(None);
    }
    let computer = store::read(db, &bot.bot_id)?;
    match &computer.place {
        Place::Computer(server_id) => remote_chat::start_on_computer(
            state, db, repository, bot, &computer, server_id, opening,
        )
        .map(Some),
        _ => Ok(None),
    }
}

/// Whether a chat about to be made here goes to the GBot's Cloud Box instead of starting here.
pub(crate) fn box_for_chat(db: &Connection, bot: &Bot, opening: &ChatOpening<'_>) -> bool {
    !stays_home(opening)
        && store::read(db, &bot.bot_id)
            .is_ok_and(|computer| matches!(computer.place, Place::CloudBox(_)))
}

/// Hands a chat made here (not started) to the GBot's Cloud Box.
pub(crate) fn hand_to_box(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    project_id: &str,
    session_id: &str,
    task: Option<&str>,
) -> Result<(Value, bool), DomainStateError> {
    let computer = store::read(db, &bot.bot_id)?;
    let Place::CloudBox(box_id) = &computer.place else {
        return Err(DomainStateError::bad_request("This GBot has no Cloud Box."));
    };
    cloud_box::hand_to_box(
        state, db, repository, bot, &computer, box_id, project_id, session_id, task,
    )
}

/// Closes every open chat of a GBot on other computers (it is being removed): each one there when
/// its computer is connected (its transcript stays in that computer's Previous Sessions), and all
/// of them here.
/// Answers how many it closed there.
pub(crate) fn close_remote_chats(db: &Connection, bot_id: &str) -> usize {
    let mut closed = 0;
    for chat in store::list_remote_chats(db, Some(bot_id)).unwrap_or_default() {
        if let Some(computer) = crate::connected_computers::find(&chat.server_id) {
            closed += link::call(
                &computer,
                "/api/killSession",
                serde_json::json!({ "projectId": chat.project_id, "sessionId": chat.session_id }),
            )
            .is_ok() as usize;
        }
    }
    let _ = store::close_bot_remote_chats(db, bot_id);
    closed
}

/// Adds `computer` (where its chats run, when not only here, or computer use) and `remoteChats`
/// (its chats on other computers, which clients nest under its row) to each GBot's `gbotsBot`.
pub(crate) fn decorate_presented_bots(
    db: &Connection,
    looks: &mut std::collections::HashMap<String, Value>,
) {
    let computers = store::all(db).unwrap_or_default();
    let chats = store::list_remote_chats(db, None).unwrap_or_default();
    for (bot_id, look) in looks.iter_mut() {
        let Some(object) = look.as_object_mut() else {
            continue;
        };
        if let Some(computer) = computers
            .get(bot_id)
            .filter(|computer| computer.place != Place::This || computer.computer_use)
        {
            object.insert(
                "computer".into(),
                serde_json::json!({
                    "kind": computer.place.kind(),
                    "name": computer.label(),
                    "computerUse": computer.computer_use,
                }),
            );
        }
        let remote: Vec<Value> = chats
            .iter()
            .filter(|chat| chat.bot_id == *bot_id)
            .map(|chat| {
                serde_json::json!({
                    "serverId": chat.server_id,
                    "projectId": chat.project_id,
                    "sessionId": chat.session_id,
                })
            })
            .collect();
        if !remote.is_empty() {
            object.insert("remoteChats".into(), Value::Array(remote));
        }
    }
}
