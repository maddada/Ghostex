use super::*;

/*
CDXC:RepoStructure 2026-06-15-18:06:
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
    /*
    CDXC:Zmx 2026-09-01:
    Every lifecycle endpoint here is fully blocking: SQLite on the calling
    thread, `zmx` and `launchctl` subprocess spawns, and the launchd readiness
    poll's `thread::sleep`. Running that directly in the async handler parked an
    executor worker for the whole wake — which is why clients measured 60-120ms
    MORE than the handler's own span on unrelated RPCs at the same time. Hand
    the whole synchronous region to the blocking pool; the JoinError arm below
    keeps a panicking dispatch answering with an error instead of hanging.
    */
    let blocking_state = state.clone();
    let blocking_endpoint_path = endpoint_path.clone();
    let blocking_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        dispatch_zmx_lifecycle_http_blocking(
            &blocking_state,
            blocking_endpoint_path,
            blocking_request_id,
            params,
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("zmx lifecycle task failed: {error}"),
            },
        ),
    }
}

pub(crate) fn dispatch_zmx_lifecycle_http_blocking(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    params: Map<String, Value>,
) -> RoutedResponse {
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
        Ok(mut output) => {
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
            // One line per close, before the threads a coordinator's close takes with it
            // (session_close_log.rs).
            if is_close_request(&endpoint_path, &params) {
                log_close_request(
                    state,
                    &endpoint_path,
                    &params,
                    output
                        .result
                        .pointer("/session/lifecycleState")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown"),
                );
            }
            if matches!(
                endpoint_path.as_str(),
                "/api/killSession" | "/api/transitionSession"
            ) && output
                .result
                .pointer("/session/lifecycleState")
                .and_then(Value::as_str)
                == Some("stopped")
            {
                if let Ok(lifecycle) = crate::zmx::read_lifecycle_params(&params) {
                    crate::session_chat_send_wake::retire_held_messages_of_closed_session(
                        state,
                        &lifecycle.project_id,
                        &lifecycle.session_id,
                    );
                    let closed_threads = super::coordinator_close::close_threads_with_coordinator(
                        state,
                        &db,
                        &repository,
                        &(lifecycle.project_id.clone(), lifecycle.session_id.clone()),
                    );
                    if closed_threads > 0 {
                        output.result["closedThreads"] = json!(closed_threads);
                    }
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, output.result),
            )
        }
        Err(error) => {
            if is_close_request(&endpoint_path, &params) {
                log_close_request(state, &endpoint_path, &params, "failed");
            }
            zmx_error_response(endpoint_path, request_id, error)
        }
    }
}

/// `/api/killSession`, or a `/api/transitionSession` whose action is close.
fn is_close_request(endpoint_path: &str, params: &Map<String, Value>) -> bool {
    endpoint_path == "/api/killSession"
        || (endpoint_path == "/api/transitionSession"
            && params.get("action").and_then(Value::as_str) == Some("close"))
}

fn log_close_request(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    outcome: &str,
) {
    if let Ok(lifecycle) = crate::zmx::read_lifecycle_params(params) {
        crate::session_close_log::log_session_close(
            &state.logger,
            state.metadata.server_id.as_str(),
            &lifecycle.project_id,
            &lifecycle.session_id,
            params,
            endpoint_path,
            outcome,
        );
    }
}

pub(crate) struct ZmxStopAllCounts {
    pub(crate) attempted: u64,
    pub(crate) failed: u64,
    pub(crate) killed: u64,
    pub(crate) skipped: u64,
}

/*
CDXC:SessionSleep 2026-08-28:
`/api/control/stopAll` promised "kill tracked zmx sessions, mark them stopped,
then stop the control plane" (gx server stop-all), but the Rust port only ever
stopped the control plane and reported zero counts. The app's Quit Ghostex &
BG Service menu item depends on the promise being real: quitting must leave no
Ghostex-spawned zmx daemon running. Sleeping and stopped sessions are skipped
because their zmx process is already gone — sleep kills the provider too and
only keeps the transcript resumable.

CDXC:SessionSleep 2026-09-01:
Killed sessions are marked "sleeping", not "stopped". "stopped" is the
terminal close state: the sidebar drops it and `effective_lifecycle_state`
refuses to ever promote it back, so Quit Ghostex & BG Service silently swept
every live session out of the user's sidebar into Quick Access history.
Quitting only needs the daemons dead; the intent is "resume my board later",
which is exactly a sleep — the same kill this endpoint already performs, and
the same state `cycle_wire_incompatible_zmx_session_daemons` writes for the
identical physical operation. The ordinary wake-on-open path restores them.
*/
pub(crate) fn kill_all_tracked_zmx_sessions(state: &AppState) -> ZmxStopAllCounts {
    let mut counts = ZmxStopAllCounts {
        attempted: 0,
        failed: 0,
        killed: 0,
        skipped: 0,
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return counts;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions(None) else {
        return counts;
    };
    for session in sessions {
        let lifecycle_state = session
            .get("lifecycleState")
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(lifecycle_state, "stopped" | "sleeping") {
            counts.skipped += 1;
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(Value::as_str),
            session.get("sessionId").and_then(Value::as_str),
        ) else {
            counts.skipped += 1;
            continue;
        };
        counts.attempted += 1;
        let lifecycle = crate::zmx::LifecycleParams {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        match crate::zmx::kill_and_cache_session_provider(&repository, &lifecycle, "sleeping") {
            Ok((kill, _)) if kill.killed => counts.killed += 1,
            _ => counts.failed += 1,
        }
    }
    counts
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
    // Sending to an agent id (no session) was a renderer command the desktop never answered; it
    // is retired (see `RENDERER_COMMAND_ACTIONS`), so say so instead of timing out.
    if endpoint_path == "/api/sendSessionMessage" && !params.contains_key("sessionId") {
        return routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                "send-message needs a session selector; sending to an agent id was retired.",
                Some(request_id),
            ),
        );
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
    CDXC:SessionChat 2026-08-24:
    The raw input-line writers ride the per-session send queue, so their
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
    /*
    CDXC:SessionChat 2026-08-26:
    Same shape for `/api/sendSessionMessage` (clear burst → text → settle →
    Enter, as one queued job): awaiting it here is what keeps the HTTP answer
    telling the caller whether the terminal actually took the message, now that
    the synchronous dispatch only enqueues.
    */
    if endpoint_path == "/api/sendSessionMessage" {
        let queued = {
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            crate::zmx::read_zmx_queued_session_message(&repository, &params)
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
    /*
    CDXC:Zmx 2026-09-01:
    What is left here is `/api/readSessionText`, which spawns `zmx history` and
    reads up to 256KiB back from it. Same reason as the lifecycle handler: it
    runs on the blocking pool rather than on an executor worker. The rusqlite
    connection moves into the closure (it is `Send`) and is dropped there.
    */
    let blocking_state = state.clone();
    let blocking_endpoint_path = endpoint_path.clone();
    let blocking_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        let repository = DomainRepository::new(&db, blocking_state.metadata.server_id.as_str());
        match dispatch_zmx_session_interaction_endpoint(
            &repository,
            &blocking_endpoint_path,
            &params,
        ) {
            Ok(result) => routed_json(
                Some(blocking_endpoint_path),
                StatusCode::OK,
                rpc_success(blocking_request_id, result),
            ),
            Err(error) => zmx_error_response(blocking_endpoint_path, blocking_request_id, error),
        }
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: format!("zmx session interaction task failed: {error}"),
            },
        ),
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
    // `ghostex close-after-done` reaches the app through here; an orchestrator is refused before
    // the app is asked, so the CLI (and the phone, which runs it) gets the plain error.
    if action == "toggleCloseAfterDone" && renderer_command_targets_orchestrator(state, &payload) {
        return routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                crate::close_after_done::ORCHESTRATOR_REFUSAL,
                Some(request_id),
            ),
        );
    }
    // Only settings an agent may write pass the API (settings_agent_writable.rs).
    if action == "updateSettingsPatch" {
        let refused = payload
            .get("patch")
            .and_then(Value::as_object)
            .map(crate::settings_agent_writable::refused_keys)
            .unwrap_or_default();
        if !refused.is_empty() {
            let message = crate::settings_agent_writable::refusal_message(&refused);
            crate::audit::record(
                crate::audit::Action::new(
                    "settings.ownerOnly",
                    "Tried to change settings only you can change",
                )
                .target(refused.join(", ")),
                crate::audit::Actor::current(),
                crate::audit::Outcome::Refused(message.clone()),
                None,
            );
            return routed_json(
                Some(endpoint_path),
                StatusCode::FORBIDDEN,
                rpc_error("forbidden", message, Some(request_id)),
            );
        }
    }
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
    let answer = if action == "updateSettingsPatch" {
        save_settings_patch(state, payload, timeout_ms).await
    } else {
        state
            .event_hub
            .dispatch_renderer_command(action, payload, timeout_ms)
            .await
            .map_err(|error| SettingsSaveError {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: error.code.to_string(),
                message: error.message,
            })
    };
    match answer {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            error.status,
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

/// Why a settings save did not go through, with the status the API answers it with.
pub(crate) struct SettingsSaveError {
    pub(crate) status: StatusCode,
    pub(crate) code: String,
    pub(crate) message: String,
}

/// Saves an `updateSettingsPatch` payload (`{patch, source}`) the way every API write does: the
/// running desktop app saves it with the Settings window's own save path, or, on a computer without
/// the app, gxserver itself (headless_settings.rs). Callers check the keys first
/// (settings_agent_writable.rs).
pub(crate) async fn save_settings_patch(
    state: &AppState,
    payload: Map<String, Value>,
    timeout_ms: u64,
) -> Result<Value, SettingsSaveError> {
    let settings_patch = payload.get("patch").and_then(Value::as_object).cloned();
    match state
        .event_hub
        .dispatch_renderer_command("updateSettingsPatch".to_string(), payload, timeout_ms)
        .await
    {
        Ok(result) => Ok(result),
        Err(error) if error.code == "dependencyUnavailable" && settings_patch.is_some() => {
            let paths = state.paths.clone();
            let patch = settings_patch.unwrap_or_default();
            match tokio::task::spawn_blocking(move || {
                crate::headless_settings::save_settings_patch(&paths, &patch)
            })
            .await
            .unwrap_or_else(|join| Err(join.to_string()))
            {
                Ok(Some(result)) => Ok(result),
                Ok(None) => Err(SettingsSaveError {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    code: error.code.to_string(),
                    message: error.message,
                }),
                Err(message) => Err(SettingsSaveError {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: "internalError".to_string(),
                    message,
                }),
            }
        }
        Err(error) => Err(SettingsSaveError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: error.code.to_string(),
            message: error.message,
        }),
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
    CDXC:CefRuntime 2026-06-21-19:22:
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

/// Whether a renderer command's session is one of this daemon's orchestrators, by its
/// `sessionTarget` (or `projectId`/`sessionId`), or by a bare session id when no project is named.
fn renderer_command_targets_orchestrator(state: &AppState, payload: &Map<String, Value>) -> bool {
    let target = payload
        .get("sessionTarget")
        .and_then(Value::as_object)
        .unwrap_or(payload);
    let text = |key: &str| {
        target
            .get(key)
            .or_else(|| payload.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let Some(selector) = text("sessionId") else {
        return false;
    };
    // A selector may also be a reply-to id (`S90:P3lv0:G4ldy`) or a sidebar id (`P3lv0:G4ldy`).
    let mut parts = selector.rsplit(':');
    let session_id = parts.next().unwrap_or(selector);
    let project_id = text("projectId").or_else(|| parts.next());
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    match project_id {
        Some(project_id) => crate::close_after_done::is_orchestrator(
            &db,
            &(project_id.to_string(), session_id.to_string()),
        ),
        None => crate::coordinators::list_coordinators(&db).is_ok_and(|coordinators| {
            coordinators
                .iter()
                .any(|coordinator| coordinator.session_id == session_id)
        }),
    }
}
