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
    /*
    CDXC:Docs 2026-11-14 DECISION:
    A file link (`/open-file/`) answers with its own cross-origin rule (fonts and, by the user's
    choice Q3119, `.js` and `.mjs` files so module scripts keep loading; file_links/serve.rs), never
    the API's: the API lets the `null` origin read it for the app's own file:// pages, and an HTML
    or Markdown page served from a link runs in a sandbox whose origin is `null` too, so it could
    otherwise read every file of the link's folder.
    */
    let file_link = request
        .uri()
        .path()
        .starts_with(crate::file_links::FILE_LINK_ROUTE_PREFIX);
    let request_id = request_id(&headers);
    let gzip_wanted = super::link_compression::wants_gzip(&headers);
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
    if !file_link {
        apply_cors_headers(&headers, &mut routed.response, &state.config);
    }
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
    if let Some(failure) = routed.response.extensions().get::<BodyReadFailure>() {
        details["errorCode"] = json!(failure.code);
        details["receivedBytes"] = json!(failure.received_bytes);
        details["declaredBytes"] = json!(failure.declared_bytes);
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
    let routed_response = super::link_compression::gzip_answer(gzip_wanted, routed.response).await;
    // CDXC:ServerApi 2026-10-23 DECISION:
    // The user chose to stop broadcasting an `apiRequestHandled` frame for every HTTP request served, because no client reads it: the desktop, the web build and `gx-client` dropped it unparsed, nothing in the phone app, the `ghostex` CLI or the extensions reads it, and it was about five frames a second to every open event socket. Clients keep tolerating it (`ServerEvent::ApiRequestHandled`, the `gx-client` peek), because gxservers of older releases still send it.
    Ok(routed_response)
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
    blocking_section(|| domain_http_as(state, endpoint_path, request_id, body, handler))
}

/// Runs `work`, which blocks (SQLite, a file read), without holding up the other tasks on this
/// async worker: on gxserver's multi-thread runtime the worker hands its queue to another thread
/// first (`tokio::task::block_in_place`), and `work` stays on this thread, so thread-locals and the
/// request's task-locals still read as before. Anywhere else (a test's current-thread runtime, a
/// plain or blocking-pool thread) `work` simply runs.
///
/// CDXC:ServerApi 2026-11-18 WHY:
/// GitHub #666 (Windows, about 40 agents): gxserver's runtime heartbeat went quiet for up to 131 s
/// and every request, git reads included, sat unanswered, with "database is locked" in the log.
/// The domain routes (`listSessions`, `findSessions` and some 80 more) ran SQLite straight on the
/// async workers, each waiting up to 5 s on a busy database, so a burst of them could fill every
/// worker while the finished git processes and the heartbeat waited behind them.
pub(crate) fn blocking_section<R>(work: impl FnOnce() -> R) -> R {
    // Already inside one on this thread (a route that reaches another through `block_on`): it holds
    // its permit and is off the worker already.
    if HOLDS_SQLITE_PERMIT.get() {
        return work();
    }
    let run = || {
        let _permit = SqlitePermit::acquire();
        work()
    };
    let multi_thread = tokio::runtime::Handle::try_current().is_ok_and(|runtime| {
        runtime.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
    });
    match multi_thread.then(BlockedSection::enter).flatten() {
        Some(_section) => tokio::task::block_in_place(run),
        None => run(),
    }
}

/*
CDXC:ServerApi 2026-11-18 WHY:
Review of the change above: with the workers no longer the bound, a whole burst of domain requests
reached SQLite at once, and SQLite's busy wait (5 s, `storage/pool.rs`) is not fair, so writes
that used to queue and then succeed failed ("Ghostex couldn't save that change"). So at most
`sqlite_permits` sections use SQLite at once; the rest wait for a permit off the workers, without
spending their busy wait. Measured on a test gxserver (18 workers), 60 saveStashedPrompt writes
behind a write lock: held 6 s, 5 failed before the change above, 59 with it, 18 with one permit
per worker and 4 with 4 permits; held 2 s, 0, 21 to 38, 8 and 0.
More permits only let more writers give up at once (one that read before writing gets "busy"
without waiting at all), so the bound is a quarter of the workers, at least 4, which also keeps
the open connections within the pool's idle set. At most `MAX_BLOCKED_SECTIONS` wait off the
workers at once, each holding one of tokio's blocking threads (shared with git reads and every
`spawn_blocking`); past that a section runs on its worker, as before.
*/
const MAX_BLOCKED_SECTIONS: usize = 128;

fn sqlite_permits() -> usize {
    static PERMITS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *PERMITS.get_or_init(|| {
        let workers = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(4);
        (workers / 4).max(4)
    })
}

thread_local! {
    static HOLDS_SQLITE_PERMIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct SqlitePermits {
    used: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
}

static SQLITE_PERMITS: SqlitePermits = SqlitePermits {
    used: std::sync::Mutex::new(0),
    freed: std::sync::Condvar::new(),
};

struct SqlitePermit;

impl SqlitePermit {
    fn acquire() -> Self {
        let limit = sqlite_permits();
        let mut used = SQLITE_PERMITS
            .used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *used >= limit {
            used = SQLITE_PERMITS
                .freed
                .wait(used)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *used += 1;
        HOLDS_SQLITE_PERMIT.set(true);
        SqlitePermit
    }
}

impl Drop for SqlitePermit {
    fn drop(&mut self) {
        HOLDS_SQLITE_PERMIT.set(false);
        let mut used = SQLITE_PERMITS
            .used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *used = used.saturating_sub(1);
        drop(used);
        SQLITE_PERMITS.freed.notify_one();
    }
}

static BLOCKED_SECTIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// One section waiting or running off its worker; `None` once `MAX_BLOCKED_SECTIONS` are.
struct BlockedSection;

impl BlockedSection {
    fn enter() -> Option<Self> {
        use std::sync::atomic::Ordering;
        if BLOCKED_SECTIONS.fetch_add(1, Ordering::AcqRel) >= MAX_BLOCKED_SECTIONS {
            BLOCKED_SECTIONS.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(BlockedSection)
    }
}

impl Drop for BlockedSection {
    fn drop(&mut self) {
        BLOCKED_SECTIONS.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

fn domain_http_as<R, F>(
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
