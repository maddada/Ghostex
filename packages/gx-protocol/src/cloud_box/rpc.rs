//! `POST /api/cloudBoxes` actions and their params and results.
//!
//! Every request is the usual RPC envelope whose params carry `action` plus the fields below
//! (camelCase). Home gxserver answers every action except the `box*` ones, which a box-mode
//! gxserver answers. Remote clients (the phone over the paired link, the GPUI web build) may call
//! the route: the registry and the provider keys live on the computer that owns the projects.
//!
//! Failures use the usual RPC error shape; the Cloud Boxes codes are in [`error_codes`].

use serde::{Deserialize, Serialize};

use super::boxes::{CloudBox, CloudBoxDraftRef};
use super::enums::{CloudConnectMethod, CloudProviderConnectionState, CloudProviderId};
use super::machine::CloudBoxMachine;
use super::provider::{CloudBoxSize, CloudProvider};
use super::settings::ProjectCloudBoxSettings;
use super::snapshot::CloudSnapshot;

open_string_enum! {
    /// The `action` of a `/api/cloudBoxes` request.
    CloudBoxesAction {
        /// → [`ProvidersResult`].
        Providers => "providers",
        /// [`ConnectParams`] → [`ConnectResult`]. Starts a browser sign-in, or saves a pasted key.
        Connect => "connect",
        /// [`ConnectStatusParams`] → [`ConnectResult`]. Polls a browser sign-in.
        ConnectStatus => "connectStatus",
        /// [`ProviderParams`] → [`ProvidersResult`]. Forgets the key (and revokes a minted one).
        Disconnect => "disconnect",
        /// [`SizesParams`] → [`SizesResult`].
        Sizes => "sizes",
        /// [`ListParams`] → [`ListResult`].
        List => "list",
        /// [`BoxParams`] → [`BoxResult`].
        Get => "get",
        /// [`CreateParams`] → [`CreateResult`]. Returns at once with the box `creating`; progress
        /// and the final state arrive as `cloudBoxesChanged`.
        Create => "create",
        /// [`BoxParams`] → [`BoxResult`]. Wakes a `paused` or `stopped` box (the send path calls it,
        /// never an open, Q1013).
        Resume => "resume",
        /// [`BoxParams`] → [`BoxResult`]. A manual stop always stops; the archive rule stops a box only
        /// once every session on it is archived (Q1012).
        Stop => "stop",
        /// [`DeleteParams`] → [`BoxResult`]. Refused with `cloudBoxHasUnpushedWork` unless `force`.
        Delete => "delete",
        /// [`BoxParams`] → [`super::CloudBoxConnectInfo`]. Only for a reachable box.
        ConnectInfo => "connectInfo",
        /// [`SnapshotsParams`] → [`SnapshotsResult`].
        Snapshots => "snapshots",
        /// [`SaveSnapshotParams`] → [`SnapshotResult`] (`pending`, then `cloudBoxesChanged`).
        SaveSnapshot => "saveSnapshot",
        /// [`SnapshotParams`] → [`SnapshotResult`].
        SetDefaultSnapshot => "setDefaultSnapshot",
        /// [`SnapshotParams`] → [`SnapshotResult`].
        DeleteSnapshot => "deleteSnapshot",
        /// [`SpendParams`] → [`super::CloudSpend`].
        Spend => "spend",
        /// [`ProjectSettingsParams`] → [`ProjectSettingsResult`]. Reads, and with `settings` replaces,
        /// the project's Cloud Box settings.
        ProjectSettings => "projectSettings",
        /// → [`super::AgentLoginsResult`]: every Claude and Codex account and whether boxes may use it.
        AgentLogins => "agentLogins",
        /// [`super::SetAgentLoginParams`] → [`super::AgentLoginsResult`].
        SetAgentLogin => "setAgentLogin",
        /// [`ReportBoxSessionsParams`] → [`BoxResult`]. A client attached to a box reports the box's
        /// sessions, so a stopped box still lists them and the archive rule sees when all are
        /// archived.
        ReportBoxSessions => "reportBoxSessions",
        /// Box mode only → [`super::BoxIdleStatus`].
        BoxIdleStatus => "boxIdleStatus",
        /// Box mode only → [`super::BoxWorkStatus`].
        BoxWorkStatus => "boxWorkStatus",
        /// Box mode only, [`BoxWriteBackParams`] → [`super::BoxWriteBackResult`].
        BoxWriteBack => "boxWriteBack",
        /// [`PullBranchParams`] → [`PullBranchResult`]. Brings a box session's pushed branch into a
        /// worktree of its home project on this computer (Q1014).
        PullBranch => "pullBranch",
        /// → [`super::CloudKit`]. What a paired phone keeps to start boxes with the computer off;
        /// refused with `cloudKitOff` while "Start boxes from your phone" is off.
        CloudKit => "cloudKit",
    }
}

/// Error codes of `/api/cloudBoxes` (the RPC error's `code`).
pub mod error_codes {
    /// The monthly cap is reached: new boxes are refused ("Raise the cap"); running boxes keep
    /// running (D8).
    pub const CAP_REACHED: &str = "cloudCapReached";
    /// The provider is not connected, or its key expired.
    pub const PROVIDER_NOT_CONNECTED: &str = "cloudProviderNotConnected";
    /// The provider can't make a box now (`local`: Docker isn't running).
    pub const PROVIDER_UNAVAILABLE: &str = "cloudProviderUnavailable";
    /// The account's plan refuses it (Instabox free: one box). The message explains and the
    /// result's `reuseBoxId` names a running box to reuse instead (D10).
    pub const PLAN_LIMIT: &str = "cloudPlanLimit";
    /// The provider answered with an error; the message is the provider's, cleaned of secrets.
    pub const PROVIDER_ERROR: &str = "cloudProviderError";
    /// D13: the box has uncommitted or unpushed work; push, or delete with `force`.
    pub const HAS_UNPUSHED_WORK: &str = "cloudBoxHasUnpushedWork";
    /// Another open session uses the box, so it is not stopped.
    pub const BOX_IN_USE: &str = "cloudBoxInUse";
    /// The box is not in a state the action needs (connectInfo on a stopped box).
    pub const WRONG_STATE: &str = "cloudBoxWrongState";
    /// `cloudKit` while Settings › Cloud Boxes › "Start boxes from your phone" is off.
    pub const KIT_OFF: &str = "cloudKitOff";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersResult {
    pub providers: Vec<CloudProvider>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderParams {
    pub provider: CloudProviderId,
}

/// `connect`. With `key`: validate and save it (method `key`). Without: start the browser sign-in
/// (method `browser`); Boat's needs `email` (its claim page asks for the account's email).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectParams {
    pub provider: CloudProviderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<CloudConnectMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Open the sign-in page in this computer's browser (default `true`). A remote client that
    /// opens it itself passes `false` and shows `authUrl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_browser: Option<bool>,
}

/// `connect` / `connectStatus` result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub provider: CloudProviderId,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudProviderConnectionState,
    /// Poll `connectStatus` with this while `connecting`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_id: Option<String>,
    /// The page the user finishes the sign-in on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    /// Boat: the 6-digit code the user types on the claim page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub browser_opened: bool,
    /// ISO-8601; the sign-in gives up after this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// The provider row once `connected`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_row: Option<CloudProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectStatusParams {
    pub provider: CloudProviderId,
    pub connect_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizesParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CloudProviderId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizesResult {
    pub sizes: Vec<CloudBoxSize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    /// Only boxes of this home project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Keep `deleted` rows (Settings' history); default false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_deleted: Option<bool>,
    /// Ask the providers for their box lists now instead of answering from the registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub boxes: Vec<CloudBox>,
    /// One hidden machine per reachable box, for clients that attach (W3).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub machines: Vec<CloudBoxMachine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxParams {
    pub box_id: String,
    /// Saved before the provider call so a retry never makes two machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteParams {
    pub box_id: String,
    /// Delete even with unpushed work (D13 "push or delete anyway").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxResult {
    #[serde(rename = "box")]
    pub cloud_box: CloudBox,
}

/// `create`: a box for a home project, from a snapshot (the project's Default unless given), or
/// blank for guided setup (Q1003).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateParams {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CloudProviderId>,
    /// A snapshot id, or a snapshot name of this project ("Base").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Start from the provider's base image instead of a snapshot (guided setup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blank: Option<bool>,
    /// The home draft whose first message is waiting; its session starts on the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<CloudBoxDraftRef>,
    /// Start the draft's session on this running (or stopped) box instead of a new box (Q1007);
    /// it gets its own worktree there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse_box_id: Option<String>,
    /// The same key returns the same box instead of making a second (a retried Send).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    #[serde(rename = "box")]
    pub cloud_box: CloudBox,
    /// The idempotency key matched an existing box; nothing new was made.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub reused: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CloudProviderId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotsResult {
    pub snapshots: Vec<CloudSnapshot>,
}

/// `saveSnapshot`: Save as snapshot / Update snapshot from a running box.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSnapshotParams {
    pub box_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Make it the project's Default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub make_default: Option<bool>,
    /// Update snapshot (D9): replace the Default, keep the old one as "Previous" for 7 days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_default: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotParams {
    pub snapshot_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResult {
    pub snapshot: CloudSnapshot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendParams {
    /// Read the providers now instead of answering from the last read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsParams {
    pub project_id: String,
    /// Replace the project's settings with these; absent reads them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectCloudBoxSettings>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsResult {
    pub project_id: String,
    pub settings: ProjectCloudBoxSettings,
    /// The project's `.env*` files (project-relative), the candidates of "Send to the box" (Q1020).
    /// Empty when the project folder is not on this computer.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub env_files: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxWriteBackParams {
    /// [`super::BoxWriteBackItem::id`]s home has saved.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub ack: Vec<String>,
}

/// `pullBranch`: the branch of one session on the box (by its id on the box or its home draft's
/// id), else the box's first session with a branch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullBranchParams {
    pub box_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// What `pullBranch` did: the worktree project on this computer that has the branch now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullBranchResult {
    pub branch: String,
    pub project_id: String,
    pub path: String,
    /// A new worktree was made (false: an existing one was brought up to date).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub created: bool,
    /// Set when the local branch has commits the box's branch does not, so it was not
    /// fast-forwarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportBoxSessionsParams {
    pub box_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sessions: Vec<super::CloudBoxSessionRow>,
}
