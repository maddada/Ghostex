//! `/api/gbots`: GBots (crate::gbots), and `/api/gbotsRemote`, a GBot's chat on a connected
//! computer reaching its home computer (gbots/computer/home.rs).

use super::*;

pub(super) async fn route_gbots_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let remote = request.endpoint.path == crate::gbots::GBOTS_REMOTE_ENDPOINT;
    if request.endpoint.path != crate::gbots::GBOTS_ENDPOINT && !remote {
        return Err(request);
    }
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        caller,
        ..
    } = request;
    if remote {
        return Ok(crate::gbots::handle_gbots_remote_http(
            state,
            caller,
            endpoint.path,
            request_id,
            &body_json,
        )
        .await);
    }
    Ok(crate::gbots::handle_gbots_http(state, caller, endpoint.path, request_id, &body_json).await)
}
