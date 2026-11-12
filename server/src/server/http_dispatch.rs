use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use serde_json::{json, Map, Value};
use std::{convert::Infallible, net::SocketAddr, sync::Arc, time::Instant};

use crate::{
    domain::{read_domain_rpc_params, DomainRepository, DomainStateError},
    logging::{log_level_from_status, DiagnosticLogScenario, GxserverLogInput},
    protocol::rpc_success_of,
    storage::open_gxserver_database,
};

use super::*;

tokio::task_local! {
    /// The TCP peer of the request being served (`axum`'s `ConnectInfo`), for the routes that
    /// trace where a request came from (`remote_access/tailnet_owner.rs`).
    static CLIENT_ADDRESS: Option<SocketAddr>;
}

/// The TCP peer of the request this task serves, when the listener recorded it.
pub(crate) fn request_client_address() -> Option<SocketAddr> {
    CLIENT_ADDRESS.try_with(|address| *address).ok().flatten()
}

pub(super) async fn handle_http_request(
    state: Arc<AppState>,
    request: Request<Body>,
) -> std::result::Result<Response<Body>, Infallible> {
    let client_address = request
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    CLIENT_ADDRESS
        .scope(client_address, handle_http_request_inner(state, request))
        .await
}

async fn handle_http_request_inner(
    state: Arc<AppState>,
    request: Request<Body>,
) -> std::result::Result<Response<Body>, Infallible> {
    let started_at = Instant::now();
    let _in_flight = super::stall_watch::request_started(request.uri().path());
    let client = request
        .extensions()
        .get::<SocketAddr>()
        .map(|address| address.ip().to_string());
    let headers = request.headers().clone();
    let request_id = request_id(&headers);
    // Who asked, for the security log (server/src/audit/caller.rs).
    let caller = crate::audit::Caller::from_headers(
        &headers,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    );
    // Boxed: the routing future is large, and moving it through the caller scope by value took
    // over a megabyte of stack in debug builds (a route test overflowed).
    let mut routed = crate::audit::with_caller(
        caller,
        Box::pin(route_http(state.clone(), request, request_id)),
    )
    .await;
    apply_cors_headers(&headers, &mut routed.response, &state.config);
    let status = routed.response.status().as_u16();
    let mut details = json!({
        "method": "http",
        "route": routed.endpoint_path,
        "statusCode": status,
    });
    if let Some(size) = routed.response.extensions().get::<BodySizeLimit>() {
        details["requestBytes"] = json!(size.size_bytes);
        details["limitBytes"] = json!(size.limit_bytes);
    }
    if let Some(size) = routed.response.extensions().get::<ImageSizeLimit>() {
        details["imageBytes"] = json!(size.size_bytes);
        details["imageLimitBytes"] = json!(size.limit_bytes);
    }
    let _ = state.logger.log_routine(
        DiagnosticLogScenario::ApiRequests,
        GxserverLogInput {
            level: log_level_from_status(status),
            event: "apiRequest".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client,
            duration_ms: Some(started_at.elapsed().as_millis()),
            // A failed route said only its status code, so a reported 500 could not be traced to its cause.
            error: routed
                .response
                .extensions()
                .get::<crate::session_chat_send_diagnostics::SendFailureReason>()
                .map(|reason| format!("{}: {}", reason.code, reason.message)),
            // The API route under `route`: the log sanitizer redacts every `path` field as a file path,
            // which hid which endpoint was failing.
            details: Some(details),
        },
    );
    // CDXC:ServerApi 2026-10-23 DECISION:
    // The user chose to stop broadcasting an `apiRequestHandled` frame for every HTTP request served, because no client reads it: the desktop, the web build and `gx-client` dropped it unparsed, nothing in the phone app, the `ghostex` CLI or the extensions reads it, and it was about five frames a second to every open event socket. Clients keep tolerating it (`ServerEvent::ApiRequestHandled`, the `gx-client` peek), because gxservers of older releases still send it.
    Ok(routed.response)
}

pub(super) fn handle_domain_http<F>(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    handler: F,
) -> RoutedResponse
where
    F: FnOnce(
        &DomainRepository<'_>,
        &rusqlite::Connection,
        &Map<String, Value>,
        &str,
    ) -> std::result::Result<Value, DomainStateError>,
{
    handle_domain_http_as(state, endpoint_path, request_id, body, handler)
}

/// `handle_domain_http` for a handler whose result is not a `Value` (listProjects' cached text).
pub(super) fn handle_domain_http_as<R, F>(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
    handler: F,
) -> RoutedResponse
where
    R: serde::Serialize,
    F: FnOnce(
        &DomainRepository<'_>,
        &rusqlite::Connection,
        &Map<String, Value>,
        &str,
    ) -> std::result::Result<R, DomainStateError>,
{
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("SQLite gxserver state error: {error}"),
                },
            );
        }
    };
    let server_id = state.metadata.server_id.as_str();
    let repository = DomainRepository::new(&db, server_id);
    // Names the endpoint in any `sessionIdentity.changed` line its handler causes.
    let identity_source =
        crate::session_identity_log::identity_write_source(format!("endpoint:{endpoint_path}"));
    let outcome = handler(&repository, &db, &params, server_id);
    drop(identity_source);
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success_of(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
