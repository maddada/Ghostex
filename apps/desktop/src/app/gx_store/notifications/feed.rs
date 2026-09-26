//! The two gxserver calls of the notification feed, returning the state message the bell reads.
//! Uses only `gx_rpc` and gx-core, so the GPUI web build can compile this file unchanged.

use ghostex_gx_core::{
    NOTIFICATION_FEED_READ_ENDPOINT, NOTIFICATION_FEED_UPDATE_ENDPOINT,
    notification_feed_state_message,
};
use serde_json::{Value, json};

use crate::app::gx_store::gx_rpc;

/// `/api/readNotificationFeed`, normalized. `None` when the call failed: the bell keeps what it
/// has, as the old runtime did.
pub(super) async fn read_notification_feed() -> Option<Value> {
    gx_rpc(None, NOTIFICATION_FEED_READ_ENDPOINT, json!({}))
        .await
        .ok()
        .map(|result| notification_feed_state_message(&result))
}

/// `/api/updateNotificationFeed` with `params` (`action`, and `notificationId` or `sessionId`),
/// answered with the whole feed after the update. `None` when the call failed.
pub(super) async fn update_notification_feed(params: Value) -> Option<Value> {
    gx_rpc(None, NOTIFICATION_FEED_UPDATE_ENDPOINT, params)
        .await
        .ok()
        .map(|result| notification_feed_state_message(&result))
}
