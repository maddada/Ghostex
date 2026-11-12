//! The two gxserver calls of the notification feed, returning the state message the bell reads.
//! Uses only `gx_rpc` and gx-core, so the GPUI web build can compile this file unchanged.

use ghostex_gx_core::{
    NOTIFICATION_FEED_READ_ENDPOINT, NOTIFICATION_FEED_UPDATE_ENDPOINT,
    notification_feed_state_message,
};
use serde_json::{Value, json};

use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// `/api/readNotificationFeed` on this computer (`None`) or the remote one `remote` names,
/// normalized. `None` when the call failed: the bell keeps what it has, as the old runtime did.
pub(super) async fn read_notification_feed(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
) -> Option<Value> {
    gx_rpc(remote, NOTIFICATION_FEED_READ_ENDPOINT, json!({}))
        .await
        .ok()
        .map(|result| notification_feed_state_message(&result))
}

/// `/api/updateNotificationFeed` with `params` (`action`, and `notificationId` or `sessionId`) on
/// the computer that owns the rows, answered with its whole feed after the update. `None` when
/// the call failed.
pub(super) async fn update_notification_feed(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    params: Value,
) -> Option<Value> {
    gx_rpc(remote, NOTIFICATION_FEED_UPDATE_ENDPOINT, params)
        .await
        .ok()
        .map(|result| notification_feed_state_message(&result))
}

/// `/api/removePairedDevice` for the device a bell row's Remove names, on the computer that owns the
/// row, answered with that computer's feed after it (the row's Remove is gone with the device).
/// `None` when either call failed.
pub(super) async fn remove_notification_device(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    device_id: String,
) -> Option<Value> {
    gx_rpc(
        remote.clone(),
        "/api/removePairedDevice",
        json!({ "deviceId": device_id }),
    )
    .await
    .ok()?;
    read_notification_feed(remote).await
}
