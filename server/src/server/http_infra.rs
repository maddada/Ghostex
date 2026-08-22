use super::*;

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn broadcast_server_stopping(state: &AppState) {
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": "serverStopping",
    }));
}

pub(crate) fn value_text(value: &Value, key: &str) -> std::result::Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state(format!("{key} missing from gxserver response state."))
        })
}

pub(crate) fn create_authenticated_health(state: &AppState) -> ServerHealthResponse {
    let minimal = MinimalHealthResponse::new(&state.version);
    ServerHealthResponse {
        ok: minimal.ok,
        product: minimal.product,
        protocol_version: minimal.protocol_version,
        version: minimal.version,
        build_identity: state.build_identity.clone(),
        capabilities: GXSERVER_CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
        listeners: state.config.listeners.clone(),
        migration: state.migration.clone(),
        pid: state.metadata.pid,
        portless: read_portless_status_payload_for_paths(&state.paths),
        port: state.metadata.port,
        server_id: state.metadata.server_id.clone(),
        started_at: state.metadata.started_at.clone(),
        tools: get_gxserver_tool_statuses(),
    }
}

/*
CDXC:SessionChatImagePaste 2026-08-01:
saveSessionChatImage is the one endpoint whose JSON body legitimately
exceeds the general RPC limit (a pasted screenshot as base64), so the body
limit is per-endpoint instead of a single global constant.
*/
pub(crate) fn json_body_limit_bytes(endpoint_path: &str) -> usize {
    if endpoint_path == "/api/saveSessionChatImage" {
        crate::constants::GXSERVER_IMAGE_BODY_LIMIT_BYTES
    } else if endpoint_path == "/api/saveSessionChatAttachment" {
        crate::constants::GXSERVER_ATTACHMENT_BODY_LIMIT_BYTES
    } else if endpoint_path == "/api/runProjectDocsAction" {
        3 * 1024 * 1024
    } else {
        GXSERVER_JSON_BODY_LIMIT_BYTES
    }
}

pub(crate) async fn read_json_body(
    headers: &HeaderMap,
    body: Body,
    limit_bytes: usize,
) -> std::result::Result<Value, ReadBodyError> {
    if content_length(headers).map(|length| length > limit_bytes as u64) == Some(true) {
        return Err(ReadBodyError::TooLarge);
    }
    let bytes = to_bytes(body, limit_bytes + 1)
        .await
        .map_err(|_| ReadBodyError::TooLarge)?;
    if bytes.len() > limit_bytes {
        return Err(ReadBodyError::TooLarge);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| ReadBodyError::InvalidJson)?
        .trim();
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(text).map_err(|_| ReadBodyError::InvalidJson)
}

pub(crate) enum ReadBodyError {
    InvalidJson,
    TooLarge,
}

pub(crate) fn content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

pub(crate) fn read_protocol_version(headers: &HeaderMap, uri: &Uri, body: Option<&Value>) -> Option<Value> {
    if let Some(header) = headers
        .get(HeaderName::from_static(GXSERVER_PROTOCOL_HEADER))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(parse_protocol_version(header));
    }
    if let Some(value) = body.and_then(|body| body.get("protocolVersion")) {
        return Some(value.clone());
    }
    query_value(uri, "protocolVersion")
        .filter(|value| !value.is_empty())
        .map(|value| parse_protocol_version(&value))
}

pub(crate) fn parse_protocol_version(value: &str) -> Value {
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        value
            .parse::<u64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(value.to_string()))
    } else {
        Value::String(value.to_string())
    }
}

pub(crate) fn is_expected_protocol_version(value: Option<&Value>) -> bool {
    value.and_then(Value::as_u64) == Some(GXSERVER_PROTOCOL_VERSION)
}

pub(crate) fn query_value(uri: &Uri, key: &str) -> Option<String> {
    uri.query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.into_owned())
    })
}

pub(crate) fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

pub(crate) fn routed_json<T: serde::Serialize>(
    endpoint_path: Option<String>,
    status: StatusCode,
    body: T,
) -> RoutedResponse {
    RoutedResponse {
        endpoint_path,
        response: json_response(status, body),
    }
}

pub(crate) fn json_response<T: serde::Serialize>(status: StatusCode, body: T) -> Response<Body> {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| {
        format!(
            r#"{{"error":"internalError","message":"Failed to serialize gxserver response.","ok":false,"product":"{GXSERVER_PRODUCT}","protocolVersion":{GXSERVER_PROTOCOL_VERSION}}}"#
        )
    });
    let mut response = Response::new(Body::from(format!("{text}\n")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

pub(crate) fn apply_cors_headers(
    request_headers: &HeaderMap,
    response: &mut Response<Body>,
    config: &GxserverConfig,
) {
    let Some(origin) = request_headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return;
    };
    append_vary_header(response, "Origin");
    append_vary_header(response, "Access-Control-Request-Private-Network");
    let explicitly_allowed = config
        .cors
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin);
    if !explicitly_allowed && !is_loopback_web_origin(origin) {
        return;
    }
    if let Ok(value) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, x-gxserver-protocol-version, x-request-id",
        ),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    if request_headers
        .get("access-control-request-private-network")
        .and_then(|value| value.to_str().ok())
        == Some("true")
    {
        response.headers_mut().insert(
            "access-control-allow-private-network",
            HeaderValue::from_static("true"),
        );
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
}

pub(crate) fn is_loopback_web_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    ["127.0.0.1", "localhost", "[::1]"].iter().any(|host| {
        authority == *host
            || authority
                .strip_prefix(&format!("{host}:"))
                .is_some_and(|port| {
                    !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
                })
    })
}

pub(crate) fn append_vary_header(response: &mut Response<Body>, value: &str) {
    let existing = response
        .headers()
        .get(header::VARY)
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    let mut values = existing
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
    if let Ok(header_value) = HeaderValue::from_str(&values.join(", ")) {
        response.headers_mut().insert(header::VARY, header_value);
    }
}

pub(crate) async fn wait_for_process_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[allow(dead_code)]
pub(crate) fn _permission_name(permission: ApiPermission) -> &'static str {
    match permission {
        ApiPermission::FullLocal => "fullLocal",
        ApiPermission::RemoteAllowed => "remoteAllowed",
        ApiPermission::RemoteBlocked => "remoteBlocked",
    }
}
