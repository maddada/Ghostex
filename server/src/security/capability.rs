//! Who is calling and what they may do: the principal behind a token, its capabilities, and the
//! access each route declares.

use serde::Serialize;

/// The role a session token was minted for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    /// An ordinary agent session.
    Agent,
    /// A coordinator: may also manage its own threads and answer their prompts.
    Coordinator,
    /// A thread started by a coordinator; same powers as an agent.
    Thread,
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
    /// Change the other settings that don't weaken security.
    OwnerSettings,
    /// Answer an approval prompt of its own threads.
    AnswerOwnThreadsPrompts,
    /// Answer any session's approval prompt without asking.
    AnswerAnyPrompt,
    /// Ask for a sensitive action (the call becomes an approval request).
    RequestSensitiveAction,
    /// See the pending approval requests, deny them, and post signed approvals.
    DecideApprovals,
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

    /// The owner's own devices: the desktop app and paired phones.
    pub fn is_owner_device(&self) -> bool {
        matches!(self, Self::Owner { .. } | Self::Device { .. })
    }

    pub fn has(&self, capability: Capability) -> bool {
        use Capability::*;
        match self {
            Self::Owner { .. } | Self::Device { .. } => true,
            Self::Session { role, .. } => match capability {
                OwnSession | ProjectSessions | AgentWritableSettings | RequestSensitiveAction => {
                    true
                }
                ManageOwnThreads | AnswerOwnThreadsPrompts => *role == SessionRole::Coordinator,
                ManageAnyThreads | OwnerSettings | AnswerAnyPrompt | DecideApprovals => false,
            },
            // The design's "CLI in your own terminal" column: thread management stays, the rest
            // is an agent's.
            Self::LocalAgent => matches!(
                capability,
                OwnSession
                    | ProjectSessions
                    | ManageOwnThreads
                    | ManageAnyThreads
                    | AgentWritableSettings
                    | RequestSensitiveAction
            ),
            // Peers, guests and extensions keep their own endpoint allow-lists
            // (`connected_computers/scope.rs`, `collab::shares::GUEST_ENDPOINTS`, the extension
            // registry); none of them may ask for a sensitive action.
            Self::Peer { .. } | Self::Guest { .. } | Self::Extension { .. } | Self::Anonymous => {
                false
            }
        }
    }

    /// Whether this caller may manage the thread whose coordinator is `coordinator_session_id`.
    pub fn may_manage_thread_of(&self, coordinator_session_id: &str) -> bool {
        if self.has(Capability::ManageAnyThreads) {
            return true;
        }
        match self {
            Self::Session {
                session_id, role, ..
            } => *role == SessionRole::Coordinator && session_id == coordinator_session_id,
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
    use ghostex_gx_protocol::approval::SensitiveAction as A;
    match path {
        "/api/health" | "/api/pairDevice" => RouteAccess::Public,
        super::APPROVALS_LIST_ENDPOINT
        | super::APPROVALS_APPROVE_ENDPOINT
        | super::APPROVALS_DENY_ENDPOINT
        | super::APPROVER_KEYS_LIST_ENDPOINT
        | super::APPROVER_KEYS_REMOVE_ENDPOINT => RouteAccess::Needs(Capability::DecideApprovals),
        // The asking caller polls its own request; the handler checks it is the requester.
        super::APPROVALS_READ_ENDPOINT => RouteAccess::Needs(Capability::RequestSensitiveAction),
        super::APPROVER_KEYS_ENROLL_ENDPOINT => RouteAccess::Presence(A::AddApproverKey),
        "/api/registerConnectedComputer" => RouteAccess::Presence(A::ComputerRegister),
        "/api/issueComputerGrant" => RouteAccess::Presence(A::ComputerGrant),
        _ => RouteAccess::Needs(Capability::ProjectSessions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(role: SessionRole) -> Principal {
        Principal::Session {
            project_id: "p".into(),
            session_id: "s1".into(),
            role,
        }
    }

    #[test]
    fn agents_ask_but_never_decide() {
        for caller in [
            session(SessionRole::Agent),
            session(SessionRole::Coordinator),
            Principal::LocalAgent,
        ] {
            assert!(caller.has(Capability::RequestSensitiveAction));
            assert!(!caller.has(Capability::DecideApprovals));
            assert!(!caller.has(Capability::AnswerAnyPrompt));
        }
        assert!(Principal::Owner {
            device_id: "d".into()
        }
        .has(Capability::DecideApprovals));
    }

    #[test]
    fn coordinators_manage_only_their_own_threads() {
        let coordinator = session(SessionRole::Coordinator);
        assert!(coordinator.may_manage_thread_of("s1"));
        assert!(!coordinator.may_manage_thread_of("s2"));
        assert!(!session(SessionRole::Agent).may_manage_thread_of("s1"));
        assert!(Principal::LocalAgent.may_manage_thread_of("s2"));
    }

    #[test]
    fn outsiders_cannot_ask_for_sensitive_actions() {
        for caller in [
            Principal::Guest {
                share_id: "x".into(),
            },
            Principal::Peer {
                server_id: "S1".into(),
                name: "Laptop".into(),
            },
            Principal::Extension {
                extension_id: "e".into(),
            },
        ] {
            assert!(!caller.has(Capability::RequestSensitiveAction));
        }
    }
}
