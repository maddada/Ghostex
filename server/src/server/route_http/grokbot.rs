//! The Grok Bot connection route, `/api/grokbot` (crate::grokbot).

use crate::domain::DomainStateError;

use super::*;

pub(super) async fn route_grokbot_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if request.endpoint.path != crate::grokbot::GROKBOT_ENDPOINT {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        ..
    } = request;
    // Off the async runtime: `connect` and `test` call Grok Bot.
    let worker_state = state.clone();
    let worker_endpoint = endpoint.path.clone();
    let worker_request_id = request_id.clone();
    let response = tokio::task::spawn_blocking(move || {
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body_json,
            |_, _, params, _| crate::grokbot::handle(&worker_state.paths, params),
        )
    })
    .await;
    Ok(match response {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint.path,
            request_id,
            DomainStateError::corrupt_state(format!("The Grok Bot request failed: {error}")),
        ),
    })
}
