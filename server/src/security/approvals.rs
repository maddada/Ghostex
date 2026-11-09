//! The approval request store: create, list pending, approve with a signature, deny, expire,
//! and spend an approval on the one call it was made for.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ghostex_gx_protocol::approval::{
    approval_signing_payload, ApprovalRequest, ApprovalRequester, ApprovalStatus, SensitiveAction,
};
use rand::Rng;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    approver_keys::{verify_approval_signature, ApproverKeyError, ApproverKeys},
    Principal,
};

/// How long a request waits for the owner.
pub const PENDING_TTL_MS: u64 = 10 * 60 * 1000;
/// How long an approval stays spendable once signed.
pub const APPROVED_TTL_MS: u64 = 2 * 60 * 1000;
/// Decided requests stay readable this long (so the asking agent learns the answer), then go.
pub const DECIDED_RETENTION_MS: u64 = 30 * 60 * 1000;

/// One sensitive call, as the gate sees it: the action, what it is about, and its exact
/// parameters. The summary is written by gxserver from these, never taken from the caller.
#[derive(Clone, Debug)]
pub struct ActionRequest {
    pub action: SensitiveAction,
    pub target: Option<String>,
    /// The call's parameters without [`ghostex_gx_protocol::approval::APPROVAL_ID_PARAM`].
    pub params: Value,
    pub summary: String,
    pub detail: Option<String>,
    /// Any caller's identical pending request is reused, and may be spent by any caller that
    /// could ask for it (see [`ActionRequest::shared`]).
    pub shared: bool,
    /// How long the request waits for the owner, when not [`PENDING_TTL_MS`].
    pub pending_ttl_ms: Option<u64>,
    /// What gxserver does itself the moment the owner approves (see [`ApprovalEffect`]).
    pub on_approve: Option<ApprovalEffect>,
}

/*
CDXC:Security 2026-11-10 WHY:
A Ghostex user with four computers approved "Let <Mac> (Ghostex app) act as the owner of Ghostex on this computer" on a connected computer, and the Mac still couldn't answer that computer's requests: the key went in only when the Mac's call came back with the approval within its two minutes (it asked again only when the owner next acted there), and its approver key then needed a second approval that the connected computer's app never showed. A key the owner approves now goes in the moment they approve it, whoever comes back for it.
*/
/// What gxserver does itself when the owner approves a request, before (and whether or not) the
/// asking call comes back with its approval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalEffect {
    /// Enroll these keys: the owner key another computer's app asked to add (with that app's
    /// approver key, so the one approval also lets it answer requests here), or an approver key.
    EnrollKeys {
        owner: Option<ghostex_gx_protocol::approval::ApproverKey>,
        approver: Option<ghostex_gx_protocol::approval::ApproverKey>,
    },
}

impl ActionRequest {
    pub fn new(action: SensitiveAction, summary: impl Into<String>) -> Self {
        Self {
            action,
            target: None,
            params: Value::Null,
            summary: summary.into(),
            detail: None,
            shared: false,
            pending_ttl_ms: None,
            on_approve: None,
        }
    }

    /// Runs `effect` the moment the owner approves (see [`ApprovalEffect`]).
    pub fn on_approve(mut self, effect: ApprovalEffect) -> Self {
        self.on_approve = Some(effect);
        self
    }

    /*
    CDXC:Security 2026-11-08 WHY:
    User, on 11.1.0: "Man i see 1 million pin requests on my windows machine saying let the agents on x and x reach each other. Please don't do this! You spammed my machine with those, have like 50 open." A request about a thing rather than about the caller (letting another computer in) is one question however many callers ask it: the app's owner session and its token file, this app's renewal and the other computer's, all reuse the one that is waiting, so the owner answers once.
    */
    /// One request for this change whoever asks: an identical pending request from any caller is
    /// reused, and the approval may be spent by any caller that can ask for it.
    pub fn shared(mut self) -> Self {
        self.shared = true;
        self
    }

    /// Waits `ttl_ms` for the owner instead of [`PENDING_TTL_MS`].
    pub fn pending_ttl(mut self, ttl_ms: u64) -> Self {
        self.pending_ttl_ms = Some(ttl_ms);
        self
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn params(mut self, params: Value) -> Self {
        self.params = strip_approval_id(params);
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn params_digest(&self) -> String {
        params_digest(&self.params)
    }
}

/// Why the store refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalError {
    NotFound,
    NotPending(ApprovalStatus),
    Expired,
    Key(ApproverKeyError),
    /// The approval exists but was made for a different action, target or parameters.
    Mismatch,
    /// The approval is still waiting for the owner.
    StillPending,
    Denied,
    BadPayload(String),
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("No such approval request."),
            Self::NotPending(status) => {
                write!(
                    f,
                    "This request was already answered ({}).",
                    status.as_str()
                )
            }
            Self::Expired => f.write_str("This approval request expired."),
            Self::Key(error) => error.fmt(f),
            Self::Mismatch => f.write_str("This approval was given for a different change."),
            Self::StillPending => f.write_str("This request is still waiting for your approval."),
            Self::Denied => f.write_str("This request was denied."),
            Self::BadPayload(message) => f.write_str(message),
        }
    }
}

struct StoredRequest {
    request: ApprovalRequest,
    /// Who asked; only they (or the owner's devices) may read the request back.
    requester: Principal,
    /// [`ActionRequest::shared`]: any caller that can ask may read and spend it.
    shared: bool,
    on_approve: Option<ApprovalEffect>,
}

impl StoredRequest {
    fn open_to(&self, caller: &Principal) -> bool {
        caller.is_owner_device()
            || self.requester == *caller
            || (self.shared && caller.has(super::Capability::RequestSensitiveAction))
    }
}

/// In-memory on purpose: requests live minutes, and a gxserver restart dropping them only
/// means the agent asks again.
#[derive(Clone, Default)]
pub struct ApprovalStore {
    requests: Arc<Mutex<HashMap<String, StoredRequest>>>,
}

impl ApprovalStore {
    /// A new pending request (`true`), or the identical one this caller already has pending
    /// (`false`).
    pub fn create(
        &self,
        requester: &Principal,
        requested_by: ApprovalRequester,
        action: &ActionRequest,
        now_ms: u64,
    ) -> (ApprovalRequest, bool) {
        let digest = action.params_digest();
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        if let Some(existing) = requests.values().find(|stored| {
            (stored.requester == *requester || (action.shared && stored.shared))
                && stored.request.status == ApprovalStatus::Pending
                && stored.request.action == action.action
                && stored.request.target == action.target
                && stored.request.params_digest == digest
        }) {
            return (existing.request.clone(), false);
        }
        let request = ApprovalRequest {
            id: format!("apr_{}", random_b64(12)),
            action: action.action.clone(),
            target: action.target.clone(),
            summary: action.summary.clone(),
            detail: action.detail.clone(),
            params_digest: digest,
            nonce: random_b64(32),
            requested_by,
            created_at_ms: now_ms,
            expires_at_ms: now_ms + action.pending_ttl_ms.unwrap_or(PENDING_TTL_MS),
            status: ApprovalStatus::Pending,
            decided_at_ms: None,
            decided_by: None,
        };
        requests.insert(
            request.id.clone(),
            StoredRequest {
                request: request.clone(),
                requester: requester.clone(),
                shared: action.shared,
                on_approve: action.on_approve.clone(),
            },
        );
        (request, true)
    }

    /// Every pending request, oldest first.
    pub fn list_pending(&self, now_ms: u64) -> Vec<ApprovalRequest> {
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        let mut pending: Vec<_> = requests
            .values()
            .filter(|stored| stored.request.status == ApprovalStatus::Pending)
            .map(|stored| stored.request.clone())
            .collect();
        pending.sort_by_key(|request| request.created_at_ms);
        pending
    }

    /// The request, for its requester or the owner's devices only.
    pub fn read(&self, id: &str, caller: &Principal, now_ms: u64) -> Option<ApprovalRequest> {
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        requests
            .get(id)
            .filter(|stored| stored.open_to(caller))
            .map(|stored| stored.request.clone())
    }

    /// Checks `signature` from approver key `key_id` over the request's signing payload and
    /// marks it approved.
    pub fn approve(
        &self,
        id: &str,
        key_id: &str,
        signature_b64: &str,
        keys: &ApproverKeys,
        server_id: &str,
        now_ms: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        let stored = requests.get_mut(id).ok_or(ApprovalError::NotFound)?;
        match &stored.request.status {
            ApprovalStatus::Pending => {}
            ApprovalStatus::Expired => return Err(ApprovalError::Expired),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        let key = keys
            .get(key_id)
            .ok_or(ApprovalError::Key(ApproverKeyError::UnknownKey))?;
        let payload = approval_signing_payload(server_id, &stored.request)
            .map_err(|error| ApprovalError::BadPayload(error.to_string()))?;
        verify_approval_signature(&key, &payload, signature_b64).map_err(ApprovalError::Key)?;
        stored.request.status = ApprovalStatus::Approved;
        stored.request.decided_at_ms = Some(now_ms);
        stored.request.decided_by = Some(key.key_id.clone());
        stored.request.expires_at_ms = now_ms + APPROVED_TTL_MS;
        Ok(stored.request.clone())
    }

    /// What request `id` does when approved ([`ApprovalEffect`]), if anything.
    pub fn effect_of(&self, id: &str) -> Option<ApprovalEffect> {
        self.lock()
            .get(id)
            .and_then(|stored| stored.on_approve.clone())
    }

    /// Denying needs no signature: saying no only closes a door.
    pub fn deny(
        &self,
        id: &str,
        by: &Principal,
        now_ms: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        let stored = requests.get_mut(id).ok_or(ApprovalError::NotFound)?;
        match &stored.request.status {
            ApprovalStatus::Pending | ApprovalStatus::Approved => {}
            ApprovalStatus::Expired => return Err(ApprovalError::Expired),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        stored.request.status = ApprovalStatus::Denied;
        stored.request.decided_at_ms = Some(now_ms);
        stored.request.decided_by = Some(match by {
            Principal::Owner { device_id } | Principal::Device { device_id } => device_id.clone(),
            other => other.kind().to_string(),
        });
        Ok(stored.request.clone())
    }

    /// Spends approval `id` on `action` for `caller`, who must be the one that asked (or one of
    /// the owner's devices). Single use: a second call with the same id fails.
    pub fn consume(
        &self,
        id: &str,
        caller: &Principal,
        action: &ActionRequest,
        now_ms: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        let mut requests = self.lock();
        sweep(&mut requests, now_ms);
        let stored = requests
            .get_mut(id)
            .filter(|stored| stored.open_to(caller))
            .ok_or(ApprovalError::NotFound)?;
        let request = &mut stored.request;
        if request.action != action.action
            || request.target != action.target
            || request.params_digest != action.params_digest()
        {
            return Err(ApprovalError::Mismatch);
        }
        match &request.status {
            ApprovalStatus::Approved => {}
            ApprovalStatus::Pending => return Err(ApprovalError::StillPending),
            ApprovalStatus::Denied => return Err(ApprovalError::Denied),
            ApprovalStatus::Expired => return Err(ApprovalError::Expired),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        request.status = ApprovalStatus::Consumed;
        Ok(request.clone())
    }

    /// Marks lapsed requests expired and returns them (for [`super::SecurityEvent::ApprovalExpired`]).
    pub fn expire(&self, now_ms: u64) -> Vec<ApprovalRequest> {
        sweep(&mut self.lock(), now_ms)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, StoredRequest>> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn sweep(requests: &mut HashMap<String, StoredRequest>, now_ms: u64) -> Vec<ApprovalRequest> {
    let mut expired = Vec::new();
    for stored in requests.values_mut() {
        let request = &mut stored.request;
        let live = matches!(
            request.status,
            ApprovalStatus::Pending | ApprovalStatus::Approved
        );
        if live && now_ms >= request.expires_at_ms {
            request.status = ApprovalStatus::Expired;
            request.decided_at_ms = Some(now_ms);
            expired.push(request.clone());
        }
    }
    requests.retain(|_, stored| {
        let request = &stored.request;
        let decided_at = request.decided_at_ms.unwrap_or(request.expires_at_ms);
        matches!(
            request.status,
            ApprovalStatus::Pending | ApprovalStatus::Approved
        ) || now_ms < decided_at + DECIDED_RETENTION_MS
    });
    expired
}

/// Lowercase hex SHA-256 of `params` as canonical JSON (object keys sorted at every level).
pub fn params_digest(params: &Value) -> String {
    let mut canonical = String::new();
    write_canonical(params, &mut canonical);
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

fn strip_approval_id(mut params: Value) -> Value {
    if let Value::Object(map) = &mut params {
        map.remove(ghostex_gx_protocol::approval::APPROVAL_ID_PARAM);
    }
    params
}

fn random_b64(bytes: usize) -> String {
    let mut random = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut random);
    URL_SAFE_NO_PAD.encode(random)
}
