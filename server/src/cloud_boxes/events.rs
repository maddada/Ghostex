//! `cloudBoxesChanged`: tells clients the registry changed.

use ghostex_gx_protocol::cloud_box::{CloudBox, CLOUD_BOXES_CHANGED_EVENT_TYPE};
use serde_json::json;

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::AppState;

/// One box changed: the whole new row (clients replace theirs).
pub(super) fn broadcast_box(state: &AppState, cloud_box: &CloudBox) {
    state.event_hub.broadcast(json!({
        "type": CLOUD_BOXES_CHANGED_EVENT_TYPE,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "box": cloud_box,
        "scope": ["boxes"],
    }));
}

/// Something else changed; clients refetch what `scope` names.
pub(super) fn broadcast_scope(state: &AppState, scope: &[&str]) {
    state.event_hub.broadcast(json!({
        "type": CLOUD_BOXES_CHANGED_EVENT_TYPE,
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "scope": scope,
    }));
}
