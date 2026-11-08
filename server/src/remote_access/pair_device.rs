use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use ghostex_gx_protocol::approval::{
    ApprovalRequester, ApprovalStatus, ApproverKey, ApproverKeyAlgorithm, ApproverPresence,
    SensitiveAction,
};
use ghostex_gx_protocol::pairing::{
    device_token_renew_payload, pairing_match_words, pairing_status_payload, pairing_words_input,
    DeviceKey, PairingStatus, DEVICE_KEY_ALGORITHM,
};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::{
    hash_device_api_token, mint_device_api_token, DeviceApiTokens, DEVICE_API_TOKEN_TTL,
};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::paths::GxserverPaths;
use crate::security::{
    approver_key_id, now_ms, ActionRequest, ApproverKeys, Principal, SecurityEvent, SecurityState,
};
use crate::tailcat::TailcatRuntime;

pub use super::authorized_keys::{append_authorized_key, remove_authorized_key};
use super::authorized_keys::{
    parse_ssh_public_key, AdministratorApprovalDeclined, ParsedSshPublicKey,
};
use super::identity::read_remote_access_identity;
use super::pairing_code::{
    hash_remote_pairing_secret, PendingPairing, PendingState, RemotePairingRuntime,
};
use super::repository::{
    now_iso, PairedDeviceStatus, RemotePairedDeviceRecord, RemotePairingRepository, StoredDeviceKey,
};

/*
CDXC:RemotePairing 2026-09-03:
`/api/pairDevice` is the one unauthenticated endpoint on the local listener:
the phone reaches it through the Easy Connect tunnel before it has any
credential, and the one-time secret in the QR is the whole gate. That is why
every attempt, valid or not, counts against a small per-process window,
why the secret is compared only by hash, and why it is consumed the moment a
key is written so the same QR cannot register a second device.

CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495, Q1721 "device keys + pending + match"): the secret alone no longer pairs
anything. `/api/pairDevice` takes the phone's own device key with the secret, consumes the secret
and makes the phone Pending: no SSH key is written and no token issued. gxserver works out three
words from the secret and the device key and shows them on the computer and on the phones
already paired, where an approval request (`pairingApproveDevice`) waits for the owner's signed
approval; the phone works out the same words itself. The phone asks `/api/pairingStatus`, signing
each ask with its device key, and the ask that finds the request approved spends the approval and
finishes the pairing: the SSH key line (restricted, see authorized_keys.rs), the device row with its
key, and a device token bound to that key. A leaked QR or an agent that read the code gets a
Pending entry with words that match no phone in the owner's hand, and nothing else.
SEE-ALSO: packages/gx-protocol/src/pairing.rs (words and signed payloads),
apps/mobile/app/src/machines/pairing.ts (the phone's side).
*/
pub const PAIR_DEVICE_ATTEMPT_LIMIT: usize = 5;
pub const PAIR_DEVICE_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);

/// Error code the client detects as "this code has expired" (it matches on
/// the substring `expired`), returned for an unknown, used, or expired secret.
pub const PAIR_DEVICE_CODE_EXPIRED_ERROR: &str = "pairingCodeExpired";
pub const PAIR_DEVICE_RATE_LIMITED_ERROR: &str = "pairingRateLimited";
/// The computer's administrator prompt was declined (Windows administrator
/// accounts). The phone shows the message; it must never contain "expired".
pub const PAIR_DEVICE_APPROVAL_DECLINED_ERROR: &str = "pairingApprovalDeclined";
/// The phone's app is older than pairing v2 (it sent no device key). Must never contain "expired".
pub const PAIR_DEVICE_NEEDS_UPDATE_ERROR: &str = "pairingNeedsAppUpdate";
/// A signed pairing or renewal ask whose signature or time didn't check out.
pub const PAIRING_SIGNATURE_ERROR: &str = "pairingSignatureRejected";

/// How far a signed ask's timestamp may be from this computer's clock.
pub const SIGNED_ASK_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
/// At most this many phones wait for approval at once.
pub const PENDING_PAIRING_LIMIT: usize = 4;
/// A finished pairing's answer is kept this long, so a phone whose first reply was lost gets the
/// same answer when it asks again (with a fresh signature).
pub const FINISHED_PAIRING_RETENTION_MS: u64 = 2 * 60 * 1000;

const DEVICE_NAME_MAX_CHARS: usize = 80;
const PLATFORM_MAX_CHARS: usize = 32;

static PAIR_DEVICE_ATTEMPTS: Mutex<Vec<Instant>> = Mutex::new(Vec::new());

/// A phone's approver key (its Face ID / fingerprint-locked key, GitHub #495 Phase 2), sent with
/// its pairing so approving the device also lets it approve requests.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingApproverKey {
    pub algorithm: ApproverKeyAlgorithm,
    pub public_key: String,
    pub presence: ApproverPresence,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDeviceParams {
    pub secret: String,
    pub device_name: String,
    pub platform: String,
    pub ssh_public_key: String,
    #[serde(default)]
    pub tailcat_client_key: Option<String>,
    /// Pairing v2: the phone's device key. A phone without one is too old to pair.
    #[serde(default)]
    pub device_key: Option<DeviceKey>,
    #[serde(default)]
    pub approver_key: Option<PairingApproverKey>,
}

/// What `/api/pairDevice` answers: the phone is Pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairingStarted {
    pub pending_id: String,
    pub words: Vec<String>,
    pub expires_at_ms: u64,
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
    /// The phone sent no device key: its app predates pairing v2.
    NeedsAppUpdate,
    /// A signed ask whose signature or timestamp didn't check out.
    BadSignature(String),
    /// The Windows administrator prompt for the keys file was declined.
    ApprovalDeclined,
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
    if device_name.chars().any(char::is_control) {
        return Err(PairDeviceError::BadRequest(
            "deviceName must be one line of text.".to_string(),
        ));
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
    // It goes onto tailcat's `--allow` list, where a malformed key stops the helper for everyone.
    if tailcat_client_key
        .as_deref()
        .is_some_and(|key| !is_tailcat_client_public_key(key))
    {
        return Err(PairDeviceError::BadRequest(
            "tailcatClientKey must be a nodekey: public key.".to_string(),
        ));
    }
    let Some(device_key) = params.device_key else {
        return Err(PairDeviceError::NeedsAppUpdate);
    };
    let device_key = DeviceKey {
        algorithm: device_key.algorithm.trim().to_string(),
        public_key: device_key.public_key.trim().to_string(),
    };
    validate_device_key(&device_key)?;
    Ok(PairDeviceParams {
        secret: params.secret.trim().to_string(),
        device_name,
        platform,
        ssh_public_key: params.ssh_public_key,
        tailcat_client_key,
        device_key: Some(device_key),
        approver_key: params.approver_key,
    })
}

/// A device key must be a P-256 public key: the 65-byte uncompressed point, base64 standard.
fn validate_device_key(key: &DeviceKey) -> Result<Vec<u8>, PairDeviceError> {
    if key.algorithm != DEVICE_KEY_ALGORITHM {
        return Err(PairDeviceError::BadRequest(format!(
            "deviceKey.algorithm must be {DEVICE_KEY_ALGORITHM}."
        )));
    }
    let bytes = STANDARD.decode(&key.public_key).map_err(|_| {
        PairDeviceError::BadRequest("deviceKey.publicKey is not base64.".to_string())
    })?;
    if bytes.len() != 65 || bytes[0] != 0x04 {
        return Err(PairDeviceError::BadRequest(
            "deviceKey.publicKey must be an uncompressed P-256 point.".to_string(),
        ));
    }
    Ok(bytes)
}

/// `nodekey:` and 64 lowercase hex digits, the form tailcat prints and reads.
fn is_tailcat_client_public_key(key: &str) -> bool {
    key.strip_prefix("nodekey:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// The three match words for a QR's nonce and a phone's device key.
pub fn match_words(nonce: &str, device_public_key: &str) -> Vec<String> {
    let digest = Sha256::digest(pairing_words_input(nonce, device_public_key));
    pairing_match_words(&digest)
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Checks a device key's ASN.1 DER P-256 signature (base64) over `payload`.
pub fn verify_device_signature(public_key_b64: &str, payload: &[u8], signature_b64: &str) -> bool {
    use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};
    let (Ok(public_key), Ok(signature)) = (
        STANDARD.decode(public_key_b64),
        STANDARD.decode(signature_b64.trim()),
    ) else {
        return false;
    };
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &public_key)
        .verify(payload, &signature)
        .is_ok()
}

fn timestamp_is_fresh(timestamp_ms: u64, now_ms: u64) -> bool {
    timestamp_ms.abs_diff(now_ms) <= SIGNED_ASK_CLOCK_SKEW_MS
}

/// What the owner approves: the action, the pending entry, and every fact about the phone, so
/// the signature can't be spent on another phone.
fn approval_action(
    pending_id: &str,
    params: &PairDeviceParams,
    key_id: &str,
    fingerprint: &str,
    words: &[String],
    approver_key_id: Option<&str>,
) -> ActionRequest {
    ActionRequest::new(
        SensitiveAction::PairingApproveDevice,
        // The name is the phone's own; say so, since gxserver didn't write it.
        format!(
            "Pair a new {} device that calls itself \"{}\" with this computer",
            params.platform, params.device_name
        ),
    )
    .target(pending_id)
    .detail(format!(
        "Approve only if the phone shows the same three words: {}. Approving lets it use this computer's Ghostex and sign in over SSH until you remove it.",
        words.join(" · ")
    ))
    .params(json!({
        "pendingId": pending_id,
        "deviceName": params.device_name,
        "platform": params.platform,
        "deviceKeyId": key_id,
        "sshKeyFingerprint": fingerprint,
        "tailcatClientKey": params.tailcat_client_key,
        "approverKeyId": approver_key_id,
        "words": words,
    }))
}

/// `/api/pairDevice`: consumes the QR's secret and makes the phone Pending.
pub fn begin_pairing(
    db: &Connection,
    pairing_runtime: &RemotePairingRuntime,
    security: &SecurityState,
    params: PairDeviceParams,
) -> Result<PairingStarted, PairDeviceError> {
    record_pair_device_attempt()?;
    let ssh_key = parse_ssh_public_key(&params.ssh_public_key)
        .map_err(|error| PairDeviceError::BadRequest(format!("{error:#}")))?;
    let device_key = params
        .device_key
        .clone()
        .ok_or(PairDeviceError::NeedsAppUpdate)?;
    let device_key_bytes = validate_device_key(&device_key)?;
    let approver_key = params
        .approver_key
        .as_ref()
        .map(|key| {
            // Checked the way enrollment checks it, so approval can't fail on a bad key later.
            ApproverKeys::default()
                .insert(ApproverKey {
                    key_id: String::new(),
                    device_id: String::new(),
                    label: params.device_name.clone(),
                    platform: params.platform.clone(),
                    algorithm: key.algorithm.clone(),
                    public_key: key.public_key.trim().to_string(),
                    presence: key.presence.clone(),
                    added_at_ms: 0,
                })
                .map_err(|error| PairDeviceError::BadRequest(format!("approverKey: {error}")))
        })
        .transpose()?;
    if pairing_runtime.pending_count(now_ms()) >= PENDING_PAIRING_LIMIT {
        return Err(PairDeviceError::RateLimited);
    }

    /*
    CDXC:RemotePairing 2026-09-03:
    The secret is taken (validated and deleted in one statement) BEFORE anything
    else happens: if two registrations race on the same code, exactly one
    deletes the row and the other sees "expired".
    */
    let repository = RemotePairingRepository::new(db);
    let secret_hash = hash_remote_pairing_secret(&params.secret);
    if !repository.consume_valid_secret(&secret_hash, &now_iso())? {
        return Err(PairDeviceError::CodeExpired);
    }
    pairing_runtime.forget_secret();

    let pending_id = format!("pd_{}", uuid::Uuid::new_v4().simple());
    let words = match_words(&params.secret, &device_key.public_key);
    let key_id = approver_key_id(&device_key_bytes);
    let fingerprint = ssh_key.fingerprint();
    let action = approval_action(
        &pending_id,
        &params,
        &key_id,
        &fingerprint,
        &words,
        approver_key.as_ref().map(|key| key.key_id.as_str()),
    );
    let now = now_ms();
    let requester = ApprovalRequester {
        kind: "newDevice".to_string(),
        label: format!(
            "A new {} device, \"{}\"",
            params.platform, params.device_name
        ),
        device_id: Some(pending_id.clone()),
        ..Default::default()
    };
    let (request, _) = security
        .approvals
        .create(&Principal::Anonymous, requester, &action, now);
    security.publish(SecurityEvent::ApprovalRequested {
        request: request.clone(),
    });
    // The security log records the ask (`pairing.request`) in the route, with its outcome.

    pairing_runtime.insert_pending(PendingPairing {
        id: pending_id.clone(),
        name: params.device_name.clone(),
        platform: params.platform.clone(),
        ssh_public_key: params.ssh_public_key.trim().to_string(),
        tailcat_client_key: params.tailcat_client_key.clone(),
        device_key: StoredDeviceKey {
            algorithm: device_key.algorithm.clone(),
            public_key: device_key.public_key.clone(),
            key_id,
        },
        approver_key,
        words: words.clone(),
        action,
        approval_id: request.id.clone(),
        requested_at_ms: now,
        expires_at_ms: request.expires_at_ms,
        last_ask_ms: 0,
        state: PendingState::Waiting,
    });

    let identity = read_remote_access_identity()?;
    Ok(PairingStarted {
        pending_id,
        words,
        expires_at_ms: request.expires_at_ms,
        user: identity.username,
        computer_name: identity.computer_name,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedAskParams {
    /// `pendingId` for `/api/pairingStatus`, `deviceId` for `/api/renewDeviceToken`.
    #[serde(alias = "pendingId", alias = "deviceId")]
    pub id: String,
    pub timestamp_ms: u64,
    pub signature: String,
}

pub fn parse_signed_ask(body: &Value) -> Result<SignedAskParams, PairDeviceError> {
    serde_json::from_value(body.clone())
        .map_err(|error| PairDeviceError::BadRequest(format!("Invalid signed ask: {error}")))
}

/// Everything finishing a pairing touches.
pub struct PairingContext<'a> {
    pub paths: &'a GxserverPaths,
    pub db: &'a Connection,
    pub tailcat_runtime: &'a TailcatRuntime,
    pub pairing_runtime: &'a RemotePairingRuntime,
    pub device_tokens: &'a DeviceApiTokens,
    pub security: &'a SecurityState,
    pub logger: &'a GxserverLogger,
    /// Writes a device's SSH key line ([`append_authorized_key`] outside tests).
    pub append_key: &'a dyn Fn(&ParsedSshPublicKey, &str, &str) -> Result<()>,
    /// Removes a device's SSH key line ([`remove_authorized_key`] outside tests).
    pub remove_key: &'a dyn Fn(&str) -> Result<bool>,
}

/// `/api/pairingStatus`'s answer.
#[derive(Clone, Debug, PartialEq)]
pub struct PairingStatusAnswer {
    pub status: PairingStatus,
    /// On `approved`: `deviceId`, `deviceApiToken`, `deviceTokenExpiresAtMs`, `user`,
    /// `computerName`.
    pub approved: Option<Value>,
}

/// `/api/pairingStatus`: a pending phone, signing with its device key, asks where its pairing
/// stands. The ask that finds the approval signed finishes the pairing.
pub fn pairing_status(
    context: &PairingContext<'_>,
    ask: SignedAskParams,
) -> Result<PairingStatusAnswer, PairDeviceError> {
    let now = now_ms();
    let runtime = context.pairing_runtime;
    let Some(pending) = runtime.pending(&ask.id, now) else {
        return Ok(PairingStatusAnswer {
            status: PairingStatus::Expired,
            approved: None,
        });
    };
    if !timestamp_is_fresh(ask.timestamp_ms, now)
        || ask.timestamp_ms <= pending.last_ask_ms
        || !verify_device_signature(
            &pending.device_key.public_key,
            &pairing_status_payload(&ask.id, ask.timestamp_ms),
            &ask.signature,
        )
    {
        return Err(PairDeviceError::BadSignature(
            "This ask isn't signed by the phone that is pairing, or its clock is more than five minutes off.".to_string(),
        ));
    }
    runtime.note_ask(&ask.id, ask.timestamp_ms);
    match &pending.state {
        PendingState::Finished { answer, .. } => {
            return Ok(PairingStatusAnswer {
                status: PairingStatus::Approved,
                approved: Some(answer.clone()),
            })
        }
        PendingState::Finalizing => {
            return Ok(PairingStatusAnswer {
                status: PairingStatus::Pending,
                approved: None,
            })
        }
        PendingState::Waiting => {}
    }
    let request = context
        .security
        .approvals
        .read(&pending.approval_id, &Principal::Anonymous, now);
    let status = request.map(|request| request.status);
    match status {
        Some(ApprovalStatus::Pending) => Ok(PairingStatusAnswer {
            status: PairingStatus::Pending,
            approved: None,
        }),
        Some(ApprovalStatus::Denied) => {
            runtime.remove_pending(&ask.id);
            Ok(PairingStatusAnswer {
                status: PairingStatus::Denied,
                approved: None,
            })
        }
        Some(ApprovalStatus::Approved) => {
            if !runtime.start_finalizing(&ask.id) {
                return Ok(PairingStatusAnswer {
                    status: PairingStatus::Pending,
                    approved: None,
                });
            }
            let approval = match context.security.approvals.consume(
                &pending.approval_id,
                &Principal::Anonymous,
                &pending.action,
                now,
            ) {
                Ok(approval) => approval,
                Err(_) => {
                    runtime.remove_pending(&ask.id);
                    return Ok(PairingStatusAnswer {
                        status: PairingStatus::Expired,
                        approved: None,
                    });
                }
            };
            context
                .security
                .publish(SecurityEvent::SensitiveActionAuthorized {
                    action: SensitiveAction::PairingApproveDevice,
                    target: Some(pending.id.clone()),
                    approval_id: approval.id.clone(),
                    caller: Principal::Anonymous,
                });
            match finish_pairing(context, &pending) {
                Ok(answer) => {
                    runtime.finish_pending(&ask.id, answer.clone(), now);
                    Ok(PairingStatusAnswer {
                        status: PairingStatus::Approved,
                        approved: Some(answer),
                    })
                }
                Err(error) => {
                    runtime.remove_pending(&ask.id);
                    Err(error)
                }
            }
        }
        // Expired, consumed elsewhere, or dropped by a restart.
        _ => {
            runtime.remove_pending(&ask.id);
            Ok(PairingStatusAnswer {
                status: PairingStatus::Expired,
                approved: None,
            })
        }
    }
}

/// Writes what an approved pairing grants: the restricted SSH key line, the device row with its
/// device key, a device token bound to that key, the tailcat allow-list entry, and the phone's
/// approver key.
fn finish_pairing(
    context: &PairingContext<'_>,
    pending: &PendingPairing,
) -> Result<Value, PairDeviceError> {
    let key = parse_ssh_public_key(&pending.ssh_public_key)
        .map_err(|error| PairDeviceError::BadRequest(format!("{error:#}")))?;
    let device_id = uuid::Uuid::new_v4().simple().to_string();
    let device_api_token = mint_device_api_token();
    let token_expires_at = chrono::Utc::now() + DEVICE_API_TOKEN_TTL;
    (context.append_key)(&key, &device_id, &pending.name).map_err(|error| {
        if error.is::<AdministratorApprovalDeclined>() {
            PairDeviceError::ApprovalDeclined
        } else {
            PairDeviceError::Internal(error)
        }
    })?;
    let approver_key = pending.approver_key.clone().map(|mut key| {
        key.device_id = device_id.clone();
        key.added_at_ms = now_ms();
        key
    });
    let record = RemotePairedDeviceRecord {
        id: device_id.clone(),
        name: pending.name.clone(),
        platform: pending.platform.clone(),
        ssh_key_fingerprint: key.fingerprint(),
        tailcat_client_key: pending.tailcat_client_key.clone(),
        api_token_hash: Some(hash_device_api_token(&device_api_token)),
        paired_at: now_iso(),
        last_seen_at: None,
        status: PairedDeviceStatus::Paired,
        device_key: Some(pending.device_key.clone()),
        api_token_expires_at: Some(
            token_expires_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
        approver_key_id: approver_key.as_ref().map(|key| key.key_id.clone()),
        legacy_key_line_removed: false,
    };
    let repository = RemotePairingRepository::new(context.db);
    if let Err(error) = repository.insert_device(&record) {
        // The key line grants access; never leave it behind without its row.
        return Err(match (context.remove_key)(&device_id) {
            Ok(_) => error.into(),
            Err(rollback_error) => {
                log_authorized_key_rollback_failure(context.logger, &device_id, &rollback_error);
                anyhow::anyhow!(
                    "{error:#}; additionally, the SSH key line ghostex-paired:{device_id} could not be removed from authorized_keys and must be deleted by hand: {rollback_error:#}"
                )
                .into()
            }
        });
    }

    let replaced = super::paired_devices::replace_earlier_pairings_of_same_phone(
        context.paths,
        context.db,
        &device_id,
        pending.tailcat_client_key.as_deref(),
        |id: &str| (context.remove_key)(id),
        |device, error| log_replaced_pairing_kept(context.logger, device, error),
    )?;
    for device in &replaced {
        super::paired_devices::forget_approver_key(context.security, device, &Principal::Anonymous);
    }
    if !replaced.is_empty() {
        log_replaced_pairings(context.logger, &device_id, &replaced);
    }

    super::paired_devices::reload_device_api_tokens(context.db, context.device_tokens)?;

    if let Some(client_key) = pending.tailcat_client_key.as_deref() {
        super::paired_devices::add_tailcat_client_key_when_allow_listed(
            context.paths,
            context.db,
            context.tailcat_runtime,
            client_key,
        )?;
    }
    if let Some(approver_key) = approver_key {
        // Phase 1 of #495 keeps approver keys in memory; Phase 2 persists them, and loads this
        // one back from the device row's `approverKeyId` with the key itself.
        if let Ok(key) = context.security.approver_keys.insert(approver_key) {
            context
                .security
                .publish(SecurityEvent::ApproverKeyAdded { key });
        }
    }
    let caller = Principal::Device {
        device_id: device_id.clone(),
    };
    context.security.record_performed(
        SensitiveAction::PairingApproveDevice,
        Some(device_id.clone()),
        &caller,
        true,
    );
    crate::audit::record(
        crate::audit::Action::new("pairing.pair", "A phone paired with this computer")
            .target(pending.name.clone()),
        crate::audit::Actor::PairedDevice {
            id: Some(device_id.clone()),
            name: pending.name.clone(),
        },
        crate::audit::Outcome::Approved,
        Some(crate::audit::ApprovalRef {
            id: pending.approval_id.clone(),
            signer: None,
        }),
    );

    let identity = read_remote_access_identity()?;
    Ok(json!({
        "deviceId": device_id,
        "deviceApiToken": device_api_token,
        "deviceTokenExpiresAtMs": token_expires_at.timestamp_millis(),
        "user": identity.username,
        "computerName": identity.computer_name,
    }))
}

/// `/api/renewDeviceToken`: a paired phone signs with its device key and gets a fresh token; the
/// old one stops working.
pub fn renew_device_token(
    db: &Connection,
    pairing_runtime: &RemotePairingRuntime,
    device_tokens: &DeviceApiTokens,
    ask: SignedAskParams,
) -> Result<Value, PairDeviceError> {
    let now = now_ms();
    let repository = RemotePairingRepository::new(db);
    let refused = || {
        PairDeviceError::BadSignature(
            "This computer doesn't know this phone's key any more, or the phone's clock is more than five minutes off. Pair the phone again if it was removed.".to_string(),
        )
    };
    let device = repository
        .find_device(&ask.id)?
        .filter(|device| device.status == PairedDeviceStatus::Paired)
        .ok_or_else(refused)?;
    let device_key = device.device_key.as_ref().ok_or_else(refused)?;
    // The signature is checked before the timestamp is remembered, so a forged ask can't push
    // the phone's next genuine renewal out.
    if !timestamp_is_fresh(ask.timestamp_ms, now)
        || !verify_device_signature(
            &device_key.public_key,
            &device_token_renew_payload(&device.id, ask.timestamp_ms),
            &ask.signature,
        )
        || !pairing_runtime.accept_renewal_timestamp(&device.id, ask.timestamp_ms)
    {
        return Err(refused());
    }
    let token = mint_device_api_token();
    let expires_at = chrono::Utc::now() + DEVICE_API_TOKEN_TTL;
    if !repository.replace_api_token(
        &device.id,
        &hash_device_api_token(&token),
        &expires_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )? {
        return Err(refused());
    }
    super::paired_devices::reload_device_api_tokens(db, device_tokens)?;
    Ok(json!({
        "deviceId": device.id,
        "deviceApiToken": token,
        "deviceTokenExpiresAtMs": expires_at.timestamp_millis(),
    }))
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

fn log_replaced_pairings(
    logger: &GxserverLogger,
    device_id: &str,
    replaced: &[RemotePairedDeviceRecord],
) {
    let replaced_ids: Vec<&str> = replaced.iter().map(|device| device.id.as_str()).collect();
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Info,
        event: "remotePairing.earlierPairingsReplaced".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(serde_json::json!({
            "deviceId": device_id,
            "replacedDeviceIds": replaced_ids,
        })),
    });
}

/// An earlier entry stays listed when its key line or its push registrations could not be removed.
fn log_replaced_pairing_kept(
    logger: &GxserverLogger,
    device: &RemotePairedDeviceRecord,
    error: &anyhow::Error,
) {
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: "remotePairing.earlierPairingKept".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(format!("{error:#}")),
        details: Some(serde_json::json!({ "deviceId": device.id })),
    });
}

/// Tests pair more than the per-minute limit in one process.
#[cfg(test)]
pub(crate) fn reset_pair_device_attempts() {
    PAIR_DEVICE_ATTEMPTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
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
