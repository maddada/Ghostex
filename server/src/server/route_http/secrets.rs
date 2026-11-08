//! Secret routes: agents' secret requests (crate::secret_requests) and the secret providers of
//! Settings > Secrets (crate::secret_providers).

use crate::secret_providers::{handle_secret_providers_http, SECRET_PROVIDER_ENDPOINTS};
use crate::secret_requests::{handle_secret_request_http, SECRET_REQUEST_ENDPOINTS};

use super::*;

pub(super) async fn route_secrets_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let path = request.endpoint.path.as_str();
    let requests = SECRET_REQUEST_ENDPOINTS.contains(&path);
    if !requests && !SECRET_PROVIDER_ENDPOINTS.contains(&path) {
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
    Ok(if requests {
        handle_secret_request_http(&state, &caller, endpoint.path, request_id, &body_json).await
    } else {
        handle_secret_providers_http(&state, endpoint.path, request_id, &body_json).await
    })
}
