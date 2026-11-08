//! What one security-log entry says: the action, who asked, how it ended and the approval behind it.

use serde::{Deserialize, Serialize};

/// What was done (or asked for). `kind` is a stable dotted id; `summary` is the plain sentence
/// the user reads; `target` names the chat, device or computer it was about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub kind: String,
    pub summary: String,
    pub target: Option<String>,
}

impl Action {
    pub fn new(kind: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            summary: summary.into(),
            target: None,
        }
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        let target = target.into();
        let target = target.trim();
        self.target = (!target.is_empty()).then(|| target.to_string());
        self
    }
}

/// Who asked for the action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Actor {
    /// The owner, proven by a signed approval or the owner session (the approvals thread).
    Owner { device: Option<String> },
    /// A Ghostex session, known from its own token.
    Session { session_id: String },
    /// Chief, with its head role (a session's own token, settled by the auth gate).
    Chief { session_id: String },
    /// A caller holding this computer's shared token: the desktop app, the CLI or an agent;
    /// `claimed_session` is the session the CLI said it runs in, which nothing proves yet.
    SharedToken { claimed_session: Option<String> },
    /// A phone paired through Easy Connect, with its own device token.
    PairedDevice { id: Option<String>, name: String },
    /// Agents on a connected computer, with the grant this computer gave it.
    ConnectedComputer { server_id: String, name: String },
    /// An installed extension's own token.
    Extension { id: String },
    /// A Ghostex Collab guest.
    CollabGuest { name: String },
    /// Autopilot, deciding for an agent on its own.
    Autopilot,
    /// Settings sync, carrying a change made on another of the user's computers.
    SettingsSync { computer: String },
    /// gxserver acting on its own (a sweep, an expiry, startup).
    Ghostex,
}

impl Actor {
    /// Who made the request being served, or `Ghostex` outside a request.
    pub fn current() -> Self {
        super::caller::current_actor()
    }

    pub(crate) fn record(&self) -> ActorRecord {
        let (kind, label, id) = match self {
            Self::Owner { device } => (
                "owner",
                match device {
                    Some(device) => format!("You, on {device}"),
                    None => "You".to_string(),
                },
                None,
            ),
            Self::Session { session_id } => (
                "session",
                format!("Session {session_id}"),
                Some(session_id.clone()),
            ),
            Self::Chief { session_id } => (
                "session",
                "Chief of Staff".to_string(),
                Some(session_id.clone()),
            ),
            Self::SharedToken { claimed_session } => (
                "sharedToken",
                match claimed_session {
                    Some(session) => format!("An agent or the app (says session {session})"),
                    None => "This computer's app or an agent on it".to_string(),
                },
                claimed_session.clone(),
            ),
            Self::PairedDevice { id, name } => {
                ("pairedDevice", format!("Phone {name}"), id.clone())
            }
            Self::ConnectedComputer { server_id, name } => (
                "connectedComputer",
                format!("Agents on {name}"),
                Some(server_id.clone()),
            ),
            Self::Extension { id } => ("extension", format!("Extension {id}"), Some(id.clone())),
            Self::CollabGuest { name } => ("collabGuest", format!("A Collab guest ({name})"), None),
            Self::Autopilot => ("autopilot", "Autopilot".to_string(), None),
            Self::SettingsSync { computer } => (
                "settingsSync",
                format!("Settings sync from {computer}"),
                None,
            ),
            Self::Ghostex => ("ghostex", "Ghostex".to_string(), None),
        };
        ActorRecord {
            kind: kind.to_string(),
            label,
            id,
        }
    }
}

/// How the action ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It happened.
    Done,
    /// It waits for the owner's approval.
    Pending,
    /// The owner approved it (with `ApprovalRef`) and it happened.
    Approved,
    /// The owner said no.
    Denied,
    /// Ghostex refused it, with the reason.
    Refused(String),
    /// It was allowed but failed, with the error.
    Failed(String),
    /// Chief's head role let it through without an approval (typing into, interrupting or
    /// answering the prompt of another session). Kept in the log, never in the bell.
    Allowed,
}

impl Outcome {
    pub(crate) fn wire(&self) -> (&'static str, Option<String>) {
        match self {
            Self::Done => ("done", None),
            Self::Pending => ("pending", None),
            Self::Approved => ("approved", None),
            Self::Denied => ("denied", None),
            Self::Refused(reason) => ("refused", Some(reason.clone())),
            Self::Failed(reason) => ("failed", Some(reason.clone())),
            Self::Allowed => ("allowed", None),
        }
    }

    /// The outcome of a handler's result: `Done` or `Failed` with the error's text.
    pub fn of<T, E: std::fmt::Display>(result: &Result<T, E>) -> Self {
        match result {
            Ok(_) => Self::Done,
            Err(error) => Self::Failed(error.to_string()),
        }
    }
}

/// The signed approval that allowed an action: its id and the device whose key signed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActorRecord {
    pub kind: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl Action {
    /// The action an approval covered, in the words gxserver wrote for its card.
    pub fn from_approval(approval: &ghostex_gx_protocol::approval::ApprovalRequest) -> Self {
        let action = Self::new(approval.action.as_str(), approval.summary.clone());
        match &approval.target {
            Some(target) => action.target(target.clone()),
            None => action,
        }
    }
}

impl From<&ghostex_gx_protocol::approval::ApprovalRequest> for ApprovalRef {
    fn from(approval: &ghostex_gx_protocol::approval::ApprovalRequest) -> Self {
        Self {
            id: approval.id.clone(),
            signer: approval.decided_by.clone(),
        }
    }
}

/// The caller the auth gate settled (`security::Principal`), as the log names it.
impl From<&crate::security::Principal> for Actor {
    fn from(principal: &crate::security::Principal) -> Self {
        use crate::security::Principal;
        match principal {
            Principal::Owner { device_id } => Self::Owner {
                device: Some(device_id.clone()),
            },
            Principal::Device { device_id } => Self::PairedDevice {
                id: Some(device_id.clone()),
                name: device_id.clone(),
            },
            Principal::Session {
                project_id,
                session_id,
                ..
            } => Self::Session {
                session_id: format!("{project_id}:{session_id}"),
            },
            Principal::LocalAgent => Self::current_or_shared_token(),
            Principal::Peer { server_id, name } => Self::ConnectedComputer {
                server_id: server_id.clone(),
                name: name.clone(),
            },
            Principal::Guest { share_id } => Self::CollabGuest {
                name: share_id.clone(),
            },
            Principal::Extension { extension_id } => Self::Extension {
                id: extension_id.clone(),
            },
            Principal::Anonymous => Self::Ghostex,
        }
    }
}

impl Actor {
    /// The shared token's caller, with the session the CLI said it runs in when this request
    /// carried one.
    fn current_or_shared_token() -> Self {
        match Self::current() {
            actor @ Self::SharedToken { .. } => actor,
            _ => Self::SharedToken {
                claimed_session: None,
            },
        }
    }
}
