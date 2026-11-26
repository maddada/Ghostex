//! `/api/remoteScreenStart`, `/api/remoteScreenStop` and `/api/remoteScreenStatus`.

use axum::http::StatusCode;
use serde_json::{json, Value};

use super::{gate, helper, proxy::REMOTE_SCREEN_PROXY_PREFIX};
use crate::{
    domain::params::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    security::Principal,
    server::{routed_json, AppState, RoutedResponse},
};

pub(crate) const REMOTE_SCREEN_START_ENDPOINT: &str = "/api/remoteScreenStart";
pub(crate) const REMOTE_SCREEN_STOP_ENDPOINT: &str = "/api/remoteScreenStop";
pub(crate) const REMOTE_SCREEN_STATUS_ENDPOINT: &str = "/api/remoteScreenStatus";

pub(crate) const REMOTE_SCREEN_ENDPOINTS: &[&str] = &[
    REMOTE_SCREEN_START_ENDPOINT,
    REMOTE_SCREEN_STOP_ENDPOINT,
    REMOTE_SCREEN_STATUS_ENDPOINT,
];

/// Who may start, stop or read a screen session.
///
/// CDXC:RemoteScreen 2026-11-26 WHY: the user's own app, a paired phone and this computer's own CLI may; a session's token, another computer's agents, Collab guests and extensions may not, since the screen shows and drives everything on this computer. Agents get it later through an approval, with the "ask once per device" prompt the user chose (Q5112).
fn may_use_screen(caller: &Principal) -> bool {
    matches!(
        caller,
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
    )
}

pub(crate) async fn handle_remote_screen_http(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let fail = |status, code: &str, message: String| {
        routed_json(
            Some(endpoint_path.clone()),
            status,
            rpc_error(code, message, Some(request_id.clone())),
        )
    };
    if !may_use_screen(caller) {
        return fail(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Only your own apps and paired phones can open this computer's screen.".to_string(),
        );
    }
    let enabled = gate::remote_screen_enabled(&state.paths);
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return fail(StatusCode::BAD_REQUEST, "badRequest", error.message),
    };
    let result = match endpoint_path.as_str() {
        REMOTE_SCREEN_STATUS_ENDPOINT => status_json(enabled),
        REMOTE_SCREEN_STOP_ENDPOINT => json!({ "stopped": helper::stop() }),
        REMOTE_SCREEN_START_ENDPOINT => {
            if !enabled {
                return fail(
                    StatusCode::FORBIDDEN,
                    "remoteScreenOff",
                    "Remote screen is off on this computer.".to_string(),
                );
            }
            let options = helper::StartOptions {
                view_only: params
                    .get("viewOnly")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                display: params
                    .get("display")
                    .and_then(|value| match value {
                        Value::String(text) => Some(text.trim().to_string()),
                        Value::Number(number) => Some(number.to_string()),
                        _ => None,
                    })
                    .filter(|text| {
                        !text.is_empty()
                            && text.len() <= 64
                            && text.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || b"\\.-_:".contains(&byte)
                            })
                    }),
            };
            match helper::start(&state.paths, options).await {
                Ok(info) => session_json(&info),
                Err(helper::StartError::HelperMissing) => {
                    return fail(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "remoteScreenHelperMissing",
                        "This computer's Ghostex has no screen helper.".to_string(),
                    )
                }
                Err(helper::StartError::Failed(message)) => {
                    return fail(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "remoteScreenFailed",
                        message,
                    )
                }
            }
        }
        _ => {
            return fail(
                StatusCode::NOT_FOUND,
                "notFound",
                "Unknown remote screen call.".to_string(),
            )
        }
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

fn session_json(info: &helper::ScreenSessionInfo) -> Value {
    json!({
        // The viewer page, relative to this gxserver; the ticket in it is the page's only key.
        "path": format!("{REMOTE_SCREEN_PROXY_PREFIX}{}/", info.ticket),
        "viewOnly": info.view_only,
        "startedAtMs": info.started_at_ms,
        "display": info.display,
    })
}

fn status_json(enabled: bool) -> Value {
    let session = helper::current();
    json!({
        "enabled": enabled,
        "helperInstalled": helper::helper_binary().is_some(),
        "running": session.is_some(),
        "viewOnly": session.as_ref().map(|info| info.view_only),
        "startedAtMs": session.as_ref().map(|info| info.started_at_ms),
    })
}
