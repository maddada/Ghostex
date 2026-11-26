//! Connectors routes (crate::connectors): read every workspace's connectors, sign a workspace in
//! to and out of Composio, list its tools again, connect one of its apps and call one of its tools;
//! install the Executor CLI, sign it in or out and remove the old built-in Executor's data; give agent sessions their
//! connector tools and run their calls; read and change a workspace's Ask/Allow/Never rules.

use serde_json::{Map, Value};

use crate::connectors::{
    bind_connector_agent_caller, broadcast_connectors_changed, call_composio_tool,
    call_connector_agent_tool, cancel_connector_agent_tool_call, cancel_executor_cli_sign_in,
    connect_composio_app, connectors_enabled, connectors_off_message, install_executor_cli,
    list_composio_tools, list_connector_agent_tools, read_connector_rules, read_connectors,
    refresh_composio, remove_old_executor, set_connector_cli_client, set_connector_rule,
    sign_in_composio, sign_in_connector_cli, sign_in_executor_cli, sign_out_composio,
    sign_out_connector_cli, sign_out_executor_cli, wait_connector_agent_tools,
};
use crate::domain::DomainStateError;
use crate::protocol::rpc_success;

use super::*;

type ConnectorAction = fn(&AppState, &Map<String, Value>) -> Result<Value, DomainStateError>;

/// What only the user's own app (or a paired phone) may do, as with pairing codes
/// (`SecurityState::keeps_door`): repoint or end the Executor sign-in every workspace's agents
/// use, run its installer, delete the old Executor's data, and change what agents may run.
///
/// CDXC:Connectors 2026-11-27 WHY:
/// These were open to agents' session tokens and to the `ghostex connectors` CLI any agent can run (security review of 769647fe4e, M3): an agent could sign the CLI in to a host of its own, which approves its own device code, and every workspace's agents would then get tools whose descriptions and results that host controls; or set its workspace's rules to Allow. The desktop sends its owner session with these calls (`is_door_call` in apps/desktop/src/app/owner_security/mod.rs).
const OWNER_ONLY: [&str; 5] = [
    "/api/installExecutorCli",
    "/api/signInExecutorCli",
    "/api/signOutExecutorCli",
    "/api/removeOldExecutor",
    "/api/setConnectorRule",
];

pub(super) async fn route_connectors_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let action: Option<ConnectorAction> = match request.endpoint.path.as_str() {
        "/api/readConnectors" => Some(read_connectors),
        "/api/signInComposio" => Some(sign_in_composio),
        "/api/signOutComposio" => Some(sign_out_composio),
        "/api/refreshComposio" => Some(refresh_composio),
        "/api/connectComposioApp" => Some(connect_composio_app),
        "/api/listComposioTools" => Some(list_composio_tools),
        "/api/callComposioTool" => Some(call_composio_tool),
        "/api/installExecutorCli" => Some(install_executor_cli),
        "/api/signInExecutorCli" => Some(sign_in_executor_cli),
        "/api/cancelExecutorCliSignIn" => Some(cancel_executor_cli_sign_in),
        "/api/signOutExecutorCli" => Some(sign_out_executor_cli),
        "/api/removeOldExecutor" => Some(remove_old_executor),
        "/api/listConnectorAgentTools" => Some(list_connector_agent_tools),
        "/api/cancelConnectorAgentToolCall" => Some(cancel_connector_agent_tool_call),
        "/api/readConnectorRules" => Some(read_connector_rules),
        "/api/setConnectorRule" => Some(set_connector_rule),
        "/api/signInConnectorCli" => Some(sign_in_connector_cli),
        "/api/signOutConnectorCli" => Some(sign_out_connector_cli),
        "/api/setConnectorCliClient" => Some(set_connector_cli_client),
        // Answered on their own below: a call may wait on the user's card for 30 minutes, and a
        // helper's wait for a change for a few.
        "/api/callConnectorAgentTool" | "/api/waitConnectorAgentTools" => None,
        _ => return Err(request),
    };
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        caller,
        ..
    } = request;
    if !connectors_enabled(&state.paths) {
        return Ok(domain_error_response(
            endpoint.path,
            request_id,
            DomainStateError::bad_request(connectors_off_message(&state.paths)),
        ));
    }
    let mut params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
    };
    if OWNER_ONLY.contains(&endpoint.path.as_str()) && !state.security.keeps_door(&caller) {
        return Ok(domain_error_response(
            endpoint.path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: "Only the Ghostex app on this computer can change that: use Settings > Connectors there.".to_string(),
            },
        ));
    }
    // An agent's tools and calls belong to the session its token is (connectors/agent/session.rs).
    let for_agent_session = matches!(
        endpoint.path.as_str(),
        "/api/listConnectorAgentTools"
            | "/api/callConnectorAgentTool"
            | "/api/cancelConnectorAgentToolCall"
    );
    if for_agent_session {
        if let Err(error) = bind_connector_agent_caller(&caller, &mut params) {
            return Ok(domain_error_response(endpoint.path, request_id, error));
        }
    }
    let Some(action) = action else {
        let result = if endpoint.path == "/api/waitConnectorAgentTools" {
            wait_connector_agent_tools(&state, params).await
        } else {
            call_connector_agent_tool(state.clone(), params).await
        };
        return Ok(routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ));
    };
    // These change what Settings shows without going through a provider's own change listener.
    let broadcasts = matches!(
        endpoint.path.as_str(),
        "/api/signOutComposio" | "/api/setConnectorRule"
    );
    let worker_state = state.clone();
    // Off the async runtime: Composio is called over the network.
    let result = tokio::task::spawn_blocking(move || action(&worker_state, &params))
        .await
        .unwrap_or_else(|error| {
            Err(DomainStateError::corrupt_state(format!(
                "Connectors stopped unexpectedly: {error}"
            )))
        });
    if broadcasts {
        if let Some(workspace) = result
            .as_ref()
            .ok()
            .and_then(|value| value.get("workspaceId"))
            .and_then(|value| value.as_str())
        {
            broadcast_connectors_changed(&state, workspace);
        }
    }
    Ok(match result {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}
