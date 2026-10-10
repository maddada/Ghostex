//! "Start in cloud" routes (crate::work_mode::cloud_work): the task a ticket's cloud session would
//! start with, and starting it. Both answer "turned off" while the Workspaces built-in extension
//! is off.

use crate::domain::DomainStateError;
use crate::protocol::rpc_success;
use crate::work_mode::{draft_cloud_work, start_cloud_work};
use crate::workspaces::workspaces_feature_enabled;

use super::super::work_mode_sync::spawn_work_mode_refresh;
use super::*;

pub(super) async fn route_work_cloud_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if !matches!(
        request.endpoint.path.as_str(),
        "/api/draftCloudWork" | "/api/startCloudWork"
    ) {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    let params = match read_domain_rpc_params(&body_json) {
        Ok(params) => params,
        Err(error) => return Ok(domain_error_response(endpoint.path, request_id, error)),
    };
    let start = endpoint.path == "/api/startCloudWork";
    let worker_state = state.clone();
    // Off the async runtime: Linear, `gh`, `git ls-remote` and the cloud start are slow.
    let result = tokio::task::spawn_blocking(move || {
        if !workspaces_feature_enabled() {
            return Err(DomainStateError::bad_request(
                "Workspaces is turned off (Settings > Extensions).",
            ));
        }
        if start {
            start_cloud_work(&worker_state, &params)
        } else {
            draft_cloud_work(&worker_state, &params)
        }
    })
    .await
    .unwrap_or_else(|error| {
        Err(DomainStateError::corrupt_state(format!(
            "Cloud start failed: {error}"
        )))
    });
    if start && result.is_ok() {
        // The ticket's team-flow step reads the new session on the next pass.
        spawn_work_mode_refresh(&state);
    }
    Ok(match result {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}
