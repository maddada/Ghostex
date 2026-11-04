//! A box row of the registry, its boot progress, and the cached session rows of a stopped box.

use serde::{Deserialize, Serialize};

use super::enums::{
    CloudBoxBootStep, CloudBoxCreatedBy, CloudBoxState, CloudBoxStepState, CloudProviderId,
};

/// One box as home gxserver's registry knows it (`list`, `get`, `create`, `cloudBoxesChanged`).
///
/// The provider's own box list is the source of truth: every box Ghostex makes carries
/// `ghostex=1`, its [`CloudBox::box_id`] and project key in the provider's metadata (labels for
/// `local`), so any client can rebuild the registry from the provider (`packages/gx-cloud`,
/// `metadata.rs`). Secrets (the box's gxserver token, provider keys) are never on this row; a
/// client that needs to attach asks `connectInfo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBox {
    /// Ghostex's id, `gx_<12 lowercase base32>`; stable across resumes and providers' renames.
    pub box_id: String,
    pub provider: CloudProviderId,
    /// The provider's machine id (Instabox machine id, Boat sandbox id, Docker container id).
    /// Absent while `pending`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_box_id: Option<String>,
    /// Short display name for the chip and lists, e.g. `box-c31`.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudBoxState,
    /// One line about the state: the failure for `failed`, "Pauses in 6 min" for `idle`,
    /// "Has unpushed work" when a delete was refused (D13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_detail: Option<String>,
    /// The project's key that survives machines: the repository's normalized origin URL when it
    /// has one, else the home project's path. Snapshots and the provider metadata use it.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub project_key: String,
    /// The project on home gxserver the box's sessions are listed under (Q1015).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    /// The Ghostex snapshot id ([`super::CloudSnapshot::snapshot_id`]) the box started from;
    /// absent for a blank box (guided setup, Q1003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// [`super::CloudBoxSize::id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// The box's own tailcat address (its WireGuard public key), once booted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailcat_address: Option<String>,
    /// The gxserver version the box runs (R10: a client offers Update on a mismatch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gxserver_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<CloudBoxCreatedBy>,
    /// ISO-8601 timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// The latest time the box became `running` (the chip's running time counts from here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    /// When the 7-day archive delete is due (Q1012), set when its sessions are all archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete_after: Option<String>,
    /// When the idle countdown stops or pauses the box, while `idle` (Q1011).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_stops_at: Option<String>,
    /// This box's spend since it was created (the chip's "≈$0.02", D2 "time and money").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_total_usd: Option<f64>,
    /// This box's spend this month (UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_month_usd: Option<f64>,
    /// The costs are Ghostex's estimate from running seconds and its price table (Instabox only
    /// reports seconds per box, R6), not the provider's own dollars.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub cost_estimated: bool,
    /// About-$ per running hour of this box's size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd_per_hour: Option<f64>,
    /// The box last reported uncommitted or unpushed work (D13: such a box is never deleted on
    /// schedule, and a manual delete needs `force`). Absent when not checked yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_unpushed_work: Option<bool>,
    /// Why "Update snapshot" is suggested (D9, the chip's dot): "The lockfile changed." or
    /// "The Default snapshot is older than 7 days." Absent when no update is suggested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_update_reason: Option<String>,
    /// The box's sessions as last seen, so a stopped box still lists its rows without waking it
    /// (Q1013).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub sessions: Vec<CloudBoxSessionRow>,
    /// Present while the box is starting or resuming, and after a failed start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<CloudBoxProgress>,
}

/// A session on a box as last seen by home gxserver (cached for stopped boxes).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxSessionRow {
    /// Ids on the box's own gxserver.
    pub project_id: String,
    pub session_id: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// The branch the session works on (Q1005, Q1014).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The agent's own session id; kept for resume and for V2 teleport (Q1024).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<String>,
    /// The home draft this session came from, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub archived: bool,
}

/// The boot progress card (Q1009): one line per [`CloudBoxBootStep`] with its time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxProgress {
    /// The home draft whose message is waiting for this box, when one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<CloudBoxDraftRef>,
    /// `create` for a new box, `resume` for a wake.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub kind: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub steps: Vec<CloudBoxProgressStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    /// The failure a user reads, when a step failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One line of the progress card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxProgressStep {
    pub step: CloudBoxBootStep,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudBoxStepState,
    /// The line text ([`CloudBoxBootStep::label`] unless the server says more).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    /// Extra words for the line ("Instabox machine m-81c2", the failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl CloudBoxProgressStep {
    pub fn waiting(step: CloudBoxBootStep) -> Self {
        Self {
            label: step.label().to_string(),
            step,
            state: CloudBoxStepState::Waiting,
            started_at: None,
            finished_at: None,
            message: None,
        }
    }
}

/// A home draft session, by its ids on home gxserver.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxDraftRef {
    pub project_id: String,
    pub session_id: String,
}
