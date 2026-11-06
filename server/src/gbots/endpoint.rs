//! `/api/gbots {action, …}`: what clients, the `ghostex gbots` CLI and a GBots session's terminal
//! client ask of GBots.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::server::AppState;

fn text(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn refused(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message.into())
}

pub(crate) async fn gbots_action(
    state: Arc<AppState>,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    tokio::task::spawn_blocking(move || run(&state, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError::corrupt_state(format!(
                "GBots action failed: {error}"
            )))
        })
}

fn run(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let action = text(params, "action");
    let needs_running = || -> Result<(), DomainStateError> {
        if super::sidecar::is_running() {
            Ok(())
        } else if !super::settings::gbots_enabled(&state.paths) {
            Err(refused(
                "GBots is off. Turn it on in Settings > Extensions.",
            ))
        } else if !super::settings::runs_here(&state.paths, &state.metadata.server_id) {
            Err(refused(
                "GBots runs on another computer. Run this there, or add --computer.",
            ))
        } else {
            Err(refused(format!(
                "GBots is not running yet ({}).",
                super::sidecar::status_value()["status"]
                    .as_str()
                    .unwrap_or("off")
            )))
        }
    };
    match action.as_str() {
        "status" => {
            let mut status = super::sidecar::status_value();
            status["enabled"] = json!(super::settings::gbots_enabled(&state.paths));
            status["computer"] = json!(super::settings::gbots_computer(&state.paths));
            status["runsHere"] = json!(super::settings::runs_here(
                &state.paths,
                &state.metadata.server_id
            ));
            Ok(status)
        }
        "list" => {
            needs_running()?;
            Ok(json!({ "bots": super::projects::list_value(state) }))
        }
        "agents" => {
            needs_running()?;
            Ok(json!({ "agents": super::create::bot_agents() }))
        }
        "create" => {
            needs_running()?;
            super::create::create_bot(state, params).map_err(refused)
        }
        "caller" => {
            needs_running()?;
            let caller = super::projects::calling_conversation(state, &text(params, "cwd"))
                .ok_or_else(|| refused("No GBot is working right now."))?;
            Ok(json!({ "caller": caller }))
        }
        "newConversation" => {
            needs_running()?;
            let (project_id, session_id) =
                super::projects::new_conversation(state, &text(params, "projectId"))
                    .map_err(refused)?;
            Ok(json!({ "projectId": project_id, "sessionId": session_id }))
        }
        "attach" => {
            needs_running()?;
            let (project_id, session_id) = (text(params, "projectId"), text(params, "sessionId"));
            let (bot_id, thread_id) =
                super::send::session_conversation(state, &project_id, &session_id)
                    .ok_or_else(|| refused("This session is not a GBots conversation."))??;
            Ok(json!({ "botId": bot_id, "threadId": thread_id }))
        }
        "conversation" => {
            needs_running()?;
            let thread_id = text(params, "threadId");
            let thread_id = super::client::id_segment(&thread_id).map_err(refused)?;
            let answer = super::client::get(&format!("/api/threads/{thread_id}/messages?limit=40"))
                .map_err(refused)?;
            let lines: Vec<Value> = super::mirror::mirror_lines(
                answer
                    .get("messages")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                super::projects::thread_busy(thread_id),
            )
            .iter()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|record| record["kind"] == "message")
            .map(|record| json!({ "id": record["id"], "role": record["role"], "text": record["text"] }))
            .collect();
            Ok(json!({ "messages": lines, "working": super::projects::thread_busy(thread_id) }))
        }
        "send" => {
            needs_running()?;
            let (project_id, session_id) = (text(params, "projectId"), text(params, "sessionId"));
            let message = text(params, "text");
            if message.is_empty() {
                return Err(refused("Type a message."));
            }
            super::send::send_chat_text(state, &project_id, &session_id, &message, false)
                .ok_or_else(|| refused("This session is not a GBots conversation."))??;
            Ok(json!({ "sent": true }))
        }
        _ => Err(refused(
            "action must be status, list, agents, create, attach, conversation or send.",
        )),
    }
}
