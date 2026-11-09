//! A GBot chat on a connected computer: created there through the endpoints that computer opens
//! to this one's agents (connected_computers/scope.rs: `createAgentSession`, `startSessionProvider`,
//! `queueSessionChatPrompt`), the way `ghostex coordinator start-thread --computer` starts a
//! thread there, and recorded here in `gbots_remote_chats`.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Plan piece 15: the bot folder stays on its home computer. A chat on another computer works in a folder there (`~/GBots/<name>` unless picked, added as a project there) and is marked `runtimeSettings.gbotHome`, so its own `ghostex gbots brief` and `ghostex gbots memory …` go back to the home gxserver (home.rs), never to a copy of the files. Work handed to it with `ghostex gbots ask` is linked as the sender's thread on another computer (`coordinator_remote_threads`), so its reports reach the sender through the supervisor that already watches threads on other computers.

use rusqlite::Connection;
use serde_json::{json, Value};

use super::link;
use super::store::{self, BotComputer, RemoteChat};
use crate::coordinators::{coordinator_launcher_family, now_iso};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

use super::super::record::Bot;
use super::super::session::{bot_agent_rows, check_agent, ChatOpening};

/// The `runtimeSettings` key that marks a session as a chat of another computer's GBot.
pub(crate) const HOME_KEY: &str = "gbotHome";

/// A remote chat as `/api/gbots` answers it.
pub(crate) fn remote_chat_summary(chat: &RemoteChat) -> Value {
    json!({
        "serverId": chat.server_id,
        "projectId": chat.project_id,
        "sessionId": chat.session_id,
        "globalRef": crate::ids::create_global_session_ref(&chat.server_id, &chat.project_id, &chat.session_id),
        "title": chat.title,
        "openedBy": chat.opened_by,
        "createdAt": chat.created_at,
    })
}

/// The session value `create_bot_chat_with` answers for a chat on another computer.
fn session_value(chat: &RemoteChat, computer_name: &str) -> Value {
    json!({
        "projectId": chat.project_id,
        "sessionId": chat.session_id,
        "serverId": chat.server_id,
        "computerName": computer_name,
        "title": chat.title,
        "kind": "agent",
        "createdAt": chat.created_at,
        "updatedAt": chat.created_at,
        "runtimeSettings": { "gbot": { "botId": chat.bot_id, "openedBy": chat.opened_by } },
    })
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Links a chat there as the sender's thread, so its reports come back here.
fn link_sender(db: &Connection, chat: &RemoteChat, opening: &ChatOpening<'_>) {
    if let Some((coordinator_project, coordinator_session, task)) = opening.link {
        let _ = crate::coordinators::link_remote_thread(
            db,
            &(
                chat.server_id.clone(),
                chat.project_id.clone(),
                chat.session_id.clone(),
            ),
            &(
                coordinator_project.to_string(),
                coordinator_session.to_string(),
            ),
            task,
            &chat.title,
        );
        let _ = crate::coordinators::set_remote_thread_resolved(
            db,
            &(
                chat.server_id.clone(),
                chat.project_id.clone(),
                chat.session_id.clone(),
            ),
            false,
        );
    }
}

/// The sender's standing chat with `bot` on that computer, if it still takes messages: the task
/// goes into it instead of a new chat.
fn reuse_standing(
    db: &Connection,
    peer: &crate::connected_computers::ConnectedComputer,
    bot: &Bot,
    computer: &BotComputer,
    opening: &ChatOpening<'_>,
) -> Option<(Value, bool)> {
    if !opening.standing {
        return None;
    }
    let from = opening.from?;
    let chat = store::list_remote_chats(db, Some(&bot.bot_id))
        .ok()?
        .into_iter()
        .find(|chat| {
            chat.standing
                && chat.server_id == peer.server_id
                && chat.from_key.as_deref() == Some(from)
        })?;
    let queued = link::call(
        peer,
        "/api/queueSessionChatPrompt",
        json!({
            "projectId": chat.project_id, "sessionId": chat.session_id,
            "text": opening.task.unwrap_or_default(), "startupSend": false,
            "sendRequestId": uuid::Uuid::new_v4().to_string(),
        }),
    );
    if queued.is_err() {
        // Gone there (closed by hand): a new chat takes the work.
        let _ = store::close_remote_chat(db, &chat.server_id, &chat.project_id, &chat.session_id);
        return None;
    }
    let _ = link::call(
        peer,
        "/api/wakeSession",
        json!({ "projectId": chat.project_id, "sessionId": chat.session_id }),
    );
    link_sender(db, &chat, opening);
    Some((session_value(&chat, &computer.label()), true))
}

/// Opens a chat with `bot` on the connected computer `server_id`. Answers the chat (with
/// `serverId`) and whether its agent started.
pub(crate) fn start_on_computer(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    bot: &Bot,
    computer: &BotComputer,
    server_id: &str,
    opening: &ChatOpening<'_>,
) -> Result<(Value, bool), DomainStateError> {
    let name = computer.label();
    let peer = link::reach(server_id, &name).map_err(|error| DomainStateError {
        code: error.code,
        message: format!(
            "{} works on {name}. {} Or give {} this computer back in GBot Settings > Computer.",
            bot.name, error.message, bot.name
        ),
    })?;
    if let Some(reused) = reuse_standing(db, &peer, bot, computer, opening) {
        return Ok(reused);
    }
    let local_rows = bot_agent_rows(repository)?;
    check_agent(&local_rows, &bot.agent_id)?;
    let family = local_rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(bot.agent_id.as_str()))
        .and_then(coordinator_launcher_family)
        .unwrap_or("claude");
    // The agent as that computer launches it.
    let hud = link::call(
        &peer,
        "/api/readSidebarHud",
        json!({ "includeAgentRoster": true }),
    )?;
    let rows: Vec<Value> = hud
        .get("agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.get("agentId").and_then(Value::as_str) == Some(bot.agent_id.as_str()))
        .collect();
    if rows.is_empty() {
        return Err(DomainStateError::bad_request(format!(
            "{name} has no {} set up, which {} runs on. Set it up there, or give {} this computer back in GBot Settings > Computer.",
            bot.agent_id, bot.name, bot.name
        )));
    }
    let mut launch = crate::coordinators::launcher_launch_settings(&rows, &bot.agent_id);
    let approval = opening.approval.unwrap_or(&bot.approval);
    if let Some(launch) = launch.as_object_mut() {
        // As here (session.rs `apply_approval`): only Full runs without asking, which that
        // computer asks you about first when its own setting for the agent doesn't.
        launch.insert(
            "acceptAllMode".into(),
            json!(if approval == "full" {
                "enabled"
            } else {
                "disabled"
            }),
        );
    }
    let task = opening.task.map(str::trim).filter(|task| !task.is_empty());
    let title = opening
        .title
        .map(str::to_string)
        .unwrap_or_else(|| super::super::session::chat_title(bot, task));
    let folder = computer
        .folder
        .clone()
        .unwrap_or_else(|| super::ops::default_folder(&bot.name));
    let mut create = json!({
        "projectPath": folder,
        "agentId": bot.agent_id,
        "launchSettings": launch,
        "title": title,
        "markedHandovers": true,
        "runtimeSettings": {
            HOME_KEY: {
                "serverId": state.metadata.server_id,
                "computerName": crate::connected_computers::this_computer_name(),
                "botId": bot.bot_id,
                "name": bot.name,
                "openedBy": opening.opened_by,
                // That computer starts it without the screen tools unless this is on
                // (agents/gbot_computer_use.rs there).
                "computerUse": computer.computer_use,
            }
        },
    });
    if let Some(model) = &bot.model {
        create["agentModel"] = json!(model);
    }
    if let Some(effort) = &bot.effort {
        create["agentEffort"] = json!(effort);
    }
    let created = link::call(&peer, "/api/createAgentSession", create)?;
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    let (project_id, session_id) = (text(&session, "projectId"), text(&session, "sessionId"));
    if session_id.is_empty() {
        return Err(DomainStateError {
            code: "remoteError",
            message: format!("{name} made no chat for {}. Try again.", bot.name),
        });
    }
    let chat = RemoteChat {
        server_id: peer.server_id.clone(),
        project_id: project_id.clone(),
        session_id: session_id.clone(),
        bot_id: bot.bot_id.clone(),
        opened_by: opening.opened_by.to_string(),
        purpose: opening
            .purpose
            .unwrap_or(if task.is_some() { "task" } else { "chat" })
            .to_string(),
        from_key: opening.from.map(str::to_string),
        standing: opening.standing,
        title: title.clone(),
        created_at: now_iso(),
    };
    store::insert_remote_chat(db, &chat)?;
    link_sender(db, &chat, opening);
    let started = link::call(
        &peer,
        "/api/startSessionProvider",
        json!({ "projectId": project_id, "sessionId": session_id }),
    )
    .is_ok();
    let _ = link::call(
        &peer,
        "/api/queueSessionChatPrompt",
        json!({
            "projectId": project_id, "sessionId": session_id,
            "text": super::super::session::first_message(bot, family, task),
            "startupSend": true, "sendRequestId": uuid::Uuid::new_v4().to_string(),
        }),
    );
    super::super::presentation::refresh_presented_bots(db);
    let _ = crate::server::schedule_presentation_project_delta(
        state,
        db,
        repository,
        &bot.project_id,
        "projectUpdated",
    );
    Ok((session_value(&chat, &peer.name), started))
}
