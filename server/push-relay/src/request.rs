//! What a computer asks the relay to send, checked, and the Apple payload built from it.
//!
//! The caller chooses only the device token, the event, the environment (production or sandbox), Apple's
//! ordering timestamp, the content state, the stale and dismissal dates and the priority. The topic, the push
//! type, the host, the attributes type and the start alert are the relay's own: a field for any of them, or
//! for anything else, is refused (`deny_unknown_fields`).

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::MAX_APNS_PAYLOAD_BYTES;

/// The Swift type of the activity's attributes in the iPhone app (`GhostexActivityAttributes.swift`); it has
/// no fields, so a start sends `"attributes": {}`.
pub const ATTRIBUTES_TYPE: &str = "GhostexActivityAttributes";
/// The alert a start carries: Apple starts no activity from a push without one. Fixed, so no caller can put
/// its own words on a lock screen through an alert.
pub const START_ALERT_TITLE: &str = "Ghostex";
pub const START_ALERT_BODY: &str = "Your agents are working.";

/// A device token: hex, 32 to 256 bytes (Live Activity and push-to-start tokens are longer than a
/// notification token).
const TOKEN_HEX_MIN: usize = 64;
const TOKEN_HEX_MAX: usize = 512;
/// How far Apple's ordering timestamp may lie from the relay's clock.
const TIMESTAMP_PAST_S: i64 = 3600;
const TIMESTAMP_FUTURE_S: i64 = 300;
/// An activity lives at most 8 hours, then up to 4 more on the Lock Screen.
const STALE_DATE_FUTURE_S: i64 = 12 * 3600;
const DISMISSAL_FUTURE_S: i64 = 4 * 3600;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    Update,
    End,
    Start,
}

impl Event {
    pub fn as_str(self) -> &'static str {
        match self {
            Event::Update => "update",
            Event::End => "end",
            Event::Start => "start",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Production,
    /// Apple's development service, for phone builds signed for development.
    Sandbox,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Environment::Production => "production",
            Environment::Sandbox => "sandbox",
        }
    }
}

/// The body of `POST /live-activity/v1/send`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SendRequest {
    /// The activity's push token (update, end) or the app's push-to-start token (start), hex.
    pub token: String,
    pub event: Event,
    #[serde(default)]
    pub environment: Environment,
    /// Unix seconds; Apple drops an update older than one it already delivered.
    pub timestamp: i64,
    /// The activity's content, the JSON the app's `GhostexActivityContent` decodes. Required for update and
    /// start; an end may carry the final content.
    #[serde(default)]
    pub content_state: Option<Map<String, Value>>,
    /// Unix seconds after which the phone marks the content as outdated.
    #[serde(default)]
    pub stale_date: Option<i64>,
    /// End only: unix seconds when the ended activity leaves the Lock Screen.
    #[serde(default)]
    pub dismissal_date: Option<i64>,
    /// Apple's priority, 5 or 10 (default 10).
    #[serde(default)]
    pub priority: Option<u8>,
}

/// A refused request: the HTTP status, a stable code and a sentence for the caller. Never logged with values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: u16,
    pub code: &'static str,
    pub detail: String,
}

impl Refusal {
    pub fn bad(detail: impl Into<String>) -> Self {
        Refusal {
            status: 400,
            code: "bad-request",
            detail: detail.into(),
        }
    }
}

/// A checked request, ready for Apple.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub token: String,
    pub event: Event,
    pub environment: Environment,
    pub priority: u8,
    /// The `aps` payload, at most `MAX_APNS_PAYLOAD_BYTES`.
    pub payload: Vec<u8>,
    /// When Apple may stop trying to deliver it (unix seconds).
    pub expiration: i64,
}

fn valid_token(token: &str) -> bool {
    (TOKEN_HEX_MIN..=TOKEN_HEX_MAX).contains(&token.len())
        && token.len() % 2 == 0
        && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Checks `request` at `now` (unix seconds) and builds Apple's payload.
pub fn prepare(request: SendRequest, now: i64) -> Result<Prepared, Refusal> {
    if !valid_token(&request.token) {
        return Err(Refusal::bad(
            "token must be a device token in hex (64 to 512 characters).",
        ));
    }
    let token = request.token.to_ascii_lowercase();
    if request.timestamp < now - TIMESTAMP_PAST_S || request.timestamp > now + TIMESTAMP_FUTURE_S {
        return Err(Refusal::bad(
            "timestamp must be the current time in unix seconds.",
        ));
    }
    let priority = request.priority.unwrap_or(10);
    if priority != 5 && priority != 10 {
        return Err(Refusal::bad("priority must be 5 or 10."));
    }
    if request.event != Event::End && request.content_state.is_none() {
        return Err(Refusal::bad(
            "contentState is required for update and start.",
        ));
    }
    if let Some(stale) = request.stale_date {
        if stale < now - 60 || stale > now + STALE_DATE_FUTURE_S {
            return Err(Refusal::bad(
                "staleDate must be unix seconds within the next 12 hours.",
            ));
        }
    }
    if let Some(dismissal) = request.dismissal_date {
        if request.event != Event::End {
            return Err(Refusal::bad("dismissalDate is for end only."));
        }
        if dismissal < now - 60 || dismissal > now + DISMISSAL_FUTURE_S {
            return Err(Refusal::bad(
                "dismissalDate must be unix seconds within the next 4 hours.",
            ));
        }
    }

    let mut aps = Map::new();
    aps.insert("timestamp".into(), json!(request.timestamp));
    aps.insert("event".into(), json!(request.event.as_str()));
    if let Some(state) = request.content_state {
        aps.insert("content-state".into(), Value::Object(state));
    }
    if let Some(stale) = request.stale_date {
        aps.insert("stale-date".into(), json!(stale));
    }
    if let Some(dismissal) = request.dismissal_date {
        aps.insert("dismissal-date".into(), json!(dismissal));
    }
    if request.event == Event::Start {
        aps.insert("attributes-type".into(), json!(ATTRIBUTES_TYPE));
        aps.insert("attributes".into(), json!({}));
        aps.insert(
            "alert".into(),
            json!({ "title": START_ALERT_TITLE, "body": START_ALERT_BODY }),
        );
    }
    let payload = serde_json::to_vec(&json!({ "aps": aps })).expect("a JSON value serializes");
    if payload.len() > MAX_APNS_PAYLOAD_BYTES {
        return Err(Refusal {
            status: 413,
            code: "too-large",
            detail: format!(
                "The push is {} bytes; Apple takes at most {MAX_APNS_PAYLOAD_BYTES}.",
                payload.len()
            ),
        });
    }
    Ok(Prepared {
        token,
        event: request.event,
        environment: request.environment,
        priority,
        payload,
        // An update is useless once the next one exists; Apple keeps only the latest for an activity anyway.
        expiration: now + 3600,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn request(value: Value) -> Result<SendRequest, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn token() -> String {
        "ab".repeat(40)
    }

    #[test]
    fn update_builds_apples_shape() {
        let prepared = prepare(
            request(json!({
                "token": token().to_uppercase(), "event": "update", "timestamp": NOW,
                "contentState": { "counts": { "working": 1, "waiting": 0 } }, "staleDate": NOW + 900
            }))
            .unwrap(),
            NOW,
        )
        .unwrap();
        assert_eq!(prepared.token, token());
        assert_eq!(prepared.priority, 10);
        let payload: Value = serde_json::from_slice(&prepared.payload).unwrap();
        assert_eq!(
            payload,
            json!({ "aps": {
                "timestamp": NOW, "event": "update", "stale-date": NOW + 900,
                "content-state": { "counts": { "working": 1, "waiting": 0 } }
            }})
        );
    }

    #[test]
    fn start_carries_the_fixed_attributes_and_alert() {
        let prepared = prepare(
            request(
                json!({ "token": token(), "event": "start", "timestamp": NOW, "contentState": {} }),
            )
            .unwrap(),
            NOW,
        )
        .unwrap();
        let payload: Value = serde_json::from_slice(&prepared.payload).unwrap();
        assert_eq!(payload["aps"]["attributes-type"], ATTRIBUTES_TYPE);
        assert_eq!(payload["aps"]["attributes"], json!({}));
        assert_eq!(
            payload["aps"]["alert"],
            json!({ "title": START_ALERT_TITLE, "body": START_ALERT_BODY })
        );
    }

    #[test]
    fn a_field_for_the_topic_host_or_alert_is_refused() {
        for field in [
            "topic",
            "pushType",
            "host",
            "alert",
            "apns-topic",
            "attributes",
        ] {
            let mut value = json!({ "token": token(), "event": "update", "timestamp": NOW, "contentState": {} });
            value[field] = json!("x");
            assert!(request(value).is_err(), "{field} was accepted");
        }
        let value = json!({ "token": token(), "event": "update", "timestamp": NOW, "contentState": {},
            "environment": "https://evil.example" });
        assert!(
            request(value).is_err(),
            "an environment other than production or sandbox was accepted"
        );
    }

    #[test]
    fn bad_values_are_refused() {
        let base =
            || json!({ "token": token(), "event": "update", "timestamp": NOW, "contentState": {} });
        let cases: Vec<(&str, Value)> = vec![
            ("token", json!("not hex")),
            ("token", json!("ab")),
            ("timestamp", json!(NOW - 7200)),
            ("priority", json!(7)),
            ("staleDate", json!(NOW + 13 * 3600)),
            ("dismissalDate", json!(NOW + 60)),
        ];
        for (field, bad) in cases {
            let mut value = base();
            value[field] = bad;
            let refusal = prepare(request(value).unwrap(), NOW).unwrap_err();
            assert_eq!(refusal.status, 400, "{field}");
        }
        let mut value = base();
        value.as_object_mut().unwrap().remove("contentState");
        assert_eq!(
            prepare(request(value).unwrap(), NOW).unwrap_err().status,
            400
        );
    }

    #[test]
    fn a_payload_over_four_kilobytes_is_refused() {
        let value = json!({ "token": token(), "event": "update", "timestamp": NOW,
            "contentState": { "filler": "x".repeat(4096) } });
        let refusal = prepare(request(value).unwrap(), NOW).unwrap_err();
        assert_eq!((refusal.status, refusal.code), (413, "too-large"));
    }
}
