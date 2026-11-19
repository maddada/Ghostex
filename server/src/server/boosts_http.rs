//! `/api/boosts`: the Boost pill's prompt editor and one-time hint, and `ghostex visual boosts`,
//! read and change the edited Boost prompts here (server/src/boosts.rs).

use super::*;

pub(crate) async fn handle_boosts_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let result = read_domain_rpc_params(body)
        .map_err(|error| DomainStateError {
            code: "badRequest",
            message: error.message,
        })
        .and_then(|params| crate::boosts::handle(&state.paths, &params));
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            if error.code == "badRequest" {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}
