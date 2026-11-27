//! What a computer asks the `/push/` routes to send, checked, and the Apple payload built from it.
//!
//! An alert's visible words are the relay's own: the caller names an event (`kind`) and the relay shows that
//! event's fixed words from `ALERT_WORDS` under the fixed title `ALERT_TITLE`, so nobody can put their own words
//! on a lock screen through the relay and no agent, project or session name reaches Apple readable. The names
//! travel only inside `envelope`, encrypted on the computer for the phone's notification service extension; the
//! relay checks its size and that it is base64, and nothing else. A widget push carries nothing at all.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Map};

use crate::apns::{Outgoing, PushType};
use crate::request::{valid_token, Environment, Refusal};
use crate::MAX_APNS_PAYLOAD_BYTES;

/// The title every alert shows before the phone replaces it with the decrypted one.
pub const ALERT_TITLE: &str = "Ghostex";
/// Each event's fixed words, the ones gxserver's alerts use (`server/src/push/message.rs`, `AlertKind::words`).
pub const ALERT_WORDS: &[(&str, &str)] = &[
    ("finished", "Finished"),
    ("waiting", "Waiting for you"),
    ("question", "Has a question"),
    ("failed", "Ended with an error"),
    ("security", "Security: check this was you"),
    ("stuck", "Not moving: it may be stuck"),
    ("blocked", "Blocked: out of usage on every account"),
    ("threadReport", "Reported to its coordinator"),
    ("outOfMemory", "Stopped to free memory"),
];
/// The kind of a background push: no alert, only `content-available`.
pub const BACKGROUND_KIND: &str = "background";
/// The longest envelope, in base64 characters: leaves the fixed words, the thread id and Apple's keys room under
/// Apple's 4 KB.
pub const MAX_ENVELOPE_CHARS: usize = 3072;
const THREAD_ID_MAX: usize = 128;
/// How far the event's time may lie from the relay's clock, as for a Live Activity push.
const TIMESTAMP_PAST_S: i64 = 3600;
const TIMESTAMP_FUTURE_S: i64 = 300;
/// An alert is worth showing for an hour after its event (gxserver's `ALERT_TTL_MS`); a background refresh only
/// for a few minutes; a widget reload for an hour (Apple keeps only the latest for a device anyway).
const ALERT_TTL_S: i64 = 3600;
const BACKGROUND_TTL_S: i64 = 600;
const WIDGETS_TTL_S: i64 = 3600;

/// The body of `POST /push/v1/alert`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AlertRequest {
    /// The app's device token, hex.
    pub token: String,
    #[serde(default)]
    pub environment: Environment,
    /// An event of `ALERT_WORDS`, or `background`.
    pub kind: String,
    /// The encrypted title and body for the phone, standard base64. Alerts only.
    #[serde(default)]
    pub envelope: Option<String>,
    /// iOS stacks the alerts of one thread together. Alerts only.
    #[serde(default)]
    pub thread_id: Option<String>,
    /// Whether the alert plays the default sound. Alerts only.
    #[serde(default)]
    pub sound: Option<bool>,
    /// Unix seconds of the event.
    pub timestamp: i64,
}

/// The body of `POST /push/v1/widgets`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WidgetsRequest {
    /// The widget extension's push token, hex.
    pub token: String,
    #[serde(default)]
    pub environment: Environment,
}

/// A checked `/push/` request, ready for Apple.
#[derive(Debug, Clone)]
pub struct NativePush {
    pub token: String,
    pub environment: Environment,
    pub push_type: PushType,
    /// For the log: the event, `background` or `widgets`.
    pub kind: &'static str,
    pub priority: u8,
    pub payload: Vec<u8>,
    pub expiration: i64,
}

impl NativePush {
    pub fn outgoing(&self) -> Outgoing<'_> {
        Outgoing {
            token: &self.token,
            environment: self.environment,
            push_type: self.push_type,
            priority: self.priority,
            expiration: self.expiration,
            payload: &self.payload,
        }
    }
}

/// The fixed words of `kind`, with the table's own name for it.
pub fn alert_words(kind: &str) -> Option<(&'static str, &'static str)> {
    ALERT_WORDS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(name, words)| (*name, *words))
}

fn checked_token(token: &str) -> Result<String, Refusal> {
    if !valid_token(token) {
        return Err(Refusal::bad(
            "token must be a device token in hex (64 to 512 characters).",
        ));
    }
    Ok(token.to_ascii_lowercase())
}

fn too_large(payload: &[u8]) -> Refusal {
    Refusal {
        status: 413,
        code: "too-large",
        detail: format!(
            "The push is {} bytes; Apple takes at most {MAX_APNS_PAYLOAD_BYTES}.",
            payload.len()
        ),
    }
}

fn valid_thread_id(thread_id: &str) -> bool {
    (1..=THREAD_ID_MAX).contains(&thread_id.len())
        && thread_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

/// Checks an alert or background request at `now` (unix seconds) and builds Apple's payload.
pub fn prepare_alert(request: AlertRequest, now: i64) -> Result<NativePush, Refusal> {
    let token = checked_token(&request.token)?;
    if request.timestamp < now - TIMESTAMP_PAST_S || request.timestamp > now + TIMESTAMP_FUTURE_S {
        return Err(Refusal::bad(
            "timestamp must be the current time in unix seconds.",
        ));
    }
    if request.kind == BACKGROUND_KIND {
        if request.envelope.is_some() || request.thread_id.is_some() || request.sound.is_some() {
            return Err(Refusal::bad(
                "A background push carries no envelope, threadId or sound.",
            ));
        }
        let payload = serde_json::to_vec(&json!({ "aps": { "content-available": 1 } }))
            .expect("a JSON value serializes");
        return Ok(NativePush {
            token,
            environment: request.environment,
            push_type: PushType::Background,
            kind: BACKGROUND_KIND,
            // Apple takes a background push only at priority 5.
            priority: 5,
            payload,
            expiration: request.timestamp + BACKGROUND_TTL_S,
        });
    }
    let Some((kind, words)) = alert_words(&request.kind) else {
        return Err(Refusal::bad(
            "kind must be an event this relay knows (README.md lists them) or background.",
        ));
    };
    let mut aps = Map::new();
    aps.insert(
        "alert".into(),
        json!({ "title": ALERT_TITLE, "body": words }),
    );
    // The phone's notification service extension runs only for an alert marked mutable.
    aps.insert("mutable-content".into(), json!(1));
    if let Some(thread_id) = request.thread_id {
        if !valid_thread_id(&thread_id) {
            return Err(Refusal::bad(
                "threadId must be 1 to 128 letters, digits or . _ : -",
            ));
        }
        aps.insert("thread-id".into(), json!(thread_id));
    }
    if request.sound == Some(true) {
        aps.insert("sound".into(), json!("default"));
    }
    let mut body = Map::new();
    body.insert("aps".into(), serde_json::Value::Object(aps));
    if let Some(envelope) = request.envelope {
        if envelope.len() > MAX_ENVELOPE_CHARS {
            return Err(Refusal {
                status: 413,
                code: "too-large",
                detail: format!(
                    "envelope is {} characters; at most {MAX_ENVELOPE_CHARS}.",
                    envelope.len()
                ),
            });
        }
        if envelope.is_empty() || STANDARD.decode(envelope.as_bytes()).is_err() {
            return Err(Refusal::bad("envelope must be standard base64."));
        }
        body.insert("envelope".into(), json!(envelope));
    }
    let payload = serde_json::to_vec(&body).expect("a JSON value serializes");
    if payload.len() > MAX_APNS_PAYLOAD_BYTES {
        return Err(too_large(&payload));
    }
    Ok(NativePush {
        token,
        environment: request.environment,
        push_type: PushType::Alert,
        kind,
        priority: 10,
        payload,
        expiration: request.timestamp + ALERT_TTL_S,
    })
}

/// Checks a widget push request at `now` (unix seconds) and builds Apple's payload.
pub fn prepare_widgets(request: WidgetsRequest, now: i64) -> Result<NativePush, Refusal> {
    let token = checked_token(&request.token)?;
    let payload = serde_json::to_vec(&json!({ "aps": { "content-changed": true } }))
        .expect("a JSON value serializes");
    Ok(NativePush {
        token,
        environment: request.environment,
        push_type: PushType::Widgets,
        kind: "widgets",
        // WidgetKit budgets these and reloads when it suits the phone; 5 asks Apple to deliver with power in mind.
        priority: 5,
        payload,
        expiration: now + WIDGETS_TTL_S,
    })
}
