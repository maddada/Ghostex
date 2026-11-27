//! The `/push/` routes: visible and background notifications, widget pushes, the token mailbox and the encrypted
//! widget snapshot. They have their own limits, budget and counters, so nothing here takes from the Live Activity
//! route; they share its Apple connection and its memory of tokens Apple dropped.

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::apns::PushType;
use crate::limits::{LimitConfig, Limiter};
use crate::native::{prepare_alert, prepare_widgets, AlertRequest, NativePush, WidgetsRequest};
use crate::request::{valid_token, Refusal};
use crate::store::{PutRefusal, Shelf, Store};
use crate::{
    answer, caller_key, rate_limited, refusal, token_digest, token_label, unix_now, Relay,
};

/// The path prefix Caddy routes to the relay for these routes.
pub const PUSH_PREFIX: &str = "/push";
/// A request body here: an alert's fields with its envelope, or a snapshot (refused over `MAX_SNAPSHOT_BYTES`).
pub const MAX_PUSH_BODY_BYTES: usize = 16 * 1024;
/// The largest snapshot the relay keeps.
pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024;
/// The largest mailbox body (an update token in hex is at most 512 characters).
pub const MAX_MAILBOX_BODY_BYTES: usize = 1024;
/// A mailbox or snapshot id: 256 bits in hex.
const ID_HEX_LEN: usize = 64;

#[derive(Default)]
struct Counters {
    sent: AtomicU64,
    refused: AtomicU64,
    rate_limited: AtomicU64,
    apple_errors: AtomicU64,
    stored: AtomicU64,
    store_full: AtomicU64,
}

pub struct PushRoutes {
    per_address: Limiter<IpAddr>,
    per_alert_token: Limiter<[u8; 32]>,
    per_background_token: Limiter<[u8; 32]>,
    per_widgets_token: Limiter<[u8; 32]>,
    global: Limiter<()>,
    in_flight: Semaphore,
    store_per_address: Limiter<IpAddr>,
    new_ids_per_address: Limiter<IpAddr>,
    per_mailbox_id: Limiter<[u8; 32]>,
    per_snapshot_id: Limiter<[u8; 32]>,
    store: Store,
    counters: Counters,
}

impl PushRoutes {
    pub fn new(limits: &LimitConfig) -> Self {
        let max_keys = limits.push_max_keys;
        PushRoutes {
            per_address: Limiter::with_max_keys(limits.push_per_address, max_keys),
            per_alert_token: Limiter::with_max_keys(limits.alert_per_token, max_keys),
            per_background_token: Limiter::with_max_keys(limits.background_per_token, max_keys),
            per_widgets_token: Limiter::with_max_keys(limits.widgets_per_token, max_keys),
            global: Limiter::new(limits.push_global),
            in_flight: Semaphore::new(limits.push_in_flight),
            store_per_address: Limiter::with_max_keys(limits.store_per_address, max_keys),
            new_ids_per_address: Limiter::with_max_keys(limits.new_ids_per_address, max_keys),
            per_mailbox_id: Limiter::with_max_keys(limits.per_mailbox_id, max_keys),
            per_snapshot_id: Limiter::with_max_keys(limits.per_snapshot_id, max_keys),
            store: Store::new(limits),
            counters: Counters::default(),
        }
    }

    /// Forgets idle buckets and expired entries.
    pub fn prune(&self, now: Instant) {
        self.per_address.prune(now);
        self.per_alert_token.prune(now);
        self.per_background_token.prune(now);
        self.per_widgets_token.prune(now);
        self.store_per_address.prune(now);
        self.new_ids_per_address.prune(now);
        self.per_mailbox_id.prune(now);
        self.per_snapshot_id.prune(now);
        self.store.prune(now);
    }
}

/// The `/push/` routes, with their own body limit.
pub fn router() -> Router<Arc<Relay>> {
    Router::new()
        .route(&format!("{PUSH_PREFIX}/healthz"), get(health))
        .route(&format!("{PUSH_PREFIX}/v1/alert"), post(alert))
        .route(&format!("{PUSH_PREFIX}/v1/widgets"), post(widgets))
        .route(
            &format!("{PUSH_PREFIX}/v1/mailbox/{{id}}"),
            get(mailbox_take).put(mailbox_put),
        )
        .route(
            &format!("{PUSH_PREFIX}/v1/snapshot/{{id}}"),
            get(snapshot_get).put(snapshot_put),
        )
        .layer(DefaultBodyLimit::max(MAX_PUSH_BODY_BYTES))
}

async fn health(State(relay): State<Arc<Relay>>) -> Response {
    let counters = &relay.push.counters;
    let held = relay.push.store.counts();
    let gone_tokens = relay.gone.lock().expect("gone lock").len();
    answer(
        200,
        json!({
            "ok": true,
            "configured": relay.apple.is_ok(),
            "sent": counters.sent.load(Ordering::Relaxed),
            "refused": counters.refused.load(Ordering::Relaxed),
            "rateLimited": counters.rate_limited.load(Ordering::Relaxed),
            "appleErrors": counters.apple_errors.load(Ordering::Relaxed),
            "stored": counters.stored.load(Ordering::Relaxed),
            "storeFull": counters.store_full.load(Ordering::Relaxed),
            "mailboxEntries": held.mailbox,
            "snapshots": held.snapshots,
            "storeBytes": held.bytes,
            "goneTokens": gone_tokens,
        }),
    )
}

async fn alert(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    push(&relay, peer, &headers, |now| {
        let request: AlertRequest = serde_json::from_slice(&body).map_err(|error| {
            // The caller's own data comes back to the caller; the log keeps only the reason.
            Refusal::bad(format!("The request is not an alert push: {error}"))
        })?;
        prepare_alert(request, now)
    })
    .await
}

async fn widgets(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    push(&relay, peer, &headers, |now| {
        let request: WidgetsRequest = serde_json::from_slice(&body)
            .map_err(|error| Refusal::bad(format!("The request is not a widget push: {error}")))?;
        prepare_widgets(request, now)
    })
    .await
}

fn token_gone() -> Response {
    answer(
        410,
        json!({ "ok": false, "error": "token-gone", "detail": "Apple no longer takes this token." }),
    )
}

/// One push of the `/push/` routes, from the caller's limit to Apple's answer.
async fn push(
    relay: &Relay,
    peer: SocketAddr,
    headers: &HeaderMap,
    prepare: impl FnOnce(i64) -> Result<NativePush, Refusal>,
) -> Response {
    let routes = &relay.push;
    let counters = &routes.counters;
    let caller = caller_key(relay.caller(peer, headers));
    let now = Instant::now();
    // As on the Live Activity route: every request spends its caller's bucket, only one about to go to Apple the
    // relay-wide one.
    if let Err(wait) = routes.per_address.take(&caller, now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log("push refused reason=rate-limited scope=address");
        return rate_limited(wait);
    }
    let Ok(apple) = relay.apple.as_ref() else {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("push refused reason=not-configured");
        return answer(
            503,
            json!({ "ok": false, "error": "not-configured",
                "detail": "This relay has no Apple push key yet, so it sends nothing." }),
        );
    };
    let push = match prepare(unix_now()) {
        Ok(push) => push,
        Err(refused) => {
            counters.refused.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!("push refused reason={}", refused.code));
            return refusal(&refused);
        }
    };
    let digest = token_digest(&push.token);
    let label = token_label(&digest);
    let (kind, environment) = (push.kind, push.environment.as_str());
    if relay
        .gone
        .lock()
        .expect("gone lock")
        .contains_key(&(push.environment, digest))
    {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "push refused reason=token-gone kind={kind} env={environment} token={label}"
        ));
        return token_gone();
    }
    let bucket = match push.push_type {
        PushType::Alert => &routes.per_alert_token,
        PushType::Background => &routes.per_background_token,
        PushType::Widgets => &routes.per_widgets_token,
        PushType::LiveActivity => unreachable!("the Live Activity route sends its own pushes"),
    };
    if let Err(wait) = bucket.take(&digest, now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "push refused reason=rate-limited scope=token kind={kind} token={label}"
        ));
        return rate_limited(wait);
    }
    if let Err(wait) = routes.global.take(&(), now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "push refused reason=rate-limited scope=global kind={kind}"
        ));
        return rate_limited(wait);
    }
    let Ok(_permit) =
        tokio::time::timeout(Duration::from_secs(5), routes.in_flight.acquire()).await
    else {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log("push refused reason=busy");
        return rate_limited(Duration::from_secs(5));
    };
    let started = Instant::now();
    let result = apple.send(&push.outgoing()).await;
    let ms = started.elapsed().as_millis();
    match result {
        Ok(apple_answer) => {
            let reason = apple_answer.reason.clone().unwrap_or_default();
            relay.log(&format!(
                "push sent kind={kind} env={environment} token={label} status={} reason={} ms={ms}",
                apple_answer.status,
                if reason.is_empty() { "-" } else { &reason }
            ));
            if apple_answer.status == 200 {
                counters.sent.fetch_add(1, Ordering::Relaxed);
                return answer(200, json!({ "ok": true, "apnsId": apple_answer.apns_id }));
            }
            counters.apple_errors.fetch_add(1, Ordering::Relaxed);
            let gone = apple_answer.status == 410
                || matches!(
                    reason.as_str(),
                    "BadDeviceToken" | "DeviceTokenNotForTopic" | "Unregistered"
                );
            if gone {
                relay.remember_gone((push.environment, digest));
                return answer(
                    410,
                    json!({ "ok": false, "error": "token-gone", "detail": "Apple no longer takes this token.",
                        "appleStatus": apple_answer.status, "appleReason": reason }),
                );
            }
            let status = if apple_answer.status == 429 { 429 } else { 502 };
            answer(
                status,
                json!({ "ok": false, "error": "apple", "detail": "Apple did not take the push.",
                    "appleStatus": apple_answer.status, "appleReason": reason }),
            )
        }
        Err(error) => {
            counters.apple_errors.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!(
                "push failed kind={kind} env={environment} token={label} error=\"{error}\" ms={ms}"
            ));
            answer(
                502,
                json!({ "ok": false, "error": "apple-unreachable", "detail": error }),
            )
        }
    }
}

/// The caller's store bucket, the id's form and the id's own bucket. The answer when one refuses.
fn admit_store_request(
    relay: &Relay,
    shelf: Shelf,
    peer: SocketAddr,
    headers: &HeaderMap,
    id: &str,
    now: Instant,
) -> Result<(IpAddr, [u8; 32]), Box<Response>> {
    let routes = &relay.push;
    let counters = &routes.counters;
    let caller = caller_key(relay.caller(peer, headers));
    let name = shelf.as_str();
    if let Err(wait) = routes.store_per_address.take(&caller, now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!("{name} refused reason=rate-limited scope=address"));
        return Err(Box::new(rate_limited(wait)));
    }
    if id.len() != ID_HEX_LEN || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!("{name} refused reason=bad-request"));
        return Err(Box::new(refusal(&Refusal::bad(
            "The id must be 256 bits in hex (64 characters).",
        ))));
    }
    let key: [u8; 32] = Sha256::digest(id.to_ascii_lowercase().as_bytes()).into();
    let bucket = match shelf {
        Shelf::Mailbox => &routes.per_mailbox_id,
        Shelf::Snapshot => &routes.per_snapshot_id,
    };
    if let Err(wait) = bucket.take(&key, now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "{name} refused reason=rate-limited scope=id id={}",
            token_label(&key)
        ));
        return Err(Box::new(rate_limited(wait)));
    }
    Ok((caller, key))
}

fn too_large(detail: String) -> Response {
    refusal(&Refusal {
        status: 413,
        code: "too-large",
        detail,
    })
}

/// Stores `value` under `key`; a new id spends the caller's quota for new ids first.
fn store_put(
    relay: &Relay,
    shelf: Shelf,
    caller: IpAddr,
    key: [u8; 32],
    value: Vec<u8>,
    now: Instant,
) -> Response {
    let routes = &relay.push;
    let counters = &routes.counters;
    let (name, label, bytes) = (shelf.as_str(), token_label(&key), value.len());
    let stored = routes.store.put(shelf, key, value, now, || {
        routes.new_ids_per_address.take(&caller, now)
    });
    match stored {
        Ok(()) => {
            counters.stored.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!("{name} stored id={label} bytes={bytes}"));
            answer(200, json!({ "ok": true }))
        }
        Err(PutRefusal::Quota(wait)) => {
            counters.rate_limited.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!(
                "{name} refused reason=too-many-new-ids id={label}"
            ));
            let mut response = answer(
                429,
                json!({ "ok": false, "error": "too-many-new-ids",
                    "detail": "This address created too many new ids; try again later.",
                    "retryAfter": wait.as_secs() }),
            );
            if let Ok(value) = HeaderValue::from_str(&wait.as_secs().to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
            response
        }
        Err(PutRefusal::Full) => {
            counters.store_full.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!("{name} refused reason=store-full id={label}"));
            let mut response = answer(
                503,
                json!({ "ok": false, "error": "store-full",
                    "detail": "The relay holds as much as it may; try again later.", "retryAfter": 60 }),
            );
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
            response
        }
    }
}

fn empty() -> Response {
    answer(
        404,
        json!({ "ok": false, "error": "empty", "detail": "Nothing is held under this id." }),
    )
}

async fn mailbox_put(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let now = Instant::now();
    let (caller, key) = match admit_store_request(&relay, Shelf::Mailbox, peer, &headers, &id, now)
    {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let counters = &relay.push.counters;
    if body.len() > MAX_MAILBOX_BODY_BYTES {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("mailbox refused reason=too-large");
        return too_large(format!(
            "The body is {} bytes; at most {MAX_MAILBOX_BODY_BYTES}.",
            body.len()
        ));
    }
    let token = std::str::from_utf8(&body).unwrap_or_default().trim();
    if !valid_token(token) {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("mailbox refused reason=bad-request");
        return refusal(&Refusal::bad(
            "The body must be a push token in hex (64 to 512 characters).",
        ));
    }
    let value = token.to_ascii_lowercase().into_bytes();
    store_put(&relay, Shelf::Mailbox, caller, key, value, now)
}

async fn mailbox_take(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let now = Instant::now();
    let (_, key) = match admit_store_request(&relay, Shelf::Mailbox, peer, &headers, &id, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    match relay.push.store.take_mailbox(&key, now) {
        Some(value) => {
            relay.log(&format!("mailbox taken id={}", token_label(&key)));
            // Stored only after `valid_token`, so it is ASCII hex.
            let token = String::from_utf8(value).unwrap_or_default();
            answer(200, json!({ "ok": true, "token": token }))
        }
        None => empty(),
    }
}

async fn snapshot_put(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let now = Instant::now();
    let (caller, key) = match admit_store_request(&relay, Shelf::Snapshot, peer, &headers, &id, now)
    {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let counters = &relay.push.counters;
    if body.len() > MAX_SNAPSHOT_BYTES {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("snapshot refused reason=too-large");
        return too_large(format!(
            "The snapshot is {} bytes; at most {MAX_SNAPSHOT_BYTES}.",
            body.len()
        ));
    }
    if body.is_empty() {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("snapshot refused reason=bad-request");
        return refusal(&Refusal::bad("The snapshot is empty."));
    }
    store_put(&relay, Shelf::Snapshot, caller, key, body.to_vec(), now)
}

async fn snapshot_get(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let now = Instant::now();
    let (_, key) = match admit_store_request(&relay, Shelf::Snapshot, peer, &headers, &id, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    match relay.push.store.snapshot(&key, now) {
        Some(value) => {
            let mut response = value.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        None => empty(),
    }
}
