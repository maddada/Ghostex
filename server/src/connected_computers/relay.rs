//! `/api/computers/<serverId>/relay/api/...`: an agent's request for another computer, forwarded
//! over the connection this computer already has to it.

use std::time::Duration;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderValue, Method, Response, StatusCode},
};
use serde_json::json;

use crate::{
    constants::GXSERVER_PROTOCOL_HEADER,
    protocol::rpc_error,
    server::{json_response, routed_json, RoutedResponse},
};

use super::registry;

pub(crate) const RELAY_ROUTE_PREFIX: &str = "/api/computers/";
const RELAY_SEGMENT: &str = "/relay";
const RELAY_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// The computer and the request path of a relay URL, or `None` when the path is not one.
pub(crate) fn parse_relay_path(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix(RELAY_ROUTE_PREFIX)?;
    let (server_id, forwarded) = rest.split_once(RELAY_SEGMENT)?;
    (registry::is_server_id(server_id)
        && forwarded.starts_with("/api/")
        && !forwarded.contains(".."))
    .then_some((server_id, forwarded))
}

/// The relay base a CLI sends every request of that computer to.
pub(crate) fn relay_base_url(local_base_url: &str, server_id: &str) -> String {
    format!("{local_base_url}{RELAY_ROUTE_PREFIX}{server_id}{RELAY_SEGMENT}")
}

/// Forwards the request. The caller has already checked it carries this gxserver's own token: a
/// token another computer was given never relays onward, so access never chains past one hop.
pub(crate) async fn relay(
    method: Method,
    path: &str,
    headers: HeaderMap,
    body: Body,
    request_id: String,
) -> RoutedResponse {
    let endpoint_path = Some(RELAY_ROUTE_PREFIX.to_string());
    let Some((server_id, forwarded)) = parse_relay_path(path) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "notFound",
            format!("{path} is not a relay path."),
            request_id,
        );
    };
    let Some(computer) = registry::find(server_id) else {
        return refusal(
            StatusCode::NOT_FOUND,
            "computerNotConnected",
            "That computer isn't connected to this one. Connect it in Settings > Remote > Remote machines, then try again.".to_string(),
            request_id,
        );
    };
    if !computer.agent_access {
        let message = match computer.direction {
            super::registry::Direction::Outbound => format!(
                "Agent access to {name} is off. Turn on Agent access for {name} in Settings > Remote > Remote machines.",
                name = computer.name
            ),
            super::registry::Direction::Inbound => format!(
                "{name} has Agent access for this computer turned off. It is turned back on in {name}'s Settings > Remote > Remote machines.",
                name = computer.name
            ),
        };
        return refusal(StatusCode::FORBIDDEN, "agentAccessOff", message, request_id);
    }
    if method != Method::POST && !(method == Method::GET && forwarded == "/api/health/server") {
        return refusal(
            StatusCode::METHOD_NOT_ALLOWED,
            "methodNotAllowed",
            format!("{forwarded} requires POST."),
            request_id,
        );
    }
    let bytes = match to_bytes(body, RELAY_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return refusal(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payloadTooLarge",
                format!(
                    "A request to another computer can be at most {RELAY_BODY_LIMIT_BYTES} bytes."
                ),
                request_id,
            );
        }
    };
    let protocol = header_text(&headers, GXSERVER_PROTOCOL_HEADER);
    let content_type = header_text(&headers, header::CONTENT_TYPE.as_str());
    let url = format!("{}{forwarded}", computer.base_url);
    let token = computer.token().to_string();
    let name = computer.name.clone();
    let forwarded_request_id = request_id.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        forward(
            &method,
            &url,
            &token,
            protocol.as_deref(),
            content_type.as_deref(),
            &forwarded_request_id,
            bytes.to_vec(),
        )
    })
    .await;
    match outcome {
        Ok(Ok((status, content_type, body))) => {
            let mut response = Response::new(Body::from(body));
            *response.status_mut() =
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            if let Some(value) = content_type.and_then(|value| HeaderValue::from_str(&value).ok()) {
                response.headers_mut().insert(header::CONTENT_TYPE, value);
            }
            RoutedResponse {
                endpoint_path,
                response,
            }
        }
        _ => refusal(
            StatusCode::BAD_GATEWAY,
            "computerUnreachable",
            format!(
                "{name} didn't answer. Ghostex reconnects it on its own, so try again in a moment."
            ),
            request_id,
        ),
    }
}

fn forward(
    method: &Method,
    url: &str,
    token: &str,
    protocol: Option<&str>,
    content_type: Option<&str>,
    request_id: &str,
    body: Vec<u8>,
) -> Result<(u16, Option<String>, Vec<u8>), ureq::Error> {
    let agent = crate::http_agent::answering_agent();
    let authorization = format!("Bearer {token}");
    let response = if *method == Method::GET {
        let mut request = agent
            .get(url)
            .header("authorization", &authorization)
            .header("x-request-id", request_id);
        if let Some(protocol) = protocol {
            request = request.header(GXSERVER_PROTOCOL_HEADER, protocol);
        }
        request
            .config()
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .call()?
    } else {
        let mut request = agent
            .post(url)
            .header("authorization", &authorization)
            .header("x-request-id", request_id)
            .header("content-type", content_type.unwrap_or("application/json"));
        if let Some(protocol) = protocol {
            request = request.header(GXSERVER_PROTOCOL_HEADER, protocol);
        }
        // Long polls (`read-session-chat --wait-ms`) wait on the other computer, so the relay
        // leaves the time limit to the CLI that asked.
        request
            .config()
            .timeout_global(Some(Duration::from_secs(15 * 60)))
            .build()
            .send(&body[..])?
    };
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response.into_body().into_reader(), &mut bytes)
        .map_err(ureq::Error::Io)?;
    Ok((status, content_type, bytes))
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

pub(crate) fn refusal(
    status: StatusCode,
    error: &str,
    message: String,
    request_id: String,
) -> RoutedResponse {
    routed_json(
        Some(RELAY_ROUTE_PREFIX.to_string()),
        status,
        rpc_error(error, message, Some(request_id)),
    )
}

/// A refusal for a token another computer holds, written for the agent on that computer.
pub(crate) fn peer_refusal(error: &str, message: String, request_id: String) -> RoutedResponse {
    RoutedResponse {
        endpoint_path: None,
        response: json_response(
            StatusCode::FORBIDDEN,
            json!({
                "ok": false,
                "error": error,
                "message": message,
                "requestId": request_id,
            }),
        ),
    }
}
