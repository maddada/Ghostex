use crate::{
    auth::read_gxserver_auth_token,
    constants::GXSERVER_PROTOCOL_VERSION,
    paths::get_gxserver_paths,
    protocol::{rpc_error, rpc_success},
};
use axum::{
    body::Body,
    http::{header, HeaderMap, Response, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;

pub(super) fn bootstrap(headers: &HeaderMap, api_url: &str) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let result = (|| -> anyhow::Result<_> {
        let auth = read_gxserver_auth_token(&get_gxserver_paths(None))?
            .ok_or_else(|| anyhow::anyhow!("Start gxserver before connecting Ghostex Web."))?;
        // The computer's name as it goes by everywhere, which the page names it by.
        let label = crate::connected_computers::this_computer_name();
        Ok((auth.token, label))
    })();
    let (token, label) = match result {
        Ok(result) => result,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(rpc_error(
                    "unavailable",
                    error.to_string(),
                    Some(request_id),
                )),
            )
                .into_response()
        }
    };
    // CDXC:Telemetry 2026-09-06 WHY:
    // The web handshake now runs in a separate process, so forward its hello to gxserver's existing telemetry endpoint to preserve the daemon's opt-out gate and throttling.
    let client_os = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(crate::telemetry::client_platform::platform_from_user_agent);
    let telemetry_url = format!("{api_url}/api/recordClientEvent");
    let telemetry_token = token.clone();
    tokio::task::spawn_blocking(move || {
        if ghostex_paths::loopback_owner::check_url(&telemetry_url).is_err() {
            return;
        }
        let _ = crate::http_agent::agent()
            .post(&telemetry_url)
            .header("Authorization", format!("Bearer {telemetry_token}"))
            .header(
                "x-gxserver-protocol-version",
                GXSERVER_PROTOCOL_VERSION.to_string(),
            )
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(2)))
            .build()
            .send_json(json!({
                "event": "client.connected",
                "properties": { "client": "web", "client_os": client_os },
            }));
    });
    // CDXC:VoiceMode 2026-10-22 WHY: the page has no read of this computer's settings (apps/gpui-web/README.md), and Read aloud in a browser follows the switch of the computer serving the page (contract section 2), so the handshake names the built-in features that are on here: the same list the phone polls (`builtInFeatures`, sessions/mobile_summary.rs).
    let mut response = Json(rpc_success(
        request_id,
        json!({
            "authToken": token,
            "baseUrl": api_url,
            "builtInFeatures": crate::ghostex_cli::built_in_extensions::enabled_built_in_features(),
            "machineLabel": label,
            "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        }),
    ))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
