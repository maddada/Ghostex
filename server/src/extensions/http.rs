use std::path::PathBuf;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};

use super::{ExtensionError, ExtensionLaunchContext, ExtensionResult, ExtensionStatePatch};

pub(crate) async fn handle_extensions_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    token_extension_id: Option<String>,
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
    /*
    CDXC:Telemetry 2026-08-26:
    Store-vs-local is only knowable from the REQUEST — `install_local`,
    `install_from_catalog`, and `install_zip` converge on one `activate_install`
    that has already lost the provenance — so it is resolved here, before the
    params move onto the blocking worker. The extension's ID is deliberately not
    read: extension ids are not ours to report.
    */
    let install_source = extension_install_source(&endpoint_path, &params);
    let registry = state.extension_registry.clone();
    let operation_path = endpoint_path.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        dispatch_extension_operation(
            &registry,
            &operation_path,
            params,
            token_extension_id.as_deref(),
        )
    })
    .await
    .unwrap_or_else(|error| {
        Err(ExtensionError::internal(format!(
            "Extension operation task failed: {error}"
        )))
    });
    match outcome {
        Ok(result) => {
            // Only a SUCCEEDED lifecycle change is counted; a rejected install
            // is not an install.
            match endpoint_path.as_str() {
                "/api/installExtension" => {
                    if let Some(source) = install_source {
                        crate::telemetry::extension_installed(source);
                    }
                }
                "/api/uninstallExtension" => crate::telemetry::extension_uninstalled(),
                _ => {}
            }
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, result),
            )
        }
        Err(error) => extension_error_response(endpoint_path, request_id, error),
    }
}

/// Which install route this request asks for, mirroring the `match` in
/// [`install_extension`]. A sideloaded directory is `local`; a catalog id, with
/// or without an explicit signed URL, is `store`.
fn extension_install_source(
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<&'static str> {
    if endpoint_path != "/api/installExtension" {
        return None;
    }
    if params.contains_key("localPath") {
        return Some("local");
    }
    params.contains_key("id").then_some("store")
}

fn dispatch_extension_operation(
    registry: &super::ExtensionRegistry,
    endpoint_path: &str,
    params: Map<String, Value>,
    token_extension_id: Option<&str>,
) -> ExtensionResult<Value> {
    match endpoint_path {
        "/api/listExtensions" => {
            let mut extensions = registry.list()?;
            if let Some(id) = token_extension_id {
                extensions.retain(|extension| extension.id == id);
            }
            Ok(json!({ "extensions": extensions }))
        }
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
        "/api/startExtension" => {
            reject_extension_token(token_extension_id)?;
            let id = required_text(&params, "id")?;
            let context: ExtensionLaunchContext =
                serde_json::from_value(params.get("context").cloned().unwrap_or_else(|| json!({})))
                    .map_err(|error| {
                        ExtensionError::bad_request(format!(
                            "Invalid extension launch context: {error}"
                        ))
                    })?;
            Ok(json!({ "status": registry.start(&id, context)? }))
        }
        "/api/stopExtension" => {
            reject_extension_token(token_extension_id)?;
            let id = required_text(&params, "id")?;
            Ok(json!({ "status": registry.stop(&id)? }))
        }
        "/api/extensionStatus" => {
            let id = authorized_extension_id(&params, token_extension_id)?;
            Ok(json!({ "status": registry.status(&id)? }))
        }
        "/api/extensionBadge" => {
            let id = authorized_extension_id(&params, token_extension_id)?;
            let lines = params
                .get("lines")
                .and_then(Value::as_array)
                .ok_or_else(|| ExtensionError::bad_request("Extension badge requires lines."))?
                .iter()
                .map(|line| {
                    line.as_str().map(str::to_string).ok_or_else(|| {
                        ExtensionError::bad_request("Extension badge lines must be strings.")
                    })
                })
                .collect::<ExtensionResult<Vec<_>>>()?;
            Ok(json!({ "id": id, "badge": registry.set_badge(&id, lines)? }))
        }
        _ => Err(ExtensionError::not_found(format!(
            "Unknown extension endpoint: {endpoint_path}"
        ))),
    }
}

fn authorized_extension_id(
    params: &Map<String, Value>,
    token_extension_id: Option<&str>,
) -> ExtensionResult<String> {
    let requested = optional_text(params, "id");
    match (token_extension_id, requested) {
        (Some(authorized), Some(requested)) if authorized != requested => Err(
            ExtensionError::bad_request("Extension API token cannot act on another extension."),
        ),
        (Some(authorized), _) => Ok(authorized.to_string()),
        (None, Some(requested)) => Ok(requested),
        (None, None) => Err(ExtensionError::bad_request(
            "Extension request requires id.",
        )),
    }
}

fn reject_extension_token(token_extension_id: Option<&str>) -> ExtensionResult<()> {
    if token_extension_id.is_some() {
        Err(ExtensionError::bad_request(
            "Extension API tokens cannot start or stop extension processes.",
        ))
    } else {
        Ok(())
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
