//! `/api/securityLog`: the newest entries and the chain check, for Settings > Security and
//! `ghostex security log`.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::protocol::{rpc_error, rpc_success};
use crate::server::{routed_json, RoutedResponse};

pub(crate) const SECURITY_LOG_ENDPOINT: &str = "/api/securityLog";
const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 5_000;

/// Params: `limit` (newest entries to return, 100 by default). Answers `entries` newest first,
/// `check` (`intact`, `entries`, `problem`) over the whole file, and the file's `path`.
pub(crate) async fn handle_security_log_http(
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let limit = body
        .pointer("/params/limit")
        .and_then(Value::as_u64)
        .map(|limit| (limit as usize).min(MAX_LIMIT))
        .unwrap_or(DEFAULT_LIMIT);
    let outcome = tokio::task::spawn_blocking(move || {
        let log = super::live_log();
        json!({
            "entries": log.read_recent(limit),
            "check": log.check(),
            "path": log.path().display().to_string(),
        })
    })
    .await;
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            StatusCode::INTERNAL_SERVER_ERROR,
            rpc_error(
                "internalError",
                format!("The security log could not be read: {error}"),
                Some(request_id),
            ),
        ),
    }
}
