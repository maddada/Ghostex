use super::*;

/*
CDXC:SessionChat 2026-08-21:
Dispatch-only glue for the Ghostex chat prompt queue. Storage, validation and
the endpoint bodies live in session_chat_queue.rs; server.rs supplies only the
three things that module deliberately does not know about — the state-database
path, the delivery path, and the live follower stream a state frame rides.
*/
pub(crate) async fn handle_session_chat_queue_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if endpoint_path == "/api/sendSessionChatQueuedPrompt" {
        return handle_send_session_chat_queued_prompt_http(
            state,
            endpoint_path,
            request_id,
            &params,
        )
        .await;
    }
    match crate::session_chat_queue::handle_session_chat_queue_endpoint(
        &state.paths,
        state.metadata.server_id.as_str(),
        &endpoint_path,
        &params,
    ) {
        Ok(result) => {
            /*
            Every mutation restates the queue to the session's other followers,
            so a row queued on the phone appears in the desktop composer without
            anyone re-reading.
            */
            if result.broadcast {
                broadcast_session_chat_queue_state(state, &result.project_id, &result.session_id);
            }
            // Retry on a row of a sleeping session wakes it (CDXC:SessionChat 2026-10-29 in
            // session_chat_send_wake.rs).
            if endpoint_path == "/api/updateSessionChatQueuedPrompt"
                && params.get("retry").and_then(Value::as_bool) == Some(true)
            {
                if let Some(prompt_id) = params.get("promptId").and_then(Value::as_str) {
                    crate::session_chat_send_wake::wake_for_retried_message(
                        state,
                        &result.project_id,
                        &result.session_id,
                        prompt_id.trim(),
                    );
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result.value),
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

/*
"Send now" delivers immediately regardless of agent state, exactly like pressing
Enter. `sent: false` therefore means the send itself FAILED and the row is now
`failed` with an errorMessage — it never means "deferred".
*/
pub(crate) async fn handle_send_session_chat_queued_prompt_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    params: &Map<String, Value>,
) -> RoutedResponse {
    let (project_id, session_id, prompt_id) = match session_chat_queue_prompt_target(params) {
        Ok(target) => target,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let sender = session_chat_queue_sender(
        state,
        &project_id,
        &session_id,
        SessionChatMessageSource::ManualQueue,
    );
    match crate::session_chat_queue::deliver_session_chat_queued_prompt(
        &state.paths,
        state.metadata.server_id.as_str(),
        &project_id,
        &session_id,
        &prompt_id,
        &sender,
        false,
    )
    .await
    {
        Ok(delivery) => {
            broadcast_session_chat_queue_state(state, &project_id, &session_id);
            let mut result = Map::new();
            delivery.snapshot.insert_into(&mut result);
            result.insert("sent".to_string(), json!(delivery.sent));
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, Value::Object(result)),
            )
        }
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn session_chat_queue_prompt_target(
    params: &Map<String, Value>,
) -> std::result::Result<(String, String, String), DomainStateError> {
    let read = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    match (read("projectId"), read("sessionId"), read("promptId")) {
        (Some(project_id), Some(session_id), Some(prompt_id)) => {
            Ok((project_id, session_id, prompt_id))
        }
        _ => Err(DomainStateError {
            code: "invalidParams",
            message: "sendSessionChatQueuedPrompt requires projectId, sessionId and promptId."
                .to_string(),
        }),
    }
}

/*
Publishes the session's current queue + draft on a `sessionChatState` frame.
Same restating discipline as the terminal-notice publisher: a state frame
REPLACES the client's prompt card, notice, pills and queue, so it must carry all
of them, which is exactly what emit_session_chat_options_state_frame builds.
No live follower ⇒ nothing is emitted and clients pick the change up from their
next readSessionChat (or, on mobile, from the long-poll fingerprint).
*/
pub(crate) fn broadcast_session_chat_queue_state(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) {
    let key = session_observer_key(project_id, session_id);
    let options = state
        .session_chat_option_cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).map(|entry| entry.value.options.clone()))
        .unwrap_or_default();
    let screen = cached_session_chat_screen_state(state, project_id, session_id);
    emit_session_chat_options_state_frame(
        &state.session_chat_followers,
        &state.event_hub,
        &state.paths,
        &state.metadata.server_id,
        project_id,
        session_id,
        options.as_ref(),
        screen.borrow(),
    );
    publish_session_chat_queue_presentation_delta(state, project_id, session_id);
    super::scheduler::kick_session_chat_queue();
}
