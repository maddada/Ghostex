//! The way-back listener: a second loopback port that a connected computer's reverse SSH forward
//! points at. It answers only the endpoints another computer's agents may use, only with the token
//! this computer gave that computer, and none of the routes the main listener serves before sign-in.
//!
//! CDXC:RemoteMachines 2026-11-03 WHY:
//! The reverse forward used to land on the main listener, so every local user of the other computer could reach this computer's pages and calls that need no token (`/api/pairDevice`, `/visual/*`, `/open-file/*`, `/ext/*`, the OAuth redirect). Here the path is checked against the allowlist and the token against the grants before anything else runs, so nothing but the allowlist is reachable from there.
//! SEE-ALSO: server/src/connected_computers/scope.rs (the allowlist), apps/desktop/src/app/helpers/remote/reverse_tunnel.rs (the `-R 127.0.0.1:0:…` forward).

use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode},
};
use std::{convert::Infallible, sync::Arc};

use crate::{
    auth::bearer_token,
    connected_computers::{authorize_peer_token, is_peer_allowed, PeerAuthorization},
    protocol::rpc_error,
};

use super::*;

pub(super) async fn handle_peer_http_request(
    state: Arc<AppState>,
    request: Request<Body>,
) -> std::result::Result<Response<Body>, Infallible> {
    let request_id = request_id(request.headers());
    let path = request.uri().path().to_string();
    let method = request.method().clone();
    let refuse = |status: StatusCode, error: &str, message: String| {
        json_response(status, rpc_error(error, message, Some(request_id.clone())))
    };
    if !is_peer_allowed(&path)
        || !(method == Method::POST || (method == Method::GET && path == "/api/health/server"))
    {
        return Ok(refuse(
            StatusCode::NOT_FOUND,
            "notFound",
            format!("{path} is not available to another computer's agents."),
        ));
    }
    match bearer_token(request.headers()).and_then(authorize_peer_token) {
        Some(PeerAuthorization::Allowed { .. }) => {}
        Some(PeerAuthorization::AccessOff { name }) => {
            let this = crate::connected_computers::this_computer_name();
            return Ok(refuse(
                StatusCode::FORBIDDEN,
                "agentAccessOff",
                format!("{this} has Agent access for this computer turned off. Turn on Agent access for {name} in {this}'s Settings > Remote > Remote machines."),
            ));
        }
        None => {
            return Ok(refuse(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Only a connected computer's agents can use this port.".to_string(),
            ));
        }
    }
    Ok(route_http(state, request, request_id.clone())
        .await
        .response)
}

/// Binds the way-back listener on a free loopback port and serves it until shutdown.
pub(super) async fn start_peer_listener(state: Arc<AppState>) {
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let Ok(address) = listener.local_addr() else {
        return;
    };
    crate::connected_computers::set_peer_listener_port(address.port());
    let mut shutdown = state.shutdown_tx.subscribe();
    let app = axum::Router::new().fallback_service(tower::service_fn(move |request| {
        handle_peer_http_request(state.clone(), request)
    }));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown.recv().await;
            })
            .await;
    });
}
