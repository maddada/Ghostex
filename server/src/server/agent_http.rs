use super::*;
use crate::session_chat_interactive::emit_session_chat_prompt_state_frame;

/*
CDXC:GxserverRustPort 2026-06-16-10:00:
Phase 6 agent endpoints share the durable domain repository and authenticated RPC envelope with earlier Rust milestones. Keep zmx fork startup explicit through the selected listener port and schedule presentation deltas only after the session row has been updated.
*/
pub(crate) async fn handle_agent_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
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
    match dispatch_agent_endpoint(
        &repository,
        &db,
        &state.paths.home_dir,
        &endpoint_path,
        &params,
        Some(&context),
    ) {
        Ok(output) => {
            let presentation_session = output.presentation_session.clone();
            let mut result = output.result;
            let fork_initial_rename = fork_initial_rename_target(&endpoint_path, &result);
            log_agent_hook_passive_identity_conflict(state, &endpoint_path, &params, &result);
            let session_chat_state_changed = endpoint_path == "/api/ingestAgentHookEvent"
                && (result
                    .get("sessionChatPromptChanged")
                    .and_then(Value::as_bool)
                    == Some(true)
                    || result
                        .get("sessionChatActivityChanged")
                        .and_then(Value::as_bool)
                        == Some(true));
            strip_agent_hook_internal_result_fields(&endpoint_path, &mut result);
            let should_queue_agent_title_metadata_check =
                should_schedule_agent_title_metadata_check(&endpoint_path, &result);
            let should_schedule_first_prompt_auto_title =
                result.get("reason").and_then(Value::as_str)
                    == Some("first-prompt-auto-title-claimed");
            if endpoint_path == "/api/updateAgentActivity" {
                if let Some((project_id, session_id)) = presentation_session.as_ref() {
                    let _ = reconcile_agent_metadata_title_for_session(
                        &repository,
                        project_id,
                        session_id,
                        &state.paths.home_dir,
                        "pending",
                    );
                }
            }
            if let Some((project_id, session_id)) = presentation_session.as_ref() {
                if let Err(error) = schedule_presentation_session_delta(
                    state,
                    &db,
                    &repository,
                    project_id,
                    session_id,
                ) {
                    return domain_error_response(endpoint_path, request_id, error);
                }
                if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        &session,
                        stale_activity_refresh_reason(&endpoint_path),
                    );
                }
            }
            if presentation_session.is_none()
                && endpoint_path == "/api/ingestTerminalTitleEvent"
                && result_activity(&result) == Some("working")
            {
                if let Some(session) = result.get("session") {
                    schedule_stale_activity_presentation_refresh(
                        state,
                        session,
                        "terminal-title-stale-activity",
                    );
                }
            }
            if should_queue_agent_title_metadata_check {
                if let Some((project_id, session_id)) = presentation_session {
                    schedule_agent_title_metadata_check(state.clone(), project_id, session_id);
                }
            }
            if should_schedule_first_prompt_auto_title {
                if let Some(session) = result.get("session") {
                    if let (Some(project_id), Some(session_id), Some(attempt_id)) = (
                        read_session_text(session, "projectId"),
                        read_session_text(session, "sessionId"),
                        read_runtime_text(session, FIRST_PROMPT_AUTO_TITLE_ATTEMPT_ID_KEY),
                    ) {
                        schedule_first_prompt_auto_title_job(
                            state.clone(),
                            project_id,
                            session_id,
                            attempt_id,
                        );
                    }
                }
            }
            if let Some(target) = fork_initial_rename {
                schedule_fork_initial_rename(state.clone(), target);
            }
            if let Some((project_id, session_id, command)) =
                requested_agent_title_command_submission(&endpoint_path, &params, &result)
            {
                /*
                CDXC:GPUIRemoteSessionRename 2026-08-12:
                A remote GPUI has no local Ghostty surface for this session.
                When its bounded native bridge opts in, submit the rename from
                the owning gxserver through zmx's separate text/Enter path.
                Local GPUI renames omit the flag and retain their native-surface
                Enter path.
                */
                let mut send_params = Map::new();
                send_params.insert("projectId".to_string(), json!(project_id));
                send_params.insert("sessionId".to_string(), json!(session_id));
                send_params.insert(
                    "diagnosticInputSource".to_string(),
                    json!("remote-session-rename-command"),
                );
                send_params.insert("submit".to_string(), Value::Bool(true));
                send_params.insert("text".to_string(), Value::String(command));
                if let Err(error) = dispatch_zmx_session_interaction_endpoint(
                    &repository,
                    "/api/sendSessionMessage",
                    &send_params,
                ) {
                    return zmx_error_response(endpoint_path, request_id, error);
                }
            }
            if session_chat_state_changed {
                if let Some(session) = result.get("session") {
                    emit_session_chat_prompt_state_frame(state, session);
                }
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => agent_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn handle_agent_hook_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::INTERNAL_SERVER_ERROR,
                rpc_error("internalError", error.message, Some(request_id)),
            )
        }
    };
    /*
    CDXC:AgentHooks 2026-06-19-14:15:
    Hook uninstall is routed with read/install through the local authenticated RPC envelope. The handler returns the TypeScript-compatible status payload plus removedPaths without writing provider paths, hook commands, or hook file contents to persistent logs.

    CDXC:AgentHooks 2026-06-22-08:23:
    Area 27 status-code parity keeps malformed hook RPC params on TypeScript gxserver's generic internalError path. File/config hook operation failures still use the shared domain-error mapping after params have been accepted.
    */
    let result = match endpoint_path.as_str() {
        "/api/installAgentHooks" => install_agent_hooks(&state.paths, &params),
        "/api/uninstallAgentHooks" => uninstall_agent_hooks(&state.paths, &params),
        _ => read_agent_hook_status(&state.paths, &params),
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

/*
CDXC:AgentSkills 2026-06-19-13:59:
Agent skill status/install endpoints are local-only because they inspect or mutate user-local agent skill directories. Route them outside domain-state handlers so install stdout/stderr are returned only in the RPC response and are not written to persistent gxserver logs.
*/
pub(crate) async fn handle_agent_skill_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = if endpoint_path == "/api/installAgentSkills" {
        install_agent_skills(&state.paths, &params).await
    } else {
        read_agent_skill_status(&state.paths, &params)
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

pub(crate) fn agent_error_response(
    endpoint_path: String,
    request_id: String,
    error: AgentEndpointError,
) -> RoutedResponse {
    match error {
        AgentEndpointError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        AgentEndpointError::DependencyUnavailable(message) => routed_json(
            Some(endpoint_path),
            StatusCode::SERVICE_UNAVAILABLE,
            rpc_error("dependencyUnavailable", message, Some(request_id)),
        ),
    }
}

pub(crate) fn log_agent_hook_passive_identity_conflict(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if endpoint_path != "/api/ingestAgentHookEvent"
        || result.get("reason").and_then(Value::as_str) != Some("passive-session-identity-conflict")
    {
        return;
    }
    let Some(conflict) = result.get("identityConflict").and_then(Value::as_object) else {
        return;
    };
    if conflict.get("source").and_then(Value::as_str) != Some("passive") {
        return;
    }
    let agent_id = conflict.get("agentId").and_then(Value::as_str);
    let current_agent_session_id = conflict
        .get("currentAgentSessionId")
        .and_then(Value::as_str);
    let incoming_agent_session_id = conflict
        .get("incomingAgentSessionId")
        .and_then(Value::as_str);
    let reason = conflict.get("reason").and_then(Value::as_str);
    let source = conflict.get("source").and_then(Value::as_str);
    /*
    CDXC:AgentHooks 2026-06-22-08:31:
    Passive hook identity conflicts need the same support-bundle evidence as TypeScript without exposing thread ids, paths, titles, prompts, or hook payloads. Hash private agent-session ids at the writer boundary and log only enum fields, stable gxserver ids, and payload-shape booleans.
    */
    let _ = state
        .logger
        .log_routine(DiagnosticLogScenario::AgentDetection, GxserverLogInput {
            level: LogLevel::Debug,
            event: "sessionIdentity.updateBlocked".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "agentId": agent_id,
                "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
                "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
                "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
                "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
                "ownerProjectId": conflict.get("ownerProjectId").cloned(),
                "ownerSessionId": conflict.get("ownerSessionId").cloned(),
                "projectId": params.get("projectId").and_then(Value::as_str),
                "reason": reason,
                "sessionId": params.get("sessionId").and_then(Value::as_str),
                "source": source,
            })),
        });
    let hook_activity = normalize_agent_hook_activity(
        params.get("status"),
        params
            .get("eventName")
            .or_else(|| params.get("rawEventName")),
        params.get("agentName"),
    );
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "sessionIdentity.passiveEventRejected".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({
            "activity": hook_activity,
            "agentId": agent_id,
            "currentAgentSessionIdHash": hash_log_identity(current_agent_session_id),
            "currentAgentSessionIdPresent": current_agent_session_id.is_some(),
            "hasAgentSessionId": params.get("agentSessionId").is_some(),
            "hasAgentSessionPath": params.get("agentSessionPath").is_some(),
            "hasExplicitStatus": params.get("status").is_some(),
            "hasFirstUserMessage": params.get("firstUserMessage").is_some(),
            "hasHookEventName": params.get("eventName").is_some() || params.get("rawEventName").is_some(),
            "hasTitle": params.get("title").is_some(),
            "identitySource": source,
            "incomingAgentSessionIdHash": hash_log_identity(incoming_agent_session_id),
            "incomingAgentSessionIdPresent": incoming_agent_session_id.is_some(),
            "ownerProjectId": conflict.get("ownerProjectId").cloned(),
            "ownerSessionId": conflict.get("ownerSessionId").cloned(),
            "projectId": params.get("projectId").and_then(Value::as_str),
            "reason": reason,
            "sessionId": params.get("sessionId").and_then(Value::as_str),
            "source": "agent-hook-event",
        })),
    });
}

pub(crate) fn strip_agent_hook_internal_result_fields(endpoint_path: &str, result: &mut Value) {
    if endpoint_path != "/api/ingestAgentHookEvent" {
        return;
    }
    if let Some(object) = result.as_object_mut() {
        object.remove("identityConflict");
        object.remove("sessionChatPromptChanged");
        object.remove("sessionChatActivityChanged");
    }
}

pub(crate) fn hash_log_identity(value: Option<&str>) -> Option<String> {
    let value = value?;
    let digest = Sha256::digest(value.as_bytes());
    Some(
        digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
}

pub(crate) fn should_schedule_agent_title_metadata_check(endpoint_path: &str, result: &Value) -> bool {
    match endpoint_path {
        "/api/requestSessionRename" => {
            result.get("pendingAgentMetadata").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestSessionStateEvent" => {
            result.get("reason").and_then(Value::as_str)
                != Some("passive-session-identity-conflict")
                && result.get("reason").and_then(Value::as_str)
                    != Some("session-state-agent-mismatch")
        }
        "/api/ingestTerminalTitleEvent" => {
            result.get("changed").and_then(Value::as_bool) == Some(true)
        }
        "/api/ingestAgentHookEvent" => !matches!(
            result.get("reason").and_then(Value::as_str),
            Some("agent-hook-agent-mismatch" | "passive-session-identity-conflict")
        ),
        "/api/updateAgentActivity" => true,
        _ => false,
    }
}

pub(crate) fn stale_activity_refresh_reason(endpoint_path: &str) -> &'static str {
    match endpoint_path {
        "/api/ingestAgentHookEvent" => "agent-hook-stale-activity",
        "/api/ingestTerminalTitleEvent" => "terminal-title-stale-activity",
        _ => "agent-activity-stale-activity",
    }
}
