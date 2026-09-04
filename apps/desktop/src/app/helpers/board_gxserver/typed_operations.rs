// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the low-level gxserver typed-operation HTTP transport (POST/
// GET over a raw TCP socket), agent-settings reconciliation, sidebar
// bootstrap, auth token reads, and HTTP response parsing helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn parse_gpui_gxserver_rpc_result(body: &str) -> Result<serde_json::Value, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid RPC response.".to_string())?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || value.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
        || value
            .get("protocolVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(GPUI_GXSERVER_PROTOCOL_VERSION)
    {
        return Err("gxserver returned an invalid RPC response.".to_string());
    }
    value
        .get("result")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .map(serde_json::Value::Object)
        .ok_or_else(|| "gxserver returned an invalid RPC result.".to_string())
}

pub(crate) fn gxserver_post_typed_operation(
    path: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> Result<(u16, String), String> {
    /*
    CDXC:ProjectBoard 2026-06-24-11:03:
    GPUI Kanban CEF parity must use the existing gxserver typed-operation boundary for Beads work. Send the same protocol-version envelope and bearer token as the native bridge to localhost only, with no bd subprocess execution, remote fallback, raw request logging, response logging, URL/title inspection, or persisted board payloads.

    CDXC:AgentProviders 2026-06-24-11:39:
    Settings fan-out also uses this narrow local gxserver RPC helper so agent-policy saves share the existing bearer-token path and response-body parser. Keep callers endpoint-specific and privacy-safe; this helper must not log tokens, URLs, settings payloads, project paths, command text, stdout/stderr, or daemon responses.

    CDXC:AgentProviders 2026-06-24-12:14:
    Startup/open-time agent-policy hydration also uses this helper for `/api/readAgentSettings` so GPUI does not invent a second daemon client or bypass the same token, localhost, timeout, protocol-header, and response-sanitization boundary.

    CDXC:StatusPet 2026-06-24-11:40:
    Settings status/action parity reuses this helper for hook status/install/uninstall and Portless state updates. Keep the helper transport-only: callers own endpoint allowlists, result validation, explicit UI error messages, and no persistent logging.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid gxserver API path.".to_string());
    }
    let token = read_gpui_gxserver_auth_token()?;
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}");
    let mut stream = TcpStream::connect(&address)
        .map_err(|_| "gxserver is not reachable on 127.0.0.1:58744.".to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver write timeout.".to_string())?;

    let body = serde_json::json!({
        "protocolVersion": GPUI_GXSERVER_PROTOCOL_VERSION,
        "params": params,
    })
    .to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|_| "Could not send gxserver request.".to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|_| "Could not read gxserver response.".to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

pub(crate) fn gxserver_get_typed_operation(
    path: &str,
    timeout: Duration,
) -> Result<(u16, String), String> {
    /*
    CDXC:StatusPet 2026-06-24-11:40:
    GPUI Settings reads gxserver health through the same localhost/token/protocol boundary as typed POST operations. Health reads must stay GET-only, short-timeout, response-unlogged, and limited to `/api/` paths so Portless HUD hydration does not introduce a second daemon client.
    */
    if !path.starts_with("/api/") {
        return Err("Invalid gxserver API path.".to_string());
    }
    let token = read_gpui_gxserver_auth_token()?;
    let address = format!("{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}");
    let mut stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], GPUI_GXSERVER_LOCAL_API_PORT)),
        timeout,
    )
    .map_err(|error| format!("gxserver connection to {address} failed: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver read timeout.".to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|_| "Could not configure gxserver write timeout.".to_string())?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n{GPUI_GXSERVER_PROTOCOL_HEADER}: {GPUI_GXSERVER_PROTOCOL_VERSION}\r\n\r\n",
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("Could not send gxserver health request: {error}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("Could not read gxserver health response: {error}"))?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "gxserver returned an invalid HTTP response.".to_string())?;
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "gxserver returned an invalid HTTP status.".to_string())?;
    Ok((status_code, gxserver_http_response_body(headers, body)?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiGxserverReadAgentSettingsResult {
    pub(crate) is_persisted: bool,
    pub(crate) settings: shared_settings::SharedGxserverAgentSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiGxserverAgentSettingsReconciliationAction {
    SeedLocal(shared_settings::SharedGxserverAgentSettings),
    ApplyCanonical(shared_settings::SharedGxserverAgentSettings),
    Noop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiGxserverAgentSettingsHydrationResult {
    pub(crate) expected_local_settings: shared_settings::SharedGxserverAgentSettings,
    pub(crate) canonical_settings: shared_settings::SharedGxserverAgentSettings,
}

pub(crate) fn gpui_gxserver_agent_settings_reconciliation_action(
    local_settings: &shared_settings::SharedGxserverAgentSettings,
    daemon_result: &GpuiGxserverReadAgentSettingsResult,
) -> GpuiGxserverAgentSettingsReconciliationAction {
    if !daemon_result.is_persisted {
        return GpuiGxserverAgentSettingsReconciliationAction::SeedLocal(local_settings.clone());
    }
    if daemon_result.settings != *local_settings {
        return GpuiGxserverAgentSettingsReconciliationAction::ApplyCanonical(
            daemon_result.settings.clone(),
        );
    }
    GpuiGxserverAgentSettingsReconciliationAction::Noop
}

pub(crate) fn reconcile_gpui_gxserver_agent_settings_with_daemon()
-> Result<Option<GpuiGxserverAgentSettingsHydrationResult>, String> {
    let daemon_agent_settings = read_gpui_gxserver_agent_settings()?;
    let local_agent_settings =
        shared_settings::shared_sidebar_settings_snapshot().gxserver_agent_settings();
    match gpui_gxserver_agent_settings_reconciliation_action(
        &local_agent_settings,
        &daemon_agent_settings,
    ) {
        GpuiGxserverAgentSettingsReconciliationAction::SeedLocal(settings) => {
            let expected_local_settings = settings.clone();
            update_gpui_gxserver_agent_settings(&settings).map(|canonical_settings| {
                Some(GpuiGxserverAgentSettingsHydrationResult {
                    expected_local_settings,
                    canonical_settings,
                })
            })
        }
        GpuiGxserverAgentSettingsReconciliationAction::ApplyCanonical(settings) => {
            Ok(Some(GpuiGxserverAgentSettingsHydrationResult {
                expected_local_settings: local_agent_settings,
                canonical_settings: settings,
            }))
        }
        GpuiGxserverAgentSettingsReconciliationAction::Noop => Ok(None),
    }
}

pub(crate) fn read_gpui_gxserver_agent_settings()
-> Result<GpuiGxserverReadAgentSettingsResult, String> {
    /*
    CDXC:AgentProviders 2026-06-24-12:14:
    Startup/open-time reconciliation reads the daemon's canonical agent-settings row through the same typed localhost RPC envelope as save-time sync. Parse only `isPersisted`, `agentAcceptAllEnabled`, and `defaultPromptAgentId`; failures remain local `Result` values and never create fallback daemon state or leak raw gxserver responses.
    */
    let (status_code, body) = gxserver_post_typed_operation(
        "/api/readAgentSettings",
        &serde_json::json!({}),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err("gxserver agent settings read failed.".to_string());
    }
    parse_gpui_gxserver_read_agent_settings_response(&body)
}

pub(crate) fn update_gpui_gxserver_agent_settings(
    settings: &shared_settings::SharedGxserverAgentSettings,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    /*
    CDXC:AgentProviders 2026-06-24-11:39:
    GPUI Settings saves use the same local gxserver HTTP/token path as other GPUI gxserver calls. The request is the macOS-compatible `/api/updateAgentSettings` RPC envelope with only Accept All and Default Prompt Agent values; failures stay as local `Result` values so callers can silently preserve the render cache without private logs or fake daemon state.
    */
    let mut params = serde_json::Map::new();
    settings.write_to_settings_object(&mut params);
    let (status_code, body) = gxserver_post_typed_operation(
        "/api/updateAgentSettings",
        &serde_json::Value::Object(params),
        Duration::from_secs(5),
    )?;
    if !(200..300).contains(&status_code) {
        return Err("gxserver agent settings request failed.".to_string());
    }
    parse_gpui_gxserver_agent_settings_response(&body)
}

pub(crate) fn parse_gpui_gxserver_agent_settings_response(
    body: &str,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    let result = parse_gpui_gxserver_agent_settings_rpc_result(body)?;
    parse_gpui_gxserver_agent_settings_from_result(&result)
}

pub(crate) fn parse_gpui_gxserver_read_agent_settings_response(
    body: &str,
) -> Result<GpuiGxserverReadAgentSettingsResult, String> {
    let result = parse_gpui_gxserver_agent_settings_rpc_result(body)?;
    let is_persisted = result
        .get("isPersisted")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let settings = parse_gpui_gxserver_agent_settings_from_result(&result)?;
    Ok(GpuiGxserverReadAgentSettingsResult {
        is_persisted,
        settings,
    })
}

pub(crate) fn parse_gpui_gxserver_agent_settings_rpc_result(
    body: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let value = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid agent settings response.".to_string())?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(true)
        || value.get("product").and_then(serde_json::Value::as_str) != Some(GPUI_GXSERVER_PRODUCT)
        || value
            .get("protocolVersion")
            .and_then(serde_json::Value::as_u64)
            != Some(GPUI_GXSERVER_PROTOCOL_VERSION)
    {
        return Err("gxserver returned an invalid agent settings response.".to_string());
    }
    value
        .get("result")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())
}

pub(crate) fn parse_gpui_gxserver_agent_settings_from_result(
    result: &serde_json::Map<String, serde_json::Value>,
) -> Result<shared_settings::SharedGxserverAgentSettings, String> {
    let settings = result
        .get("settings")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let agent_accept_all_enabled = settings
        .get("agentAcceptAllEnabled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    let default_prompt_agent_id = settings
        .get("defaultPromptAgentId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "gxserver returned an invalid agent settings response.".to_string())?;
    Ok(shared_settings::SharedGxserverAgentSettings::new(
        agent_accept_all_enabled,
        default_prompt_agent_id,
    ))
}

pub(crate) fn gpui_sidebar_gxserver_bootstrap(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
    focus_state: &GpuiGxserverPresentationFocusState,
    local_focus_key: Option<&GpuiLocalWorkspaceSessionKey>,
) -> Option<cef::SidebarGxserverBootstrap> {
    /*
    CDXC:ServerDaemon 2026-06-24-11:17:
    Build the GPUI sidebar bootstrap only from real local gxserver facts: the selected loopback API port, the existing auth-token helper, protocol version 1, a stable GPUI sidebar client id, and the explicit active project id already stored from the live sidebar snapshot. Do not infer optional session ids from project paths, titles, shell terminal ids, Browser tabs, fixtures, or fallback state.

    CDXC:ServerDaemon 2026-06-24-13:34 (revised 2026-08-07):
    `initialActiveProjectId` may be supplied only from the validated latest sidebar active-project snapshot, the exact local workspace-focus key whose raw session matches `focusedSessionId`, or the contract-validated persisted focus state's own `activeProjectId` (cold-start replay of the last active workspace project). This helper must not query gxserver project lists or log project identity.

    CDXC:FocusRouting 2026-06-24-21:07:
    `focusedSessionId` and `visibleSessionIds` may be supplied only from the separate GPUI focus state that has already accepted real gxserver presentation ids. Local ids remain raw daemon session ids; remote ids remain machine-scoped sidebar ids so bootstrap replay is collision-safe.

    CDXC:FocusRouting 2026-06-27-13:25:
    A GPUI sidebar local-session click carries the authoritative project/session pair through the fixed workspace-focus bridge. If that project matches the stored focused session exactly, prefer it over the latest active-project snapshot so a second click cannot bootstrap the current session under a stale project and render an empty selection.
    */
    let initial_active_project_id = local_focus_key
        .filter(|key| focus_state.focused_session_id.as_deref() == Some(key.session_id.as_str()))
        .map(|key| key.project_id.as_str())
        .or_else(|| gpui_active_project_id_from_snapshot(latest_snapshot))
        .or(focus_state.active_project_id.as_deref())
        .map(str::to_string);
    Some(cef::SidebarGxserverBootstrap {
        base_url: format!("http://{GPUI_GXSERVER_LOCAL_API_HOST}:{GPUI_GXSERVER_LOCAL_API_PORT}"),
        auth_token: read_gpui_gxserver_auth_token().ok()?,
        protocol_version: GPUI_GXSERVER_PROTOCOL_VERSION as i32,
        client_id: GPUI_SIDEBAR_GXSERVER_CLIENT_ID.to_string(),
        initial_active_project_id,
        focused_session_id: focus_state.focused_session_id.clone(),
        visible_session_ids: focus_state.visible_session_ids.clone(),
    })
}

pub(crate) fn read_gpui_gxserver_auth_token() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return windows_terminal_backend::auth_token()
            .ok_or_else(|| "gxserver auth token is unavailable.".to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let token_path = shared_settings::ghostex_storage_paths()
            .gxserver_state_dir()
            .join("auth")
            .join("token");
        let token = fs::read_to_string(token_path)
            .map_err(|error| format!("gxserver auth token could not be read: {error}"))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("gxserver auth token is empty.".to_string());
        }
        Ok(token)
    }
}

pub(crate) fn gxserver_http_response_body(headers: &str, body: &str) -> Result<String, String> {
    if !headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
    }) {
        return Ok(body.to_string());
    }
    gxserver_decode_chunked_http_body(body)
}

pub(crate) fn gxserver_decode_chunked_http_body(body: &str) -> Result<String, String> {
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut output = Vec::new();
    loop {
        let line_end = gxserver_find_crlf(bytes, index)
            .ok_or_else(|| "Invalid chunked gxserver response.".to_string())?;
        let size_text = std::str::from_utf8(&bytes[index..line_end])
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| "Invalid chunked gxserver response size.".to_string())?;
        index = line_end + 2;
        if size == 0 {
            return String::from_utf8(output)
                .map_err(|_| "Invalid UTF-8 in gxserver response.".to_string());
        }
        let chunk_end = index
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "Invalid chunked gxserver response.".to_string())?;
        output.extend_from_slice(&bytes[index..chunk_end]);
        index = chunk_end;
        if bytes.get(index..index + 2) != Some(b"\r\n") {
            return Err("Invalid chunked gxserver response.".to_string());
        }
        index += 2;
    }
}

pub(crate) fn gxserver_find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}
