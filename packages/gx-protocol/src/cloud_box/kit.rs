//! The cloud kit: what a paired phone keeps so it can start a box from a project's Default snapshot
//! while the computer is off (plan §2.3).
//!
//! CDXC:CloudBoxes 2026-11-02 DECISION:
//! User (Q1033 D7 "start chat wake stop"): with the computer off the phone starts a box from an
//! existing snapshot, chats with it, wakes, stops and deletes it; connecting providers, the first
//! setup, saving snapshots and picking secrets stay on the computer. So the computer hands the
//! phone, only while Settings › Cloud Boxes › "Start boxes from your phone" is on, everything a
//! start needs: the provider keys, the agent logins allowed on boxes, each project's Default
//! snapshot and the values of its ticked secrets, and the device keys a box admits. The phone keeps
//! it in its secure store.
//! SEE-ALSO: server/src/cloud_boxes/kit.rs, apps/mobile/app/src/cloudBoxes/cloudKit.ts,
//! packages/shared/gxserver-protocol-cloud-boxes.ts (`GxserverCloudKit`).

use serde::{Deserialize, Serialize};

use super::{BoxInjectedAccount, BoxSecretFile, BoxSecretVar, CloudProviderId};

/// The kit's format; a phone ignores a kit with a version it does not know.
pub const CLOUD_KIT_VERSION: u64 = 1;

/// One provider the phone may call with its key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKitProvider {
    pub provider: CloudProviderId,
    pub key: String,
    /// The provider's plan (`free`, `pro`), so a phone create follows the plan's rules (Instabox's
    /// free plan refuses an idle pause of its own). Absent from an older computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

/// A project's Default snapshot as a phone start needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKitSnapshot {
    pub snapshot_id: String,
    pub provider: CloudProviderId,
    pub provider_snapshot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default)]
    pub name: String,
}

/// One project a phone may start a box for: it has a ready Default snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKitProject {
    pub home_project_id: String,
    pub project_key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    pub snapshot: CloudKitSnapshot,
    /// The values of the project's ticked secrets (Q1020), read on the computer.
    #[serde(default)]
    pub files: Vec<BoxSecretFile>,
    #[serde(default)]
    pub vars: Vec<BoxSecretVar>,
}

/// `cloudKit` → this.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKit {
    pub version: u64,
    /// The computer's gxserver id, which a phone-started box records as its home.
    pub home_machine_id: String,
    /// The gxserver version boxes install (`ghostex_gx_cloud::install::install_script`).
    pub gxserver_version: String,
    pub issued_at: String,
    #[serde(default)]
    pub providers: Vec<CloudKitProvider>,
    /// The Claude box tokens and Codex logins allowed on boxes (D5, D6).
    #[serde(default)]
    pub accounts: Vec<BoxInjectedAccount>,
    /// Tailcat client keys a box admits (Q1021 private only).
    #[serde(default)]
    pub allow_device_keys: Vec<String>,
    pub idle_stop_minutes: u64,
    #[serde(default)]
    pub projects: Vec<CloudKitProject>,
    /// The monthly cap and the spend this computer last read, so the phone refuses a new box at
    /// the cap while the computer is off.
    #[serde(default)]
    pub spend: CloudKitSpend,
}

/// The cap rule's inputs as of the kit (D8).
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose that with the computer off the kit carries
/// the cap and the last spend seen, and the phone refuses a new box once that spend reached the
/// cap (100%), as the computer does; running boxes are never touched. A kit from an earlier month
/// says nothing about this month, so the phone checks only a kit whose `month` is the current one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudKitSpend {
    /// `YYYY-MM`, UTC, of `total_usd`.
    #[serde(default)]
    pub month: String,
    #[serde(default)]
    pub total_usd: f64,
    /// Absent: no cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_usd: Option<f64>,
}
