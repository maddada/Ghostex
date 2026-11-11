//! Ghostex's Live Activity push relay.
//!
//! A user's computer (gxserver) keeps the iPhone's Live Activity current while the phone is locked by asking
//! this relay to send Apple a Live Activity push: an update, an end, or a start (iOS 17.2+). Only the relay
//! holds the Apple push key; a computer never sees it. Expo's push service, which carries Ghostex's
//! notifications, does not carry Live Activity pushes, and the key cannot go on users' computers.
//!
//! The relay sends Live Activity pushes for one topic and nothing else: the topic, the push type and Apple's
//! host (production or sandbox) are fixed here, and a request naming any of them is refused. Like Expo's push
//! service it is open: the device token is what lets someone push to a phone, and only the computers that
//! phone paired with receive it. What someone else can do with the relay is in `README.md`.
//!
//! It logs counts, Apple's status and reason, and an 8-hex hash of a token; never a token, a content state or
//! anything from the key's files.
//!
//! CDXC:Notifications 2026-11-11 DECISION: User, on keeping the Lock Screen banner current: "yes sure do it
//! now but can we have it on same vps we have already???". The relay runs on relay-eu.ghostex.dev beside the
//! Easy Connect relay and the share relay, behind the same Caddy, at share.ghostex.dev/live-activity/.

pub mod apns;
pub mod limits;
pub mod request;

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::apns::Apple;
use crate::limits::{LimitConfig, Limiter};
use crate::request::{Environment, Event, Refusal, SendRequest};

/// The iPhone app's bundle id with Apple's Live Activity suffix: the only topic the relay sends to.
pub const TOPIC: &str = "com.maddada.ghostex.ios.push-type.liveactivity";
/// Apple's limit for a Live Activity push.
pub const MAX_APNS_PAYLOAD_BYTES: usize = 4096;
/// A request body: the content state plus a few fields.
pub const MAX_BODY_BYTES: usize = 8 * 1024;
/// A token Apple called unknown or expired is refused here this long without asking Apple again.
pub const GONE_TOKEN_MEMORY: Duration = Duration::from_secs(24 * 3600);
/// The path prefix Caddy routes to the relay on share.ghostex.dev.
pub const PREFIX: &str = "/live-activity";

pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Options {
    /// Take the caller's address from the last `X-Forwarded-For` entry, the one Caddy adds.
    pub behind_proxy: bool,
    /// Apple's service with the key, or why there is none (`not configured`).
    pub apple: Result<Apple, String>,
    pub limits: LimitConfig,
    pub log: LogSink,
}

#[derive(Default)]
struct Counters {
    sent: AtomicU64,
    refused: AtomicU64,
    rate_limited: AtomicU64,
    apple_errors: AtomicU64,
}

pub struct Relay {
    behind_proxy: bool,
    apple: Result<Apple, String>,
    log: LogSink,
    per_address: Limiter<IpAddr>,
    per_token: Limiter<[u8; 32]>,
    per_start_token: Limiter<[u8; 32]>,
    global: Limiter<()>,
    in_flight: Semaphore,
    gone: Mutex<HashMap<(Environment, [u8; 32]), Instant>>,
    counters: Counters,
}

fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// What the log calls a token: the first 8 hex of its SHA-256.
fn token_label(digest: &[u8; 32]) -> String {
    hex::encode(&digest[..4])
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs() as i64)
        .unwrap_or(0)
}

fn answer(status: u16, body: serde_json::Value) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn refusal(refusal: &Refusal) -> Response {
    answer(
        refusal.status,
        json!({ "ok": false, "error": refusal.code, "detail": refusal.detail }),
    )
}

fn rate_limited(wait: Duration) -> Response {
    let mut response = answer(
        429,
        json!({ "ok": false, "error": "rate-limited", "detail": "Too many pushes; try again later.",
            "retryAfter": wait.as_secs() }),
    );
    if let Ok(value) = HeaderValue::from_str(&wait.as_secs().to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

impl Relay {
    pub fn new(options: Options) -> Arc<Self> {
        let limits = options.limits;
        Arc::new(Relay {
            behind_proxy: options.behind_proxy,
            apple: options.apple,
            log: options.log,
            per_address: Limiter::new(limits.per_address),
            per_token: Limiter::new(limits.per_token),
            per_start_token: Limiter::new(limits.per_start_token),
            global: Limiter::new(limits.global),
            in_flight: Semaphore::new(limits.in_flight),
            gone: Mutex::new(HashMap::new()),
            counters: Counters::default(),
        })
    }

    fn log(&self, line: &str) {
        (self.log)(line);
    }

    /// The relay's routes under `PREFIX`.
    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route(&format!("{PREFIX}/healthz"), get(health))
            .route(&format!("{PREFIX}/v1/send"), post(send))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(self.clone())
    }

    /// Forgets idle buckets and old gone tokens; run once a minute.
    pub fn prune(&self) {
        let now = Instant::now();
        self.per_address.prune(now);
        self.per_token.prune(now);
        self.per_start_token.prune(now);
        self.gone
            .lock()
            .expect("gone lock")
            .retain(|_, since| now.duration_since(*since) < GONE_TOKEN_MEMORY);
    }

    fn caller(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        if self.behind_proxy {
            let forwarded = headers
                .get_all("x-forwarded-for")
                .iter()
                .filter_map(|value| value.to_str().ok())
                .flat_map(|value| value.split(','))
                .filter_map(|entry| entry.trim().parse::<IpAddr>().ok())
                .last();
            if let Some(address) = forwarded {
                return address;
            }
        }
        peer.ip()
    }
}

async fn health(State(relay): State<Arc<Relay>>) -> Response {
    let counters = &relay.counters;
    answer(
        200,
        json!({
            "ok": true,
            "configured": relay.apple.is_ok(),
            "sent": counters.sent.load(Ordering::Relaxed),
            "refused": counters.refused.load(Ordering::Relaxed),
            "rateLimited": counters.rate_limited.load(Ordering::Relaxed),
            "appleErrors": counters.apple_errors.load(Ordering::Relaxed),
        }),
    )
}

async fn send(
    State(relay): State<Arc<Relay>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let counters = &relay.counters;
    let caller = relay.caller(peer, &headers);
    let now = Instant::now();
    // The caller's own bucket first, so one noisy address does not spend the relay's.
    if let Err(wait) = relay
        .per_address
        .take(&caller, now)
        .and_then(|()| relay.global.take(&(), now))
    {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log("refused reason=rate-limited scope=address");
        return rate_limited(wait);
    }
    let Ok(apple) = relay.apple.as_ref() else {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log("refused reason=not-configured");
        return answer(
            503,
            json!({ "ok": false, "error": "not-configured",
                "detail": "This relay has no Apple push key yet, so it sends nothing." }),
        );
    };
    let parsed: SendRequest = match serde_json::from_slice(&body) {
        Ok(parsed) => parsed,
        Err(error) => {
            counters.refused.fetch_add(1, Ordering::Relaxed);
            relay.log("refused reason=bad-request");
            // The caller's own data comes back to the caller; the log keeps only the reason.
            return refusal(&Refusal::bad(format!(
                "The request is not a Live Activity push: {error}"
            )));
        }
    };
    let push = match request::prepare(parsed, unix_now()) {
        Ok(push) => push,
        Err(refused) => {
            counters.refused.fetch_add(1, Ordering::Relaxed);
            relay.log(&format!("refused reason={}", refused.code));
            return refusal(&refused);
        }
    };
    let digest = token_digest(&push.token);
    let label = token_label(&digest);
    let (event, environment) = (push.event.as_str(), push.environment.as_str());
    if relay
        .gone
        .lock()
        .expect("gone lock")
        .contains_key(&(push.environment, digest))
    {
        counters.refused.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "refused reason=token-gone event={event} env={environment} token={label}"
        ));
        return answer(
            410,
            json!({ "ok": false, "error": "token-gone", "detail": "Apple no longer takes this token." }),
        );
    }
    let bucket = if push.event == Event::Start {
        &relay.per_start_token
    } else {
        &relay.per_token
    };
    if let Err(wait) = bucket.take(&digest, now) {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log(&format!(
            "refused reason=rate-limited scope=token event={event} token={label}"
        ));
        return rate_limited(wait);
    }
    let Ok(_permit) = tokio::time::timeout(Duration::from_secs(5), relay.in_flight.acquire()).await
    else {
        counters.rate_limited.fetch_add(1, Ordering::Relaxed);
        relay.log("refused reason=busy");
        return rate_limited(Duration::from_secs(5));
    };
    let started = Instant::now();
    let result = apple.send(&push).await;
    let ms = started.elapsed().as_millis();
    match result {
        Ok(apple_answer) => {
            let reason = apple_answer.reason.clone().unwrap_or_default();
            relay.log(&format!(
                "sent event={event} env={environment} token={label} status={} reason={} ms={ms}",
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
                relay
                    .gone
                    .lock()
                    .expect("gone lock")
                    .insert((push.environment, digest), Instant::now());
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
                "failed event={event} env={environment} token={label} error=\"{error}\" ms={ms}"
            ));
            answer(
                502,
                json!({ "ok": false, "error": "apple-unreachable", "detail": error }),
            )
        }
    }
}
