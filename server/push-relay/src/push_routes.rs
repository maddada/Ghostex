//! The `/push/` routes: visible and background notifications, widget pushes, the token mailbox and the widget
//! snapshot (which the computer is meant to encrypt; the relay does not check). They have their own limits,
//! budget, counters and memory of tokens Apple dropped, so none of these changes what the Live Activity route
//! allows or reports. They share its Apple client (connection and provider token), the relay-wide places for
//! request bodies (`ReadBody`) and the process with its memory limit (README, "Memory").
//!
//! A mailbox or snapshot id is a capability, so it travels in the `Authorization` header (`Bearer <id>`), never in
//! the URL, where a proxy's logs would keep it.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
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
use crate::request::{valid_token, Environment, Refusal};
use crate::store::{PutRefusal, Shelf, Store};
use crate::{
    answer, caller_key, rate_limited, refusal, token_digest, token_label, unix_now, ReadBody,
    Relay, GONE_TOKEN_MEMORY,
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

/// A token Apple dropped, for one environment and one topic.
type GoneKey = (Environment, &'static str, [u8; 32]);

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
    gone: Mutex<HashMap<GoneKey, Instant>>,
    max_gone: usize,
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
            gone: Mutex::new(HashMap::new()),
            max_gone: limits.push_max_gone_tokens.max(1),
            counters: Counters::default(),
        }
    }

    /// Forgets idle buckets, expired entries and old gone tokens.
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
        self.gone
            .lock()
            .expect("gone lock")
            .retain(|_, since| now.duration_since(*since) < GONE_TOKEN_MEMORY);
    }

    fn is_gone(&self, key: &GoneKey) -> bool {
        self.gone.lock().expect("gone lock").contains_key(key)
    }

    /// Remembers a token Apple dropped. Full, it forgets expired ones first, then the oldest.
    fn remember_gone(&self, key: GoneKey) {
        let now = Instant::now();
        let mut gone = self.gone.lock().expect("gone lock");
        if !gone.contains_key(&key) && gone.len() >= self.max_gone {
            gone.retain(|_, since| now.duration_since(*since) < GONE_TOKEN_MEMORY);
            if gone.len() >= self.max_gone {
                if let Some(oldest) = gone
                    .iter()
                    .min_by_key(|(_, since)| **since)
                    .map(|(key, _)| *key)
                {
                    gone.remove(&oldest);
                }
            }
        }
        gone.insert(key, now);
    }
}

/// The `/push/` routes, with their own body limit.
pub fn router() -> Router<Arc<Relay>> {
    Router::new()
        .route(&format!("{PUSH_PREFIX}/healthz"), get(health))
        .route(&format!("{PUSH_PREFIX}/v1/alert"), post(alert))
        .route(&format!("{PUSH_PREFIX}/v1/widgets"), post(widgets))
        .route(
            &format!("{PUSH_PREFIX}/v1/mailbox"),
            get(mailbox_take).put(mailbox_put).head(mailbox_head),
        )
        .route(
            &format!("{PUSH_PREFIX}/v1/snapshot"),
            get(snapshot_get).put(snapshot_put),
        )
        .layer(DefaultBodyLimit::max(MAX_PUSH_BODY_BYTES))
}

async fn health(State(relay): State<Arc<Relay>>) -> Response {
    let counters = &relay.push.counters;
    let held = relay.push.store.counts();
    let gone_tokens = relay.push.gone.lock().expect("gone lock").len();
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
    body: ReadBody,
) -> Response {
    push(&relay, peer, &headers, |now| {
        let request: AlertRequest = serde_json::from_slice(&body.bytes).map_err(|error| {
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
    body: ReadBody,
) -> Response {
    push(&relay, peer, &headers, |now| {
        let request: WidgetsRequest = serde_json::from_slice(&body.bytes)
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
    // As on the Live Activity route: every request spends its caller's bucket; the relay-wide one is spent after
    // the relay's own checks and before the wait for a slot, so a busy refusal has spent it too.
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
    let gone_key = (push.environment, push.push_type.topic(), digest);
    if routes.is_gone(&gone_key) {
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
                routes.remember_gone(gone_key);
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

/// The id in `Authorization: Bearer <id>`: 64 hex characters.
fn bearer_id(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, id) = value.split_once(' ')?;
    let id = id.trim();
    (scheme.eq_ignore_ascii_case("bearer")
        && id.len() == ID_HEX_LEN
        && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then_some(id)
}

/// The caller's store bucket and the id from `Authorization`. The answer when either refuses. The id's own rate is
/// counted by the store, and only for an id it holds or admits.
fn admit_store_request(
    relay: &Relay,
    shelf: Shelf,
    peer: SocketAddr,
    headers: &HeaderMap,
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
    let Some(id) = bearer_id(headers) else {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!("{name} refused reason=bad-request"));
        return Err(Box::new(refusal(&Refusal::bad(
            "The id goes in the Authorization header: Bearer and 256 bits in hex (64 characters).",
        ))));
    };
    let key: [u8; 32] = Sha256::digest(id.to_ascii_lowercase().as_bytes()).into();
    Ok((caller, key))
}

fn id_limiter(routes: &PushRoutes, shelf: Shelf) -> &Limiter<[u8; 32]> {
    match shelf {
        Shelf::Mailbox => &routes.per_mailbox_id,
        Shelf::Snapshot => &routes.per_snapshot_id,
    }
}

fn id_rate_limited(relay: &Relay, shelf: Shelf, key: &[u8; 32], wait: Duration) -> Response {
    relay
        .push
        .counters
        .rate_limited
        .fetch_add(1, Ordering::Relaxed);
    relay.log(&format!(
        "{} refused reason=rate-limited scope=id id={}",
        shelf.as_str(),
        token_label(key)
    ));
    rate_limited(wait)
}

fn too_large(detail: String) -> Response {
    refusal(&Refusal {
        status: 413,
        code: "too-large",
        detail,
    })
}

/// Stores the `len` bytes `make` builds under `key`, once the store admitted it (`Store::put` has the order).
fn store_put(
    relay: &Relay,
    shelf: Shelf,
    caller: IpAddr,
    key: [u8; 32],
    len: usize,
    now: Instant,
    make: impl FnOnce() -> Vec<u8>,
) -> Response {
    let routes = &relay.push;
    let counters = &routes.counters;
    let (name, label) = (shelf.as_str(), token_label(&key));
    let stored = routes.store.put(
        shelf,
        key,
        len,
        now,
        || routes.new_ids_per_address.take(&caller, now),
        || id_limiter(routes, shelf).take(&key, now),
        make,
    );
    match stored {
        Ok(()) => {
            counters.stored.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!("{name} stored id={label} bytes={len}"));
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
        Err(PutRefusal::IdLimit(wait)) => id_rate_limited(relay, shelf, &key, wait),
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
    headers: HeaderMap,
    body: ReadBody,
) -> Response {
    let now = Instant::now();
    let (caller, key) = match admit_store_request(&relay, Shelf::Mailbox, peer, &headers, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let counters = &relay.push.counters;
    if body.bytes.len() > MAX_MAILBOX_BODY_BYTES {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("mailbox refused reason=too-large");
        return too_large(format!(
            "The body is {} bytes; at most {MAX_MAILBOX_BODY_BYTES}.",
            body.bytes.len()
        ));
    }
    let token = std::str::from_utf8(&body.bytes).unwrap_or_default().trim();
    if !valid_token(token) {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("mailbox refused reason=bad-request");
        return refusal(&Refusal::bad(
            "The body must be a push token in hex (64 to 512 characters).",
        ));
    }
    store_put(
        &relay,
        Shelf::Mailbox,
        caller,
        key,
        token.len(),
        now,
        || token.to_ascii_lowercase().into_bytes(),
    )
}

/// A HEAD must not take the token a GET hands out once, so the mailbox refuses it.
async fn mailbox_head() -> Response {
    let mut response = answer(
        405,
        json!({ "ok": false, "error": "method-not-allowed",
            "detail": "The mailbox is read with GET, which takes the entry." }),
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, PUT"));
    response
}

async fn mailbox_take(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let now = Instant::now();
    let (_, key) = match admit_store_request(&relay, Shelf::Mailbox, peer, &headers, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let routes = &relay.push;
    match routes
        .store
        .take_mailbox(&key, now, || routes.per_mailbox_id.take(&key, now))
    {
        Ok(Some(value)) => {
            relay.log(&format!("mailbox taken id={}", token_label(&key)));
            // Stored only after `valid_token`, so it is ASCII hex.
            let token = String::from_utf8(value).unwrap_or_default();
            answer(200, json!({ "ok": true, "token": token }))
        }
        Ok(None) => empty(),
        Err(wait) => id_rate_limited(&relay, Shelf::Mailbox, &key, wait),
    }
}

async fn snapshot_put(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: ReadBody,
) -> Response {
    let now = Instant::now();
    let (caller, key) = match admit_store_request(&relay, Shelf::Snapshot, peer, &headers, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let counters = &relay.push.counters;
    if body.bytes.len() > MAX_SNAPSHOT_BYTES {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("snapshot refused reason=too-large");
        return too_large(format!(
            "The snapshot is {} bytes; at most {MAX_SNAPSHOT_BYTES}.",
            body.bytes.len()
        ));
    }
    if body.bytes.is_empty() {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("snapshot refused reason=bad-request");
        return refusal(&Refusal::bad("The snapshot is empty."));
    }
    store_put(
        &relay,
        Shelf::Snapshot,
        caller,
        key,
        body.bytes.len(),
        now,
        || body.bytes.to_vec(),
    )
}

async fn snapshot_get(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let now = Instant::now();
    let (_, key) = match admit_store_request(&relay, Shelf::Snapshot, peer, &headers, now) {
        Ok(admitted) => admitted,
        Err(response) => return *response,
    };
    let routes = &relay.push;
    match routes
        .store
        .snapshot(&key, now, || routes.per_snapshot_id.take(&key, now))
    {
        Ok(Some(value)) => {
            let mut response = value.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Ok(None) => empty(),
        Err(wait) => id_rate_limited(&relay, Shelf::Snapshot, &key, wait),
    }
}
