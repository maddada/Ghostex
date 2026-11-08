//! The wire contract between gxserver and the Ghostex Collab component (`ghostex-collab`), the
//! sidecar that holds all of Collab's working code (shares, codes, the relay connection, the
//! end-to-end lock, the guest page pin, held guest messages). The app ships none of that code:
//! the component is downloaded only when the owner installs Collab in Settings › Extensions,
//! with their signed approval (GitHub #495).
//!
//! The channel is local and owner-authenticated both ways:
//! - gxserver starts the sidecar and writes one [`SidecarHello`] line to its stdin (never argv or
//!   the environment): gxserver's loopback address, the sidecar's data folder and two random
//!   secrets. The sidecar answers with one [`SidecarReady`] line on stdout once it listens on
//!   `127.0.0.1`.
//! - gxserver → sidecar: `POST` [`SIDECAR_OWNER_PATH`] ([`OwnerCall`] → [`OwnerReply`]) and
//!   [`SIDECAR_EVENT_PATH`] ([`SidecarEvent`]), with `Authorization: Bearer <sidecar_token>`.
//! - sidecar → gxserver: `POST` [`COLLAB_HOST_ENDPOINT`] with a [`HostCall`] and
//!   `Authorization: Bearer <host_token>`; gxserver answers no other caller there.
//!
//! The approval gate stays in gxserver: the sidecar answers [`OwnerReply::needs_presence`]
//! instead of acting, gxserver asks the owner, and repeats the same call with the [`GrantInfo`]
//! once they approved.
//!
//! SEE-ALSO: server/collab (the sidecar), server/src/collab_shim (gxserver's side).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::approval::SensitiveAction;

/// The component's name in the sealed on-demand manifest, the component store and the gate's
/// `installExtensionComponent` target.
pub const COLLAB_COMPONENT: &str = "collab";
/// The sidecar executable's name (with `.exe` on Windows).
pub const COLLAB_SIDECAR_BINARY: &str = "ghostex-collab";
/// Bumped when this contract changes incompatibly; gxserver doesn't start a sidecar that speaks
/// another version (the owner updates it in Settings › Updates).
pub const COLLAB_PROTOCOL_VERSION: u32 = 1;
/// gxserver's endpoint for the sidecar's calls.
pub const COLLAB_HOST_ENDPOINT: &str = "/api/collabHost";
/// The header the sidecar names a guest with when it replays that guest's reads with their
/// share's token.
pub const COLLAB_VIEWER_HEADER: &str = "x-ghostex-collab-viewer";
pub const SIDECAR_OWNER_PATH: &str = "/owner";
pub const SIDECAR_EVENT_PATH: &str = "/event";
pub const SIDECAR_HEALTH_PATH: &str = "/health";

/// gxserver → sidecar, one JSON line on stdin at start.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarHello {
    pub protocol: u32,
    /// gxserver's loopback base address (`http://127.0.0.1:<port>`).
    pub host_url: String,
    /// gxserver's protocol version header value, for the guest reads it replays.
    pub gxserver_protocol_version: u32,
    /// The folder the sidecar keeps its choices, shares and log in; removed on uninstall.
    pub data_dir: String,
    /// What the sidecar calls [`COLLAB_HOST_ENDPOINT`] with.
    pub host_token: String,
    /// What gxserver calls the sidecar with.
    pub sidecar_token: String,
}

/// sidecar → gxserver, one JSON line on stdout once it listens.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarReady {
    pub protocol: u32,
    pub port: u16,
    pub version: String,
}

/// One live share, as gxserver needs it to let that share's guests read their one chat.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareScope {
    pub share_id: String,
    pub project_id: String,
    pub session_id: String,
    /// The share's `gxshare_` token the sidecar replays guest reads with.
    pub token: String,
    /// The guests in the share now; a replayed read must name one of them.
    #[serde(default)]
    pub viewer_ids: Vec<String>,
    /// Pictures this share's guests uploaded, which they may read back.
    #[serde(default)]
    pub images: Vec<String>,
}

/// An approval gxserver got from the owner for the call it repeats.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantInfo {
    pub approval_id: String,
    pub action: SensitiveAction,
    pub target: String,
}

/// gxserver → sidecar: one owner call to `/api/collab`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerCall {
    /// The call's params as the caller sent them (`approvalId` dropped).
    pub params: Map<String, Value>,
    /// Set when gxserver repeats the call after the owner approved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<GrantInfo>,
}

/// What the call needs approved before the sidecar makes the change. The words are the
/// component's, built from the chat's title and the stored share, never from the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceNeed {
    pub action: SensitiveAction,
    pub target: String,
    pub summary: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub code: String,
    pub message: String,
}

/// An approved change the sidecar made (or tried to): gxserver records it in the security log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Performed {
    pub approval_id: String,
    pub ok: bool,
}

/// sidecar → gxserver: the answer to an [`OwnerCall`]. Exactly one of `result`, `refused` and
/// `needs_presence` is set.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Refusal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_presence: Option<PresenceNeed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub performed: Vec<Performed>,
}

impl OwnerReply {
    pub fn ok(result: Value) -> Self {
        Self {
            result: Some(result),
            ..Self::default()
        }
    }

    pub fn refused(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            refused: Some(Refusal {
                code: code.into(),
                message: message.into(),
            }),
            ..Self::default()
        }
    }

    pub fn needs_presence(need: PresenceNeed) -> Self {
        Self {
            needs_presence: Some(need),
            ..Self::default()
        }
    }
}

/// gxserver → sidecar: something happened on gxserver's side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum SidecarEvent {
    /// A chat was deleted: its share ends with it.
    #[serde(rename_all = "camelCase")]
    ChatDeleted {
        project_id: String,
        session_id: String,
    },
    /// The owner is typing in (or stopped typing in) a chat.
    #[serde(rename_all = "camelCase")]
    OwnerTyping {
        project_id: String,
        session_id: String,
        typing: bool,
    },
    /// Collab is being uninstalled: end every share (the relay link dies with them).
    EndAll,
}

/// sidecar → gxserver ([`COLLAB_HOST_ENDPOINT`]). gxserver re-checks that every chat named here
/// is a chat the sidecar shares now ([`HostCall::SetShares`]), except `chatInfo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum HostCall {
    /// Every live share (replaces the last list). `[]` turns every share token off.
    SetShares { shares: Vec<ShareScope> },
    /// The chat's Collab part of its frame (`collab` on the chat view: shared, guests, typing,
    /// held messages), or `null` for not shared. gxserver republishes the chat.
    #[serde(rename_all = "camelCase")]
    SetChatExtras {
        project_id: String,
        session_id: String,
        extras: Value,
    },
    /// → `{exists, title, projectName}` (`exists: null` when gxserver can't tell).
    #[serde(rename_all = "camelCase")]
    ChatInfo {
        project_id: String,
        session_id: String,
    },
    /// Sends a guest's (labelled) words to the shared chat's agent. → `{}`.
    #[serde(rename_all = "camelCase")]
    Deliver {
        project_id: String,
        session_id: String,
        text: String,
        #[serde(default)]
        image_paths: Vec<String>,
    },
    /// Saves a guest's pasted picture like the owner's own. `params` are
    /// `saveSessionChatImage`'s. → `{path}`.
    #[serde(rename_all = "camelCase")]
    SaveGuestImage {
        project_id: String,
        session_id: String,
        params: Map<String, Value>,
    },
    /// A guest's answers to a page the shared chat published, as the message they send.
    /// → `{message}`.
    #[serde(rename_all = "camelCase")]
    GuestPageAnswersMessage {
        project_id: String,
        session_id: String,
        page_id: String,
        answers: Value,
    },
    /// A guest joined: a row in the owner's notifications. → `{}`.
    #[serde(rename_all = "camelCase")]
    NotifyGuestJoined {
        project_id: String,
        session_id: String,
        guest: String,
    },
}
