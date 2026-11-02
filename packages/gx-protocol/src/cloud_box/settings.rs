//! Where Cloud Box choices are stored: per project, per draft, per box session, and the
//! session's presentation field; plus the shared settings keys gxserver reads.

use serde::{Deserialize, Serialize};

use super::enums::{CloudBoxRemember, CloudBoxState, CloudProviderId};

/// The key under `runtimeSettings` (project, draft, and box session alike).
pub const CLOUD_BOX_RUNTIME_SETTINGS_KEY: &str = "cloudBox";

/// Settings keys (in the shared settings file, `ghostex settings`; rows in
/// `packages/settings-catalog`, W5). gxserver reads them; it never writes them.
pub mod setting_keys {
    /// Monthly cap in dollars across providers; absent, `null` or `0` means off (Q1001, D8). The
    /// catalog's default is `0`, so `ghostex settings set` treats it as a number.
    pub const MONTHLY_CAP_USD: &str = "cloudBoxesMonthlyCapUsd";
    /// Minutes with no agent turn and no open terminal before a box pauses or stops (Q1011).
    pub const IDLE_STOP_MINUTES: &str = "cloudBoxesIdleStopMinutes";
    /// Days a stopped box whose sessions are all archived is kept before it is deleted (Q1012).
    pub const DELETE_STOPPED_AFTER_DAYS: &str = "cloudBoxesDeleteStoppedAfterDays";
    /// "Start boxes from your phone": sync the cloud kit to paired phones (D7).
    pub const PHONE_START_ENABLED: &str = "cloudBoxesPhoneStartEnabled";
    /// The existing "Show debug UI controls" switch; it also lists the `local` provider.
    pub const DEBUG_UI: &str = "debuggingMode";
}

/// Defaults of the keys above.
pub const DEFAULT_IDLE_STOP_MINUTES: u64 = 15;
pub const DEFAULT_DELETE_STOPPED_AFTER_DAYS: u64 = 7;

/// Environment switch that lists the `local` provider without Show debug UI controls (CI, the
/// GPUI web build, threads proving a stream).
pub const LOCAL_PROVIDER_ENV: &str = "GHOSTEX_CLOUD_BOXES_LOCAL_PROVIDER";

/// Per project: `projects.runtimeSettingsJson.cloudBox` on home gxserver (Q1002, Q1008, Q1010,
/// Q1020). Secret values never go here, only which files and variable names are sent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloudBoxSettings {
    /// What the new-chat tray last used in this project: This folder / New worktree (`local`) or
    /// the Cloud Box segment (`cloud`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remember: Option<CloudBoxRemember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CloudProviderId>,
    /// The project's Default snapshot ([`super::CloudSnapshot::snapshot_id`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// [`super::CloudBoxSize::id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default)]
    pub secrets: CloudBoxSecretPicks,
}

/// The ticked list "Send to the box" (Q1020).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxSecretPicks {
    /// Project-relative paths of `.env*` files copied into the box's checkout at boot.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub files: Vec<String>,
    /// Environment variable names whose values (read from the project's env files or the
    /// computer's environment at boot) are set for the box session.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub vars: Vec<String>,
}

/// A draft that picked the Cloud Box segment: `runtimeSettings.cloudBox` on the home draft row.
/// Nothing exists at the provider and nothing bills until the first Send (Q1009).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftCloudBox {
    /// Always `true` on a draft; the field tells a draft's record from a box session's.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<CloudProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Run in this already-running box (a new worktree there) instead of a new box (Q1007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse_box_id: Option<String>,
    /// The tray's This folder / New worktree choice applied on the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<bool>,
    /// Start from the provider's base image instead of a snapshot: "Set up cloud box" (Q1003,
    /// guided setup) stages its prompt in a draft with this set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blank: Option<bool>,
    /// The first message's pictures on this computer, kept by Send so the box start copies them
    /// onto the box.
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_strings",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub image_paths: Vec<String>,
    /// Set once Send started a box for this draft; the draft row retires when its session exists
    /// on the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_id: Option<String>,
}

/// A session on a box: `runtimeSettings.cloudBox` on the box's own gxserver. `branch` and
/// `agentSessionId` keep V2 teleport possible (Q1024).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCloudBoxRuntime {
    pub box_id: String,
    pub provider: CloudProviderId,
    /// The project on home gxserver this session is listed under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    /// Home gxserver's `serverId`, so a client merges the row into the right machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_machine_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
}

/// The session field `cloudBox` of a presentation row (the ☁ badge and its state dot, Q1015).
/// Sent by home gxserver for a pending draft and by a box's gxserver for its sessions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationCloudBox {
    pub box_id: String,
    pub provider: CloudProviderId,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudBoxState,
    /// [`super::CloudBox::name`], e.g. `box-c31`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub box_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    /// A draft whose tray picked Cloud Box: its first message creates the box. `box_id` is empty
    /// until Send starts one.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub pending: bool,
    /// The branch the box session was put on ([`SessionCloudBoxRuntime::branch`]), which its
    /// git row may not carry: clients read it for Pull branch and Open PR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}
