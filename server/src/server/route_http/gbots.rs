//! `/api/gbots` and `/api/gbotsHome`: GBots (crate::gbots).

use crate::protocol::rpc_success;

use super::*;

pub(super) async fn route_gbots_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let home = request.endpoint.path == "/api/gbotsHome";
    if request.endpoint.path != "/api/gbots" && !home {
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
    let answer = if home {
        crate::gbots::gbots_home_action(state, params).await
    } else {
        crate::gbots::gbots_action(state, params).await
    };
    Ok(match answer {
        Ok(value) => routed_json(
            Some(endpoint.path),
            StatusCode::OK,
            rpc_success(request_id, value),
        ),
        Err(error) => domain_error_response(endpoint.path, request_id, error),
    })
}
