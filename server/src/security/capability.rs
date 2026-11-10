//! Who is calling and what they may do: the principal behind a token, its capabilities, and the
//! access each route declares.

use serde::{Deserialize, Serialize};

/// The role a session token was minted for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    /// An ordinary agent session.
    Agent,
    /// A coordinator: may also manage its own threads.
    Coordinator,
    /// A thread started by a coordinator; same powers as an agent.
    Thread,
    /// Chief, the Ghostex-wide coordinator, while its extension is on. Never stored with a
    /// token: only `crate::chief::settle_principal` gives it, per request (see
    /// [`SessionRole::stored`]).
    Head,
}

impl SessionRole {
    /// The role a token may be stored with: `Head` is never stored, so a token minted or
    /// restored with it holds a coordinator's role, and only the auth gate's per-request check
    /// (`crate::chief::settle_principal`) can make a caller the head.
    pub fn stored(self) -> Self {
        match self {
            Self::Head => Self::Coordinator,
            role => role,
        }
    }
}

/// The caller behind a request's token, settled once at the auth gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Principal {
    /// The desktop app's owner session (`gxo_`), minted after it proved the owner key, held
    /// only in the app's memory.
    Owner { device_id: String },
    /// A paired phone (`gxd_` today; pairing v2 replaces it with signed requests).
    Device { device_id: String },
    /// One session's own token (`gxs_`), bound to it and revoked when it ends.
    Session {
        project_id: String,
        session_id: String,
        role: SessionRole,
    },
    /// The shared token file (`gxserver/auth/token`), downgraded to agent-level powers. The CLI
    /// in the user's own terminal uses it and can't be told apart from an agent there.
    LocalAgent,
    /// Another computer's agents, through the grant this computer gave it.
    Peer { server_id: String, name: String },
    /// A Collab guest, through a share token: only its own chat.
    Guest { share_id: String },
    /// An installed extension's own API token.
    Extension { extension_id: String },
    /// No token, on a public route (`/api/health`, the pairing handshake).
    Anonymous,
}

/// What a principal may do. Sensitive actions are not capabilities: they need a signed
/// approval whoever calls (see `presence.rs`), and only [`Capability::RequestSensitiveAction`]
/// decides who may ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// Read and write its own session, its chat, pages, `ghostex show`, visuals.
    OwnSession,
    /// Create sessions; send to and read other sessions of the project.
    ProjectSessions,
    /// Start, rename, promote and close the threads it coordinates.
    ManageOwnThreads,
    /// Do that for any coordinator's threads.
    ManageAnyThreads,
    /// Change the settings the catalog marks `agentWritable`.
    AgentWritableSettings,
    /// Ask for a sensitive action (the call becomes an approval request).
    RequestSensitiveAction,
    /// See the pending approval requests, approve them with a click, and deny them: the door's
    /// keepers, your own devices (CDXC:Security 2026-11-10 in security/policy.rs).
    DecideApprovals,
    /// Answer a secret request and have gxserver read a secret provider reference (1Password)
    /// for the caller. The owner's devices only: a secret is only ever given by the user.
    OwnerSecrets,
}

impl Capability {
    /// Every capability (tables and tests walk it).
    pub const KNOWN: &'static [Self] = &[
        Self::OwnSession,
        Self::ProjectSessions,
        Self::ManageOwnThreads,
        Self::ManageAnyThreads,
        Self::AgentWritableSettings,
        Self::RequestSensitiveAction,
        Self::DecideApprovals,
        Self::OwnerSecrets,
    ];
}

impl Principal {
    /// A short kind name for logs and [`ghostex_gx_protocol::approval::ApprovalRequester`].
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Owner { .. } => "owner",
            Self::Device { .. } => "device",
            Self::Session { .. } => "session",
            Self::LocalAgent => "localAgent",
            Self::Peer { .. } => "peer",
            Self::Guest { .. } => "guest",
            Self::Extension { .. } => "extension",
            Self::Anonymous => "anonymous",
        }
    }

    /// You: the desktop app (its owner session, or the token file it reads), the GPUI web build and
    /// the `ghostex` CLI in your own terminal (the token file), and your paired phones. These approve
    /// what still asks and answer secret requests (CDXC:Security 2026-11-10 in security/policy.rs).
    pub fn is_owner_device(&self) -> bool {
        matches!(
            self,
            Self::Owner { .. } | Self::Device { .. } | Self::LocalAgent
        )
    }

    /*
    CDXC:Security 2026-11-10 DECISION:
    User (Q2140 = B): "thinking of removing all these dumb blocks because they're adding friction and a lot of bugs … i dont know why we want to ban agents from doing everything with ghostex". You (the owner session, the token file, a paired phone) may do everything. An agent's session token may do everything too except the two things that keep the door: approving what lets someone new in (`DecideApprovals`) and answering or reading secrets (`OwnerSecrets`). Coordinators still manage only their own threads, which is how threads are organised, not a security rule. This supersedes the 8 Nov table that kept setup, owner settings and approvals from the token file and from agents.
    */
    pub fn has(&self, capability: Capability) -> bool {
        use Capability::*;
        match self {
            Self::Owner { .. } | Self::Device { .. } | Self::LocalAgent => true,
            Self::Session { role, .. } => match capability {
                ManageOwnThreads => matches!(role, SessionRole::Coordinator | SessionRole::Head),
                ManageAnyThreads | DecideApprovals | OwnerSecrets => false,
                OwnSession | ProjectSessions | AgentWritableSettings | RequestSensitiveAction => {
                    true
                }
            },
            // Agents on a connected computer reach only `PEER_ALLOWED_ENDPOINTS`
            // (connected_computers/scope.rs), and may ask for what those need.
            Self::Peer { .. } => capability == RequestSensitiveAction,
            // Guests and extensions keep their own endpoint allow-lists
            // (`collab_shim::registry::GUEST_READ_ENDPOINTS`, the extension registry) and never ask.
            Self::Guest { .. } | Self::Extension { .. } | Self::Anonymous => false,
        }
    }

    /// Whether this caller is Chief with its head role (settled for this request).
    pub fn is_head(&self) -> bool {
        matches!(
            self,
            Self::Session {
                role: SessionRole::Head,
                ..
            }
        )
    }

    /// Whether this caller may manage the thread whose coordinator is `coordinator_session_id`.
    pub fn may_manage_thread_of(&self, coordinator_session_id: &str) -> bool {
        if self.has(Capability::ManageAnyThreads) {
            return true;
        }
        match self {
            Self::Session {
                session_id, role, ..
            } => {
                matches!(role, SessionRole::Coordinator | SessionRole::Head)
                    && session_id == coordinator_session_id
            }
            _ => false,
        }
    }
}

/// What a route needs from its caller, declared in [`route_access`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteAccess {
    /// No token at all (`/api/health`, the pairing handshake).
    Public,
    /// Any principal holding the capability.
    Needs(Capability),
    /// The whole call is one sensitive action: the gate asks for a signed approval with the
    /// call's body as the parameters, before any handler runs.
    Presence(ghostex_gx_protocol::approval::SensitiveAction),
}

/// The access every route declares.
///
/// Phase 1 of #495 lists the security routes explicitly and lets every other route default to
/// [`Capability::ProjectSessions`] (what any agent may do today). Phase 2 classifies every
/// endpoint here, with a test that fails when `protocol::endpoint_for` knows a path this table
/// doesn't.
///
/// A route that is only partly sensitive (for example `/api/autopilot` with `op: "set"` that
/// turns on `answerApprovals`) keeps an ordinary access here and calls
/// [`super::require_owner_presence`] from its handler for the sensitive part.
pub fn route_access(path: &str) -> RouteAccess {
    match path {
        "/api/health" | "/api/pairDevice" | "/api/pairingStatus" | "/api/renewDeviceToken" => {
            RouteAccess::Public
        }
        // A paired computer has no token here; its request is signed with its pairing device key.
        crate::remote_access::COMPUTER_EASY_CONNECT_KEYS_ENDPOINT => RouteAccess::Public,
        // The Collab component's calls: its handler takes only the host token gxserver gave it.
        "/api/collabHost" => RouteAccess::Public,
        // Only asks: the phone becomes Pending until the owner approves it with matching words.
        // The handler takes only the token file (a phone's SSH login), not a session's token.
        "/api/pairOverSsh" => RouteAccess::Needs(Capability::RequestSensitiveAction),
        super::APPROVALS_LIST_ENDPOINT
        | super::APPROVALS_APPROVE_ENDPOINT
        | super::APPROVALS_DENY_ENDPOINT
        | super::APPROVER_KEYS_LIST_ENDPOINT
        | super::APPROVER_KEYS_REMOVE_ENDPOINT => RouteAccess::Needs(Capability::DecideApprovals),
        // The asking caller polls its own request; the handler checks it is the requester.
        super::APPROVALS_READ_ENDPOINT => RouteAccess::Needs(Capability::RequestSensitiveAction),
        // The handler takes the first key on the owner's word and gates every later one
        // behind `addApproverKey`.
        super::APPROVER_KEYS_ENROLL_ENDPOINT => RouteAccess::Needs(Capability::DecideApprovals),
        // The desktop starts with only the token file: it enrolls its owner key (first one,
        // trust on first use) and opens its owner session from there.
        super::OWNER_KEY_ENROLL_ENDPOINT
        | super::OWNER_CHALLENGE_ENDPOINT
        | super::OWNER_SESSION_ENDPOINT => RouteAccess::Needs(Capability::RequestSensitiveAction),
        super::SECURITY_STATUS_ENDPOINT => RouteAccess::Needs(Capability::OwnSession),
        // Answering a secret request may read a 1Password reference, and checking a reference
        // reads it; `/api/setVoiceAgentKey` with a reference is refused in security_gates.rs.
        "/api/answerSecretRequest" | "/api/checkSecretReference" => {
            RouteAccess::Needs(Capability::OwnerSecrets)
        }
        _ => RouteAccess::Needs(Capability::ProjectSessions),
    }
}
