//! `/api/gbots`: GBots (crate::gbots).

use super::*;

pub(super) async fn route_gbots_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if request.endpoint.path != crate::gbots::GBOTS_ENDPOINT {
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
    Ok(crate::gbots::handle_gbots_http(state, caller, endpoint.path, request_id, &body_json).await)
}
