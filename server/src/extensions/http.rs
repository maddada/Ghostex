use std::path::PathBuf;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};

use super::{ExtensionError, ExtensionResult, ExtensionStatePatch};

pub(crate) async fn handle_extensions_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return extension_error_response(
                endpoint_path,
                request_id,
                ExtensionError {
                    code: error.code,
                    message: error.message,
                },
            );
        }
    };
    let registry = state.extension_registry.clone();
    let operation_path = endpoint_path.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        dispatch_extension_operation(&registry, &operation_path, params)
    })
    .await
    .unwrap_or_else(|error| {
        Err(ExtensionError::internal(format!(
            "Extension operation task failed: {error}"
        )))
    });
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => extension_error_response(endpoint_path, request_id, error),
    }
}

fn dispatch_extension_operation(
    registry: &super::ExtensionRegistry,
    endpoint_path: &str,
    params: Map<String, Value>,
) -> ExtensionResult<Value> {
    match endpoint_path {
        "/api/listExtensions" => Ok(json!({ "extensions": registry.list()? })),
        "/api/extensionsCatalog" => serde_json::to_value(registry.catalog()?).map_err(|error| {
            ExtensionError::internal(format!("Could not encode catalog: {error}"))
        }),
        "/api/installExtension" => {
            let installed = install_extension(registry, &params)?;
            Ok(json!({ "extension": installed }))
        }
        "/api/uninstallExtension" => {
            let id = required_text(&params, "id")?;
            registry.uninstall(&id)?;
            Ok(json!({ "id": id, "uninstalled": true }))
        }
        "/api/updateExtensionState" => {
            let id = required_text(&params, "id")?;
            let patch: ExtensionStatePatch =
                serde_json::from_value(params.get("patch").cloned().unwrap_or_else(|| json!({})))
                    .map_err(|error| {
                    ExtensionError::bad_request(format!("Invalid extension state patch: {error}"))
                })?;
            Ok(json!({ "extension": registry.update_state(&id, patch)? }))
        }
        _ => Err(ExtensionError::not_found(format!(
            "Unknown extension endpoint: {endpoint_path}"
        ))),
    }
}

fn install_extension(
    registry: &super::ExtensionRegistry,
    params: &Map<String, Value>,
) -> ExtensionResult<super::InstalledExtension> {
    let local_path = optional_text(params, "localPath");
    let id = optional_text(params, "id");
    let url = optional_text(params, "url");
    let sha256 = optional_text(params, "sha256");
    match (local_path, id, url, sha256) {
        (Some(path), None, None, None) => registry.install_local(&PathBuf::from(path)),
        (None, Some(id), Some(url), Some(sha256)) => registry.install_zip(&id, &url, &sha256),
        (None, Some(id), None, None) => registry.install_from_catalog(&id),
        _ => Err(ExtensionError::bad_request(
            "installExtension requires localPath, id, or id + url + sha256.",
        )),
    }
}

fn required_text(params: &Map<String, Value>, key: &str) -> ExtensionResult<String> {
    optional_text(params, key)
        .ok_or_else(|| ExtensionError::bad_request(format!("Extension request requires {key}.")))
}

fn optional_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extension_error_response(
    endpoint_path: String,
    request_id: String,
    error: ExtensionError,
) -> RoutedResponse {
    let status = match error.code {
        "badRequest" => StatusCode::BAD_REQUEST,
        "notFound" => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(error.code, error.message, Some(request_id)),
    )
}
