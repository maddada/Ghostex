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
    caller: &crate::security::Principal,
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
    /*
    CDXC:ServerLinks 2026-11-19 FIX:
    Fable 5.2's review of server links (GitHub #674), adopted by the user (2A): any agent's session token could call these for any server id, so it could take a linked server's grant (get its live token back, rotate it, lift a deny with `owner: true`) or register its own address under that server's id. For a server id a server link names here, only this computer's own clients (the app and the CLI through the token file, the owner session, a paired phone) may change its grant or registration; an agent's session token is refused. Every other server id, and the desktop app's own link, behave as before.
    */
    if matches!(caller, crate::security::Principal::Session { .. })
        && endpoint_path != "/api/listConnectedComputers"
    {
        let server_id = params
            .get("serverId")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !server_id.is_empty()
            && crate::remote_access::server_links::is_linked_server_id(&state.paths, server_id)
        {
            return routed_json(
                Some(endpoint_path),
                StatusCode::FORBIDDEN,
                rpc_error(
                    "forbidden",
                    "That computer is linked to this one as a server: only you can change its link (ghostex computers, or Settings › Remote).",
                    Some(request_id),
                ),
            );
        }
    }
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
            let had_grant = registry::has_grant(server_id);
            let grant = registry::issue_grant(server_id, name, agent_access, rotate, ttl, owner);
            if matches!(grant, Ok(Some(_))) && (!had_grant || rotate) {
                crate::audit::record(
                    crate::audit::Action::new(
                        "computers.grant",
                        format!("Let agents on {name} use this computer"),
                    )
                    .target(name),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Done,
                    None,
                );
            }
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
            let revoked = registry::revoke_grant(server_id, deny, ttl);
            if revoked {
                crate::audit::record(
                    crate::audit::Action::new(
                        "computers.revoke",
                        "Stopped agents on a connected computer from using this computer",
                    )
                    .target(server_id),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Done,
                    None,
                );
            }
            json!({ "revoked": revoked })
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

/// The name this computer goes by everywhere (its chosen name, else its host name), the one
/// pairing hands phones and other computers (`read_computer_name` in remote_access/identity.rs).
pub(crate) fn this_computer_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        crate::remote_access::cached_remote_access_identity()
            .ok()
            .map(|identity| identity.computer_name.trim().to_string())
            .filter(|name| !name.is_empty())
            .or_else(|| {
                std::env::var("COMPUTERNAME")
                    .or_else(|_| std::env::var("HOSTNAME"))
                    .ok()
                    .map(|name| crate::remote_access::identity::short_host_name(&name))
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or_else(|| "Local".to_string())
    })
    .clone()
}
