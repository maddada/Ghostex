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

/// Whether Cloud Box is offered when a session starts (the New Thread picker's chip and the new
/// chat's bar): once the user has set Cloud Boxes up, that is
/// a cloud provider (Instabox or Boat) is connected, or they have a box.
///
/// CDXC:CloudBoxes 2026-11-04 DECISION:
/// User (Q1224, randomsand: "hide the Cloud Box option for people who don't use cloud boxes"): the setting that turns Cloud Boxes on is on by default, so everybody saw a Cloud Box chip they had never set up. The chip now waits until Cloud Boxes is in use: an Instabox or Boat account is connected (a sign-in that expired still counts, so a lapsed key shows the chip that explains it), or a box exists, which also covers the local Docker provider, whose "connected" only means Docker is there. Turning Cloud Boxes off still hides it everywhere; Settings › Cloud Boxes is where you set it up. The new chat's bar follows the same rule, and also shows it for a project whose last choice was Cloud Box or a draft already set to one (server/src/server/worktree_ops/draft_cloud_box.rs).
pub fn cloud_boxes_set_up(providers: &[CloudProvider], has_boxes: bool) -> bool {
    use super::CloudProviderConnectionState as State;
    has_boxes
        || providers.iter().any(|provider| {
            !provider.debug_only
                && provider.id != super::CloudProviderId::Local
                && matches!(provider.state, State::Connected | State::Expired)
        })
}

#[cfg(test)]
mod set_up_tests {
    use super::{cloud_boxes_set_up, CloudProvider};
    use serde_json::json;

    fn provider(id: &str, state: &str) -> CloudProvider {
        serde_json::from_value(json!({"id": id, "state": state})).unwrap()
    }

    #[test]
    fn cloud_box_waits_until_a_cloud_provider_is_connected_or_a_box_exists() {
        // Docker being there is not using Cloud Boxes.
        let fresh = [
            provider("instabox", "notConnected"),
            provider("local", "connected"),
        ];
        assert!(!cloud_boxes_set_up(&fresh, false));
        assert!(cloud_boxes_set_up(&fresh, true));
        assert!(cloud_boxes_set_up(&[provider("boat", "connected")], false));
        assert!(cloud_boxes_set_up(
            &[provider("instabox", "expired")],
            false
        ));
        assert!(!cloud_boxes_set_up(&[], false));
    }
}
