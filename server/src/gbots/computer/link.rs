//! Calls from this gxserver to a connected computer's gxserver, over the connection the desktop
//! app keeps to it (crate::connected_computers), with that computer's own token for this one.

use std::time::Duration;

use serde_json::{json, Value};

use crate::connected_computers::ConnectedComputer;
use crate::constants::{GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION};
use crate::domain::DomainStateError;

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

fn away(message: String) -> DomainStateError {
    DomainStateError {
        code: "computerNotConnected",
        message,
    }
}

/// The connected computer `server_id`, refused with one sentence when it isn't connected or its
/// Agent access is off. `name` is what the GBot's record calls it.
pub(crate) fn reach(server_id: &str, name: &str) -> Result<ConnectedComputer, DomainStateError> {
    let computer = crate::connected_computers::find(server_id).ok_or_else(|| {
        away(format!(
            "{name} isn't connected to this computer now. Connect it in Settings > Remote > Remote machines, then try again."
        ))
    })?;
    if !computer.agent_access {
        return Err(away(crate::connected_computers::access_off_message(
            &computer,
        )));
    }
    Ok(computer)
}

/// `path` on `computer` with `params`; its answer's `result`, or its error as a domain error.
pub(crate) fn call(
    computer: &ConnectedComputer,
    path: &str,
    params: Value,
) -> Result<Value, DomainStateError> {
    let url = format!("{}{path}", computer.base_url);
    computer
        .check_listener(&url)
        .map_err(|foreign| away(foreign.to_string()))?;
    let body = json!({ "params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION });
    let response = crate::http_agent::answering_agent()
        .post(&url)
        .header("authorization", format!("Bearer {}", computer.token()))
        .header("content-type", "application/json")
        .header(
            GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(CALL_TIMEOUT))
        .build()
        .send(body.to_string())
        .map_err(|_| {
            away(format!(
                "{} didn't answer. Ghostex reconnects it on its own, so try again in a moment.",
                computer.name
            ))
        })?;
    let status = response.status();
    let envelope: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
    if status.is_success() && envelope.get("ok").and_then(Value::as_bool) != Some(false) {
        return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
    }
    let code = envelope
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("remoteError");
    let message = envelope
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} answered {status}.", computer.name));
    Err(DomainStateError {
        code: if code == "forbidden" {
            "forbidden"
        } else {
            "remoteError"
        },
        message: format!("{}: {message}", computer.name),
    })
}
