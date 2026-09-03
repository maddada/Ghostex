use std::sync::{Arc, Mutex};

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::repository::RemotePairingRepository;

/*
CDXC:RemotePairing 2026-09-03:
Mirror of `packages/shared/ghostex-remote-pairing.ts`: each code is
`<prefix><base64url(compact JSON)>` with no padding and no whitespace. Field
names are the wire names; keep them identical to the TS interfaces.
*/
pub const EASY_CONNECT_CODE_PREFIX: &str = "ghostex-ec1:";
pub const TAILSCALE_CODE_PREFIX: &str = "ghostex-ts1:";

pub const REMOTE_PAIRING_SECRET_TTL: Duration = Duration::minutes(15);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyConnectCode {
    pub v: u8,
    pub address: String,
    pub name: String,
    pub user: String,
    pub port: u16,
    pub ssh_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleCode {
    pub v: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub port: u16,
    pub user: String,
}

pub fn encode_easy_connect_code(code: &EasyConnectCode) -> String {
    format!("{EASY_CONNECT_CODE_PREFIX}{}", encode_base64url_json(code))
}

pub fn encode_tailscale_code(code: &TailscaleCode) -> String {
    format!("{TAILSCALE_CODE_PREFIX}{}", encode_base64url_json(code))
}

fn encode_base64url_json<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).expect("pairing code serializes");
    URL_SAFE_NO_PAD.encode(json)
}

/// A live pairing secret: the plaintext goes into the QR, only the hash is
/// stored. The process keeps the plaintext so consecutive status polls show
/// the same QR until the secret expires or a device consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePairingSecret {
    pub secret: String,
    pub secret_hash: String,
    pub expires_at: DateTime<Utc>,
}

impl RemotePairingSecret {
    pub fn expires_at_iso(&self) -> String {
        self.expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

#[derive(Clone, Default)]
pub struct RemotePairingRuntime {
    live_secret: Arc<Mutex<Option<RemotePairingSecret>>>,
}

impl RemotePairingRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /*
    CDXC:RemotePairing 2026-09-03:
    Reuse the live secret while the stored hash still matches it and it has
    not expired; anything else (first poll, expiry, a consumed row after a
    device paired, a database restored from elsewhere) mints a fresh one, so
    the QR silently rotates without the UI having to know why.
    */
    pub fn ensure_secret(&self, db: &Connection) -> Result<RemotePairingSecret> {
        let repository = RemotePairingRepository::new(db);
        let now = Utc::now();
        let stored = repository.read_secret()?;
        let mut live = self.live_secret.lock().expect("remote pairing secret lock");
        if let (Some(live_secret), Some(stored)) = (live.as_ref(), stored.as_ref()) {
            if live_secret.secret_hash == stored.secret_hash && live_secret.expires_at > now {
                return Ok(live_secret.clone());
            }
        }
        let fresh = mint_remote_pairing_secret(now + REMOTE_PAIRING_SECRET_TTL);
        repository.replace_secret(&fresh.secret_hash, &fresh.expires_at_iso())?;
        *live = Some(fresh.clone());
        Ok(fresh)
    }

    /// Drops the in-memory plaintext, e.g. after `/api/pairDevice` consumed
    /// the stored row, so the next poll mints a new code.
    pub fn forget_secret(&self) {
        *self.live_secret.lock().expect("remote pairing secret lock") = None;
    }
}

fn mint_remote_pairing_secret(expires_at: DateTime<Utc>) -> RemotePairingSecret {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let secret = URL_SAFE_NO_PAD.encode(bytes);
    RemotePairingSecret {
        secret_hash: hash_remote_pairing_secret(&secret),
        secret,
        expires_at,
    }
}

pub fn hash_remote_pairing_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}
