/*
CDXC:AnonymousAnalytics 2026-08-26:
The PostHog batch transport. Deliberately the dumbest part of the system: it
takes an already-validated batch and posts it, and every failure is a debug log
and a return value. Telemetry must never crash the daemon, never block startup,
and never surface an error to a user, so there is no `?` that escapes to a
caller who would have to decide something.

The project write token is a PUBLIC ingestion key by design (it can only append
events), which is why hardcoding it is correct rather than a leak.
*/

use std::time::Duration;

use serde_json::{json, Value};

pub const DEFAULT_POSTHOG_HOST: &str = "https://us.i.posthog.com";
pub const DEFAULT_POSTHOG_KEY: &str = "phc_ODuv7uILtuJWkydq4Uq8J7Gvo9xPVvRSVh7zwfwnvMQ";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct PostHogEndpoint {
    pub batch_url: String,
    pub api_key: String,
}

impl PostHogEndpoint {
    /// Env overrides exist for dev and for the verification run against a
    /// throwaway project; the shipped defaults are the constants above.
    pub fn from_environment() -> Self {
        let host = std::env::var("GHOSTEX_POSTHOG_HOST")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_POSTHOG_HOST.to_string());
        let api_key = std::env::var("GHOSTEX_POSTHOG_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_POSTHOG_KEY.to_string());
        Self {
            batch_url: format!("{}/batch/", host.trim_end_matches('/')),
            api_key,
        }
    }
}

/// Post one batch. `true` means PostHog accepted it and the events can be
/// forgotten; `false` means the caller should requeue within its retry cap.
///
/// Blocking by construction (`ureq`), so every caller runs it inside
/// `spawn_blocking`.
pub fn send_batch(endpoint: &PostHogEndpoint, batch: Vec<Value>) -> bool {
    if batch.is_empty() {
        return true;
    }
    let body = json!({
        "api_key": endpoint.api_key,
        "batch": batch,
    });
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .build();
    match agent
        .post(&endpoint.batch_url)
        .set("content-type", "application/json")
        .send_json(body)
    {
        // ureq surfaces every non-2xx as `Error::Status`, so reaching here means
        // PostHog accepted the batch.
        Ok(_) => true,
        Err(ureq::Error::Status(status, _)) => {
            /*
            A 4xx other than 429 means PostHog will NEVER accept this batch —
            wrong key, malformed body — so retrying it would burn the attempt cap
            and delay every later batch behind a request that cannot succeed.
            Report it as consumed so the queue moves on.
            */
            if (400..500).contains(&status) && status != 429 {
                super::debug_log(format!(
                    "telemetry batch rejected with status {status}; dropping"
                ));
                return true;
            }
            super::debug_log(format!("telemetry batch failed with status {status}"));
            false
        }
        Err(error) => {
            super::debug_log(format!("telemetry batch transport failed: {error}"));
            false
        }
    }
}
