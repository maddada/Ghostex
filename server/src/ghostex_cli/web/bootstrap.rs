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
        let label = local_machine_label().map_err(anyhow::Error::msg)?;
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

#[cfg(unix)]
pub(crate) fn local_machine_label() -> std::result::Result<String, String> {
    let mut buffer = [0_u8; 256];
    let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if status != 0 {
        return Err("Failed to read the local machine hostname.".to_string());
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| "The local machine hostname is too long.".to_string())?;
    let hostname = std::str::from_utf8(&buffer[..length])
        .map_err(|_| "The local machine hostname is not valid UTF-8.".to_string())?;
    if hostname.is_empty() {
        return Err("The local machine hostname is empty.".to_string());
    }
    Ok(hostname.to_string())
}

#[cfg(windows)]
pub(crate) fn local_machine_label() -> std::result::Result<String, String> {
    use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

    let mut buffer = [0_u16; 256];
    let mut length = buffer.len() as u32;
    if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &mut length) } == 0 {
        return Err("Failed to read the local machine hostname.".to_string());
    }
    let hostname = String::from_utf16(&buffer[..length as usize])
        .map_err(|_| "The local machine hostname is not valid UTF-16.".to_string())?;
    if hostname.is_empty() {
        return Err("The local machine hostname is empty.".to_string());
    }
    Ok(hostname)
}
