//! The Live Activity push relay (`server/push-relay`, share.ghostex.dev/live-activity/), which holds the
//! Apple push key and sends the pushes to Apple. gxserver never holds that key.

use std::time::Duration;

use serde_json::Value;

/// The relay Ghostex runs. `GHOSTEX_LIVE_ACTIVITY_RELAY_URL` replaces it (a test's stand-in, a relay of
/// one's own), as `GHOSTEX_COLLAB_RELAY_URL` does for the share relay.
pub(crate) const DEFAULT_RELAY_URL: &str = "https://share.ghostex.dev/live-activity/v1/send";
pub(crate) const RELAY_URL_ENV: &str = "GHOSTEX_LIVE_ACTIVITY_RELAY_URL";
/// The whole request; a stop waits at most this long for the one in flight.
pub(crate) const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_BYTES_MAX: u64 = 16 * 1024;
const RETRY_AFTER_MAX_SECS: u64 = 3600;

pub(crate) fn relay_url_from_env() -> String {
    std::env::var(RELAY_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.starts_with("https://") || value.starts_with("http://127.0.0.1:"))
        .unwrap_or_else(|| DEFAULT_RELAY_URL.to_string())
}

/// What the relay answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RelayOutcome {
    Sent,
    /// Rate limited by the relay or by Apple: not before this long.
    RetryAfter(Duration),
    /// Apple no longer takes the token: forget it.
    TokenGone,
    /// The relay has no Apple key yet.
    NotConfigured,
    /// Refused for another reason, or no answer; `code` names it for the log (never a token).
    Failed {
        code: String,
        retry: bool,
    },
}

pub(crate) struct RelayClient {
    url: String,
    agent: ureq::Agent,
}

impl RelayClient {
    pub(crate) fn new(url: String) -> Self {
        let agent = crate::http_agent::config()
            .timeout_global(Some(HTTP_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .into();
        Self { url, agent }
    }

    /// Sends one request body (`server/push-relay/README.md` has its fields).
    pub(crate) fn send(&self, body: &Value) -> RelayOutcome {
        let mut response = match self
            .agent
            .post(&self.url)
            .header("accept", "application/json")
            .content_type("application/json")
            .send(body.to_string())
        {
            Ok(response) => response,
            Err(ureq::Error::Timeout(_)) => return failed("timeout", true),
            Err(error) if crate::http_agent::never_connected(&error) => {
                return failed("connectFailed", true)
            }
            Err(_) => return failed("transport", true),
        };
        let status = response.status().as_u16();
        let answer = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_BYTES_MAX)
            .read_to_vec()
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let error = answer.get("error").and_then(Value::as_str).unwrap_or("");
        match status {
            200..=299 if answer.get("ok") == Some(&Value::Bool(true)) => RelayOutcome::Sent,
            200..=299 => failed("malformedAnswer", false),
            429 => RelayOutcome::RetryAfter(Duration::from_secs(
                answer
                    .get("retryAfter")
                    .and_then(Value::as_u64)
                    .unwrap_or(30)
                    .clamp(1, RETRY_AFTER_MAX_SECS),
            )),
            410 => RelayOutcome::TokenGone,
            503 if error == "not-configured" => RelayOutcome::NotConfigured,
            500..=599 => failed(&format!("http{status}"), true),
            _ => failed(&bounded_code(error, status), false),
        }
    }
}

fn failed(code: &str, retry: bool) -> RelayOutcome {
    RelayOutcome::Failed {
        code: code.to_string(),
        retry,
    }
}

/// The relay's error code when it looks like one, else the status.
fn bounded_code(error: &str, status: u16) -> String {
    if !error.is_empty()
        && error.len() <= 32
        && error
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        error.to_string()
    } else {
        format!("http{status}")
    }
}
