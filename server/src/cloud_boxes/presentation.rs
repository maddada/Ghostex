//! `PresentationSession.cloudBox`: the ☁ badge's facts on a session row (Q1015).
//!
//! A box's own gxserver publishes it for its sessions (`runtimeSettings.cloudBox` written by
//! `box_mode/prepare_session.rs`), and home gxserver for a draft whose tray picked Cloud Box
//! (`runtimeSettings.cloudBox.pending`). The projection reads the session row only, never a file
//! or the registry: clients take the box's state and name from the registry (`list`,
//! `cloudBoxesChanged`), which knows a stopped box that publishes nothing.
//!
//! SEE-ALSO: packages/gx-protocol/src/cloud_box/settings.rs (`PresentationCloudBox`),
//! packages/gx-core/src/cloud_boxes.rs (the merge and the badge).

use ghostex_gx_protocol::cloud_box::{
    CloudBoxState, DraftCloudBox, PresentationCloudBox, SessionCloudBoxRuntime,
    CLOUD_BOX_RUNTIME_SETTINGS_KEY,
};
use serde_json::Value;

/// The `cloudBox` field of a session's presentation row, or `None` for a session that neither
/// runs on a box nor waits to start one.
pub(crate) fn presentation_cloud_box_value(session: &Value) -> Option<Value> {
    let record = session
        .get("runtimeSettings")?
        .get(CLOUD_BOX_RUNTIME_SETTINGS_KEY)?;
    if record.get("pending").and_then(Value::as_bool) == Some(true) {
        let draft: DraftCloudBox = serde_json::from_value(record.clone()).ok()?;
        let presentation = PresentationCloudBox {
            box_id: draft.box_id.unwrap_or_default(),
            provider: draft.provider?,
            state: CloudBoxState::Pending,
            box_name: String::new(),
            home_project_id: None,
            pending: true,
        };
        return serde_json::to_value(presentation).ok();
    }
    let runtime: SessionCloudBoxRuntime = serde_json::from_value(record.clone()).ok()?;
    if runtime.box_id.is_empty() {
        return None;
    }
    let presentation = PresentationCloudBox {
        box_id: runtime.box_id,
        provider: runtime.provider,
        // The box's gxserver is answering, so the box is up.
        state: CloudBoxState::Running,
        box_name: String::new(),
        home_project_id: runtime.home_project_id,
        pending: false,
    };
    serde_json::to_value(presentation).ok()
}
