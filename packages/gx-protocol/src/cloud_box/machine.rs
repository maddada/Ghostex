//! How a client reaches a box: the hidden machine it registers for each running box (W3), and
//! the secrets `connectInfo` hands over to attach.

use serde::{Deserialize, Serialize};

use super::enums::{CloudBoxState, CloudProviderId};

/// Prefix of the machine id a client gives a box's hidden machine, `cloudbox:<boxId>`. Box
/// machines never get a machine tab and never enter the user's saved remote machines list.
pub const CLOUD_BOX_MACHINE_PREFIX: &str = "cloudbox:";

/// The machine id of a box's hidden machine.
pub fn cloud_box_machine_id(box_id: &str) -> String {
    format!("{CLOUD_BOX_MACHINE_PREFIX}{box_id}")
}

/// The box id of a `cloudbox:<boxId>` machine id.
pub fn cloud_box_id_from_machine_id(machine_id: &str) -> Option<&str> {
    machine_id
        .strip_prefix(CLOUD_BOX_MACHINE_PREFIX)
        .filter(|box_id| !box_id.is_empty())
}

/// The machine a client keeps for one box (plan §2.6 "a machine kind `cloudBox`"), built from a
/// [`super::CloudBox`] row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxMachine {
    /// [`cloud_box_machine_id`].
    pub machine_id: String,
    pub box_id: String,
    pub provider: CloudProviderId,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudBoxState,
    /// The home project the box's sessions merge into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

impl CloudBoxMachine {
    pub fn from_box(cloud_box: &super::CloudBox) -> Self {
        Self {
            machine_id: cloud_box_machine_id(&cloud_box.box_id),
            box_id: cloud_box.box_id.clone(),
            provider: cloud_box.provider.clone(),
            state: cloud_box.state.clone(),
            home_project_id: cloud_box.home_project_id.clone(),
            started_at: cloud_box.started_at.clone(),
            cost_usd: cloud_box.cost_total_usd,
        }
    }
}

/// Result of `connectInfo`: what a client needs to attach to a running box's gxserver. Only
/// home gxserver hands it out, only for a reachable box, and only to an authenticated client.
///
/// `transport` says how: `tailcat` (Instabox, Boat: dial `tailcatAddress` with the client's own
/// device key, which the box's allow-list holds, and reach gxserver on the box's loopback
/// `gxserverPort`), or `direct` (`local`: `baseUrl` is a loopback port Docker publishes on this
/// computer only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxConnectInfo {
    pub box_id: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailcat_address: Option<String>,
    /// The box's sshd port behind tailcat (the existing remote-computer path runs over SSH).
    #[serde(
        default,
        deserialize_with = "crate::de::lenient_opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub ssh_port: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    /// gxserver's port on the box's loopback.
    #[serde(default, deserialize_with = "crate::de::lenient_u64")]
    pub gxserver_port: u64,
    /// For `direct`: `http://127.0.0.1:<published port>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The box gxserver's bearer token.
    pub gxserver_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gxserver_version: Option<String>,
}

/// [`CloudBoxConnectInfo::transport`] values.
pub const CLOUD_BOX_TRANSPORT_TAILCAT: &str = "tailcat";
pub const CLOUD_BOX_TRANSPORT_DIRECT: &str = "direct";
