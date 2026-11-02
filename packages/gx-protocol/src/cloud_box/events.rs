//! The `cloudBoxesChanged` event frame.

use serde::{Deserialize, Serialize};

use super::boxes::CloudBox;
use crate::event::EventHeader;

pub const CLOUD_BOXES_CHANGED_EVENT_TYPE: &str = "cloudBoxesChanged";

/// Home gxserver tells clients the registry changed: a box's state, progress line, cost, or a
/// provider's connection. `box` carries the whole new row when one box changed (replace, never
/// merge; a `deleted` row means drop it); without `box`, a client refetches `list` (and
/// `providers` / `spend` when `scope` names them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBoxesChangedFrame {
    #[serde(flatten)]
    pub header: EventHeader,
    #[serde(default, rename = "box", skip_serializing_if = "Option::is_none")]
    pub cloud_box: Option<CloudBox>,
    /// What else to refetch: any of `boxes`, `providers`, `snapshots`, `spend`.
    #[serde(default, deserialize_with = "crate::de::lenient_strings")]
    pub scope: Vec<String>,
}
