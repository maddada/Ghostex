//! Who may do what on this gxserver (GitHub #495, the 11.0.1 security release).
//!
//! - [`Principal`] is the caller behind a token, settled once at the auth gate; [`Capability`]
//!   is what it may do; [`route_access`] is what each route needs.
//! - [`SecurityState::require_owner_presence`] is the gate for every [`SensitiveAction`]: the
//!   call becomes an [`ApprovalRequest`] until the owner approves it with a signature from a
//!   biometric- or PIN-locked approver key ([`ApproverKeys`]), and the approval is spent on that
//!   one change.
//! - [`SessionTokens`] are the per-session `gxs_` tokens and the desktop's `gxo_` owner sessions.
//! - [`SecurityEvent`] is the stream the audit log and the approval UIs subscribe to.
//!
//! The contract other threads build on (pairing, Collab, the audit log) is written out in
//! `ai/security-tokens-and-approvals.md`; the wire types and the signed payload are in
//! `ghostex_gx_protocol::approval`.

mod approvals;
mod approver_keys;
mod capability;
mod events;
mod presence;
mod session_tokens;

pub use approvals::{
    params_digest, ActionRequest, ApprovalError, ApprovalStore, APPROVED_TTL_MS, PENDING_TTL_MS,
};
#[cfg(test)]
pub(crate) use approver_keys::test_keys;
pub use approver_keys::{
    approver_key_id, verify_approval_signature, ApproverKeyError, ApproverKeys,
};
pub use capability::{route_access, Capability, Principal, RouteAccess, SessionRole};
pub use events::SecurityEvent;
pub use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};
pub use presence::{requester_for, PresenceGrant, PresenceOutcome};
pub use session_tokens::{
    SessionTokens, OWNER_TOKEN_PREFIX, SESSION_TOKEN_ENV, SESSION_TOKEN_PREFIX,
};

use std::sync::Arc;

use tokio::sync::broadcast;

/// Pending requests, oldest first (owner's devices).
pub const APPROVALS_LIST_ENDPOINT: &str = "/api/listApprovalRequests";
/// One request, for its requester (to learn the answer) or the owner's devices.
pub const APPROVALS_READ_ENDPOINT: &str = "/api/readApprovalRequest";
/// Approve with `{ id, keyId, signature }` (owner's devices).
pub const APPROVALS_APPROVE_ENDPOINT: &str = "/api/approveApprovalRequest";
/// Deny with `{ id }` (owner's devices).
pub const APPROVALS_DENY_ENDPOINT: &str = "/api/denyApprovalRequest";
/// The registered approver keys (owner's devices).
pub const APPROVER_KEYS_LIST_ENDPOINT: &str = "/api/listApproverKeys";
/// Add an approver key; itself a sensitive action after the first key (Phase 2).
pub const APPROVER_KEYS_ENROLL_ENDPOINT: &str = "/api/enrollApproverKey";
/// Remove an approver key with `{ keyId }` (owner's devices).
pub const APPROVER_KEYS_REMOVE_ENDPOINT: &str = "/api/removeApproverKey";

pub const SECURITY_ENDPOINTS: &[&str] = &[
    APPROVALS_LIST_ENDPOINT,
    APPROVALS_READ_ENDPOINT,
    APPROVALS_APPROVE_ENDPOINT,
    APPROVALS_DENY_ENDPOINT,
    APPROVER_KEYS_LIST_ENDPOINT,
    APPROVER_KEYS_ENROLL_ENDPOINT,
    APPROVER_KEYS_REMOVE_ENDPOINT,
];

/// Everything security-related one gxserver holds; cheap to clone (shared inside).
#[derive(Clone)]
pub struct SecurityState {
    /// This gxserver's id; part of every signed payload, so an approval for one computer can't
    /// be replayed on another.
    pub server_id: Arc<str>,
    pub approvals: ApprovalStore,
    pub approver_keys: ApproverKeys,
    pub session_tokens: SessionTokens,
    events: broadcast::Sender<SecurityEvent>,
}

impl SecurityState {
    pub fn new(server_id: &str) -> Self {
        Self {
            server_id: Arc::from(server_id),
            approvals: ApprovalStore::default(),
            approver_keys: ApproverKeys::default(),
            session_tokens: SessionTokens::default(),
            events: broadcast::channel(256).0,
        }
    }

    /// Every security event from now on. A slow subscriber loses the oldest events
    /// (`RecvError::Lagged`), never blocks gxserver.
    pub fn subscribe(&self) -> broadcast::Receiver<SecurityEvent> {
        self.events.subscribe()
    }

    pub fn publish(&self, event: SecurityEvent) {
        let _ = self.events.send(event);
    }

    /// A handler calls this after it ran a sensitive action (or an always-allowed change the
    /// log should show, such as turning Easy Connect off).
    pub fn record_performed(
        &self,
        action: SensitiveAction,
        target: Option<String>,
        caller: &Principal,
        ok: bool,
    ) {
        self.publish(SecurityEvent::SensitiveActionPerformed {
            action,
            target,
            caller: caller.clone(),
            ok,
        });
    }

    /// Marks lapsed requests expired and publishes them; run from a periodic sweep.
    pub fn expire_lapsed(&self, now_ms: u64) {
        for request in self.approvals.expire(now_ms) {
            self.publish(SecurityEvent::ApprovalExpired { request });
        }
    }
}

/// Milliseconds since the Unix epoch, the clock every approval timestamp uses.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
