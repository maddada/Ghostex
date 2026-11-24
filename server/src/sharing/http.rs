//! HTTP RPC handler for `/api/sharing`.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! Hard security rule: creating or managing share links is always an explicit user action from
//! Ghostex on the user's computer or paired phone (`Principal::is_owner_device()`). AI agent
//! session tokens, Collab guests, extensions, connected-computer peers, and anonymous callers are
//! refused with 403 Forbidden.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::Value;

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    security::Principal,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::config::ShareExpiry;

pub(crate) const SHARING_ENDPOINT: &str = "/api/sharing";
pub(crate) const SHARING_ENDPOINTS: &[&str] = &[SHARING_ENDPOINT];

pub(crate) async fn handle_sharing_http(
    state: Arc<AppState>,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    if !caller.is_owner_device() {
        return routed_json(
            Some(endpoint_path),
            StatusCode::FORBIDDEN,
            rpc_error(
                "forbidden",
                "Only you, from Ghostex on your computer or phone, can manage share links.",
                Some(request_id),
            ),
        );
    }

    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let op = params.get("op").and_then(Value::as_str).unwrap_or("read");

    match op {
        "read" | "state" => {
            let status = state.share_manager.read_status().await;
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, status),
            )
        }
        "setConfig" => {
            let patch = params
                .get("patch")
                .cloned()
                .unwrap_or_else(|| Value::Object(params.clone()));
            match state.share_manager.update_config(&state, &patch).await {
                Ok(status) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, status),
                ),
                Err(message) => routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("invalidSharingConfig", message, Some(request_id)),
                ),
            }
        }
        "createPage" => {
            let Some(page_id) = params
                .get("pageId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error(
                        "badRequest",
                        "createPage requires pageId.",
                        Some(request_id),
                    ),
                );
            };
            let expiry = match params.get("expiry").and_then(Value::as_str) {
                Some(raw) => match ShareExpiry::parse(raw) {
                    Some(parsed) => Some(parsed),
                    None => {
                        return routed_json(
                            Some(endpoint_path),
                            StatusCode::BAD_REQUEST,
                            rpc_error(
                                "badRequest",
                                "Invalid expiry. Use 1h, 24h, 7d, or untilStopped.",
                                Some(request_id),
                            ),
                        );
                    }
                },
                None => None,
            };
            let require_passcode = params
                .get("requirePasscode")
                .or_else(|| params.get("passcode"))
                .and_then(Value::as_bool);
            #[cfg(debug_assertions)]
            let debug_expires_in_ms = params.get("debugExpiresInMs").and_then(Value::as_i64);
            #[cfg(not(debug_assertions))]
            let debug_expires_in_ms: Option<i64> = None;
            match state
                .share_manager
                .create_page_share(
                    state.clone(),
                    page_id,
                    expiry,
                    require_passcode,
                    debug_expires_in_ms,
                )
                .await
            {
                Ok(result) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(message) => routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("shareFailed", message, Some(request_id)),
                ),
            }
        }
        "createPort" => {
            let Some(port) = params
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
            else {
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error(
                        "badRequest",
                        "createPort requires a valid port number.",
                        Some(request_id),
                    ),
                );
            };
            let title = params
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string);
            let expiry = match params.get("expiry").and_then(Value::as_str) {
                Some(raw) => match ShareExpiry::parse(raw) {
                    Some(parsed) => Some(parsed),
                    None => {
                        return routed_json(
                            Some(endpoint_path),
                            StatusCode::BAD_REQUEST,
                            rpc_error(
                                "badRequest",
                                "Invalid expiry. Use 1h, 24h, 7d, or untilStopped.",
                                Some(request_id),
                            ),
                        );
                    }
                },
                None => None,
            };
            let require_passcode = params
                .get("requirePasscode")
                .or_else(|| params.get("passcode"))
                .and_then(Value::as_bool);
            match state
                .share_manager
                .create_port_share(state.clone(), port, title, expiry, require_passcode)
                .await
            {
                Ok(result) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, result),
                ),
                Err(message) => routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("shareFailed", message, Some(request_id)),
                ),
            }
        }
        "stop" => {
            let Some(share_id) = params
                .get("shareId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("badRequest", "stop requires shareId.", Some(request_id)),
                );
            };
            match state.share_manager.stop_share(&state, share_id).await {
                Ok(status) => routed_json(
                    Some(endpoint_path),
                    StatusCode::OK,
                    rpc_success(request_id, status),
                ),
                Err(message) => routed_json(
                    Some(endpoint_path),
                    StatusCode::BAD_REQUEST,
                    rpc_error("shareFailed", message, Some(request_id)),
                ),
            }
        }
        "stopAll" => {
            let status = state.share_manager.stop_all(Some(&state)).await;
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, status),
            )
        }
        other => routed_json(
            Some(endpoint_path),
            StatusCode::BAD_REQUEST,
            rpc_error(
                "badRequest",
                format!("Unknown sharing operation '{other}'."),
                Some(request_id),
            ),
        ),
    }
}
