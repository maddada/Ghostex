//! Separate ephemeral loopback share-proxy spawned per active share.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! Hard security rule: NEVER point a tunnel at gxserver (`127.0.0.1:58744`). Every share binds
//! its own ephemeral `127.0.0.1:0` listener that serves ONLY:
//! - For a Live page: the one frozen page HTML at `/` (and `/index.html`) plus the narrow
//!   `POST /__gx_share/answers` endpoint for that single page id. Every other path returns 404.
//! - For a localhost port: HTTP and WebSocket reverse-proxying strictly to `127.0.0.1:<port>`,
//!   ignoring visitor `Host` headers for routing and stripping spoofed forwarding headers.
//! Both kinds enforce expiry and optional passcode cookies with brute-force lockout.

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode},
    response::IntoResponse,
    Router,
};
use hyper_util::rt::TokioIo;
use rand::TryRng;
use ring::hmac;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::oneshot};

use crate::{
    server::AppState,
    visual_pages::{submit_outside_answers, SHARED_PAGE_CONTENT_SECURITY_POLICY},
};

const SHARE_COOKIE_NAME: &str = "__gx_share_auth";
const UNLOCK_PATH: &str = "/__gx_share/unlock";
const ANSWERS_PATH: &str = "/__gx_share/answers";
const ANSWERS_MAX_BYTES: usize = 64 * 1024;
const UNLOCK_MAX_BYTES: usize = 4 * 1024;
const PROXY_BODY_MAX_BYTES: usize = 32 * 1024 * 1024;
const PASSCODE_MAX_FAILURES: usize = 5;
const PASSCODE_LOCKOUT_SECS: u64 = 5 * 60;
const ANSWERS_RATE_LIMIT_MAX: usize = 10;
const ANSWERS_RATE_LIMIT_WINDOW_SECS: u64 = 60;

#[derive(Clone)]
pub(crate) enum ShareProxyTarget {
    Page {
        page_id: String,
        title: String,
        html: Arc<String>,
        answerable: bool,
    },
    Port {
        port: u16,
        title: String,
    },
}

struct PasscodeState {
    passcode: String,
    cookie_token: String,
    failed_attempts: VecDeque<Instant>,
    locked_until: Option<Instant>,
}

struct ProxyState {
    app_state: Arc<AppState>,
    target: ShareProxyTarget,
    expires_at_ms: Option<i64>,
    stopped: AtomicBool,
    passcode: Option<Mutex<PasscodeState>>,
    answer_timestamps: Mutex<VecDeque<Instant>>,
}

pub(crate) struct RunningShareProxy {
    pub(crate) listen_addr: SocketAddr,
    stopped: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl RunningShareProxy {
    pub(crate) fn stop(mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.task.abort();
    }
}

/// Spawns an isolated ephemeral loopback HTTP server for a single share.
pub(crate) async fn spawn_share_proxy(
    app_state: Arc<AppState>,
    target: ShareProxyTarget,
    expires_at_ms: Option<i64>,
    passcode: Option<String>,
) -> Result<RunningShareProxy, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("Could not start local share proxy: {error}"))?;
    let listen_addr = listener
        .local_addr()
        .map_err(|error| format!("Could not read share proxy address: {error}"))?;

    let passcode_state = passcode.map(|code| {
        let mut key_bytes = [0u8; 32];
        rand::rngs::SysRng
            .try_fill_bytes(&mut key_bytes)
            .expect("OS RNG failed");
        let key = hmac::Key::new(hmac::HMAC_SHA256, &key_bytes);
        let tag = hmac::sign(&key, code.as_bytes());
        let cookie_token = hex::encode(tag.as_ref());
        Mutex::new(PasscodeState {
            passcode: code,
            cookie_token,
            failed_attempts: VecDeque::new(),
            locked_until: None,
        })
    });

    let stopped = Arc::new(AtomicBool::new(false));
    let state = Arc::new(ProxyState {
        app_state,
        target,
        expires_at_ms,
        stopped: AtomicBool::new(false),
        passcode: passcode_state,
        answer_timestamps: Mutex::new(VecDeque::new()),
    });
    let stopped_flag = stopped.clone();
    let state_for_stop = state.clone();

    let router = Router::new().fallback(move |req: Request| {
        let state = state.clone();
        async move { handle_request(state, req).await }
    });

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
                state_for_stop.stopped.store(true, Ordering::SeqCst);
                stopped_flag.store(true, Ordering::SeqCst);
            })
            .await;
    });

    Ok(RunningShareProxy {
        listen_addr,
        stopped,
        shutdown_tx: Some(shutdown_tx),
        task,
    })
}

fn is_expired_or_stopped(state: &ProxyState) -> bool {
    if state.stopped.load(Ordering::SeqCst) {
        return true;
    }
    state
        .expires_at_ms
        .is_some_and(|exp| chrono::Utc::now().timestamp_millis() >= exp)
}

async fn handle_request(state: Arc<ProxyState>, req: Request) -> Response<Body> {
    if is_expired_or_stopped(&state) {
        return plain_or_json(
            req.headers(),
            StatusCode::GONE,
            "This shared link has expired.",
        );
    }

    let path = req.uri().path().to_string();
    let method = req.method().clone();

    // Passcode unlock endpoint (only active when passcode is enabled on this share).
    if path == UNLOCK_PATH && method == Method::POST {
        return handle_unlock(&state, req).await;
    }

    // Passcode gate on every request when enabled.
    if state.passcode.is_some() && !has_valid_passcode_cookie(&state, req.headers()) {
        if path == ANSWERS_PATH || wants_json(req.headers()) {
            return json_response(
                StatusCode::UNAUTHORIZED,
                json!({ "ok": false, "error": "Passcode required." }),
            );
        }
        let title = match &state.target {
            ShareProxyTarget::Page { title, .. } | ShareProxyTarget::Port { title, .. } => {
                title.as_str()
            }
        };
        return html_response(StatusCode::UNAUTHORIZED, passcode_gate_html(title, None));
    }

    match &state.target {
        ShareProxyTarget::Page {
            page_id,
            html,
            answerable,
            ..
        } => {
            if method == Method::GET && (path == "/" || path == "/index.html") {
                let mut res = Response::new(Body::from(html.as_str().to_owned()));
                *res.status_mut() = StatusCode::OK;
                for (name, value) in [
                    (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                    (
                        header::CONTENT_SECURITY_POLICY,
                        SHARED_PAGE_CONTENT_SECURITY_POLICY,
                    ),
                    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                    (header::REFERRER_POLICY, "no-referrer"),
                    (header::CACHE_CONTROL, "no-store"),
                ] {
                    if let Ok(val) = HeaderValue::from_str(value) {
                        res.headers_mut().insert(name, val);
                    }
                }
                return res;
            }
            if method == Method::POST && path == ANSWERS_PATH {
                if !*answerable {
                    return json_response(
                        StatusCode::BAD_REQUEST,
                        json!({ "ok": false, "error": "This shared page has no open questions." }),
                    );
                }
                return handle_shared_answers(&state, page_id, req).await;
            }
            // Hard rule: every other path (including /api/*, /visual/*, etc.) returns 404.
            StatusCode::NOT_FOUND.into_response()
        }
        ShareProxyTarget::Port { port, .. } => proxy_to_localhost_port(*port, req).await,
    }
}

fn check_answers_rate_limit(state: &ProxyState) -> bool {
    let mut guard = state
        .answer_timestamps
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let window = Duration::from_secs(ANSWERS_RATE_LIMIT_WINDOW_SECS);
    while guard
        .front()
        .is_some_and(|ts| now.duration_since(*ts) > window)
    {
        guard.pop_front();
    }
    if guard.len() >= ANSWERS_RATE_LIMIT_MAX {
        return false;
    }
    guard.push_back(now);
    true
}

async fn handle_shared_answers(state: &ProxyState, page_id: &str, req: Request) -> Response<Body> {
    if !check_answers_rate_limit(state) {
        let mut res = json_response(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "ok": false, "error": "Too many submissions. Please wait a minute and try again." }),
        );
        res.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
        return res;
    }
    let Ok(bytes) = to_bytes(req.into_body(), ANSWERS_MAX_BYTES).await else {
        return json_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({ "ok": false, "error": "The answers are larger than 64 KiB." }),
        );
    };
    let Ok(mut payload) = serde_json::from_slice::<Value>(&bytes) else {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "error": "Invalid JSON body." }),
        );
    };
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let Some(answers) = payload
        .as_object_mut()
        .and_then(|map| map.remove("answers"))
    else {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "error": "Missing answers." }),
        );
    };
    match submit_outside_answers(state.app_state.clone(), page_id, &name, answers).await {
        Ok(result) => json_response(StatusCode::OK, result),
        Err((status, error)) => json_response(status, json!({ "ok": false, "error": error })),
    }
}

fn has_valid_passcode_cookie(state: &ProxyState, headers: &HeaderMap) -> bool {
    let Some(lock) = state.passcode.as_ref() else {
        return true;
    };
    let expected = {
        let guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        guard.cookie_token.clone()
    };
    let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix(&format!("{SHARE_COOKIE_NAME}=")) {
            let value_bytes = value.trim().as_bytes();
            let expected_bytes = expected.as_bytes();
            if value_bytes.len() == expected_bytes.len()
                && bool::from(value_bytes.ct_eq(expected_bytes))
            {
                return true;
            }
        }
    }
    false
}

async fn handle_unlock(state: &ProxyState, req: Request) -> Response<Body> {
    let Some(lock) = state.passcode.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let json_client = wants_json(req.headers());
    let title = match &state.target {
        ShareProxyTarget::Page { title, .. } | ShareProxyTarget::Port { title, .. } => {
            title.clone()
        }
    };
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Ok(bytes) = to_bytes(req.into_body(), UNLOCK_MAX_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let submitted = if content_type.contains("application/json") {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| {
                v.get("passcode")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default()
    } else {
        let text = String::from_utf8_lossy(&bytes);
        url::form_urlencoded::parse(text.as_bytes())
            .find(|(k, _)| k == "passcode")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    };
    let submitted_clean: String = submitted.chars().filter(|ch| !ch.is_whitespace()).collect();

    let outcome = {
        let mut guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        if let Some(until) = guard.locked_until {
            if now < until {
                let remaining = until.duration_since(now).as_secs().max(1);
                Err((StatusCode::TOO_MANY_REQUESTS, remaining))
            } else {
                guard.locked_until = None;
                guard.failed_attempts.clear();
                verify_passcode_locked(&mut guard, &submitted_clean, now)
            }
        } else {
            verify_passcode_locked(&mut guard, &submitted_clean, now)
        }
    };

    match outcome {
        Ok(cookie_token) => {
            let cookie =
                format!("{SHARE_COOKIE_NAME}={cookie_token}; HttpOnly; SameSite=Lax; Path=/");
            if json_client {
                let mut res = json_response(StatusCode::OK, json!({ "ok": true }));
                if let Ok(val) = HeaderValue::from_str(&cookie) {
                    res.headers_mut().insert(header::SET_COOKIE, val);
                }
                res
            } else {
                let mut res = Response::new(Body::empty());
                *res.status_mut() = StatusCode::SEE_OTHER;
                res.headers_mut()
                    .insert(header::LOCATION, HeaderValue::from_static("/"));
                if let Ok(val) = HeaderValue::from_str(&cookie) {
                    res.headers_mut().insert(header::SET_COOKIE, val);
                }
                res
            }
        }
        Err((StatusCode::TOO_MANY_REQUESTS, retry_after)) => {
            let msg = format!(
                "Too many wrong passcodes. Try again in {}.",
                ghostex_gx_protocol::plural(retry_after, "second", "seconds")
            );
            let mut res = if json_client {
                json_response(
                    StatusCode::TOO_MANY_REQUESTS,
                    json!({ "ok": false, "error": msg, "retryAfterSeconds": retry_after }),
                )
            } else {
                html_response(
                    StatusCode::TOO_MANY_REQUESTS,
                    passcode_gate_html(&title, Some(&msg)),
                )
            };
            if let Ok(val) = HeaderValue::from_str(&retry_after.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, val);
            }
            res
        }
        Err((_, _)) => {
            let msg = "Incorrect passcode. Please try again.";
            if json_client {
                json_response(
                    StatusCode::UNAUTHORIZED,
                    json!({ "ok": false, "error": msg }),
                )
            } else {
                html_response(
                    StatusCode::UNAUTHORIZED,
                    passcode_gate_html(&title, Some(msg)),
                )
            }
        }
    }
}

fn verify_passcode_locked(
    guard: &mut PasscodeState,
    submitted: &str,
    now: Instant,
) -> Result<String, (StatusCode, u64)> {
    let window = Duration::from_secs(PASSCODE_LOCKOUT_SECS);
    while guard
        .failed_attempts
        .front()
        .is_some_and(|ts| now.duration_since(*ts) > window)
    {
        guard.failed_attempts.pop_front();
    }
    let matches = submitted.len() == guard.passcode.len()
        && bool::from(submitted.as_bytes().ct_eq(guard.passcode.as_bytes()));
    if matches {
        guard.failed_attempts.clear();
        guard.locked_until = None;
        return Ok(guard.cookie_token.clone());
    }
    guard.failed_attempts.push_back(now);
    if guard.failed_attempts.len() >= PASSCODE_MAX_FAILURES {
        guard.locked_until = Some(now + window);
        Err((StatusCode::TOO_MANY_REQUESTS, PASSCODE_LOCKOUT_SECS))
    } else {
        Err((StatusCode::UNAUTHORIZED, 0))
    }
}

/// Forwards an HTTP (or WebSocket upgrade) request strictly to `127.0.0.1:<target_port>`.
/// Completely ignores visitor `Host` header for routing and strips spoofed forwarding headers.
async fn proxy_to_localhost_port(target_port: u16, mut req: Request) -> Response<Body> {
    let target_addr = format!("127.0.0.1:{target_port}");
    let Ok(stream) = tokio::net::TcpStream::connect(&target_addr).await else {
        return plain_or_json(
            req.headers(),
            StatusCode::BAD_GATEWAY,
            "The local server on this port is not responding.",
        );
    };

    let io = TokioIo::new(stream);
    let Ok((mut sender, conn)) = hyper::client::conn::http1::handshake(io).await else {
        return plain_or_json(
            req.headers(),
            StatusCode::BAD_GATEWAY,
            "Could not connect to the local server.",
        );
    };

    let is_upgrade = req.headers().get(header::UPGRADE).is_some();

    tokio::spawn(async move {
        if is_upgrade {
            let _ = conn.with_upgrades().await;
        } else {
            let _ = conn.await;
        }
    });

    let incoming_upgrade = if is_upgrade {
        Some(hyper::upgrade::on(&mut req))
    } else {
        None
    };
    let (parts, body) = req.into_parts();

    let Ok(body_bytes) = to_bytes(body, PROXY_BODY_MAX_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let Ok(upstream_uri) = path_and_query.parse::<axum::http::Uri>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let mut upstream_builder = hyper::Request::builder()
        .method(parts.method)
        .uri(upstream_uri);

    if let Some(headers_mut) = upstream_builder.headers_mut() {
        for (name, value) in &parts.headers {
            if is_stripped_request_header(name, is_upgrade) {
                continue;
            }
            // Strip our own share auth cookie before sending to user's dev server.
            if name == header::COOKIE {
                if let Ok(raw) = value.to_str() {
                    let filtered: Vec<&str> = raw
                        .split(';')
                        .map(str::trim)
                        .filter(|kv| !kv.starts_with(&format!("{SHARE_COOKIE_NAME}=")))
                        .collect();
                    if !filtered.is_empty() {
                        if let Ok(clean) = HeaderValue::from_str(&filtered.join("; ")) {
                            headers_mut.append(header::COOKIE, clean);
                        }
                    }
                }
                continue;
            }
            headers_mut.append(name, value.clone());
        }
        if let Ok(host_val) = HeaderValue::from_str(&format!("localhost:{target_port}")) {
            headers_mut.insert(header::HOST, host_val);
        }
    }

    let Ok(upstream_req) = upstream_builder.body(Body::from(body_bytes)) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Ok(mut upstream_res) = sender.send_request(upstream_req).await else {
        return StatusCode::BAD_GATEWAY.into_response();
    };

    if upstream_res.status() == StatusCode::SWITCHING_PROTOCOLS {
        let mut client_res = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
        if let Some(headers_mut) = client_res.headers_mut() {
            for (name, value) in upstream_res.headers() {
                headers_mut.append(name, value.clone());
            }
        }
        if let Some(client_upgrade) = incoming_upgrade {
            let upstream_upgrade = hyper::upgrade::on(&mut upstream_res);
            tokio::spawn(async move {
                if let (Ok(client_io), Ok(upstream_io)) =
                    (client_upgrade.await, upstream_upgrade.await)
                {
                    let mut c = TokioIo::new(client_io);
                    let mut u = TokioIo::new(upstream_io);
                    let _ = tokio::io::copy_bidirectional(&mut c, &mut u).await;
                }
            });
        }
        return client_res
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }

    let status = upstream_res.status();
    let res_headers = upstream_res.headers().clone();
    let upstream_body = upstream_res.into_body();
    let mut out = Response::new(Body::new(upstream_body));
    *out.status_mut() = status;
    for (name, value) in &res_headers {
        if is_hop_by_hop_header(name) {
            continue;
        }
        out.headers_mut().append(name, value.clone());
    }
    out
}

fn is_stripped_request_header(name: &HeaderName, keep_upgrade: bool) -> bool {
    let lower = name.as_str();
    if matches!(
        lower,
        "host"
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
    ) {
        return true;
    }
    if !keep_upgrade && matches!(lower, "connection" | "upgrade") {
        return true;
    }
    false
}

fn is_hop_by_hop_header(name: &HeaderName) -> bool {
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

fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"))
        || headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json"))
}

fn json_response(status: StatusCode, body: Value) -> Response<Body> {
    let mut res = Response::new(Body::from(body.to_string()));
    *res.status_mut() = status;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn html_response(status: StatusCode, html: String) -> Response<Body> {
    let mut res = Response::new(Body::from(html));
    *res.status_mut() = status;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    res
}

fn plain_or_json(headers: &HeaderMap, status: StatusCode, message: &str) -> Response<Body> {
    if wants_json(headers) {
        json_response(status, json!({ "ok": false, "error": message }))
    } else {
        let escaped = html_escape(message);
        html_response(
            status,
            format!(
                "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Ghostex Share</title><style>:root{{color-scheme:dark light;font-family:-apple-system,BlinkMacSystemFont,system-ui,sans-serif}}body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#131313;color:#e5e5e5;padding:24px}}@media(prefers-color-scheme:light){{body{{background:#f0f0f0;color:#18181b}}}}.card{{max-width:420px;width:100%;padding:24px;border-radius:14px;background:rgba(255,255,255,.06);border:1px solid rgba(255,255,255,.12)}}h1{{margin:0 0 8px;font-size:17px}}p{{margin:0;font-size:14px;opacity:.78;line-height:1.45}}</style></head><body><div class=\"card\"><h1>Shared link unavailable</h1><p>{escaped}</p></div></body></html>"
            ),
        )
    }
}

fn passcode_gate_html(title: &str, error: Option<&str>) -> String {
    let escaped_title = html_escape(title);
    let error_html = error
        .map(|msg| {
            format!(
                "<div class=\"err\" role=\"alert\">{}</div>",
                html_escape(msg)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Enter passcode · {escaped_title}</title>
<style>
  :root {{ color-scheme: dark light; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }}
  body {{ margin: 0; min-height: 100vh; display: flex; align-items: center; justify-content: center; background: #131313; color: #e5e5e5; padding: 24px; box-sizing: border-box; }}
  @media (prefers-color-scheme: light) {{
    body {{ background: #f0f0f0; color: #18181b; }}
    .card {{ background: #ffffff; border-color: rgba(0,0,0,.12); }}
    input {{ background: #f5f5f5; border-color: rgba(0,0,0,.16); color: #18181b; }}
    button {{ background: #18181b; color: #ffffff; }}
  }}
  .card {{ width: min(380px, 100%); padding: 24px; border-radius: 16px; background: #1c1c1e; border: 1px solid rgba(255,255,255,.12); box-shadow: 0 18px 40px rgba(0,0,0,.35); }}
  h1 {{ margin: 0 0 6px; font-size: 17px; font-weight: 600; }}
  p {{ margin: 0 0 16px; font-size: 13.5px; opacity: .74; line-height: 1.45; }}
  form {{ display: flex; flex-direction: column; gap: 10px; }}
  input {{ height: 42px; padding: 0 14px; border-radius: 10px; border: 1px solid rgba(255,255,255,.18); background: #131313; color: #fcfcfc; font: 600 18px/1 ui-monospace, SFMono-Regular, Menlo, monospace; letter-spacing: .2em; text-align: center; outline: none; }}
  input:focus {{ border-color: #86d3f8; }}
  button {{ height: 38px; border: 0; border-radius: 10px; background: #e5e5e5; color: #121212; font: 600 14px/1 inherit; cursor: pointer; }}
  .err {{ margin-bottom: 12px; padding: 9px 12px; border-radius: 8px; background: rgba(251,113,133,.14); border: 1px solid rgba(251,113,133,.34); color: #fb7185; font-size: 13px; }}
</style>
</head>
<body>
  <div class="card">
    <h1>{escaped_title}</h1>
    <p>Enter the 6-digit passcode to open this shared link.</p>
    {error_html}
    <form method="POST" action="{UNLOCK_PATH}">
      <input name="passcode" type="text" inputmode="numeric" autocomplete="one-time-code" maxlength="12" placeholder="000000" autofocus required>
      <button type="submit">Continue</button>
    </form>
  </div>
</body>
</html>"#
    )
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
