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
            accounts_http::handle_accounts_http(&state, endpoint.path, request_id, &body_json).await
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
        // The Ghostex Pi and OMP extensions' chat channel (pi_chat_channel.rs).
        "/api/agentChatChannel" => {
            let params = match read_domain_rpc_params(&body_json) {
                Ok(params) => params,
                Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
            };
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
            handle_zmx_session_interaction_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/dispatchRendererCommand" => {
            handle_renderer_command_http(&state, endpoint.path, request_id, &body_json).await
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
