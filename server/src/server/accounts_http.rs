use super::*;
pub(crate) async fn handle_accounts_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(p) => p,
        Err(e) => return domain_error_response(endpoint_path, request_id, e),
    };
    let owned = state.clone();
    match tokio::task::spawn_blocking(move || crate::accounts::endpoint::dispatch(&owned, &params))
        .await
    {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => domain_error_response(endpoint_path, request_id, error),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            crate::accounts::store::error(error),
        ),
    }
}
