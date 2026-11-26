//! `/api/remoteScreen/<ticket>/…`: the helper's viewer page, scripts and stream, carried through
//! gxserver so a viewer reaches them over the link it already has (an SSH or Easy Connect
//! tunnel to this gxserver) and never needs a port or a password of the helper's own.

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use hyper_util::rt::TokioIo;
use std::sync::Arc;

use super::{gate, helper};
use crate::server::AppState;

pub(crate) const REMOTE_SCREEN_PROXY_PREFIX: &str = "/api/remoteScreen/";
/// The helper's page posts nothing large: uploads are off (`--file-transfers=none`).
const BODY_LIMIT: usize = 1024 * 1024;

/// CDXC:RemoteScreen 2026-11-26 WHY: the viewer is a web page (CEF on the desktop, a WebView on the phone), and a page cannot put gxserver's token on its own WebSocket. So `/api/remoteScreenStart`, which does take the token, hands back a path holding a fresh random ticket, and the ticket alone opens this proxy while that helper runs; a new start makes a new ticket. The helper listens on loopback with a password only gxserver holds, and the proxy adds it, so the page never sees it. The page loads over `http://127.0.0.1:<tunnel port>`, a secure context, which WebCodecs needs.
pub(crate) async fn handle_remote_screen_proxy(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Response<Body> {
    if !gate::remote_screen_enabled(&state.paths) {
        return plain(
            StatusCode::NOT_FOUND,
            "Remote screen is off on this computer.",
        );
    }
    let path = request.uri().path().to_string();
    let Some(after) = path.strip_prefix(REMOTE_SCREEN_PROXY_PREFIX) else {
        return plain(StatusCode::NOT_FOUND, "Not found.");
    };
    let (ticket, rest) = match after.split_once('/') {
        Some((ticket, rest)) => (ticket, rest),
        // The page loads its scripts and socket by relative URL, so it must sit under a slash.
        None => {
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
            if let Ok(location) = HeaderValue::from_str(&format!("{after}/")) {
                response.headers_mut().insert(header::LOCATION, location);
            }
            return response;
        }
    };
    let Some(target) = helper::target_for_ticket(ticket) else {
        return plain(
            StatusCode::NOT_FOUND,
            "This screen session has ended. Open the screen again.",
        );
    };
    let query = request
        .uri()
        .query()
        .map(|query| format!("?{query}"))
        .unwrap_or_default();
    let upstream_path = format!("/{rest}{query}");
    let mut response = forward(target, upstream_path, request).await;
    let headers = response.headers_mut();
    // The ticket is in the page's address: keep it out of caches and referrers.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn forward(
    target: helper::HelperTarget,
    upstream_path: String,
    mut request: Request,
) -> Response<Body> {
    let address = format!("127.0.0.1:{}", target.port);
    let Ok(stream) = tokio::net::TcpStream::connect(&address).await else {
        return plain(
            StatusCode::BAD_GATEWAY,
            "The screen helper is not answering.",
        );
    };
    let _ = stream.set_nodelay(true);
    let Ok((mut sender, connection)) =
        hyper::client::conn::http1::handshake(TokioIo::new(stream)).await
    else {
        return plain(
            StatusCode::BAD_GATEWAY,
            "The screen helper is not answering.",
        );
    };
    let is_upgrade = request.headers().contains_key(header::UPGRADE);
    tokio::spawn(async move {
        if is_upgrade {
            let _ = connection.with_upgrades().await;
        } else {
            let _ = connection.await;
        }
    });
    let client_upgrade = is_upgrade.then(|| hyper::upgrade::on(&mut request));
    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, BODY_LIMIT).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let mut builder = hyper::Request::builder()
        .method(parts.method)
        .uri(upstream_path.as_str());
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &parts.headers {
            if dropped_request_header(name, is_upgrade) {
                continue;
            }
            headers.append(name, value.clone());
        }
        // The helper answers only its own loopback host and page, and its password.
        let own = format!("127.0.0.1:{}", target.port);
        if let Ok(host) = HeaderValue::from_str(&own) {
            headers.insert(header::HOST, host);
        }
        if parts.headers.contains_key(header::ORIGIN) {
            if let Ok(origin) = HeaderValue::from_str(&format!("http://{own}")) {
                headers.insert(header::ORIGIN, origin);
            }
        }
        if let Ok(bearer) = HeaderValue::from_str(&format!("Bearer {}", target.password)) {
            headers.insert(header::AUTHORIZATION, bearer);
        }
    }
    let Ok(upstream_request) = builder.body(Body::from(body)) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(mut upstream) = sender.send_request(upstream_request).await else {
        return plain(
            StatusCode::BAD_GATEWAY,
            "The screen helper is not answering.",
        );
    };
    if upstream.status() == StatusCode::SWITCHING_PROTOCOLS {
        let mut response = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
        if let Some(headers) = response.headers_mut() {
            for (name, value) in upstream.headers() {
                headers.append(name, value.clone());
            }
        }
        if let Some(client_upgrade) = client_upgrade {
            let upstream_upgrade = hyper::upgrade::on(&mut upstream);
            tokio::spawn(async move {
                if let (Ok(client), Ok(helper)) = (client_upgrade.await, upstream_upgrade.await) {
                    let mut client = TokioIo::new(client);
                    let mut helper = TokioIo::new(helper);
                    let _ = tokio::io::copy_bidirectional(&mut client, &mut helper).await;
                }
            });
        }
        return response
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let mut response = Response::new(Body::new(upstream.into_body()));
    *response.status_mut() = status;
    for (name, value) in &headers {
        if hop_by_hop(name) || name == header::WWW_AUTHENTICATE {
            continue;
        }
        response.headers_mut().append(name, value.clone());
    }
    response
}

/// Headers that must not reach the helper: gxserver's own credentials, the viewer's host and
/// cookies, forwarding claims, and hop-by-hop headers (kept for a WebSocket upgrade).
fn dropped_request_header(name: &HeaderName, upgrade: bool) -> bool {
    match name.as_str() {
        "host"
        | "authorization"
        | "cookie"
        | "x-forwarded-for"
        | "x-forwarded-host"
        | "x-forwarded-proto"
        | "x-real-ip"
        | "forwarded"
        | "proxy-connection"
        | "keep-alive"
        | "transfer-encoding"
        | "te"
        | "trailer"
        | "x-gxserver-protocol-version"
        | "x-ghostex-caller-session" => true,
        "connection" | "upgrade" => !upgrade,
        _ => false,
    }
}

fn hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn plain(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(message));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
