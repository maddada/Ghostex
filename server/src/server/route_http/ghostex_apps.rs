//! The Ghostex Apps routes (crate::ghostex_apps): publish, list, read, open, allow and delete.

use super::*;

pub(super) async fn route_ghostex_apps_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    if !crate::ghostex_apps::is_ghostex_apps_endpoint(&request.endpoint.path) {
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
    Ok(crate::ghostex_apps::handle_ghostex_apps_http(
        state,
        endpoint.path,
        request_id,
        body_json,
        caller,
    )
    .await)
}
