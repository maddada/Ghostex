//! What a computer asks the `/push/` routes to send, checked, and the Apple payload built from it.
//!
//! An alert's visible words are the relay's own: the caller names an event (`kind`) and the relay shows that
//! event's fixed words from `ALERT_WORDS` under the fixed title `ALERT_TITLE`, so nobody can put their own words
//! into the alert's clear text through the relay. The names are meant to travel only inside `envelope`, which the
//! computer is meant to encrypt for the phone's notification service extension. What else reaches Apple readable
//! is held to the form of identifiers: `threadId` is `<machineId>:<sessionId>` and `data` carries the identifiers
//! a tap needs. The relay enforces form and size; that the envelope really is encrypted and that the identifiers
//! carry no names only the computer that sends and the phone that receives can guarantee. A widget push carries
//! no snapshot content: its fixed payload, the token, push type, topic, priority and expiry go to Apple.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};

use crate::apns::{Outgoing, PushType};
use crate::request::{valid_token, Environment, Refusal};
use crate::MAX_APNS_PAYLOAD_BYTES;

/// The title every alert shows unless the phone decrypts the envelope and replaces it.
pub const ALERT_TITLE: &str = "Ghostex";
/// Each event's fixed words. They follow gxserver's alert words (`server/src/push/message.rs`, `AlertKind::words`),
/// with `approval` added here; gxserver's "Running low" words have no kind here.
pub const ALERT_WORDS: &[(&str, &str)] = &[
    ("finished", "Finished"),
    ("approval", "Waiting for approval"),
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
/// The longest envelope, in base64 characters: leaves the fixed words, the thread id, `data` and Apple's keys room
/// under Apple's 4 KB.
pub const MAX_ENVELOPE_CHARS: usize = 3072;
/// The envelope's format version, its first byte once decoded; then a 12-byte nonce, then the ciphertext with its
/// 16-byte tag.
pub const ENVELOPE_VERSION: u8 = 1;
const ENVELOPE_NONCE_BYTES: usize = 12;
const ENVELOPE_TAG_BYTES: usize = 16;
/// Meant to match the phone's limits for the identifiers of a push (`src/notifications/pushMessage.ts` in the
/// phone app; not checked here).
const IDENTIFIER_MAX: usize = 64;
const NOTIFICATION_ID_MAX: usize = 128;
/// How far the event's time may lie from the relay's clock, as for a Live Activity push.
const TIMESTAMP_PAST_S: i64 = 3600;
const TIMESTAMP_FUTURE_S: i64 = 300;
/// Apple's expiry: an alert an hour after its event (gxserver's `ALERT_TTL_MS`), a background push 10 minutes
/// after it, a widget push an hour after the relay received it.
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
    /// The title and body for the phone, standard base64, meant to be encrypted by the computer (the relay checks
    /// only the form). Alerts only.
    #[serde(default)]
    pub envelope: Option<String>,
    /// `<machineId>:<sessionId>`; iOS stacks the alerts of one thread together (Apple's documentation). Alerts only.
    #[serde(default)]
    pub thread_id: Option<String>,
    /// What a tap on the alert is meant to open, as the phone reads it (not checked here). Alerts only.
    #[serde(default)]
    pub data: Option<AlertData>,
    /// Whether the alert plays the default sound. Alerts only.
    #[serde(default)]
    pub sound: Option<bool>,
    /// Unix seconds of the event.
    pub timestamp: i64,
}

/// The identifiers a tap needs, a subset of what gxserver's Expo push carries in `data`
/// (`server/src/push/message.rs`): Expo's `openSessionId` and `url` have no field here and are refused. Meant to be
/// identifiers only, never names or text; the relay checks only their form.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AlertData {
    pub kind: String,
    pub machine_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<bool>,
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

/// Printable ASCII without spaces, at most `max` characters: meant to match how the computer writes its
/// identifiers and the phone reads them.
fn identifier(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// `<machineId>:<sessionId>`, each an identifier.
fn valid_thread_id(thread_id: &str) -> bool {
    thread_id.split_once(':').is_some_and(|(machine, session)| {
        identifier(machine, IDENTIFIER_MAX) && identifier(session, IDENTIFIER_MAX)
    })
}

fn valid_data(data: &AlertData) -> bool {
    identifier(&data.kind, IDENTIFIER_MAX)
        && identifier(&data.machine_id, IDENTIFIER_MAX)
        && identifier(&data.session_id, IDENTIFIER_MAX)
        && data
            .notification_id
            .as_deref()
            .is_none_or(|id| identifier(id, NOTIFICATION_ID_MAX))
}

/// Standard base64 of version 1, a 12-byte nonce and at least 16 more bytes (the ciphertext's tag); whether the
/// rest really is a ciphertext is not checked.
fn valid_envelope(envelope: &str) -> bool {
    STANDARD.decode(envelope.as_bytes()).is_ok_and(|bytes| {
        bytes.len() >= 1 + ENVELOPE_NONCE_BYTES + ENVELOPE_TAG_BYTES && bytes[0] == ENVELOPE_VERSION
    })
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
        if request.envelope.is_some()
            || request.thread_id.is_some()
            || request.data.is_some()
            || request.sound.is_some()
        {
            return Err(Refusal::bad(
                "A background push carries no envelope, threadId, data or sound.",
            ));
        }
        let payload = serde_json::to_vec(&json!({ "aps": { "content-available": 1 } }))
            .expect("a JSON value serializes");
        return Ok(NativePush {
            token,
            environment: request.environment,
            push_type: PushType::Background,
            kind: BACKGROUND_KIND,
            // Apple's documentation: a background push is taken only at priority 5.
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
    // Apple's documentation: the notification service extension runs only for an alert marked mutable.
    aps.insert("mutable-content".into(), json!(1));
    if let Some(thread_id) = request.thread_id {
        if !valid_thread_id(&thread_id) {
            return Err(Refusal::bad(
                "threadId must be <machineId>:<sessionId>, each 1 to 64 printable characters without spaces.",
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
        if !valid_envelope(&envelope) {
            return Err(Refusal::bad(
                "envelope must be standard base64 of version 1, a 12-byte nonce and the ciphertext with its 16-byte tag.",
            ));
        }
        body.insert("envelope".into(), json!(envelope));
    }
    if let Some(data) = request.data {
        if !valid_data(&data) {
            return Err(Refusal::bad(
                "data holds kind, machineId and sessionId (1 to 64 printable characters without spaces), optional notificationId (up to 128) and optional worker.",
            ));
        }
        body.insert(
            "data".into(),
            serde_json::to_value(&data).expect("the data serializes"),
        );
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
        // Apple's documentation: WidgetKit budgets these and reloads when it suits the phone; 5 asks Apple to
        // deliver with power in mind.
        priority: 5,
        payload,
        expiration: now + WIDGETS_TTL_S,
    })
}
