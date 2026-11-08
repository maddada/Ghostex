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
mod owner;
pub mod persist;
mod presence;
mod session_tokens;

pub use approvals::{
    params_digest, ActionRequest, ApprovalError, ApprovalStore, APPROVED_TTL_MS, PENDING_TTL_MS,
};
pub use approver_keys::{
    approver_key_id, key_id_of, verify_approval_signature, ApproverKeyError, ApproverKeys,
};
pub use capability::{route_access, Capability, Principal, RouteAccess, SessionRole};
pub use events::SecurityEvent;
pub use ghostex_gx_protocol::approval::{ApprovalRequest, SensitiveAction};
pub use owner::{OwnerChallenges, OwnerError, OWNER_CHALLENGE_TTL_MS};
pub use presence::{requester_for, PresenceGrant, PresenceOutcome};
pub use session_tokens::{
    SessionTokens, OWNER_TOKEN_PREFIX, SESSION_TOKEN_ENV, SESSION_TOKEN_PREFIX,
};

use std::{path::PathBuf, sync::Arc};

use ghostex_gx_protocol::approval::ApproverKey;
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
/// Add an approver key `{ key }` (owner's devices); the first one directly, every later one
/// through an approved `addApproverKey`.
pub const APPROVER_KEYS_ENROLL_ENDPOINT: &str = "/api/enrollApproverKey";
/// Remove an approver key with `{ keyId }` (owner's devices).
pub const APPROVER_KEYS_REMOVE_ENDPOINT: &str = "/api/removeApproverKey";
/// Enroll the desktop's owner key `{ key }`: the first one from any local caller (trust on
/// first use), the same one again is a no-op, another one through an approved `addApproverKey`.
pub const OWNER_KEY_ENROLL_ENDPOINT: &str = "/api/enrollOwnerKey";
/// A one-time challenge for the owner key (any local caller).
pub const OWNER_CHALLENGE_ENDPOINT: &str = "/api/ownerChallenge";
/// `{ keyId, challenge, signature }` → `{ token, deviceId }`: the desktop's owner session.
pub const OWNER_SESSION_ENDPOINT: &str = "/api/openOwnerSession";
/// Whether an owner key and approver keys are enrolled, and who the caller is (any caller).
pub const SECURITY_STATUS_ENDPOINT: &str = "/api/securityStatus";

pub const SECURITY_ENDPOINTS: &[&str] = &[
    APPROVALS_LIST_ENDPOINT,
    APPROVALS_READ_ENDPOINT,
    APPROVALS_APPROVE_ENDPOINT,
    APPROVALS_DENY_ENDPOINT,
    APPROVER_KEYS_LIST_ENDPOINT,
    APPROVER_KEYS_ENROLL_ENDPOINT,
    APPROVER_KEYS_REMOVE_ENDPOINT,
    OWNER_KEY_ENROLL_ENDPOINT,
    OWNER_CHALLENGE_ENDPOINT,
    OWNER_SESSION_ENDPOINT,
    SECURITY_STATUS_ENDPOINT,
];

/// Everything security-related one gxserver holds; cheap to clone (shared inside).
#[derive(Clone)]
pub struct SecurityState {
    /// This gxserver's id; part of every signed payload, so an approval for one computer can't
    /// be replayed on another.
    pub server_id: Arc<str>,
    pub approvals: ApprovalStore,
    pub approver_keys: ApproverKeys,
    /// The desktop app's owner keys (normally one per computer).
    pub owner_keys: ApproverKeys,
    pub owner_challenges: OwnerChallenges,
    pub session_tokens: SessionTokens,
    /// Connected computers the owner approved (server id → when, ms); see
    /// [`SecurityState::computer_approved`].
    approved_computers: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u64>>>,
    /// Projects the owner let another project's agents reach (caller's project, project reached),
    /// saved in security.json: see [`SecurityState::project_reach_approved`].
    approved_project_reach: Arc<std::sync::Mutex<std::collections::BTreeSet<(String, String)>>>,
    /// `gxserver/auth/security.json`; `None` in tests that need no disk.
    persist_path: Option<Arc<PathBuf>>,
    save_lock: Arc<std::sync::Mutex<()>>,
    events: broadcast::Sender<SecurityEvent>,
}

impl SecurityState {
    /// An empty state that is never saved (tests).
    pub fn new(server_id: &str) -> Self {
        Self {
            server_id: Arc::from(server_id),
            approvals: ApprovalStore::default(),
            approver_keys: ApproverKeys::default(),
            owner_keys: ApproverKeys::default(),
            owner_challenges: OwnerChallenges::default(),
            session_tokens: SessionTokens::default(),
            approved_computers: Arc::default(),
            approved_project_reach: Arc::default(),
            persist_path: None,
            save_lock: Arc::default(),
            events: broadcast::channel(256).0,
        }
    }

    /// The state saved in `auth_dir`, saved there again on every change.
    pub fn load(auth_dir: &std::path::Path, server_id: &str) -> std::io::Result<Self> {
        let path = persist::security_file(auth_dir);
        let saved = persist::load(&path)?;
        let mut state = Self::new(server_id);
        state.owner_keys.restore(saved.owner_keys);
        state.approver_keys.restore(saved.approver_keys);
        state.session_tokens.restore(saved.session_tokens);
        state.session_tokens.restore_owner(saved.owner_sessions);
        *state
            .approved_computers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = saved.approved_computers;
        *state
            .approved_project_reach
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = saved
            .approved_project_reach
            .into_iter()
            .map(|reach| (pair_caller(reach.caller), reach.target))
            .collect();
        state.persist_path = Some(Arc::new(path));
        Ok(state)
    }

    /// Writes the keys and session token hashes to disk (no-op without a path).
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.persist_path else {
            return Ok(());
        };
        let _guard = self
            .save_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        persist::save(
            path,
            &persist::PersistedSecurity {
                version: 1,
                owner_keys: self.owner_keys.list(),
                approver_keys: self.approver_keys.list(),
                session_tokens: self.session_tokens.snapshot(),
                owner_sessions: self.session_tokens.owner_snapshot(),
                approved_computers: self
                    .approved_computers
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
                approved_project_reach: self
                    .approved_project_reach
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .iter()
                    .map(|(caller, target)| persist::PersistedProjectReach {
                        caller: caller.clone(),
                        target: target.clone(),
                    })
                    .collect(),
            },
        )
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

    /// The token for a session that is starting: the one it already holds, or a new one.
    pub fn issue_session_token(
        &self,
        project_id: &str,
        session_id: &str,
        role: SessionRole,
    ) -> std::io::Result<String> {
        let (token, new) = self
            .session_tokens
            .issue_session(project_id, session_id, role);
        self.save()?;
        if new {
            self.publish(SecurityEvent::SessionTokenMinted {
                principal: Principal::Session {
                    project_id: project_id.to_string(),
                    session_id: session_id.to_string(),
                    role,
                },
            });
        }
        Ok(token)
    }

    /// Revokes a session's token when the session ends.
    pub fn revoke_session_token(&self, project_id: &str, session_id: &str) {
        let removed = self.session_tokens.revoke_session(project_id, session_id);
        if removed.is_empty() {
            return;
        }
        let _ = self.save();
        for principal in removed {
            self.publish(SecurityEvent::SessionTokenRevoked { principal });
        }
    }

    /*
    CDXC:Security 2026-11-08 WHY:
    The desktop renews a connected computer's registration and grant every few minutes (connected_computers/registry.rs keeps them 3 minutes), so asking for an approval on each renewal would prompt all day. The owner approves a computer once; renewals for it pass for 30 days (the design's longest grant), and a new computer, or one whose approval lapsed, asks again. Registering a computer, or renewing it, with Agent access off never asks (`security_gates.rs`; until 11.1.1 only the grant route skipped it, so turning Agent access off still asked on every registration).
    */
    /// Whether the owner approved computer `server_id` within the last 30 days.
    pub fn computer_approved(&self, server_id: &str, now_ms: u64) -> bool {
        self.approved_computers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(server_id)
            .is_some_and(|approved| now_ms.saturating_sub(*approved) < COMPUTER_APPROVAL_MS)
    }

    /// Remembers that the owner approved computer `server_id` now.
    pub fn approve_computer(&self, server_id: &str, now_ms: u64) {
        self.approved_computers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(server_id.to_string(), now_ms);
        let _ = self.save();
    }

    /*
    CDXC:Security 2026-11-08 DECISION:
    User (#495 follow-up, Q1863 "ask once per project"): an agent's first message to, or new session in, a project other than its own asks the owner.
    CDXC:Security 2026-11-08 DECISION:
    User, on the in-memory first version: "make these approvals not save in memory pls so u dont ask me to approve again later". They are saved in security.json with the other security state and loaded at start, so a gxserver restart asks nothing again.
    CDXC:Security 2026-11-08 DECISION:
    User, after 11.1.2 asked once per agent session: one approval lets every agent in a project reach the other project (ask once per project pair). The approval is kept under the caller's project (its root, when it is a worktree registered as its own project) and the project reached, and goes when either project is removed (`/api/removeProject`, or the security sweep, `tidy_project_reach`), not when an agent closes. Entries saved per agent session by 11.1.2 (`<project>:<session>`) carry over as their project pair.
    */
    /// Whether the owner already let agents of project `caller_key` (the caller's root project, or
    /// `localAgent`) reach project `project_id`.
    pub fn project_reach_approved(&self, caller_key: &str, project_id: &str) -> bool {
        self.approved_project_reach
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&(caller_key.to_string(), project_id.to_string()))
    }

    /// Remembers that the owner let agents of `caller_key` reach project `project_id`.
    pub fn approve_project_reach(&self, caller_key: &str, project_id: &str) {
        self.approved_project_reach
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert((caller_key.to_string(), project_id.to_string()));
        let _ = self.save();
    }

    /// Drops every remembered reach from or to project `project_id` (it was removed).
    pub fn forget_project_reach_of(&self, project_id: &str) {
        let forgot = {
            let mut approved = self
                .approved_project_reach
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let before = approved.len();
            approved.retain(|(caller, target)| caller != project_id && target != project_id);
            approved.len() != before
        };
        if forgot {
            let _ = self.save();
        }
    }

    /// Adds an approver key: the first one directly (it is the owner's call, checked by the
    /// route), later ones only after the route spent an `addApproverKey` approval.
    pub fn add_approver_key(
        &self,
        mut key: ApproverKey,
        now_ms: u64,
    ) -> Result<ApproverKey, OwnerError> {
        key.added_at_ms = now_ms;
        let key = self.approver_keys.insert(key).map_err(OwnerError::Key)?;
        self.save()
            .map_err(|error| OwnerError::Save(error.to_string()))?;
        self.publish(SecurityEvent::ApproverKeyAdded { key: key.clone() });
        Ok(key)
    }
}

/// How long an approved connected computer's renewals pass without asking again.
pub const COMPUTER_APPROVAL_MS: u64 = 30 * 24 * 60 * 60 * 1000;

static INSTALLED: std::sync::OnceLock<SecurityState> = std::sync::OnceLock::new();

/// Makes `state` the one session launches read their tokens from (gxserver start; the first
/// call wins). Session launch code runs deep in the zmx and wmx providers, where `AppState`
/// isn't passed, like the Collab share and connected-computer registries.
pub fn install(state: &SecurityState) {
    let _ = INSTALLED.set(state.clone());
}

/// The token a session starting now exports as [`SESSION_TOKEN_ENV`]; `None` before
/// [`install`] (tests) or when it can't be saved (the session then runs with the token file's
/// agent level only). `coordinator_environment` is the session's coordinator variables
/// (`coordinators::coordinator_launch_environment`), which give its role.
pub fn session_token_for_launch(
    project_id: &str,
    session_id: &str,
    coordinator_environment: &[(&'static str, String)],
) -> Option<String> {
    let state = INSTALLED.get()?;
    let role = coordinator_environment
        .iter()
        .find(|(key, _)| *key == crate::coordinators::COORDINATOR_ROLE_ENV)
        .map(|(_, value)| match value.as_str() {
            "coordinator" => SessionRole::Coordinator,
            "thread" => SessionRole::Thread,
            _ => SessionRole::Agent,
        })
        .unwrap_or(SessionRole::Agent);
    state.issue_session_token(project_id, session_id, role).ok()
}

/// Revokes the tokens of sessions that no longer exist (`live` holds every session's
/// `(project id, session id)`); catches deletions that don't go through `/api/removeSession`,
/// such as a project removed with its sessions.
pub fn revoke_tokens_of_deleted_sessions(
    state: &SecurityState,
    live: &std::collections::HashSet<(String, String)>,
) {
    for (project_id, session_id) in state.session_tokens.sessions() {
        if !live.contains(&(project_id.clone(), session_id.clone())) {
            state.revoke_session_token(&project_id, &session_id);
        }
    }
}

/// Tidies the remembered reach approvals against the projects that exist (`roots`: every
/// project's id → its root project, itself unless it is a worktree registered as its own
/// project): a pair whose caller project or project reached is gone goes, and a caller saved
/// under a worktree project moves to its root. A folder reached (`folder:<path>`) stays while its
/// caller project does.
pub fn tidy_project_reach(
    state: &SecurityState,
    roots: &std::collections::HashMap<String, String>,
) {
    let changed = {
        let mut approved = state
            .approved_project_reach
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let tidied: std::collections::BTreeSet<(String, String)> = approved
            .iter()
            .filter_map(|(caller, target)| {
                let root = if caller == "localAgent" {
                    caller.clone()
                } else {
                    roots.get(caller)?.clone()
                };
                (target.starts_with("folder:") || roots.contains_key(target))
                    .then(|| (root, target.clone()))
            })
            .collect();
        let changed = tidied != *approved;
        *approved = tidied;
        changed
    };
    if changed {
        let _ = state.save();
    }
}

/// The caller project of a saved reach: 11.1.2 saved `<project>:<session>`, which carries over as
/// its project.
fn pair_caller(saved: String) -> String {
    match saved.split_once(':') {
        Some((project, _)) => project.to_string(),
        None => saved,
    }
}

/// Milliseconds since the Unix epoch, the clock every approval timestamp uses.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}
