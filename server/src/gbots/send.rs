//! Chat sends and Stop for a GBots conversation: they go to the sidecar, never into the session's
//! terminal (which runs Ghostex's GBots client).

use serde_json::{json, Map, Value};

use super::projects::{gbots_bot_id, session_thread, stamp_thread};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

fn refused(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

/// The bot and conversation a session shows. A session started from a bot's "+" has no
/// conversation yet: one is made for it now and recorded on the session. `None` for every session
/// that is not a GBots conversation.
pub(crate) fn session_conversation(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<Result<(String, String), DomainStateError>> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository.get_session(project_id, session_id).ok()??;
    if let Some(found) = session_thread(&session) {
        return Some(Ok(found));
    }
    let project = repository.get_project(project_id).ok()??;
    let bot_id = gbots_bot_id(&project)?.to_string();
    if session.get("agentId").and_then(Value::as_str) != Some(super::GBOTS_AGENT_ID)
        && session
            .pointer("/launchSettings/icon")
            .and_then(Value::as_str)
            != Some(super::GBOTS_AGENT_ID)
    {
        return None;
    }
    Some(new_conversation(&repository, &session, &bot_id))
}

fn new_conversation(
    repository: &DomainRepository<'_>,
    session: &Value,
    bot_id: &str,
) -> Result<(String, String), DomainStateError> {
    let created = super::client::post(
        &format!(
            "/api/bots/{}/tasks",
            super::client::id_segment(bot_id).map_err(refused)?
        ),
        &json!({ "select": false }),
    )
    .map_err(refused)?;
    let thread_id = created
        .pointer("/task/threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| refused("GBots did not start a conversation."))?
        .to_string();
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    stamp_thread(&mut runtime, bot_id, &thread_id);
    let mut update = Map::new();
    for key in ["projectId", "sessionId"] {
        update.insert(
            key.to_string(),
            session.get(key).cloned().unwrap_or_default(),
        );
    }
    update.insert("runtimeSettings".to_string(), Value::Object(runtime));
    repository.update_session(&update)?;
    Ok((bot_id.to_string(), thread_id))
}

/// A chat message to a GBots conversation; `None` for every other session.
pub(crate) fn send_chat_text(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
    has_images: bool,
) -> Option<Result<(), DomainStateError>> {
    let conversation = session_conversation(state, project_id, session_id)?;
    Some((|| {
        let (bot_id, thread_id) = conversation?;
        if has_images {
            return Err(refused(
                "GBots takes text only for now. Send the text without the images.",
            ));
        }
        super::client::post(
            &format!(
                "/api/bots/{}/messages",
                super::client::id_segment(&bot_id).map_err(refused)?
            ),
            &json!({ "text": text, "threadId": thread_id, "followUp": "queue" }),
        )
        .map_err(refused)?;
        super::mirror::sync_thread(&thread_id);
        Ok(())
    })())
}

/// Stop for a GBots conversation; `None` for every other session.
pub(crate) fn interrupt_gbots_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Option<Result<(), DomainStateError>> {
    let conversation = session_conversation(state, project_id, session_id)?;
    Some((|| {
        let (bot_id, thread_id) = conversation?;
        super::client::post(
            &format!(
                "/api/bots/{}/interrupt",
                super::client::id_segment(&bot_id).map_err(refused)?
            ),
            &json!({ "threadId": thread_id }),
        )
        .map_err(refused)?;
        Ok(())
    })())
}
