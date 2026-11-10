//! The approval request store: create, list pending, approve, deny, expire, and spend an
//! approval on the one call it was made for. Saved in `gxserver/auth/approval-requests.json`, so a
//! request still waiting after a gxserver restart keeps its id, words and expiry.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
pub use ghostex_gx_protocol::approval::{expired_message, GONE_MESSAGE};
use ghostex_gx_protocol::approval::{
    ApprovalRequest, ApprovalRequester, ApprovalStatus, SensitiveAction,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{approver_keys::ApproverKeyError, Principal};

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
    /// What this request is about for [`ApprovalStore::create`]'s replacing: a newer request with
    /// the same key from the same asker replaces the one still waiting. `None`: the action and
    /// its target (see [`ActionRequest::replace_key`]).
    pub replaces: Option<String>,
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
            replaces: None,
        }
    }

    /// A newer request with the same `key` from the same asker replaces the one still waiting
    /// (a phone asking to pair again with the same device key, whatever its new words).
    pub fn replaces(mut self, key: impl Into<String>) -> Self {
        self.replaces = Some(key.into());
        self
    }

    /// The key a newer request replaces an older one by: [`ActionRequest::replaces`], else the
    /// action and its target. A Collab guest's message is never replaced: each one is its own
    /// question.
    pub fn replace_key(&self) -> Option<String> {
        if let Some(key) = &self.replaces {
            return Some(format!("{}:{key}", self.action.as_str()));
        }
        (self.action != SensitiveAction::CollabApproveGuestMessage).then(|| {
            format!(
                "{}:{}",
                self.action.as_str(),
                self.target.as_deref().unwrap_or("")
            )
        })
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
    /// Lapsed unanswered (or unused once approved); the words say what to do now.
    Expired(String),
    /// A newer request from the same asker for the same thing replaced it: approve that one.
    Replaced(String),
    Key(ApproverKeyError),
    /// The approval exists but was made for a different action, target or parameters.
    Mismatch,
    /// The approval is still waiting for the owner.
    StillPending,
    Denied,
    BadPayload(String),
    /// The caller is the one that asked: a request never approves itself.
    OwnRequest,
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str(GONE_MESSAGE),
            Self::NotPending(status) => {
                write!(
                    f,
                    "This request was already answered ({}).",
                    status.as_str()
                )
            }
            Self::Expired(message) => f.write_str(message),
            Self::Replaced(_) => f.write_str(
                "A newer request for the same thing replaced this one. Approve that one instead.",
            ),
            Self::Key(error) => error.fmt(f),
            Self::Mismatch => f.write_str("This approval was given for a different change."),
            Self::StillPending => f.write_str("This request is still waiting for your approval."),
            Self::Denied => f.write_str("This request was denied."),
            Self::BadPayload(message) => f.write_str(message),
            Self::OwnRequest => {
                f.write_str("A request can't be approved by the one that asked for it.")
            }
        }
    }
}

/*
CDXC:Security 2026-11-10 WHY:
Gregg's report (gregg-approval-bug): waiting requests lived only in gxserver's memory, so after any restart, or once a newer request for the same thing was made, the card on the computer or phone held an id gxserver no longer had. Approve answered 404, the app said "already answered or ran out of time", and the fresh request sat unseen. The user (task 1 of 3): waiting requests are stored and come back after a restart with the same id, words and expiry; the same asker asking for the same thing gets the open request back; an id that was replaced points at the one that replaced it, and one that truly expired says so plainly. The pairing and Collab parts that finish an approved request keep what they need with it (`resume`), so approving a card shown before a restart still pairs the phone or sends the guest's message.
*/
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRequest {
    request: ApprovalRequest,
    /// Who asked; only they (or the owner's devices) may read the request back.
    requester: Principal,
    /// [`ActionRequest::shared`]: any caller that can ask may read and spend it.
    #[serde(default)]
    shared: bool,
    /// [`ActionRequest::replace_key`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replace_key: Option<String>,
    /// What the part of gxserver that asked keeps to finish the request once approved
    /// ([`ApprovalStore::attach`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resume: Option<Resume>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resume {
    kind: String,
    data: Value,
}

/// `gxserver/auth/approval-requests.json`.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedRequests {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    requests: Vec<StoredRequest>,
}

/// The file the requests are saved in, beside `security.json`.
pub const APPROVAL_REQUESTS_FILE_NAME: &str = "approval-requests.json";

/// What [`ApprovalStore::create`] did.
pub struct Created {
    pub request: ApprovalRequest,
    /// `false`: the asker's identical request was still waiting and is given back.
    pub is_new: bool,
    /// The asker's older requests for the same thing this one replaced (now `expired`).
    pub replaced: Vec<ApprovalRequest>,
}

impl StoredRequest {
    fn open_to(&self, caller: &Principal) -> bool {
        caller.is_owner_device()
            || self.requester == *caller
            || (self.shared && caller.has(super::Capability::RequestSensitiveAction))
    }
}

/// Saved on every change when it has a file ([`ApprovalStore::load`]); tests keep it in memory.
#[derive(Clone, Default)]
pub struct ApprovalStore {
    requests: Arc<Mutex<HashMap<String, StoredRequest>>>,
    path: Option<Arc<PathBuf>>,
}

impl ApprovalStore {
    /// The requests saved at `path`, lapsed ones marked expired. A file that can't be read is
    /// moved aside and the store starts empty: the askers ask again, as before requests were saved.
    pub fn load(path: PathBuf, now_ms: u64) -> Self {
        let saved = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str::<SavedRequests>(&text).unwrap_or_else(|error| {
                let aside = path.with_extension(format!("json.unreadable-{now_ms}"));
                let _ = std::fs::rename(&path, &aside);
                eprintln!(
                    "[security] {} couldn't be read ({error}); moved to {}",
                    path.display(),
                    aside.display()
                );
                SavedRequests::default()
            }),
            Err(_) => SavedRequests::default(),
        };
        let store = Self {
            requests: Arc::new(Mutex::new(
                saved
                    .requests
                    .into_iter()
                    .map(|stored| (stored.request.id.clone(), stored))
                    .collect(),
            )),
            path: Some(Arc::new(path)),
        };
        {
            let mut requests = store.lock();
            sweep(&mut requests, now_ms);
            store.save(&requests);
        }
        store
    }

    /// Writes every request (no-op without a file). Called with the lock held, so writes never
    /// interleave.
    fn save(&self, requests: &HashMap<String, StoredRequest>) {
        let Some(path) = &self.path else {
            return;
        };
        let mut saved = SavedRequests {
            version: 1,
            requests: requests.values().cloned().collect(),
        };
        saved
            .requests
            .sort_by_key(|stored| stored.request.created_at_ms);
        let written = serde_json::to_vec_pretty(&saved)
            .map_err(std::io::Error::other)
            .and_then(|bytes| super::persist::write_file_private(path, &bytes));
        if let Err(error) = written {
            eprintln!("[security] approval requests not saved: {error}");
        }
    }

    /// A new pending request, or the identical one this caller already has pending; a new one
    /// replaces the caller's older requests for the same thing ([`ActionRequest::replace_key`]).
    pub fn create(
        &self,
        requester: &Principal,
        requested_by: ApprovalRequester,
        action: &ActionRequest,
        now_ms: u64,
    ) -> Created {
        let digest = action.params_digest();
        let replace_key = action.replace_key();
        let mut requests = self.lock();
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        let same_asker = |stored: &StoredRequest| {
            stored.request.status == ApprovalStatus::Pending
                && (stored.requester == *requester || (action.shared && stored.shared))
        };
        if let Some(existing) = requests.values().find(|stored| {
            same_asker(stored)
                && stored.request.action == action.action
                && stored.request.target == action.target
                && stored.request.params_digest == digest
        }) {
            return Created {
                request: existing.request.clone(),
                is_new: false,
                replaced: Vec::new(),
            };
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
            replaced_by: None,
        };
        let mut replaced = Vec::new();
        if replace_key.is_some() {
            for stored in requests.values_mut() {
                if same_asker(stored) && stored.replace_key == replace_key {
                    stored.request.status = ApprovalStatus::Expired;
                    stored.request.decided_at_ms = Some(now_ms);
                    stored.request.replaced_by = Some(request.id.clone());
                    stored.resume = None;
                    replaced.push(stored.request.clone());
                }
            }
        }
        requests.insert(
            request.id.clone(),
            StoredRequest {
                request: request.clone(),
                requester: requester.clone(),
                shared: action.shared,
                replace_key,
                resume: None,
            },
        );
        self.save(&requests);
        Created {
            request,
            is_new: true,
            replaced,
        }
    }

    /// Keeps `data` with request `id` for the part of gxserver that asked (`kind`), so it can
    /// finish the request once approved, after a restart too ([`ApprovalStore::resumable`]).
    pub fn attach(&self, id: &str, kind: &str, data: Value) {
        let mut requests = self.lock();
        if let Some(stored) = requests.get_mut(id) {
            stored.resume = Some(Resume {
                kind: kind.to_string(),
                data,
            });
            self.save(&requests);
        }
    }

    /// The requests still waiting or approved (not yet spent) that keep data of `kind`, with it.
    pub fn resumable(&self, kind: &str, now_ms: u64) -> Vec<(ApprovalRequest, Value)> {
        let mut requests = self.lock();
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        requests
            .values()
            .filter(|stored| {
                matches!(
                    stored.request.status,
                    ApprovalStatus::Pending | ApprovalStatus::Approved
                )
            })
            .filter_map(|stored| {
                let resume = stored
                    .resume
                    .as_ref()
                    .filter(|resume| resume.kind == kind)?;
                Some((stored.request.clone(), resume.data.clone()))
            })
            .collect()
    }

    /// The newest request that replaced `request` (following a chain of replacements), for
    /// `caller`; `None` when it wasn't replaced.
    pub fn replacement(
        &self,
        request: &ApprovalRequest,
        caller: &Principal,
        now_ms: u64,
    ) -> Option<ApprovalRequest> {
        let mut next = request.replaced_by.clone()?;
        // A chain is short; the bound only guards against a loop in an edited file.
        for _ in 0..32 {
            let found = self.read(&next, caller, now_ms)?;
            match found.replaced_by.clone() {
                Some(newer) => next = newer,
                None => return Some(found),
            }
        }
        None
    }

    /// Every pending request, oldest first.
    pub fn list_pending(&self, now_ms: u64) -> Vec<ApprovalRequest> {
        let mut requests = self.lock();
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
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
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        requests
            .get(id)
            .filter(|stored| stored.open_to(caller))
            .map(|stored| stored.request.clone())
    }

    /// Approves request `id` on a click of `by` (one of your devices, checked by the route): no
    /// signature (CDXC:Security 2026-11-10 in security/policy.rs). A request never approves itself.
    pub fn approve_plain(
        &self,
        id: &str,
        by: &Principal,
        now_ms: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        let mut requests = self.lock();
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        let stored = requests.get_mut(id).ok_or(ApprovalError::NotFound)?;
        match &stored.request.status {
            ApprovalStatus::Pending => {}
            ApprovalStatus::Expired => return Err(lapsed(&stored.request)),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        if stored.requester == *by {
            return Err(ApprovalError::OwnRequest);
        }
        stored.request.status = ApprovalStatus::Approved;
        stored.request.decided_at_ms = Some(now_ms);
        stored.request.decided_by = Some(match by {
            Principal::Owner { device_id } | Principal::Device { device_id } => device_id.clone(),
            other => other.kind().to_string(),
        });
        stored.request.expires_at_ms = now_ms + APPROVED_TTL_MS;
        let approved = stored.request.clone();
        self.save(&requests);
        Ok(approved)
    }

    /// Denying needs no signature: saying no only closes a door.
    pub fn deny(
        &self,
        id: &str,
        by: &Principal,
        now_ms: u64,
    ) -> Result<ApprovalRequest, ApprovalError> {
        let mut requests = self.lock();
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        let stored = requests.get_mut(id).ok_or(ApprovalError::NotFound)?;
        match &stored.request.status {
            ApprovalStatus::Pending | ApprovalStatus::Approved => {}
            ApprovalStatus::Expired => return Err(lapsed(&stored.request)),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        stored.request.status = ApprovalStatus::Denied;
        stored.request.decided_at_ms = Some(now_ms);
        stored.request.decided_by = Some(match by {
            Principal::Owner { device_id } | Principal::Device { device_id } => device_id.clone(),
            other => other.kind().to_string(),
        });
        stored.resume = None;
        let denied = stored.request.clone();
        self.save(&requests);
        Ok(denied)
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
        if sweep(&mut requests, now_ms).1 {
            self.save(&requests);
        }
        let stored = requests
            .get_mut(id)
            .filter(|stored| stored.open_to(caller))
            .ok_or(ApprovalError::NotFound)?;
        let lapsed_error = lapsed(&stored.request);
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
            ApprovalStatus::Expired => return Err(lapsed_error),
            other => return Err(ApprovalError::NotPending(other.clone())),
        }
        request.status = ApprovalStatus::Consumed;
        let consumed = request.clone();
        stored.resume = None;
        self.save(&requests);
        Ok(consumed)
    }

    /// Marks lapsed requests expired and returns them (for [`super::SecurityEvent::ApprovalExpired`]).
    pub fn expire(&self, now_ms: u64) -> Vec<ApprovalRequest> {
        let mut requests = self.lock();
        let (expired, changed) = sweep(&mut requests, now_ms);
        if changed {
            self.save(&requests);
        }
        expired
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, StoredRequest>> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Why a request that is `expired` can't be used: replaced by a newer one, or lapsed.
fn lapsed(request: &ApprovalRequest) -> ApprovalError {
    match &request.replaced_by {
        Some(newer) => ApprovalError::Replaced(newer.clone()),
        None => ApprovalError::Expired(expired_message(request)),
    }
}

/// Marks lapsed requests expired and drops decided ones past their retention: the newly expired,
/// and whether anything changed (so the file is written only then).
fn sweep(
    requests: &mut HashMap<String, StoredRequest>,
    now_ms: u64,
) -> (Vec<ApprovalRequest>, bool) {
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
            stored.resume = None;
            expired.push(request.clone());
        }
    }
    let before = requests.len();
    requests.retain(|_, stored| {
        let request = &stored.request;
        let decided_at = request.decided_at_ms.unwrap_or(request.expires_at_ms);
        matches!(
            request.status,
            ApprovalStatus::Pending | ApprovalStatus::Approved
        ) || now_ms < decided_at + DECIDED_RETENTION_MS
    });
    let changed = !expired.is_empty() || requests.len() != before;
    (expired, changed)
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
