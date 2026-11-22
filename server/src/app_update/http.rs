//! `/api/appUpdateStatus`, `/api/appUpdateCheck`, `/api/appUpdateInstall`, `/api/appUpdateSetAuto`.
use axum::http::StatusCode;
use serde_json::{Map, Value};

use super::record::{now_iso, read_settled_record, write_record, PendingInstall};
use super::status::{ask_app, compose_status, AppAnswer};
use crate::constants::GXSERVER_VERSION;
use crate::domain::read_domain_rpc_params;
use crate::protocol::{rpc_error, rpc_success};
use crate::server::{routed_json, AppState, RoutedResponse};

pub(crate) async fn handle_app_update_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return failure(endpoint_path, request_id, "badRequest", error.message),
    };
    let result = match endpoint_path.as_str() {
        "/api/appUpdateStatus" => Ok(compose_status(state, &ask_app(state, "status").await)),
        "/api/appUpdateCheck" => Ok(compose_status(state, &ask_app(state, "check").await)),
        "/api/appUpdateInstall" => start_install(state, "request").await,
        "/api/appUpdateSetAuto" => set_auto(state, &params).await,
        _ => Err(("badRequest", "Unknown app update endpoint.".to_string())),
    };
    match result {
        Ok(status) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, serde_json::json!({ "status": status })),
        ),
        Err((code, message)) => failure(endpoint_path, request_id, code, message),
    }
}

fn failure(
    endpoint_path: String,
    request_id: String,
    code: &'static str,
    message: String,
) -> RoutedResponse {
    let status = if code == "badRequest" {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::CONFLICT
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(code, message, Some(request_id)),
    )
}

/// Asks the app to download and install the update and records the install, so the gxserver the new build starts can report how it ended.
pub(super) async fn start_install(
    state: &AppState,
    source: &str,
) -> Result<Value, (&'static str, String)> {
    let record = read_settled_record(&state.paths);
    if record.pending_install.is_some() {
        // A second press while the first install runs only reads its progress.
        return Ok(compose_status(state, &ask_app(state, "status").await));
    }
    let answer = ask_app(state, "install").await;
    match &answer {
        AppAnswer::NotRunning => {
            return Err((
                "appNotRunning",
                "Ghostex is not open on this computer. Open it there (signed in to its desktop) and try again."
                    .to_string(),
            ));
        }
        AppAnswer::Refused(message) => return Err(("updateRefused", message.clone())),
        // The app says why it cannot install: no update, an updater this build lacks, a dialog in the way.
        AppAnswer::Report(report)
            if report.get("accepted").and_then(Value::as_bool) == Some(false) =>
        {
            return Err((
                "updateRefused",
                report
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Ghostex could not start the update.")
                    .to_string(),
            ));
        }
        AppAnswer::Report(report) => {
            let mut record = read_settled_record(&state.paths);
            record.pending_install = Some(PendingInstall {
                from_version: GXSERVER_VERSION.to_string(),
                target_version: report
                    .get("availableVersion")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                requested_at: now_iso(),
                source: source.to_string(),
            });
            record.last_result = None;
            write_record(&state.paths, &record);
        }
    }
    Ok(compose_status(state, &answer))
}

async fn set_auto(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, (&'static str, String)> {
    let Some(enabled) = params.get("enabled").and_then(Value::as_bool) else {
        return Err(("badRequest", "enabled must be true or false.".to_string()));
    };
    let mut record = read_settled_record(&state.paths);
    record.auto_update = enabled;
    record.last_auto_attempt = None;
    write_record(&state.paths, &record);
    if enabled {
        super::auto::wake_auto_update();
    }
    Ok(compose_status(state, &ask_app(state, "status").await))
}

/// The status every client reads, for Settings > Updates (updates/items.rs).
pub(crate) async fn current_app_update_status(state: &AppState) -> Value {
    compose_status(state, &ask_app(state, "status").await)
}

/// Asks the app to install its update, as the Update button here does, for Settings > Updates.
pub(crate) async fn request_app_update_install(
    state: &AppState,
    source: &str,
) -> Result<Value, String> {
    start_install(state, source)
        .await
        .map_err(|(_, message)| message)
}

/// The "Keep this machine up to date" switch, which is the Ghostex card's Update automatically.
pub(crate) async fn set_app_auto_update(state: &AppState, enabled: bool) -> Result<Value, String> {
    let mut params = Map::new();
    params.insert("enabled".to_string(), Value::Bool(enabled));
    set_auto(state, &params)
        .await
        .map_err(|(_, message)| message)
}
