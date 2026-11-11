//! Apple's push service: the signing key, the provider token and the request.
//!
//! The provider token is a JWT signed with the `.p8` key (ES256), naming the Key ID and the Team ID. Apple
//! refuses one older than an hour and answers `TooManyProviderTokenUpdates` when it changes more often than
//! every 20 minutes, so one token is made and reused for 40 minutes.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use serde_json::{json, Value};

use crate::request::{Environment, Prepared};
use crate::TOPIC;

pub const PRODUCTION_ORIGIN: &str = "https://api.push.apple.com";
pub const SANDBOX_ORIGIN: &str = "https://api.sandbox.push.apple.com";

/// A provider token is reused this long.
const PROVIDER_TOKEN_REUSE: Duration = Duration::from_secs(40 * 60);
/// Apple refuses a new provider token sooner than this after the last.
const PROVIDER_TOKEN_MIN_AGE: Duration = Duration::from_secs(20 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The key and its two ids. The key never leaves this struct: no `Debug`, no accessor.
pub struct Credentials {
    key: SigningKey,
    key_id: String,
    team_id: String,
}

fn valid_id(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

impl Credentials {
    /// Reads the `.p8` key at `key_path` and `KEY_ID=` and `TEAM_ID=` from `config_path`. The error names what
    /// is missing or wrong, never a value from either file.
    pub fn load(key_path: &Path, config_path: &Path) -> Result<Self, String> {
        let config = std::fs::read_to_string(config_path)
            .map_err(|error| format!("cannot read {} ({})", config_path.display(), error.kind()))?;
        let mut key_id = None;
        let mut team_id = None;
        for line in config.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"').to_string();
            match name.trim() {
                "KEY_ID" => key_id = Some(value),
                "TEAM_ID" => team_id = Some(value),
                _ => {}
            }
        }
        let key_id =
            key_id.ok_or_else(|| format!("{} has no KEY_ID line", config_path.display()))?;
        let team_id =
            team_id.ok_or_else(|| format!("{} has no TEAM_ID line", config_path.display()))?;
        if !valid_id(&key_id) {
            return Err("KEY_ID is not 10 capital letters or digits".into());
        }
        if !valid_id(&team_id) {
            return Err("TEAM_ID is not 10 capital letters or digits".into());
        }
        let pem = std::fs::read_to_string(key_path)
            .map_err(|error| format!("cannot read {} ({})", key_path.display(), error.kind()))?;
        let key = SigningKey::from_pkcs8_pem(&pem).map_err(|_| {
            format!(
                "{} is not an Apple .p8 key (PKCS#8, P-256)",
                key_path.display()
            )
        })?;
        Ok(Credentials {
            key,
            key_id,
            team_id,
        })
    }

    /// For the tests: a key held in memory.
    pub fn from_parts(key: SigningKey, key_id: &str, team_id: &str) -> Self {
        Credentials {
            key,
            key_id: key_id.into(),
            team_id: team_id.into(),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// A provider token issued at `issued_at` (unix seconds).
    pub fn provider_token(&self, issued_at: u64) -> String {
        let header =
            URL_SAFE_NO_PAD.encode(json!({ "alg": "ES256", "kid": self.key_id }).to_string());
        let claims =
            URL_SAFE_NO_PAD.encode(json!({ "iss": self.team_id, "iat": issued_at }).to_string());
        let signing_input = format!("{header}.{claims}");
        let signature: Signature = self.key.sign(signing_input.as_bytes());
        format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }
}

/// Where requests go: Apple, or for local tests a stand-in on loopback (never anything else).
#[derive(Clone, Debug)]
pub enum Target {
    Apple,
    /// `http://127.0.0.1:<port>` or `http://[::1]:<port>`, spoken HTTP/2 without TLS.
    Loopback(String),
}

impl Target {
    /// Accepts only a loopback origin with a port.
    pub fn loopback(origin: &str) -> Result<Self, String> {
        let rest = origin
            .strip_prefix("http://")
            .ok_or("the stand-in origin must be http://")?;
        let (host, port) = rest
            .rsplit_once(':')
            .ok_or("the stand-in origin needs a port")?;
        if host != "127.0.0.1" && host != "[::1]" {
            return Err("the stand-in origin must be 127.0.0.1 or [::1]".into());
        }
        port.parse::<u16>()
            .map_err(|_| "the stand-in origin's port is not a number")?;
        Ok(Target::Loopback(origin.to_string()))
    }

    fn origin(&self, environment: Environment) -> String {
        match (self, environment) {
            (Target::Apple, Environment::Production) => PRODUCTION_ORIGIN.into(),
            (Target::Apple, Environment::Sandbox) => SANDBOX_ORIGIN.into(),
            (Target::Loopback(origin), Environment::Production) => format!("{origin}/production"),
            (Target::Loopback(origin), Environment::Sandbox) => format!("{origin}/sandbox"),
        }
    }
}

/// What Apple answered.
#[derive(Debug, Clone)]
pub struct AppleAnswer {
    pub status: u16,
    /// Apple's `reason` on an error, e.g. `BadDeviceToken`.
    pub reason: Option<String>,
    pub apns_id: Option<String>,
}

pub struct Apple {
    client: reqwest::Client,
    target: Target,
    credentials: Credentials,
    provider: Mutex<Option<(String, Instant)>>,
}

impl Apple {
    pub fn new(credentials: Credentials, target: Target) -> Result<Self, String> {
        let builder = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("ghostex-push-relay");
        let builder = match target {
            Target::Apple => builder.https_only(true),
            Target::Loopback(_) => builder.http2_prior_knowledge(),
        };
        let client = builder
            .build()
            .map_err(|error| format!("cannot build the HTTP client: {error}"))?;
        Ok(Apple {
            client,
            target,
            credentials,
            provider: Mutex::new(None),
        })
    }

    pub fn key_id(&self) -> &str {
        self.credentials.key_id()
    }

    /// The cached provider token, or a new one when it is 40 minutes old, or when `renew` and Apple allows a
    /// new one (20 minutes). The flag says whether this call made a new one.
    fn provider_token(&self, renew: bool) -> (String, bool) {
        let mut cached = self.provider.lock().expect("provider token lock");
        if let Some((token, made)) = cached.as_ref() {
            let age = made.elapsed();
            if age < PROVIDER_TOKEN_REUSE && !(renew && age >= PROVIDER_TOKEN_MIN_AGE) {
                return (token.clone(), false);
            }
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs())
            .unwrap_or(0);
        let token = self.credentials.provider_token(now);
        *cached = Some((token.clone(), Instant::now()));
        (token, true)
    }

    async fn post(&self, push: &Prepared, provider_token: &str) -> Result<AppleAnswer, String> {
        let url = format!(
            "{}/3/device/{}",
            self.target.origin(push.environment),
            push.token
        );
        let response = self
            .client
            .post(url)
            .header("authorization", format!("bearer {provider_token}"))
            .header("apns-push-type", "liveactivity")
            .header("apns-topic", TOPIC)
            .header("apns-priority", push.priority.to_string())
            .header("apns-expiration", push.expiration.to_string())
            .header("content-type", "application/json")
            .body(push.payload.clone())
            .send()
            .await
            // The error text can carry the URL, and so the token: only its kind goes out.
            .map_err(|error| {
                if error.is_timeout() {
                    "no answer from Apple in time".to_string()
                } else if error.is_connect() {
                    "cannot reach Apple".to_string()
                } else {
                    "the request to Apple failed".to_string()
                }
            })?;
        let status = response.status().as_u16();
        let apns_id = response
            .headers()
            .get("apns-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let reason = if status == 200 {
            None
        } else {
            let body = response.bytes().await.unwrap_or_default();
            serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .filter(|reason| {
                    reason.len() <= 64 && reason.bytes().all(|byte| byte.is_ascii_alphanumeric())
                })
        };
        Ok(AppleAnswer {
            status,
            reason,
            apns_id,
        })
    }

    /// Sends one push. A provider token Apple calls expired or invalid is made again once, when Apple allows a
    /// new one, and the push retried with it.
    pub async fn send(&self, push: &Prepared) -> Result<AppleAnswer, String> {
        let answer = self.post(push, &self.provider_token(false).0).await?;
        let stale_token = answer.status == 403
            && matches!(
                answer.reason.as_deref(),
                Some("ExpiredProviderToken" | "InvalidProviderToken")
            );
        if !stale_token {
            return Ok(answer);
        }
        // Retried only with a new token: the same one would get the same answer.
        match self.provider_token(true) {
            (renewed, true) => self.post(push, &renewed).await,
            (_, false) => Ok(answer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;

    #[test]
    fn the_provider_token_verifies_against_the_key() {
        let key = SigningKey::random(&mut rand_core::OsRng);
        let credentials = Credentials::from_parts(key.clone(), "ABCDE12345", "TEAM123456");
        let token = credentials.provider_token(1_800_000_000);
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(header, json!({ "alg": "ES256", "kid": "ABCDE12345" }));
        assert_eq!(
            claims,
            json!({ "iss": "TEAM123456", "iat": 1_800_000_000u64 })
        );
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        VerifyingKey::from(&key)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn only_a_loopback_stand_in_is_accepted() {
        assert!(Target::loopback("http://127.0.0.1:9000").is_ok());
        assert!(Target::loopback("http://[::1]:9000").is_ok());
        for origin in [
            "https://127.0.0.1:9000",
            "http://evil.example:80",
            "http://127.0.0.1",
            "http://10.0.0.1:1",
        ] {
            assert!(Target::loopback(origin).is_err(), "{origin}");
        }
    }

    #[test]
    fn ids_must_look_like_apples() {
        assert!(valid_id("ABCDE12345"));
        assert!(!valid_id("abcde12345"));
        assert!(!valid_id("ABCDE1234"));
    }
}
