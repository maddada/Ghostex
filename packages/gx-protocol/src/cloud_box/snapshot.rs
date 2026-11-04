//! Project snapshots (Q1002: snapshots belong to a project, one is Default ★).

use serde::{Deserialize, Serialize};

use super::enums::{CloudProviderId, CloudSnapshotStatus};

/// One saved snapshot of a project (`snapshots`, Settings › project › Cloud box).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSnapshot {
    /// Ghostex's id, `gx_<12 lowercase base32>`.
    pub snapshot_id: String,
    pub provider: CloudProviderId,
    /// The provider's snapshot id (Instabox snapshot, Boat saved snapshot, Docker image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_snapshot_id: Option<String>,
    /// [`super::CloudBox::project_key`].
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub project_key: String,
    /// "Base", "Previous", or what the user typed.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub name: String,
    /// The project's Default (★): what Cloud Box starts from unless the picker says otherwise.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub is_default: bool,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub status: CloudSnapshotStatus,
    /// The box it was saved from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_box_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub size_bytes: Option<u64>,
    /// [`super::CloudBoxSize::id`] boxes from this snapshot start with unless picked otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// ISO-8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// The project's setup script on the box (`.ghostex/cloud-setup.sh`), when guided setup wrote
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_script_path: Option<String>,
    /// For a "Previous" snapshot: the snapshot that replaced it as Default (D9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_of: Option<String>,
    /// When a "Previous" snapshot is deleted (7 days after it was replaced, D9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Instabox snapshots are shared with every member of the org (R7); Settings warns when this
    /// is true.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub shared_with_org: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
