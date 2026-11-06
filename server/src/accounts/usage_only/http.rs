//! The plumbing the three providers share: one agent that returns every status, PKCE, JWT claims,
//! and the provider failures a usage read or sign-in reports.

use std::sync::OnceLock;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use serde_json::Value;
use sha2::{Digest, Sha256};

const USER_AGENT: &str = "Ghostex";

pub(crate) fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        crate::http_agent::config()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(15)))
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .build()
            .into()
    })
}

/// Why a read or sign-in did not work.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The provider refused the sign-in for good: the user has to sign in again.
    SignInExpired,
    /// Anything time may fix (an outage, a timeout) or the provider's own explanation.
    Unavailable(String),
}

impl Failure {
    pub(crate) fn message(&self) -> String {
        match self {
            Failure::SignInExpired => "Its sign-in stopped working. Sign in again.".to_string(),
            Failure::Unavailable(message) => message.clone(),
        }
    }
}

/// A status and a JSON body (`Null` when the body is not JSON).
pub(crate) struct Answer {
    pub status: u16,
    pub body: Value,
}

pub(crate) fn answer(
    result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    provider: &str,
) -> Result<Answer, Failure> {
    let mut response = result.map_err(|error| {
        Failure::Unavailable(format!("{provider} could not be reached ({error})."))
    })?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    Ok(Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or(Value::Null),
    })
}

impl Answer {
    pub(crate) fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub(crate) fn refused(&self) -> bool {
        matches!(self.status, 401 | 403)
    }
    /// A failure for an answer that was neither a success nor a refusal.
    pub(crate) fn unavailable(&self, provider: &str) -> Failure {
        Failure::Unavailable(format!(
            "{provider} usage is unavailable right now (HTTP {}). Ghostex tries again shortly.",
            self.status
        ))
    }
}

/// A PKCE verifier and its S256 challenge.
pub(crate) fn pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub(crate) fn random_state() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A JWT's claims, unverified: only read to name the account and time its renewal.
pub(crate) fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A JSON number, or a number written as a string (Connect writes 64-bit integers that way).
pub(crate) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|number: &f64| number.is_finite())
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn rfc3339_from_ms(ms: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).map(|at| at.to_rfc3339())
}

/// Percent used, kept within 0 to 100.
pub(crate) fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

pub(crate) fn url_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
