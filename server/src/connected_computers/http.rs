//! `registerConnectedComputer`, `unregisterConnectedComputer` and `listConnectedComputers`: the
//! desktop app tells gxserver which computers it is connected to.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};

use super::registry::{self, Registration};

pub(crate) const CONNECTED_COMPUTER_ENDPOINTS: &[&str] = &[
    "/api/registerConnectedComputer",
    "/api/unregisterConnectedComputer",
    "/api/listConnectedComputers",
    "/api/issueComputerGrant",
    "/api/revokeComputerGrant",
];

pub(crate) fn handle_connected_computers_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => Value::Object(params),
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", error.message, Some(request_id)),
            );
        }
    };
    let result = match endpoint_path.as_str() {
        "/api/registerConnectedComputer" => {
            match Registration::from_params(&params, &state.metadata.server_id) {
                Ok(registration) => {
                    let agent_access = registry::register(registration);
                    json!({
                        "registered": true,
                        "agentAccess": agent_access,
                        "serverId": state.metadata.server_id,
                        "name": this_computer_name(),
                    })
                }
                Err(message) => {
                    return routed_json(
                        Some(endpoint_path),
                        StatusCode::BAD_REQUEST,
                        rpc_error("badRequest", message, Some(request_id)),
                    );
                }
            }
        }
        "/api/unregisterConnectedComputer" => {
            let server_id = params
                .get("serverId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            json!({ "unregistered": registry::unregister(server_id) })
        }
        "/api/issueComputerGrant" => {
            let server_id = params
                .get("serverId")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !registry::is_server_id(server_id) || server_id == state.metadata.server_id {
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error(
                        "badRequest",
                        "serverId must be another computer's server id.",
                        Some(request_id),
                    ),
                );
            }
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(server_id);
            let agent_access = params
                .get("agentAccess")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let rotate = params.get("rotate").and_then(Value::as_bool) == Some(true);
            let owner = params.get("owner").and_then(Value::as_bool) == Some(true);
            let ttl = params
                .get("ttlMs")
                .and_then(Value::as_u64)
                .map(std::time::Duration::from_millis)
                .unwrap_or(registry::DEFAULT_REGISTRATION_TTL);
            let grant = registry::issue_grant(server_id, name, agent_access, rotate, ttl, owner);
            json!({
                "serverId": state.metadata.server_id,
                "name": this_computer_name(),
                // This computer's own switch is off for that computer: nothing is minted.
                "denied": grant.is_err(),
                "peerToken": grant.ok().flatten(),
                // The way back lands on the listener that serves only the allowlist.
                "peerPort": super::peer_listener::peer_listener_port(),
            })
        }
        "/api/revokeComputerGrant" => {
            let server_id = params
                .get("serverId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let deny = params.get("deny").and_then(Value::as_bool) == Some(true);
            let ttl = params
                .get("ttlMs")
                .and_then(Value::as_u64)
                .map(std::time::Duration::from_millis)
                .unwrap_or(registry::DEFAULT_REGISTRATION_TTL);
            json!({ "revoked": registry::revoke_grant(server_id, deny, ttl) })
        }
        _ => json!({
            "serverId": state.metadata.server_id,
            "name": this_computer_name(),
            "computers": registry::list()
                .iter()
                .map(registry::ConnectedComputer::public_view)
                .collect::<Vec<_>>(),
        }),
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

/// The name the other computer shows for this one: its host name without the `.local` suffix.
pub(crate) fn this_computer_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let name = std::process::Command::new("hostname")
            .arg("-s")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|name| !name.is_empty())
            .or_else(|| std::env::var("COMPUTERNAME").ok())
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "This computer".to_string());
        name.trim_end_matches(".local").to_string()
    })
    .clone()
}
