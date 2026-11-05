//! The one HTTP read the presentation stream needs.

use std::time::Duration;

use ghostex_gx_protocol::{
    RpcRequest, RpcResponse, GXSERVER_PROTOCOL_VERSION, GXSERVER_PROTOCOL_VERSION_HEADER,
};
use serde_json::{json, Value};

use crate::socket::Endpoint;

/// A response larger than this is not a project list.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        // The envelope's `ok` and `error` decide, not the status.
        .http_status_as_error(false)
        // A loopback request must never be sent to a proxy from the environment.
        .proxy(None)
        .build()
        .into()
}

/// `POST /api/listProjects`: the full domain project rows, which carry the fields (recent, worktree,
/// icon) the presentation rows do not.
pub(crate) fn list_projects(
    agent: &ureq::Agent,
    endpoint: &Endpoint,
    auth_token: &str,
) -> Result<Vec<Value>, String> {
    let body = serde_json::to_string(&RpcRequest::new(json!({})))
        .map_err(|error| format!("could not encode the request: {error}"))?;
    // The token goes only to a listener of this account (ghostex_paths::loopback_owner).
    ghostex_paths::loopback_owner::check_host_port(&endpoint.host, endpoint.port)
        .map_err(|error| error.to_string())?;
    let mut response = agent
        .post(endpoint.http_url("/api/listProjects"))
        .header("Authorization", format!("Bearer {auth_token}"))
        .header(
            GXSERVER_PROTOCOL_VERSION_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .header("content-type", "application/json")
        .send(body.as_str())
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|error| format!("could not read the response (HTTP {status}): {error}"))?;
    match RpcResponse::<Value>::parse(&text) {
        Ok(RpcResponse::Success(success)) if !success.protocol_version_matches() => Err(format!(
            "the daemon answered with protocol version {}",
            success.protocol_version
        )),
        // `get_mut`, never `result["projects"]`: indexing a result that is not an object panics.
        Ok(RpcResponse::Success(mut success)) => {
            match success.result.get_mut("projects").map(Value::take) {
                Some(Value::Array(projects)) => Ok(projects),
                Some(_) => Err("`projects` in the response is not a list".to_string()),
                None => Err("the response has no `projects` list".to_string()),
            }
        }
        // The code only: a daemon message can quote a path.
        Ok(RpcResponse::Failure(failure)) => Err(format!(
            "the daemon answered `{}` (HTTP {status})",
            failure.error.as_str()
        )),
        Err(_) => Err(format!(
            "the response is not an RPC envelope (HTTP {status})"
        )),
    }
}
