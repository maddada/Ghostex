//! Settings > Secrets routes: list, add or change, and remove providers, and check that a reference
//! can be read (the answer says only whether it worked, never the value).

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use super::store::{self, SecretReference};
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

pub(crate) const SECRET_PROVIDER_ENDPOINTS: &[&str] = &[
    "/api/readSecretProviders",
    "/api/saveSecretProvider",
    "/api/removeSecretProvider",
    "/api/checkSecretReference",
];

fn text(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn run(
    state: &AppState,
    path: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let paths = &state.paths;
    match path {
        "/api/readSecretProviders" => Ok(store::read_for_settings(paths)),
        "/api/saveSecretProvider" => {
            let id = text(params, "id");
            let kind = match text(params, "kind") {
                kind if kind.is_empty() => store::KIND_ONE_PASSWORD.to_string(),
                kind => kind,
            };
            let provider = store::save(
                paths,
                (!id.is_empty()).then_some(id.as_str()),
                &kind,
                &text(params, "label"),
                &text(params, "account"),
            )
            .map_err(DomainStateError::bad_request)?;
            Ok(json!({ "ok": true, "provider": provider }))
        }
        "/api/removeSecretProvider" => {
            let removed =
                store::remove(paths, &text(params, "id")).map_err(DomainStateError::bad_request)?;
            Ok(json!({ "ok": true, "removed": removed }))
        }
        "/api/checkSecretReference" => {
            let reference = SecretReference {
                provider: text(params, "provider"),
                reference: text(params, "reference"),
            };
            // Read and dropped at once: the caller learns only whether it worked.
            Ok(match store::resolve(paths, &reference) {
                Ok(value) => {
                    drop(value);
                    json!({ "ok": true, "readable": true })
                }
                Err(message) => json!({ "ok": true, "readable": false, "message": message }),
            })
        }
        _ => Err(DomainStateError::not_found(
            "Unknown secret provider endpoint.",
        )),
    }
}

pub(crate) async fn handle_secret_providers_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let worker_state = state.clone();
    let path = endpoint_path.clone();
    // Off the async runtime: `op` may wait for the user to unlock 1Password.
    let result = tokio::task::spawn_blocking(move || run(&worker_state, &path, &params))
        .await
        .unwrap_or_else(|_| {
            Err(DomainStateError::corrupt_state(
                "Secret providers stopped unexpectedly.",
            ))
        });
    match result {
        Ok(value) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
