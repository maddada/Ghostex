//! `/api/openConversation` over HTTP: the decision is `crate::open_conversation`; its steps are
//! this daemon's own endpoints, called over the loopback API with the daemon's token, so a
//! restore and a resume publish, log and refuse exactly as they do for a client.

use super::*;

use crate::open_conversation::{open_conversation, OpenConversationRequest};

/// A fork starts a provider and can take a while.
const STEP_TIMEOUT_MS: u64 = 60_000;

pub(crate) async fn handle_open_conversation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let worker_state = state.clone();
    let worker_endpoint = endpoint_path.clone();
    let worker_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body,
            |_, _, params, _| {
                let request = OpenConversationRequest::read(params)?;
                let token = worker_state.auth_token.clone();
                open_conversation(&request, &|path, params| loopback(&token, path, params))
            },
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::corrupt_state(format!("openConversation failed: {error}")),
        ),
    }
}

fn loopback(
    token: &str,
    path: &str,
    params: Value,
) -> std::result::Result<Value, DomainStateError> {
    let unavailable =
        |detail: String| DomainStateError::corrupt_state(format!("{path} failed: {detail}"));
    let port = crate::config::read_selected_local_api_port()
        .map_err(|error| unavailable(error.to_string()))?;
    let address = format!("{}:{port}", crate::constants::GXSERVER_LOCAL_API_HOST);
    let body = json!({
        "params": params,
        "protocolVersion": crate::constants::GXSERVER_PROTOCOL_VERSION,
    })
    .to_string();
    let mut stream =
        std::net::TcpStream::connect(&address).map_err(|error| unavailable(error.to_string()))?;
    let timeout = Some(Duration::from_millis(STEP_TIMEOUT_MS));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{}: {}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        crate::constants::GXSERVER_PROTOCOL_HEADER,
        crate::constants::GXSERVER_PROTOCOL_VERSION,
        body.len(),
    );
    use std::io::{Read, Write};
    stream
        .write_all(request.as_bytes())
        .map_err(|error| unavailable(error.to_string()))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| unavailable(error.to_string()))?;
    let envelope = response
        .split_once("\r\n\r\n")
        .and_then(|(_, body)| serde_json::from_str::<Value>(body.trim()).ok())
        .ok_or_else(|| unavailable("the answer was not JSON".to_string()))?;
    if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
    }
    let message = envelope
        .get("message")
        .or_else(|| envelope.get("error"))
        .and_then(Value::as_str)
        .unwrap_or("gxserver rejected the request.")
        .to_string();
    Err(match envelope.get("error").and_then(Value::as_str) {
        Some("notFound") => DomainStateError::not_found(message),
        Some("badRequest") => DomainStateError::bad_request(message),
        _ => DomainStateError::corrupt_state(message),
    })
}
