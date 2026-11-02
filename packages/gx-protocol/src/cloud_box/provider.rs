//! A connected (or connectable) provider and the box sizes it offers.

use serde::{Deserialize, Serialize};

use super::enums::{CloudConnectMethod, CloudProviderConnectionState, CloudProviderId};

/// One provider row of `providers` (Settings › Cloud Boxes, the picker footer's provider menu).
/// Keys and refresh tokens never leave home gxserver's vault; this row carries only what a user
/// reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProvider {
    pub id: CloudProviderId,
    /// "Instabox", "Boat", "Local (Docker)".
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub label: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudProviderConnectionState,
    /// The ways this provider can be connected, preferred first.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub connect_methods: Vec<CloudConnectMethod>,
    /// The provider can make a box right now (connected and, for `local`, Docker running).
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub available: bool,
    /// Why `available` is false, in words a user reads ("Docker isn't running.").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// The account the key belongs to, as the provider names it (org or user name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    /// Instabox org id; Boat has none for personal accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    /// The plan as the provider names it ("free", "pro", "trial").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<CloudProviderLimits>,
    /// ISO-8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<String>,
    /// The last failure (a rejected key, an expired refresh token).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The development provider: hidden unless Show debug UI controls is on.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub debug_only: bool,
}

/// What the account's plan allows (R8, D10: free plans and trials still work, and the picker
/// explains the limit and offers to reuse a running box). Absent fields mean "no limit known".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudProviderLimits {
    /// Boxes in any state (Instabox free: 1).
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_boxes: Option<u64>,
    /// Boxes running at once (Instabox Pro: 100).
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_running_boxes: Option<u64>,
    /// Saved snapshots (Instabox free: 5; Boat: 10 included, then billed per day).
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_snapshots: Option<u64>,
    /// The longest a box may run before the provider stops it (Boat trial: 7200).
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_box_run_seconds: Option<u64>,
    /// Size ids ([`CloudBoxSize::id`]) the plan may create; empty means every size.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub allowed_sizes: Vec<String>,
    /// One sentence for the picker and Settings ("Free plan: one box at a time.").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A size a provider can make (the picker footer's size menu, Q1008).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxSize {
    /// Stable id `<vcpus>x<memGiB>`, e.g. `2x4`.
    pub id: String,
    pub provider: CloudProviderId,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub vcpus: u64,
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub mem_mib: u64,
    /// "2 CPU · 4 GB".
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub label: String,
    /// About-$ per running hour from Ghostex's price table (`packages/gx-cloud`), for "≈ $/h";
    /// `0` for `local`. Absent when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd_per_hour: Option<f64>,
    /// The size Ghostex picks when the project has none saved.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub is_default: bool,
}

impl CloudBoxSize {
    /// The `<vcpus>x<memGiB>` id of a size.
    pub fn id_for(vcpus: u64, mem_mib: u64) -> String {
        format!("{vcpus}x{}", mem_mib / 1024)
    }
}
