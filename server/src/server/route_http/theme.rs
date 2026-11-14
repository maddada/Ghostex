//! `/api/theme`: the look settings and their change history (crate::theme).

use super::*;

pub(super) async fn route_theme_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if request.endpoint.path != crate::theme::THEME_ENDPOINT {
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
    Ok(crate::theme::handle_theme_http(state, caller, endpoint.path, request_id, &body_json).await)
}
