//! Agent routes: automations, agent CLI maintenance, managed tools, agent boxes, accounts, agent settings and resume plans, hooks, zmx session lifecycle and interaction, and renderer commands.

use super::*;

pub(super) async fn route_agents_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
        caller,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/readAutomationState"
        | "/api/saveAutomation"
        | "/api/deleteAutomation"
        | "/api/runAutomationNow"
        | "/api/setAutomationEnabled"
        | "/api/archiveAutomationRun"
        | "/api/markAutomationRunRead" => {
            handle_automation_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readMemoryGuard" => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(
                request_id,
                crate::memory_guard::read_endpoint_value(&state.paths),
            ),
        ),
        "/api/readClaudeCompaction" => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(
                request_id,
                crate::agents::claude_idle_compaction::read_endpoint_value(),
            ),
        ),
        "/api/agentCliMaintenance" => {
            agent_cli_http::handle(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/managedTools" => {
            managed_tools_http::handle(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/machineIntegrations" => {
            machine_integrations_http::handle(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/agentbox" => {
            agentbox_http::handle(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/cloudBoxes" => {
            agentbox_http::handle_cloud_boxes(state, endpoint.path, request_id, &body_json).await
        }
        "/api/agentAccounts" => {
            accounts_http::handle_accounts_http(
                &state,
                &caller,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/readAgentSettings"
        | "/api/updateAgentSettings"
        | "/api/readAgentResumePlan"
        | "/api/forkSession"
        | "/api/startSideChat"
        | "/api/switchDraftAgent"
        | "/api/draftRunLocation"
        | "/api/switchSessionAgent"
        | "/api/requestSessionRename"
        | "/api/cancelFirstPromptAutoTitle"
        | "/api/ingestTerminalTitleEvent"
        | "/api/updateAgentActivity"
        | "/api/ingestAgentHookEvent" => {
            handle_agent_http(&state, endpoint.path, request_id, &body_json).await
        }
        // The Ghostex Pi, OMP and ACP chat channel (pi_chat_channel.rs).
        "/api/agentChatChannel" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            // A session token may only poll, ack, close or check trust for its own `sessionRef`
            // channel, never hijack another session's channel.
            if let crate::security::Principal::Session {
                project_id: own_project,
                session_id: own_session,
                ..
            } = &caller
            {
                let reference = params
                    .get("sessionRef")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .unwrap_or_default();
                let matches_self =
                    match crate::agent_hooks::probing::parse_global_session_ref(reference) {
                        (Some(pid), Some(sid)) => pid == *own_project && sid == *own_session,
                        _ => false,
                    };
                if !matches_self {
                    return Ok(domain_error_response(
                        endpoint.path,
                        request_id,
                        DomainStateError {
                            code: "forbidden",
                            message: "A session token may only access its own chat channel."
                                .to_string(),
                        },
                    ));
                }
                // An ACP child agent inherits `GHOSTEX_SESSION_TOKEN` and must never impersonate
                // `acp-host` on its own `/api/agentChatChannel` to steal or fake-ACK `"mode"`
                // downgrades. Require the per-session host channel secret (which `acp-host`
                // strips from the child subprocess environment before spawning the agent).
                let agent_is_acp = params
                    .get("agent")
                    .and_then(Value::as_str)
                    .is_some_and(|a| a.starts_with("acp:"));
                let live_is_acp = crate::pi_chat_channel::live(own_project, own_session)
                    .is_some_and(|info| info.agent.starts_with("acp:"));
                let stored_is_acp =
                    crate::acp::channel::session_is_acp_in_db(&state, own_project, own_session);
                if agent_is_acp || live_is_acp || stored_is_acp {
                    let provided = params.get("channelSecret").and_then(Value::as_str);
                    if !crate::acp::channel::verify_channel_secret(
                        &state.auth_token,
                        own_project,
                        own_session,
                        provided,
                    ) {
                        return Ok(domain_error_response(
                            endpoint.path,
                            request_id,
                            DomainStateError {
                                code: "forbidden",
                                message: "A Native session token cannot access its chat channel without the host channel secret.".to_string(),
                            },
                        ));
                    }
                }
            }
            match crate::pi_chat_channel::dispatch(&state, &params).await {
                Ok(result) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => domain_error_response(endpoint.path, request_id, error),
            }
        }
        "/api/listSideChats" | "/api/bringBackSideChat" | "/api/deleteSideChat" => {
            handle_side_chat_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readAgentHookStatus" | "/api/installAgentHooks" | "/api/uninstallAgentHooks" => {
            handle_agent_hook_http(&state, endpoint.path, request_id, &body_json)
        }
        // First-run setup's sign-in check (agent_sign_in.rs): off the async runtime, since a
        // CLI's status command can take seconds.
        "/api/agentSignIn" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            let worker_state = state.clone();
            let result = tokio::task::spawn_blocking(move || {
                crate::agent_sign_in::dispatch(&worker_state, &params)
            })
            .await
            .unwrap_or_else(|error| {
                Err(DomainStateError::corrupt_state(format!(
                    "The sign-in check stopped unexpectedly: {error}"
                )))
            });
            match result {
                Ok(result) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => domain_error_response(endpoint.path, request_id, error),
            }
        }
        "/api/readHooklessActivityComparison" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            let result = crate::hookless_activity::read_comparison(&state, &params);
            routed_json(
                Some(endpoint.path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        "/api/createWorkspaceTerminal"
        | "/api/attachSessionMetadata"
        | "/api/startSessionProvider"
        | "/api/transitionSession"
        | "/api/sleepSession"
        | "/api/wakeSession"
        | "/api/killSession" => {
            if matches!(
                endpoint.path.as_str(),
                "/api/startSessionProvider" | "/api/wakeSession" | "/api/attachSessionMetadata"
            ) && !caller.is_user_device()
            {
                if let Ok(params) = read_domain_rpc_params(&body_json) {
                    let has_startup_text = params
                        .get("startupText")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty());
                    if has_startup_text {
                        return Ok(domain_error_response(
                            endpoint.path,
                            request_id,
                            DomainStateError {
                                code: "forbidden",
                                message: "Only your own devices can pass a custom startup command."
                                    .to_string(),
                            },
                        ));
                    }
                }
            }
            let mut body_json = body_json;
            if matches!(
                endpoint.path.as_str(),
                "/api/killSession" | "/api/transitionSession"
            ) {
                if let crate::security::Principal::Session {
                    project_id,
                    session_id,
                    ..
                } = &caller
                {
                    if let Some(params) = body_json.get_mut("params").and_then(Value::as_object_mut)
                    {
                        params.insert(
                            crate::session_close_log::CLOSED_BY_PARAM.to_string(),
                            Value::String(crate::ids::create_global_session_ref(
                                state.metadata.server_id.as_str(),
                                project_id,
                                session_id,
                            )),
                        );
                    }
                }
            }
            handle_zmx_lifecycle_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/reloadSessionAgent" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
            match crate::session_chat_agent_restart::reload(&state, &params).await {
                Ok(result) => routed_json(
                    Some(endpoint.path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(error) => domain_error_response(endpoint.path, request_id, error),
            }
        }
        "/api/readSessionText"
        | "/api/sendSessionText"
        | "/api/sendSessionMessage"
        | "/api/sendSessionEnter"
        | "/api/focusSession" => {
            // A message typed into another session says who sent it (session_chat_send/sender_mark.rs).
            let mut body_json = body_json;
            let as_user_log = if endpoint.path == "/api/sendSessionMessage" {
                crate::session_chat_send::sender_mark::prepare(
                    &state.paths,
                    state.metadata.server_id.as_str(),
                    &endpoint.path,
                    &caller,
                    &mut body_json,
                )
            } else {
                None
            };
            let response = handle_zmx_session_interaction_http(
                &state,
                &caller,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await;
            crate::session_chat_send::sender_mark::record_as_user_log(as_user_log, &response);
            response
        }
        "/api/dispatchRendererCommand" => {
            if !debug_modal_caller_allowed(&caller, &body_json) {
                return Ok(routed_json(
                    Some(endpoint.path),
                    StatusCode::FORBIDDEN,
                    rpc_error(
                        "forbidden",
                        "ghostex debug show-modal opens a pop-up in this computer's own Ghostex app, so only this computer can ask for it, not a phone or another computer.",
                        Some(request_id),
                    ),
                ));
            }
            handle_renderer_command_http(&state, &caller, endpoint.path, request_id, &body_json)
                .await
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
                caller,
            })
        }
    })
}

/// `showDebugModal` (`ghostex debug show-modal`) puts a pop-up on the app's window here, for the
/// user's own checks: the app's owner, the CLI with this computer's token file, or a session on
/// this computer may ask; a phone, another computer, a Collab guest or an extension may not.
/// Every other renderer command keeps its own rules.
fn debug_modal_caller_allowed(caller: &crate::security::Principal, body: &Value) -> bool {
    use crate::security::Principal;
    let action = body.pointer("/params/action").and_then(Value::as_str);
    action != Some("showDebugModal")
        || matches!(
            caller,
            Principal::Owner { .. } | Principal::LocalAgent | Principal::Session { .. }
        )
}
