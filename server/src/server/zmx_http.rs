use super::*;

/*
CDXC:GxserverRustPort 2026-06-15-18:06:
Phase 5 lifecycle and session-I/O endpoints run through the same authenticated RPC envelope as the TypeScript daemon while zmx process work stays behind explicit endpoint handlers. Presentation deltas are still scheduled from durable state after lifecycle mutations so clients never infer sidebar state from subprocess output.
*/
pub(crate) async fn handle_zmx_lifecycle_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let db_result = if endpoint_path == "/api/createWorkspaceTerminal" {
        /*
        Concurrent Windows terminal clicks use independent gxserver SQLite
        connections. Apply bounded lock waiting before even the connection
        PRAGMAs so the atomic create path cannot fail immediately with
        SQLITE_BUSY midway through provider materialization or compensation.
        Other lifecycle endpoints retain their existing connection behavior.
        */
        open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
    } else {
        open_gxserver_database(&state.paths)
    };
    let db = match db_result {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    if endpoint_path == "/api/createWorkspaceTerminal" {
        return match create_started_workspace_terminal(&repository, &params, &context) {
            Ok(output) => {
                if let Some((project_id, session_id)) = output.presentation_session.as_ref() {
                    if let Err(mut error) = schedule_presentation_session_delta(
                        state,
                        &db,
                        &repository,
                        project_id,
                        session_id,
                    ) {
                        if let Some(identity) = output.created_workspace_terminal.as_ref() {
                            if let Err(cleanup_error) =
                                compensate_created_workspace_terminal(&repository, identity)
                            {
                                error.message.push_str(&format!(
                                    " Compensating cleanup also failed for the new terminal: {cleanup_error}"
                                ));
                            }
                        }
                        /*
                        The row may have appeared in a concurrent snapshot
                        before the failed revision write. Re-project its exact
                        post-cleanup state: normally sessionRemoved, or the
                        surviving exact row if durable removal itself failed.
                        */
                        if let Err(repair_error) = schedule_presentation_session_delta(
                            state,
                            &db,
                            &repository,
                            project_id,
                            session_id,
                        ) {
                            error.message.push_str(&format!(
                                " Compensating presentation reconciliation also failed: {}",
                                repair_error.message
                            ));
                        }
                        return domain_error_response(endpoint_path, request_id, error);
                    }
                }
                routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, output.result),
                )
            }
            Err(mut failure) => {
                if let Some((project_id, session_id)) = failure.presentation_session.as_ref() {
                    if let Err(repair_error) = schedule_presentation_session_delta(
                        state,
                        &db,
                        &repository,
                        project_id,
                        session_id,
                    ) {
                        append_zmx_endpoint_error_context(
                            &mut failure.error,
                            &format!(
                                " Compensating presentation reconciliation also failed: {}",
                                repair_error.message
                            ),
                        );
                    }
                }
                zmx_error_response(endpoint_path, request_id, failure.error)
            }
        };
    }
    let agent_settings = match read_agent_settings(&db) {
        Ok(settings) => settings,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = dispatch_zmx_lifecycle_endpoint(
        &repository,
        &endpoint_path,
        &params,
        &context,
        &agent_settings,
    );
    match result {
        Ok(output) => {
            if let Some((project_id, session_id)) = output.presentation_session.as_ref() {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                ) {
                    return domain_error_response(endpoint_path, request_id, error);
                }
                if let Some(target) = claim_first_user_input_draft(
                    &repository,
                    &endpoint_path,
                    &output.result,
                    project_id,
                    session_id,
                ) {
                    schedule_first_user_input_draft(state.clone(), target);
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, output.result),
            )
        }
        Err(error) => zmx_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) async fn handle_zmx_session_interaction_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    if endpoint_path == "/api/sendSessionMessage" && !params.contains_key("sessionId") {
        return match state
            .event_hub
            .dispatch_renderer_command("sendMessage".to_string(), params, 15_000)
            .await
        {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => zmx_error_response(
                endpoint_path,
                request_id,
                ZmxEndpointError::DependencyUnavailable(error.message),
            ),
        };
    }
    if endpoint_path == "/api/focusSession" {
        let prepared = {
            let db = match open_gxserver_database(&state.paths) {
                Ok(db) => db,
                Err(error) => {
                    return domain_error_response(
                        endpoint_path,
                        request_id,
                        DomainStateError {
                            code: "internalError",
                            message: format!("SQLite gxserver state error: {error}"),
                        },
                    );
                }
            };
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            prepare_focus_session_renderer_command(&repository, &params)
        };
        let (session, payload) = match prepared {
            Ok(command) => command,
            Err(error) => return zmx_error_response(endpoint_path, request_id, error),
        };
        return match state
            .event_hub
            .dispatch_renderer_command("focusSession".to_string(), payload, 15_000)
            .await
        {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    merge_session_with_renderer_result(session, result),
                ),
            ),
            Err(error) => zmx_error_response(
                endpoint_path,
                request_id,
                ZmxEndpointError::DependencyUnavailable(error.message),
            ),
        };
    }
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    /*
    CDXC:SessionChatSerializedWriters 2026-08-24:
    The two raw input-line writers ride the per-session send queue, so their
    delivery is awaited here rather than performed inside the synchronous
    dispatch. The repository is scoped and the connection dropped before the
    await, because a rusqlite handle cannot be held across one.
    */
    if matches!(
        endpoint_path.as_str(),
        "/api/sendSessionText" | "/api/sendSessionEnter"
    ) {
        let queued = {
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            crate::zmx::read_zmx_queued_session_write(&repository, &endpoint_path, &params)
        };
        drop(db);
        let queued = match queued {
            Ok(queued) => queued,
            Err(error) => return zmx_error_response(endpoint_path, request_id, error),
        };
        return match queued.execute().await {
            Ok(result) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            ),
            Err(error) => zmx_error_response(endpoint_path, request_id, error),
        };
    }
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    match dispatch_zmx_session_interaction_endpoint(&repository, &endpoint_path, &params) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => zmx_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn zmx_error_response(
    endpoint_path: String,
    request_id: String,
    error: ZmxEndpointError,
) -> RoutedResponse {
    match error {
        ZmxEndpointError::Domain(error) => domain_error_response(endpoint_path, request_id, error),
        ZmxEndpointError::DependencyUnavailable(message) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error("dependencyUnavailable", message, Some(request_id)),
        ),
    }
}

pub(crate) async fn handle_renderer_command_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let action = match params.get("action").and_then(Value::as_str) {
        Some(action) if RENDERER_COMMAND_ACTIONS.contains(&action) => action.to_string(),
        _ => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error(
                    "badRequest",
                    format!(
                        "Unsupported renderer command action: {}",
                        params
                            .get("action")
                            .map(Value::to_string)
                            .unwrap_or_else(|| "undefined".to_string())
                    ),
                    Some(request_id),
                ),
            );
        }
    };
    let payload = match params.get("payload") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(payload)) => payload.clone(),
        Some(_) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error(
                    "badRequest",
                    "Renderer command payload must be an object.",
                    Some(request_id),
                ),
            );
        }
    };
    let payload = with_renderer_session_target(payload);
    let timeout_ms = match normalize_renderer_command_timeout_ms(params.get("timeoutMs")) {
        Ok(timeout_ms) => timeout_ms,
        Err(message) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", message, Some(request_id)),
            );
        }
    };
    match state
        .event_hub
        .dispatch_renderer_command(action, payload, timeout_ms)
        .await
    {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

pub(crate) fn with_renderer_session_target(mut payload: Map<String, Value>) -> Map<String, Value> {
    if payload
        .get("sessionTarget")
        .and_then(Value::as_object)
        .is_some()
    {
        return payload;
    }
    let Some(project_id) = payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return payload;
    };
    let Some(session_id) = payload
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return payload;
    };
    /*
    CDXC:GxserverRendererCommands 2026-06-21-19:22:
    gxserver renderer commands target durable project/session ids, but macOS may
    render sessions with combined presentation ids. Add a structured target at
    the daemon boundary so every CLI or API caller gets the same renderer lookup
    contract without depending on sidebar id encoding.
    */
    let mut session_target = Map::new();
    if let Some(global_ref) = payload
        .get("globalRef")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        session_target.insert(
            "globalRef".to_string(),
            Value::String(global_ref.to_string()),
        );
    }
    session_target.insert("projectId".to_string(), Value::String(project_id));
    session_target.insert("sessionId".to_string(), Value::String(session_id));
    payload.insert("sessionTarget".to_string(), Value::Object(session_target));
    payload
}

pub(crate) fn normalize_renderer_command_timeout_ms(value: Option<&Value>) -> Result<u64, String> {
    let Some(value) = value else {
        return Ok(15_000);
    };
    let Some(raw) = value.as_f64() else {
        return Err("Renderer command timeoutMs must be a positive number.".to_string());
    };
    if !raw.is_finite() || raw <= 0.0 {
        return Err("Renderer command timeoutMs must be a positive number.".to_string());
    }
    Ok(raw.round().clamp(1_000.0, 60_000.0) as u64)
}
