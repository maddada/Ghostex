//! Side chat routes (server/src/side_chats/): the question a new side chat starts with, the list a
//! main session's Side chats pill shows, Bring back and Delete.

use super::*;

fn database_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

fn required(params: &Map<String, Value>, key: &str) -> Result<String, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

/// Rebuilds the side chat lists and re-sends every main chat whose list changed. The supervisor's
/// tick does this every two seconds; a start or a delete does it at once.
pub(crate) fn refresh_side_chats(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
) {
    for (project_id, session_id) in crate::side_chats::refresh_side_chat_panels(db, repository) {
        coordinator_runtime::republish_session_chat(state, &project_id, &session_id);
    }
}

/// After `/api/startSideChat` forked the session: the question goes to the side chat as its first
/// message, with the side chat's note in front (held until the agent's input box is up), and the
/// main chat's pill lists the new side chat.
pub(crate) fn after_side_chat_start(state: &AppState, params: &Map<String, Value>, result: &Value) {
    let Some(session) = result.pointer("/fork/session") else {
        return;
    };
    let (Some(project_id), Some(session_id)) = (
        session.get("projectId").and_then(Value::as_str),
        session.get("sessionId").and_then(Value::as_str),
    ) else {
        return;
    };
    let question = params
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let first_message =
        ghostex_gx_chat_core::composer::side_chat::side_chat_first_message(question);
    if let Err(error) =
        queue_session_chat_prompt(state, project_id, session_id, &first_message, true)
    {
        eprintln!(
            "Side chat {project_id}/{session_id} could not queue its question: {}",
            error.message
        );
    }
    if let Ok(db) = open_gxserver_database(&state.paths) {
        // A Side chat sent from the main chat box used its draft, as a send does: the box must not
        // offer it back as a saved draft.
        if let (Ok(Some(version)), Some(parent_project), Some(parent_session)) = (
            crate::session_chat_draft_versions::parse(params),
            params.get("projectId").and_then(Value::as_str),
            params.get("sessionId").and_then(Value::as_str),
        ) {
            if crate::session_chat_draft_versions::consume(
                &db,
                parent_project,
                parent_session,
                &version,
            )
            .is_ok()
            {
                crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                    state,
                    parent_project,
                    parent_session,
                );
            }
        }
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        refresh_side_chats(state, &db, &repository);
    }
}

/// The side chat a call names: `sideChatSessionId` when a main chat asks about one of its side
/// chats (a chat's host always fills `sessionId` with its own session), else `sessionId`.
fn side_chat_ids(params: &Map<String, Value>) -> Result<(String, String), DomainStateError> {
    let project_id = required(params, "projectId")?;
    let session_id =
        required(params, "sideChatSessionId").or_else(|_| required(params, "sessionId"))?;
    Ok((project_id, session_id))
}

/// The side chat a call names, and its main session's id.
fn side_chat_of(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<(Value, String), DomainStateError> {
    let (project_id, session_id) = side_chat_ids(params)?;
    let session = repository
        .get_session(&project_id, &session_id)?
        .ok_or_else(|| DomainStateError::not_found("That side chat no longer exists."))?;
    let parent = crate::side_chats::parent_of(&session)
        .ok_or_else(|| DomainStateError::bad_request("That session is not a side chat."))?;
    Ok((session, parent))
}

/// The side chat's transcript, as `readSessionChat` reads it for its chat.
async fn read_messages(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Vec<Value>, DomainStateError> {
    let body = json!({
        "params": {
            "projectId": project_id,
            "sessionId": session_id,
            "limit": crate::session_chat::SESSION_CHAT_MAX_LIMIT,
        }
    });
    let routed = crate::session_chat_read::handle_read_session_chat_http(
        state,
        "/api/readSessionChat".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    let ok = routed.response.status().is_success();
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice::<Value>(&bytes).unwrap_or_default();
    if !ok {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Could not read the side chat.")
                .to_string(),
        });
    }
    Ok(value
        .pointer("/result/messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// CDXC:SideChats 2026-11-02 DECISION:
/// User: Bring back is in the first version. It sends what the side chat said (its questions and answers since the fork, newest kept when long) to the main session as one message, delivered the way `ghostex agents send` delivers: typed now, picked up by a busy agent at its next input boundary, and a sleeping main session is woken for it (a queued message would sit in a sleeping session's queue).
async fn bring_back(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (session, parent) = {
        let db = open_gxserver_database(&state.paths).map_err(database_error)?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        side_chat_of(&repository, params)?
    };
    let (project_id, session_id) = side_chat_ids(params)?;
    let messages = read_messages(state, &project_id, &session_id).await?;
    let title = crate::side_chats::side_chat_title(&session)
        .unwrap_or_else(|| crate::coordinators::session_title_of(&session));
    let message = crate::side_chats::bring_back_message(&messages, &title).ok_or_else(|| {
        DomainStateError::bad_request("This side chat has nothing to bring back yet.")
    })?;
    let body = json!({
        "params": {
            "projectId": project_id,
            "sessionId": parent,
            "text": message,
            "sendRequestId": Uuid::new_v4().to_string(),
        }
    });
    let routed = crate::session_chat_send::handle_send_session_chat_message_http(
        state,
        "/api/sendSessionChatMessage".to_string(),
        Uuid::new_v4().to_string(),
        &body,
    )
    .await;
    if !routed.response.status().is_success() {
        let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
            .await
            .unwrap_or_default();
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|body| {
                body.get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "The main session did not take the message.".to_string());
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message,
        });
    }
    Ok(json!({ "ok": true, "projectId": project_id, "parentSessionId": parent }))
}

/// Delete: the side chat's agent is stopped and its session removed, then the main chat's pill
/// drops it.
fn delete(state: &AppState, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(database_error)?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let (_, parent) = side_chat_of(&repository, params)?;
    let (project_id, session_id) = side_chat_ids(params)?;
    let settings = crate::agents::read_agent_settings(&db).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("{error:?}"),
    })?;
    let mut lifecycle = Map::new();
    lifecycle.insert("projectId".to_string(), json!(project_id));
    lifecycle.insert("sessionId".to_string(), json!(session_id));
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    // A side chat that already stopped has no daemon to kill; removing it is what matters.
    lifecycle.insert("closeSource".to_string(), json!("server.sideChatDeleted"));
    let killed = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/killSession",
        &lifecycle,
        &context,
        &settings,
    );
    crate::session_close_log::log_session_close(
        &state.logger,
        &state.metadata.server_id,
        &project_id,
        &session_id,
        &lifecycle,
        "/api/killSession",
        if killed.is_ok() { "stopped" } else { "failed" },
    );
    crate::session_chat_send_wake::retire_held_messages_of_closed_session(
        state,
        &project_id,
        &session_id,
    );
    repository.remove_session(&lifecycle)?;
    let _ = schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
    refresh_side_chats(state, &db, &repository);
    Ok(json!({
        "ok": true,
        "projectId": project_id,
        "sessionId": session_id,
        "parentSessionId": parent,
    }))
}

/// `/api/listSideChats`, `/api/bringBackSideChat`, `/api/deleteSideChat`.
pub(crate) async fn handle_side_chat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/listSideChats" => (|| {
            let project_id = required(&params, "projectId")?;
            let session_id = required(&params, "sessionId")?;
            if let Ok(db) = open_gxserver_database(&state.paths) {
                let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
                refresh_side_chats(state, &db, &repository);
            }
            Ok(json!({
                "sideChats": crate::side_chats::cached_side_chats(&project_id, &session_id),
            }))
        })(),
        "/api/bringBackSideChat" => bring_back(state, &params).await,
        "/api/deleteSideChat" => delete(state, &params),
        _ => Err(DomainStateError::not_found("Unknown side chat endpoint.")),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
