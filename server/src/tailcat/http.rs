use std::time::Instant;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
    storage::open_gxserver_database,
};

use super::status::*;
use super::types::*;

pub(crate) async fn handle_tailcat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return tailcat_error_response(endpoint_path, request_id, error.code, error.message);
        }
    };
    let paths = state.paths.clone();
    let runtime = state.tailcat_runtime.clone();
    let logger = state.logger.clone();

    if endpoint_path == "/api/installTailcat" {
        super::install::start_installation(paths.clone(), runtime.clone());
    }

    if endpoint_path == "/api/tailcatStatus" || endpoint_path == "/api/installTailcat" {
        let status = tokio::task::spawn_blocking(move || {
            read_tailcat_status_payload_for_paths(&paths, &runtime)
        })
        .await;
        return match status {
            Ok(status) => routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "status": status })),
            ),
            Err(error) => tailcat_error_response(
                endpoint_path,
                request_id,
                "internalError",
                format!("tailcat status read failed: {error}"),
            ),
        };
    }

    let update = match serde_json::from_value::<TailcatStateUpdate>(Value::Object(params)) {
        Ok(update) => update,
        Err(error) => {
            return tailcat_error_response(
                endpoint_path,
                request_id,
                "badRequest",
                format!("Invalid tailcat state update payload: {error}"),
            );
        }
    };

    let started_at = Instant::now();
    let outcome = tokio::task::spawn_blocking({
        let update = update.clone();
        move || {
            let db = open_gxserver_database(&paths).map_err(|error| {
                (
                    TailcatLogErrorCode::StateUpdateDatabaseUnavailable,
                    format!("SQLite gxserver state error: {error}"),
                )
            })?;
            let record =
                apply_tailcat_state_update(&paths, &db, &runtime, update).map_err(|error| {
                    (
                        TailcatLogErrorCode::StateUpdateFailed,
                        format!("tailcat state update failed: {error}"),
                    )
                })?;
            Ok((record, read_tailcat_status_payload(&db, &runtime)))
        }
    })
    .await;

    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => Err((
            TailcatLogErrorCode::StateUpdateFailed,
            format!("tailcat state update task failed: {error}"),
        )),
    };

    match outcome {
        Ok((record, status)) => {
            log_tailcat_state_update_success(
                &logger,
                &update,
                &record,
                started_at.elapsed().as_millis(),
            );
            routed_json(
                Some(endpoint_path),
                StatusCode::OK,
                rpc_success(request_id, json!({ "status": status })),
            )
        }
        Err((error_code, message)) => {
            log_tailcat_state_update_failure(
                &logger,
                &update,
                error_code,
                started_at.elapsed().as_millis(),
            );
            tailcat_error_response(endpoint_path, request_id, "internalError", message)
        }
    }
}

fn tailcat_error_response(
    endpoint_path: String,
    request_id: String,
    code: &'static str,
    message: impl Into<String>,
) -> RoutedResponse {
    let status = if code == "badRequest" {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    routed_json(
        Some(endpoint_path),
        status,
        rpc_error(code, message.into(), Some(request_id)),
    )
}
