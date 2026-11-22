//! `/api/restorePreviousSession` over HTTP: the decision is `crate::previous_session_restore`; its
//! steps are this daemon's own endpoints over the loopback API, like `/api/openConversation`.

use super::*;

use crate::previous_session_restore::{restore_previous_session, RestorePreviousSessionRequest};

pub(crate) async fn handle_restore_previous_session_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let worker_state = state.clone();
    let worker_endpoint = endpoint_path.clone();
    let worker_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body,
            |repository, db, params, _| {
                let request = RestorePreviousSessionRequest::read(params)?;
                let token = worker_state.auth_token.clone();
                restore_previous_session(&request, repository, db, &|path, params| {
                    super::open_conversation_http::loopback(&token, path, params)
                })
            },
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::corrupt_state(format!("restorePreviousSession failed: {error}")),
        ),
    }
}
