use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ghostex_gx_protocol::approval::ApproverKey;
use ghostex_gx_protocol::pairing::PendingDevice;
use serde_json::Value;

use super::repository::{RemotePairingRepository, StoredDeviceKey};
use crate::security::{ActionRequest, SecurityState};
use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};
use serde_json::json;

/*
CDXC:RemotePairing 2026-09-03:
Mirror of `packages/shared/ghostex-remote-pairing.ts`: each code is
`<prefix><base64url(compact JSON)>` with no padding and no whitespace. Field
names are the wire names; keep them identical to the TS interfaces.
*/
pub const EASY_CONNECT_CODE_PREFIX: &str = "ghostex-ec1:";
pub const TAILSCALE_CODE_PREFIX: &str = "ghostex-ts1:";

/*
CDXC:RemotePairing 2026-10-24 DECISION:
User: "even tho i have ghostex installed on my phone it says there aren't any
apps that can open this link when i open it (this is on android when i use
default camera, pls fix, i want same on ios also if possible)". A bare
`ghostex-ec1:…` code reads to a system camera as a link with the unknown
scheme `ghostex-ec1`, which no app claims. The QR now carries the code as the
fragment of an https link on ghostex.dev, which the phone app claims as an
Android App Link and an iOS Universal Link, so the system camera opens the
app straight into pairing. Without the app, the link opens ghostex.dev/pair,
a page that says to install Ghostex. The code stays after `#` because a
browser never sends the fragment to the server, so the one-time secret
cannot reach ghostex.dev's logs; its lifetime and one-time use are unchanged.
The app's scanner and the desktop's code field read the link and the bare
code alike.
SEE-ALSO: apps/mobile/app/app.json (intentFilters, associatedDomains),
maddada-com/public/.well-known/ (assetlinks.json, apple-app-site-association).
*/
pub const PAIRING_LINK_PREFIX: &str = "https://ghostex.dev/pair#";

/*
CDXC:RemotePairing 2026-10-27 WHY:
Testers' Android phones could not read the pairing QR: a link around
base64url(JSON) whose `address` is itself base64url made a version 19 code
(93 modules a side), too dense for the phone's scanner at the size the Show QR
dialog draws it. The compact code carries the same fields as bytes in base32,
behind an upper-case prefix, so after the link everything is in the QR's
alphanumeric mode (5.5 bits a character instead of 8): about version 11 at
error correction L. Layout, every length a big-endian u16: address (the `tc`
blob's base64url bytes), secret (its base64url bytes), port, SSH port, user
(UTF-8), name (UTF-8).
SEE-ALSO: packages/shared/ghostex-remote-pairing.ts and
apps/mobile/app/src/machines/pairingCodes.ts (`readEasyConnectCompactCode`),
apps/desktop/src/app/window/settings_modal/tabs/remote/model.rs.
*/
pub const EASY_CONNECT_COMPACT_CODE_PREFIX: &str = "GHOSTEX-EC2:";
const TAILCAT_ADDRESS_PREFIX: &str = "tc";
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495, Q1721): a pairing code is short-lived and single-use, and only the desktop's
own Show QR sheet makes one. Five minutes (was 15): the sheet rotates the code by itself while it
stays open, and the code alone no longer pairs anything (the phone waits as Pending for the owner's
signed approval), so a short life costs nothing.
*/
pub const REMOTE_PAIRING_SECRET_TTL: Duration = Duration::minutes(5);

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

/// `code` in the compact `GHOSTEX-EC2:` form, or `None` when its address or
/// secret is not canonical base64url (the bytes would not give the same text
/// back) or a field does not fit its u16 length.
pub fn encode_easy_connect_compact_code(code: &EasyConnectCode) -> Option<String> {
    let address = decode_canonical_base64url(code.address.strip_prefix(TAILCAT_ADDRESS_PREFIX)?)?;
    let secret = decode_canonical_base64url(code.secret.as_deref()?)?;
    let mut bytes = Vec::new();
    push_field(&mut bytes, &address)?;
    push_field(&mut bytes, &secret)?;
    bytes.extend_from_slice(&code.port.to_be_bytes());
    bytes.extend_from_slice(&code.ssh_port.to_be_bytes());
    push_field(&mut bytes, code.user.as_bytes())?;
    push_field(&mut bytes, code.name.as_bytes())?;
    Some(format!(
        "{EASY_CONNECT_COMPACT_CODE_PREFIX}{}",
        encode_base32(&bytes)
    ))
}

fn decode_canonical_base64url(text: &str) -> Option<Vec<u8>> {
    let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
    (URL_SAFE_NO_PAD.encode(&bytes) == text).then_some(bytes)
}

fn push_field(bytes: &mut Vec<u8>, field: &[u8]) -> Option<()> {
    bytes.extend_from_slice(&u16::try_from(field.len()).ok()?.to_be_bytes());
    bytes.extend_from_slice(field);
    Some(())
}

/// RFC 4648 base32 without padding.
fn encode_base32(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32_ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32_ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The QR payload: an encoded pairing code wrapped in the ghostex.dev link that
/// opens the phone app.
pub fn pairing_link(code: &str) -> String {
    format!("{PAIRING_LINK_PREFIX}{code}")
}

/// The Easy Connect QR payload: the compact code in its ghostex.dev link, or the
/// `ghostex-ec1:` code when the compact form cannot carry this code.
///
/// CDXC:RemotePairing 2026-11-02 DECISION:
/// The user chose (Q618) to switch the pairing QR to this code "in the release after" the phones
/// got its reader (10.37); it went out with the fix for issue #394, whose cause was the Android
/// release build (see the phone's plugins/withAndroidReleaseSize.js), not the QR's density.
/// `/api/remotePairingCode` puts it in the QR's link and keeps
/// `ghostex-ec1:` for Copy as text, which every phone and computer reads; a code this cannot
/// encode falls back to the `ghostex-ec1:` link. Phone apps older than 10.37 (not updated over the
/// air) cannot read the new QR and pair with Copy as text.
pub fn easy_connect_pairing_link(code: &EasyConnectCode) -> String {
    match encode_easy_connect_compact_code(code) {
        Some(compact) => pairing_link(&compact),
        None => pairing_link(&encode_easy_connect_code(code)),
    }
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
    /// Phones waiting for approval (pairing v2), by pending id. A waiting one is also kept with
    /// its approval request ([`PendingPairing::saved`]), so after a gxserver restart it waits
    /// again ([`RemotePairingRuntime::restore_waiting`]) and the card shown before still pairs it.
    pending: Arc<Mutex<HashMap<String, PendingPairing>>>,
    /// The last renewal timestamp each device key signed, so a renewal can't be replayed.
    renewals: Arc<Mutex<HashMap<String, u64>>>,
}

/// A phone that scanned the QR and waits for the owner to approve it.
#[derive(Clone, Debug)]
pub struct PendingPairing {
    pub id: String,
    pub name: String,
    pub platform: String,
    /// Empty for a computer.
    pub ssh_public_key: String,
    pub kind: super::pair_device::PairingDeviceKind,
    /// A phone that asked over its own SSH login (`/api/pairOverSsh`): only its device token,
    /// unless it also sent Easy Connect keys (`ssh_public_key`, `tailcat_client_key`), which then
    /// get the SSH key line and the allow-list entry a QR's phone gets.
    pub over_ssh: bool,
    /// Approved without a click: the phone signs in over SSH from the owner's own tailnet device
    /// (`tailnet_owner.rs`); the node it came from, for the security log. Never saved: such a
    /// pairing finishes on the phone's next ask, seconds later.
    pub auto_approved: Option<String>,
    /// The nonce an over-SSH phone's words come from (empty for a QR's phone, whose words come
    /// from the code), so the same phone asking again gets the same request back.
    pub nonce: String,
    pub tailcat_client_key: Option<String>,
    pub device_key: StoredDeviceKey,
    pub approver_key: Option<ApproverKey>,
    pub words: Vec<String>,
    /// What the owner approves; spent with exactly these facts.
    pub action: ActionRequest,
    pub approval_id: String,
    pub requested_at_ms: u64,
    pub expires_at_ms: u64,
    /// The newest signed ask's timestamp; older or equal ones are refused.
    pub last_ask_ms: u64,
    pub state: PendingState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PendingState {
    Waiting,
    /// The approval was spent and the pairing is being written (a Windows administrator prompt
    /// can hold it); a second ask meanwhile hears "pending".
    Finalizing,
    /// Paired: the answer is kept briefly for a phone whose reply was lost.
    Finished {
        answer: Value,
        at_ms: u64,
    },
}

/// The kind of data a waiting pairing keeps with its approval request (`ApprovalStore::attach`).
pub const PAIRING_RESUME: &str = "pairing";

impl PendingPairing {
    /// What a waiting pairing keeps with its approval request: everything but the request's own
    /// id and expiry, which come back with it.
    pub fn saved(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "platform": self.platform,
            "sshPublicKey": self.ssh_public_key,
            "kind": match self.kind {
                super::pair_device::PairingDeviceKind::Phone => "phone",
                super::pair_device::PairingDeviceKind::Computer => "computer",
            },
            "overSsh": self.over_ssh,
            "nonce": self.nonce,
            "tailcatClientKey": self.tailcat_client_key,
            "deviceKey": {
                "algorithm": self.device_key.algorithm,
                "publicKey": self.device_key.public_key,
                "keyId": self.device_key.key_id,
            },
            "approverKey": self.approver_key,
            "words": self.words,
            "action": {
                "target": self.action.target,
                "params": self.action.params,
                "summary": self.action.summary,
                "detail": self.action.detail,
                "replaces": self.action.replaces,
            },
            "requestedAtMs": self.requested_at_ms,
            "lastAskMs": self.last_ask_ms,
        })
    }

    /// The waiting pairing [`PendingPairing::saved`] kept with `request`.
    pub fn from_saved(saved: &Value, request: &ApprovalRequest) -> Option<Self> {
        let text = |value: &Value| value.as_str().map(str::to_string);
        let action = &saved["action"];
        let mut approval = ActionRequest::new(
            SensitiveAction::PairingApproveDevice,
            text(&action["summary"])?,
        )
        .params(action["params"].clone());
        if let Some(target) = text(&action["target"]) {
            approval = approval.target(target);
        }
        if let Some(detail) = text(&action["detail"]) {
            approval = approval.detail(detail);
        }
        if let Some(key) = text(&action["replaces"]) {
            approval = approval.replaces(key);
        }
        Some(Self {
            id: text(&saved["id"])?,
            name: text(&saved["name"])?,
            platform: text(&saved["platform"])?,
            ssh_public_key: text(&saved["sshPublicKey"]).unwrap_or_default(),
            kind: serde_json::from_value(saved["kind"].clone()).ok()?,
            over_ssh: saved["overSsh"].as_bool().unwrap_or(false),
            auto_approved: None,
            nonce: text(&saved["nonce"]).unwrap_or_default(),
            tailcat_client_key: text(&saved["tailcatClientKey"]),
            device_key: StoredDeviceKey {
                algorithm: text(&saved["deviceKey"]["algorithm"])?,
                public_key: text(&saved["deviceKey"]["publicKey"])?,
                key_id: text(&saved["deviceKey"]["keyId"])?,
            },
            approver_key: serde_json::from_value(saved["approverKey"].clone())
                .ok()
                .flatten(),
            words: serde_json::from_value(saved["words"].clone()).ok()?,
            action: approval,
            approval_id: request.id.clone(),
            requested_at_ms: saved["requestedAtMs"]
                .as_u64()
                .unwrap_or(request.created_at_ms),
            expires_at_ms: request.expires_at_ms,
            last_ask_ms: saved["lastAskMs"].as_u64().unwrap_or(0),
            state: PendingState::Waiting,
        })
    }

    pub fn summary(&self) -> PendingDevice {
        PendingDevice {
            id: self.id.clone(),
            name: self.name.clone(),
            platform: self.platform.clone(),
            words: self.words.clone(),
            approval_id: self.approval_id.clone(),
            requested_at_ms: self.requested_at_ms,
            expires_at_ms: self.expires_at_ms,
            over_ssh: self.over_ssh,
        }
    }

    fn is_live(&self, now_ms: u64) -> bool {
        match &self.state {
            PendingState::Finished { at_ms, .. } => {
                now_ms < at_ms + super::pair_device::FINISHED_PAIRING_RETENTION_MS
            }
            PendingState::Finalizing => true,
            PendingState::Waiting => now_ms < self.expires_at_ms,
        }
    }
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

    fn pending_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingPairing>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn insert_pending(&self, pending: PendingPairing) {
        self.pending_map().insert(pending.id.clone(), pending);
    }

    /// Keeps `pending` waiting and saves it with its approval request, so it waits again after a
    /// gxserver restart.
    pub fn insert_waiting(&self, security: &SecurityState, pending: PendingPairing) {
        security
            .approvals
            .attach(&pending.approval_id, PAIRING_RESUME, pending.saved());
        self.insert_pending(pending);
    }

    /// Drops the waiting pairings whose requests `replaced` lists (a newer ask from the same
    /// device replaced them).
    pub fn drop_replaced(&self, replaced: &[ApprovalRequest]) {
        self.pending_map().retain(|_, pending| {
            !replaced
                .iter()
                .any(|request| request.id == pending.approval_id)
        });
    }

    /// At gxserver start: the pairings still waiting (or approved, not yet picked up by the phone)
    /// when it stopped wait again under the same pending id, words and expiry.
    pub fn restore_waiting(&self, security: &SecurityState) -> usize {
        let now = crate::security::now_ms();
        let restored: Vec<_> = security
            .approvals
            .resumable(PAIRING_RESUME, now)
            .into_iter()
            .filter_map(|(request, saved)| PendingPairing::from_saved(&saved, &request))
            .collect();
        let count = restored.len();
        for pending in restored {
            self.insert_pending(pending);
        }
        count
    }

    /// The live entry for `id`; lapsed entries are dropped on the way.
    pub fn pending(&self, id: &str, now_ms: u64) -> Option<PendingPairing> {
        let mut map = self.pending_map();
        map.retain(|_, pending| pending.is_live(now_ms));
        map.get(id).cloned()
    }

    /// The phones still waiting for approval, oldest first.
    pub fn list_waiting(&self, now_ms: u64) -> Vec<PendingDevice> {
        let mut map = self.pending_map();
        map.retain(|_, pending| pending.is_live(now_ms));
        let mut waiting: Vec<_> = map
            .values()
            .filter(|pending| {
                pending.state == PendingState::Waiting && pending.auto_approved.is_none()
            })
            .map(PendingPairing::summary)
            .collect();
        waiting.sort_by_key(|pending| pending.requested_at_ms);
        waiting
    }

    /// The live over-SSH request still waiting for the phone with this device key, if any.
    pub fn waiting_over_ssh(&self, device_public_key: &str, now_ms: u64) -> Option<PendingPairing> {
        let mut map = self.pending_map();
        map.retain(|_, pending| pending.is_live(now_ms));
        map.values()
            .filter(|pending| {
                pending.over_ssh
                    && pending.auto_approved.is_none()
                    && pending.state == PendingState::Waiting
                    && pending.device_key.public_key == device_public_key
            })
            .max_by_key(|pending| pending.requested_at_ms)
            .cloned()
    }

    pub fn note_ask(&self, id: &str, timestamp_ms: u64) {
        if let Some(pending) = self.pending_map().get_mut(id) {
            pending.last_ask_ms = pending.last_ask_ms.max(timestamp_ms);
        }
    }

    /// Marks `id` as being written; `false` when another ask already is.
    pub fn start_finalizing(&self, id: &str) -> bool {
        match self.pending_map().get_mut(id) {
            Some(pending) if pending.state == PendingState::Waiting => {
                pending.state = PendingState::Finalizing;
                true
            }
            _ => false,
        }
    }

    pub fn finish_pending(&self, id: &str, answer: Value, now_ms: u64) {
        if let Some(pending) = self.pending_map().get_mut(id) {
            pending.state = PendingState::Finished {
                answer,
                at_ms: now_ms,
            };
        }
    }

    pub fn remove_pending(&self, id: &str) -> Option<PendingPairing> {
        self.pending_map().remove(id)
    }

    /// Records a renewal's timestamp; `false` for one no newer than the last accepted.
    pub fn accept_renewal_timestamp(&self, device_id: &str, timestamp_ms: u64) -> bool {
        let mut renewals = self
            .renewals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let last = renewals.entry(device_id.to_string()).or_insert(0);
        if timestamp_ms <= *last {
            return false;
        }
        *last = timestamp_ms;
        true
    }
}

/*
CDXC:RemotePairing 2026-10-27 WHY:
16 random bytes (22 characters): the secret is single-use, lives 15 minutes and
is only reachable through the Easy Connect tunnel, so 128 bits is ample, and
the 21 characters saved shrink the QR. Codes minted with 32 bytes still pair.
*/
fn mint_remote_pairing_secret(expires_at: DateTime<Utc>) -> RemotePairingSecret {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let secret = URL_SAFE_NO_PAD.encode(bytes);
    RemotePairingSecret {
        secret_hash: hash_remote_pairing_secret(&secret),
        secret,
        expires_at,
    }
}

pub fn hash_remote_pairing_secret(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn easy_connect_code() -> EasyConnectCode {
        EasyConnectCode {
            v: 1,
            address: "tc1qexampleaddress".to_string(),
            name: "Studio".to_string(),
            user: "madda".to_string(),
            port: 58744,
            ssh_port: 22,
            secret: Some("s3cr3t-pairing-secret".to_string()),
            expires_at: Some("2026-10-24T07:15:00.000Z".to_string()),
        }
    }

    #[test]
    fn easy_connect_code_keeps_its_bare_format() {
        let encoded = encode_easy_connect_code(&easy_connect_code());
        let body = encoded.strip_prefix("ghostex-ec1:").expect("bare prefix");
        let json: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).expect("base64url")).unwrap();
        assert_eq!(json["secret"], "s3cr3t-pairing-secret");
        assert_eq!(json["sshPort"], 22);
    }

    #[test]
    fn pairing_link_wraps_the_code_in_the_fragment() {
        let code = encode_easy_connect_code(&easy_connect_code());
        let link = pairing_link(&code);
        assert_eq!(link, format!("https://ghostex.dev/pair#{code}"));
        let (before_fragment, fragment) = link.split_once('#').expect("fragment");
        assert_eq!(before_fragment, "https://ghostex.dev/pair");
        assert_eq!(fragment, code);
        // Nothing a web server sees (scheme, host, path, query) carries the code.
        assert!(!before_fragment.contains("ghostex-ec1"));
        assert!(!before_fragment.contains('?'));
    }

    /// The vector `ghostex-remote-pairing.test.ts`, the phone's `pairingCodes.test.ts` and the
    /// desktop's `model.rs` tests decode.
    const COMPACT_VECTOR: &str = "GHOSTEX-EC2:AAUAAAICAMCAKBQHBAEQUCYMBUHA6EARCIJRIFIWC4MBSGQ3DQOR4HZAEERCGJBFEYTQAEDEMVTGO2DJNJVWY3LON5YHC4TT4V4AAFQAAVWWCZDEMEAA4U3UOVSGS37CQCMXGICNMFRQ";

    fn compact_vector_code() -> EasyConnectCode {
        EasyConnectCode {
            v: 1,
            address: "tcAAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJw".to_string(),
            name: "Studio\u{2019}s Mac".to_string(),
            user: "madda".to_string(),
            port: 58744,
            ssh_port: 22,
            secret: Some("ZGVmZ2hpamtsbW5vcHFycw".to_string()),
            expires_at: None,
        }
    }

    #[test]
    fn compact_code_matches_the_shared_vector() {
        assert_eq!(
            encode_easy_connect_compact_code(&compact_vector_code()).as_deref(),
            Some(COMPACT_VECTOR)
        );
    }

    #[test]
    fn compact_code_link_is_alphanumeric_after_the_fragment() {
        let link = pairing_link(&encode_easy_connect_compact_code(&compact_vector_code()).unwrap());
        let fragment = link.strip_prefix(PAIRING_LINK_PREFIX).unwrap();
        assert!(fragment.bytes().all(|byte| byte.is_ascii_uppercase()
            || byte.is_ascii_digit()
            || byte == b'-'
            || byte == b':'));
    }

    /// A code shaped like a real one (a 172-character `tc` address, a minted 16-byte secret, a
    /// name with a curly apostrophe); the phone's `pairingCodes.test.ts` reads the same vector.
    const REAL_SIZE_COMPACT_VECTOR: &str = "GHOSTEX-EC2:ACWKIYLQLAQKLUCIJT3JNPW66TINDCM6S4MCEQPGBD46JDXIZPMAE3MRB5BVYLDBNNMCAWEHCYJZHSN7VQMY2JJLNVYKPTEUUHDNXVMBW4FJG2AEKMULINKHMFYVQIAP6E4KDHT62XKPPYQIIOHARJT35TNTXIBMRMRJILTSZ7VFBYA2KZQXFANBMFHIDI3BNBXHIYZTGAZWCLTJOBXC4ZDFOZQTI3ZRHA2S4MJXHAXDEMBSFYYTSN3BGZYTEYJQGA5GIZBYGA5DEMB2HIZDANYACAROR4FHD5VGFD4HJP3I7BIW7LROK6AACYAAK3LBMRSGCAAWKN2HKZDJN7RIBGLTEBGWCY2CN5XWWICQOJXQ";

    #[test]
    fn easy_connect_qr_carries_the_compact_code() {
        assert_eq!(
            easy_connect_pairing_link(&compact_vector_code()),
            format!("https://ghostex.dev/pair#{COMPACT_VECTOR}")
        );
        let code = EasyConnectCode {
            v: 1,
            address: "tcpGFwWCCl0EhM9pa-3vTQ0YmelxgiQeYI-eSO6MvYAm2RD0NcLGFrWCBYhxYTk8m_rBmNJSttcKfMlKHG29WBtwqTaARTKLQ1R2FxWCAP8Tihnn7V1PfiCEOOCKZ77Ns7oCyLIpQucs_qUOAaVmFygaFhToGjYWhudGMzMDNhLmlwbi5kZXZhNG8xODUuMTc4LjIwMi4xOTdhNnEyYTAwOmRkODA6MjA6OjIwNw".to_string(),
            name: "Studio\u{2019}s MacBook Pro".to_string(),
            user: "madda".to_string(),
            port: 58744,
            ssh_port: 22,
            secret: Some("Iujwpx9qYo-HS_aPhRb64g".to_string()),
            expires_at: None,
        };
        let link = easy_connect_pairing_link(&code);
        assert_eq!(
            link,
            format!("https://ghostex.dev/pair#{REAL_SIZE_COMPACT_VECTOR}")
        );
        // About a fifth shorter than the `ghostex-ec1:` link.
        assert!(link.len() * 5 < pairing_link(&encode_easy_connect_code(&code)).len() * 4);
    }

    #[test]
    fn easy_connect_qr_keeps_the_json_code_when_compact_cannot_carry_it() {
        let mut code = compact_vector_code();
        code.address = "not-a-tailcat-address".to_string();
        assert_eq!(
            easy_connect_pairing_link(&code),
            pairing_link(&encode_easy_connect_code(&code))
        );
    }

    #[test]
    fn compact_code_refuses_what_it_cannot_give_back() {
        let mut code = compact_vector_code();
        code.secret = None;
        assert_eq!(encode_easy_connect_compact_code(&code), None);
        let mut code = compact_vector_code();
        code.address = "not-a-tailcat-address".to_string();
        assert_eq!(encode_easy_connect_compact_code(&code), None);
        let mut code = compact_vector_code();
        // Non-zero trailing bits decode to bytes that encode back differently.
        code.secret = Some("ZGVmZ2hpamtsbW5vcHFycx".to_string());
        assert_eq!(encode_easy_connect_compact_code(&code), None);
    }

    #[test]
    fn minted_secret_is_sixteen_bytes() {
        let secret = mint_remote_pairing_secret(Utc::now());
        assert_eq!(secret.secret.len(), 22);
        assert_eq!(URL_SAFE_NO_PAD.decode(&secret.secret).unwrap().len(), 16);
    }

    #[test]
    fn tailscale_link_uses_the_same_wrapper() {
        let code = encode_tailscale_code(&TailscaleCode {
            v: 1,
            name: "Studio".to_string(),
            host: Some("studio.tail1234.ts.net".to_string()),
            ip: None,
            port: 22,
            user: "madda".to_string(),
        });
        assert!(code.starts_with("ghostex-ts1:"));
        assert_eq!(
            pairing_link(&code),
            format!("https://ghostex.dev/pair#{code}")
        );
    }
}
