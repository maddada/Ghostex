//! The sidecar's REST API, on loopback. A headless OpenMausBot server treats loopback requests as
//! its owner, so no token is needed.

use std::time::Duration;

use serde_json::Value;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The sidecar's base URL while it runs.
pub(crate) fn base_url() -> Result<String, String> {
    super::sidecar::running_port()
        .map(|port| format!("http://127.0.0.1:{port}"))
        .ok_or_else(|| "GBots is not running on this computer.".to_string())
}

fn answer(
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Value, String> {
    let response = response.map_err(|error| format!("GBots did not answer: {error}"))?;
    let status = response.status().as_u16();
    let body: Value = crate::http_agent::read_json(response).unwrap_or(Value::Null);
    if status >= 400 {
        let message = body
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(message);
    }
    Ok(body)
}

pub(crate) fn get(path: &str) -> Result<Value, String> {
    let url = format!("{}{path}", base_url()?);
    answer(
        crate::http_agent::answering_agent()
            .get(&url)
            .config()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .call(),
    )
}

pub(crate) fn send(method: &str, path: &str, body: &Value) -> Result<Value, String> {
    let url = format!("{}{path}", base_url()?);
    let agent = crate::http_agent::answering_agent();
    let response = match method {
        "PATCH" => agent
            .patch(&url)
            .config()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .send_json(body),
        "PUT" => agent
            .put(&url)
            .config()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .send_json(body),
        _ => agent
            .post(&url)
            .config()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .send_json(body),
    };
    answer(response)
}

pub(crate) fn post(path: &str, body: &Value) -> Result<Value, String> {
    send("POST", path, body)
}

/// Whether a server answers `/api/health` on `port` as OpenMausBot does.
pub(crate) fn healthy(port: u16) -> bool {
    crate::http_agent::answering_agent()
        .get(&format!("http://127.0.0.1:{port}/api/health"))
        .config()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .call()
        .ok()
        .filter(|response| response.status().as_u16() == 200)
        .and_then(|response| crate::http_agent::read_json::<Value>(response).ok())
        .is_some_and(|body| body.get("app").and_then(Value::as_str) == Some("openmausbot"))
}

/// Path-segment encoding for the ids the sidecar hands out (uuids), which need none; anything
/// else is refused rather than escaped.
pub(crate) fn id_segment(id: &str) -> Result<&str, String> {
    (!id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
    .then_some(id)
    .ok_or_else(|| format!("\"{id}\" is not a GBots id."))
}
