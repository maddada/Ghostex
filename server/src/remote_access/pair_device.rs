use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::paths::GxserverPaths;
use crate::tailcat::TailcatRuntime;

use super::authorized_keys::{append_authorized_key, parse_ssh_public_key, remove_authorized_key};
use super::identity::read_remote_access_identity;
use super::pairing_code::{hash_remote_pairing_secret, RemotePairingRuntime};
use super::repository::{now_iso, RemotePairedDeviceRecord, RemotePairingRepository};

/*
CDXC:RemotePairing 2026-09-03:
`/api/pairDevice` is the one unauthenticated endpoint on the local listener:
the phone reaches it through the Easy Connect tunnel before it has any
credential, and the one-time secret in the QR is the whole gate. That is why
every attempt, valid or not, counts against a small per-process window,
why the secret is compared only by hash, and why it is consumed the moment a
key is written so the same QR cannot register a second device.
*/
pub const PAIR_DEVICE_ATTEMPT_LIMIT: usize = 5;
pub const PAIR_DEVICE_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);

/// Error code the client detects as "this code has expired" (it matches on
/// the substring `expired`), returned for an unknown, used, or expired secret.
pub const PAIR_DEVICE_CODE_EXPIRED_ERROR: &str = "pairingCodeExpired";
pub const PAIR_DEVICE_RATE_LIMITED_ERROR: &str = "pairingRateLimited";

const DEVICE_NAME_MAX_CHARS: usize = 80;
const PLATFORM_MAX_CHARS: usize = 32;

static PAIR_DEVICE_ATTEMPTS: Mutex<Vec<Instant>> = Mutex::new(Vec::new());

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDeviceParams {
    pub secret: String,
    pub device_name: String,
    pub platform: String,
    pub ssh_public_key: String,
    #[serde(default)]
    pub tailcat_client_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairDeviceOutcome {
    pub device_id: String,
    pub user: String,
    pub computer_name: String,
}

#[derive(Debug)]
pub enum PairDeviceError {
    /// Too many attempts in the current window.
    RateLimited,
    /// Unknown, already used, or expired secret.
    CodeExpired,
    /// Malformed request body.
    BadRequest(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for PairDeviceError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

pub fn parse_pair_device_params(body: &Value) -> Result<PairDeviceParams, PairDeviceError> {
    let params: PairDeviceParams = serde_json::from_value(body.clone()).map_err(|error| {
        PairDeviceError::BadRequest(format!("Invalid pairDevice params: {error}"))
    })?;
    let device_name = params.device_name.trim().to_string();
    if device_name.is_empty() {
        return Err(PairDeviceError::BadRequest(
            "deviceName is empty.".to_string(),
        ));
    }
    if device_name.chars().count() > DEVICE_NAME_MAX_CHARS {
        return Err(PairDeviceError::BadRequest(format!(
            "deviceName must be at most {DEVICE_NAME_MAX_CHARS} characters."
        )));
    }
    let platform = params.platform.trim().to_string();
    if platform.is_empty() || platform.chars().count() > PLATFORM_MAX_CHARS {
        return Err(PairDeviceError::BadRequest(
            "platform is invalid.".to_string(),
        ));
    }
    if params.secret.trim().is_empty() {
        return Err(PairDeviceError::BadRequest("secret is empty.".to_string()));
    }
    let tailcat_client_key = params
        .tailcat_client_key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    Ok(PairDeviceParams {
        secret: params.secret.trim().to_string(),
        device_name,
        platform,
        ssh_public_key: params.ssh_public_key,
        tailcat_client_key,
    })
}

pub fn pair_device(
    paths: &GxserverPaths,
    db: &Connection,
    tailcat_runtime: &TailcatRuntime,
    pairing_runtime: &RemotePairingRuntime,
    logger: &GxserverLogger,
    params: PairDeviceParams,
) -> Result<PairDeviceOutcome, PairDeviceError> {
    record_pair_device_attempt()?;
    let key = parse_ssh_public_key(&params.ssh_public_key)
        .map_err(|error| PairDeviceError::BadRequest(format!("{error:#}")))?;

    /*
    CDXC:RemotePairing 2026-09-03:
    The secret is taken (validated and deleted in one statement) BEFORE any
    access is granted: if two registrations race on the same code, exactly
    one deletes the row and the other sees "expired". Nothing is written to
    `authorized_keys` for a request that did not win the secret.
    */
    let repository = RemotePairingRepository::new(db);
    let secret_hash = hash_remote_pairing_secret(&params.secret);
    if !repository.consume_valid_secret(&secret_hash, &now_iso())? {
        return Err(PairDeviceError::CodeExpired);
    }
    pairing_runtime.forget_secret();

    let device_id = uuid::Uuid::new_v4().simple().to_string();
    append_authorized_key(&key, &device_id, &params.device_name)?;
    let record = RemotePairedDeviceRecord {
        id: device_id.clone(),
        name: params.device_name,
        platform: params.platform,
        ssh_key_fingerprint: key.fingerprint(),
        tailcat_client_key: params.tailcat_client_key.clone(),
        paired_at: now_iso(),
        last_seen_at: None,
    };
    if let Err(error) = repository.insert_device(&record) {
        // The key line grants access; never leave it behind without its row.
        return Err(match remove_authorized_key(&device_id) {
            Ok(_) => error.into(),
            Err(rollback_error) => {
                log_authorized_key_rollback_failure(logger, &device_id, &rollback_error);
                anyhow::anyhow!(
                    "{error:#}; additionally, the SSH key line ghostex-paired:{device_id} could not be removed from authorized_keys and must be deleted by hand: {rollback_error:#}"
                )
                .into()
            }
        });
    }

    if let Some(client_key) = params.tailcat_client_key.as_deref() {
        super::paired_devices::add_tailcat_client_key_when_allow_listed(
            paths,
            db,
            tailcat_runtime,
            client_key,
        )?;
    }

    let identity = read_remote_access_identity()?;
    Ok(PairDeviceOutcome {
        device_id,
        user: identity.username,
        computer_name: identity.computer_name,
    })
}

fn log_authorized_key_rollback_failure(
    logger: &GxserverLogger,
    device_id: &str,
    error: &anyhow::Error,
) {
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Error,
        event: "remotePairing.authorizedKeyRollbackFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(format!("{error:#}")),
        details: Some(serde_json::json!({ "deviceId": device_id })),
    });
}

fn record_pair_device_attempt() -> Result<(), PairDeviceError> {
    let now = Instant::now();
    let mut attempts = PAIR_DEVICE_ATTEMPTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    attempts.retain(|attempt| now.duration_since(*attempt) < PAIR_DEVICE_ATTEMPT_WINDOW);
    if attempts.len() >= PAIR_DEVICE_ATTEMPT_LIMIT {
        return Err(PairDeviceError::RateLimited);
    }
    attempts.push(now);
    Ok(())
}
