use super::*;

/*
CDXC:ProjectActions 2026-08-01:
Both the HUD read and the HUD settings mutation answer the same opt-in
`includeAllProjectCommands` request, because clients that render per-project
quick actions replace their whole HUD snapshot from either response. Keep the
opt-in in one place so the two cannot drift apart and silently blank project
rows after a Settings save.
*/
pub(crate) fn apply_commands_by_project_if_requested(
    hud: &mut Value,
    projects: &[Value],
    params: &Map<String, Value>,
) {
    if params
        .get("includeAllProjectCommands")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return;
    }
    if let Some(hud) = hud.as_object_mut() {
        hud.insert(
            "commandsByProject".to_string(),
            read_sidebar_hud_commands_by_project(projects),
        );
    }
}

/*
CDXC:GxserverLogs 2026-06-19-14:45:
Rust must route `/api/queryLogs` instead of returning milestone `notImplemented`. Keep the TypeScript RPC envelope and local authenticated gates while returning only sanitized JSONL entries already present in the support log file.
*/
pub(crate) fn handle_query_logs_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match query_gxserver_logs(&state.paths, &params) {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(LogQueryError::Input(message)) => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error("badRequest", message, Some(request_id)),
        ),
        Err(LogQueryError::Io(_)) => routed_json(
            Some(endpoint_path),
            StatusCode::INTERNAL_SERVER_ERROR,
            rpc_error(
                "internalError",
                "Failed to query gxserver logs.",
                Some(request_id),
            ),
        ),
    }
}

pub(crate) fn handle_portless_state_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    /*
    CDXC:PortlessFailureUX 2026-06-23-04:28:
    Native-sidebar reports only enum-like Portless settings/admin outcomes to
    server. The daemon persists setup recovery state, clears route files
    for Disable/remove, and returns refreshed metadata without paths, command
    text, process output, environment values, URLs, tokens, or file contents.
    */
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let update = match serde_json::from_value::<PortlessStateUpdate>(Value::Object(params.clone()))
    {
        Ok(update) => update,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError::bad_request(format!(
                    "Invalid Portless state update payload: {error}"
                )),
            );
        }
    };
    let started_at = Instant::now();
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            log_portless_state_update_failure(
                &state.logger,
                &update,
                PortlessLogErrorCode::StateUpdateDatabaseUnavailable,
                started_at.elapsed().as_millis(),
            );
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
    match apply_portless_state_update(&state.paths, &db, update.clone()) {
        Ok(record) => {
            log_portless_state_update_success(
                &state.logger,
                &update,
                &record,
                started_at.elapsed().as_millis(),
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(
                    request_id,
                    json!({
                        "presentation": read_portless_presentation_payload(&db),
                        "status": read_portless_status_payload(&db),
                    }),
                ),
            )
        }
        Err(error) => {
            log_portless_state_update_failure(
                &state.logger,
                &update,
                PortlessLogErrorCode::StateUpdateFailed,
                started_at.elapsed().as_millis(),
            );
            domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Portless state update failed: {error}"),
                },
            )
        }
    }
}

/*
CDXC:GxserverRustPort 2026-06-14-22:38:
Phase 3 Rust domain endpoints must share the TypeScript RPC envelope, durable SQLite database, and error-status mapping. Keep routing synchronous and explicit here so unsupported lifecycle/provider endpoints still return milestone notImplemented instead of silently mutating partial state.
*/
/*
CDXC:MobileKeepAwake 2026-08-19:
`/api/holdSessionsAwake` takes ONE list so a phone tailing several attached tabs
renews every hold in a single SSH round trip instead of one exec per tab.

Every entry is validated against this daemon's own sessions table before a lease
is recorded: the request carries selectors, never authority, and an id that does
not resolve is reported back as `unknown` rather than failing the whole renewal —
a tab whose session was killed elsewhere must not stop the other tabs' holds.
*/
pub(crate) fn hold_sessions_awake(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let holder_id = session_keep_awake::normalize_holder_id(
        params.get("holderId").and_then(Value::as_str),
    );
    let ttl_ms = session_keep_awake::normalize_ttl_ms(
        params.get("ttlMs").and_then(Value::as_i64),
    );
    let release = params.get("release").and_then(Value::as_bool) == Some(true);
    let requested = params
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if requested.is_empty() {
        return Err(DomainStateError::bad_request(
            "holdSessionsAwake requires a non-empty sessions list.",
        ));
    }
    let mut held: Vec<Value> = Vec::new();
    let mut unknown: Vec<Value> = Vec::new();
    for entry in &requested {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let project_id = read_project_id(entry)?;
        let session_id = read_session_id(entry)?;
        if repository.get_session(&project_id, &session_id)?.is_none() {
            unknown.push(json!({ "projectId": project_id, "sessionId": session_id }));
            continue;
        }
        if release {
            session_keep_awake::release(&project_id, &session_id, &holder_id);
            held.push(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "keptAwake": session_keep_awake::is_held_awake(&project_id, &session_id),
            }));
            continue;
        }
        let expires_at =
            session_keep_awake::hold(&project_id, &session_id, &holder_id, ttl_ms);
        held.push(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "keptAwake": true,
            "keepAwakeUntil": iso_from_ms(expires_at),
        }));
    }
    Ok(json!({
        "holderId": holder_id,
        "released": release,
        "sessions": held,
        "ttlMs": ttl_ms,
        "unknownSessions": unknown,
    }))
}

/*
CDXC:BoardStartWork 2026-08-07:
The daemon-owned Project Board "Start work" dispatch. The whole
resolve → reuse-check → create → link sequence runs while holding the
process-wide start-work gate, so two concurrent calls for one bead serialize
and the second reuses the first call's link (`created: false`) instead of
creating a duplicate worker. The zmx provider start happens after the gate is
released: the link is already durable, so a concurrent caller reuses the
session whether or not its provider has finished materializing.
*/
pub(crate) async fn handle_board_start_work_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let bd = match require_system_bd() {
        Ok(bd) => bd,
        Err(message) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "dependencyUnavailable",
                    message,
                },
            );
        }
    };
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let gate = Arc::clone(&state.board_start_work_gate);
    let bd_executable_path = bd.executable_path;
    let outcome = tokio::task::spawn_blocking(move || {
        let _gate = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, &server_id);
        crate::board_start_work::start_board_work(
            &repository,
            &db,
            &server_id,
            &params,
            &bd_executable_path,
        )
    })
    .await;
    let mut outcome = match outcome {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => return domain_error_response(endpoint_path, request_id, error),
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Board start-work task failed: {error}"),
                },
            )
        }
    };
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
            )
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    if let Some((project_id, session_id)) = outcome.created_session.clone() {
        if let Err(error) =
            schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)
        {
            return domain_error_response(endpoint_path, request_id, error);
        }
        // Materialize the zmx provider like `ghostex create-agent`, so the
        // staged bead prompt reaches a live agent instead of an inert row.
        let context = ZmxServerContext {
            auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
            base_url: format!(
                "http://{}:{}",
                state.config.listeners.local.host, state.config.listeners.local.port
            ),
        };
        let agent_settings = match read_agent_settings(&db) {
            Ok(settings) => settings,
            Err(error) => return domain_error_response(endpoint_path, request_id, error),
        };
        let mut provider_params = Map::new();
        provider_params.insert("projectId".to_string(), json!(&project_id));
        provider_params.insert("sessionId".to_string(), json!(&session_id));
        let provider_start = dispatch_zmx_lifecycle_endpoint(
            &repository,
            "/api/startSessionProvider",
            &provider_params,
            &context,
            &agent_settings,
        );
        if let Some(result) = outcome.result.as_object_mut() {
            match provider_start {
                Ok(_) => {
                    result.insert("providerStarted".to_string(), Value::Bool(true));
                }
                Err(error) => {
                    let message = match error {
                        ZmxEndpointError::DependencyUnavailable(message) => message,
                        ZmxEndpointError::Domain(error) => error.message,
                    };
                    let _ = state.logger.log(GxserverLogInput {
                        level: LogLevel::Warn,
                        event: "boardStartWork.providerStartFailed".to_string(),
                        server_id: Some(state.metadata.server_id.clone()),
                        request_id: Some(request_id.clone()),
                        client: None,
                        duration_ms: None,
                        error: Some(message.clone()),
                        details: Some(json!({
                            "projectId": project_id,
                            "sessionId": session_id,
                        })),
                    });
                    result.insert("providerStarted".to_string(), Value::Bool(false));
                    result.insert("providerStartError".to_string(), Value::String(message));
                }
            }
        }
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id);
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, outcome.result),
    )
}

pub(crate) fn create_quick_project_params(
    home_dir: &Path,
    params: &Map<String, Value>,
) -> std::result::Result<Map<String, Value>, DomainStateError> {
    /*
    CDXC:GPUIQuickActions 2026-07-11:
    Quick actions must mirror macOS by creating a real projectless workspace
    under ~/ghostex/chats before creating its first terminal or agent. Keep the
    filesystem authority in gxserver, which already owns the authenticated
    project registry and the user's configured HOME, rather than deriving a
    private home path in the sidebar renderer.
    */
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|kind| matches!(*kind, "terminal" | "agent"))
        .ok_or_else(|| DomainStateError {
            code: "badRequest",
            message: "kind must be terminal or agent.".to_string(),
        })?;
    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d-%H%M%S%3f");
    let suffix = Uuid::new_v4().simple().to_string();
    let suffix = &suffix[..8];
    let chat_path = home_dir
        .join("ghostex")
        .join("chats")
        .join(format!("{timestamp}-{kind}-{suffix}"));
    fs::create_dir_all(&chat_path).map_err(|_| DomainStateError {
        code: "internalError",
        message: "Unable to create the Quick workspace directory.".to_string(),
    })?;

    let mut launch_settings = Map::new();
    launch_settings.insert("isChat".to_string(), Value::Bool(true));
    launch_settings.insert("isQuick".to_string(), Value::Bool(true));
    launch_settings.insert(
        "quickKind".to_string(),
        Value::String("terminal".to_string()),
    );

    let mut project_params = Map::new();
    project_params.insert(
        "name".to_string(),
        Value::String(now.format("Chat %Y-%m-%d %H:%M").to_string()),
    );
    project_params.insert(
        "path".to_string(),
        Value::String(chat_path.to_string_lossy().to_string()),
    );
    project_params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    Ok(project_params)
}

pub(crate) fn domain_error_response(
    endpoint_path: String,
    request_id: String,
    error: DomainStateError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "notFound" => StatusCode::NOT_FOUND,
        "corruptState" => StatusCode::CONFLICT,
        "projectPathUnavailable" => StatusCode::CONFLICT,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

pub(crate) async fn handle_automation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    /*
    CDXC:GxserverAutomations 2026-06-29-15:55:
    Automation RPCs are first-class gxserver endpoints now. Route them through the modular automation runtime instead of `/api/dispatchRendererCommand` so CLI, macOS, and remote clients do not depend on a native sidebar renderer being connected.
    */
    match handle_automation_endpoint(&state.automation_runtime, &endpoint_path, body).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn handle_delayed_send_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match state
        .delayed_send_runtime
        .handle_endpoint(&endpoint_path, &params)
    {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) async fn handle_repository_clone_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let runtime = RepositoryCloneRuntime {
        event_hub: state.event_hub.clone(),
        logger: state.logger.clone(),
        paths: state.paths.clone(),
        presentation_event_sequence: state.presentation_event_sequence.clone(),
        server_id: state.metadata.server_id.clone(),
    };
    match dispatch_repository_clone_endpoint(
        state.repository_clone_jobs.clone(),
        runtime,
        &endpoint_path,
        &params,
    )
    .await
    {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => repository_clone_error_response(endpoint_path, request_id, error),
    }
}

/*
CDXC:AddProjectDialog 2026-07-30:
Provider discovery and repository lookup shell out to `gh`/`glab`, so they run
on the async route like the clone endpoints do. The probe cwd is the daemon's
own home directory unless the caller names an existing one, which keeps the
endpoint from becoming a way to ask about arbitrary directories.
*/
pub(crate) async fn handle_source_control_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match dispatch_source_control_endpoint(&endpoint_path, &params, &state.paths.home_dir).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => source_control_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn source_control_error_response(
    endpoint_path: String,
    request_id: String,
    error: SourceControlError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}

pub(crate) fn repository_clone_error_response(
    endpoint_path: String,
    request_id: String,
    error: RepositoryCloneError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "dependencyUnavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "forbidden" => StatusCode::FORBIDDEN,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}
