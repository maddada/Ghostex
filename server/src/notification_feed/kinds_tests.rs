//! Every feed kind gxserver writes is one each client knows.
//!
//! CDXC:Notifications 2026-11-08 WHY:
//! gxserver began writing `security` rows (143486257) that gx-core's normalizer did not know, so
//! it dropped them while the daemon still counted them, and the bell lit over a panel with nothing
//! unread. A kind must be added in four places: the `NOTIFICATION_FEED_KIND_*` constants in
//! store.rs, gx-core's `KINDS` (packages/gx-core/src/notification_feed.rs), the desktop's
//! `GpuiNotificationFeedKind::from_wire` (apps/desktop/src/notification_feed/mod.rs) and the
//! TypeScript contract's `NOTIFICATION_FEED_KINDS`. This test reads the server's constants from
//! store.rs, so a new one fails here until every client knows it.

use serde_json::json;

const STORE_RS: &str = include_str!("store.rs");
const DESKTOP_FEED_RS: &str = include_str!("../../../apps/desktop/src/notification_feed/mod.rs");
const TS_CONTRACT: &str =
    include_str!("../../../packages/shared/notification-feed/notification-feed-contract.ts");

/// Every `pub(crate) const NOTIFICATION_FEED_KIND_<NAME>: &str = "<kind>";` in store.rs.
fn server_kinds() -> Vec<&'static str> {
    STORE_RS
        .lines()
        .filter_map(|line| {
            let rest = line
                .trim()
                .strip_prefix("pub(crate) const NOTIFICATION_FEED_KIND_")?;
            let (_, value) = rest.split_once(": &str = \"")?;
            value.strip_suffix("\";")
        })
        .collect()
}

/// The quoted entries of the TypeScript `NOTIFICATION_FEED_KINDS` array.
fn ts_kinds() -> Vec<&'static str> {
    let start = TS_CONTRACT
        .find("export const NOTIFICATION_FEED_KINDS")
        .expect("NOTIFICATION_FEED_KINDS in the TypeScript contract");
    let array = &TS_CONTRACT[start..];
    let array = &array[array.find("= [").expect("array start")..];
    let array = &array[..array.find("];").expect("array end")];
    array.split('\'').skip(1).step_by(2).collect()
}

#[test]
fn the_kind_scan_finds_the_constants_it_guards() {
    // The scan reads store.rs as text: if its constants change shape, it must not pass on nothing.
    let kinds = server_kinds();
    use super::store::*;
    for kind in [
        NOTIFICATION_FEED_KIND_FINISHED,
        NOTIFICATION_FEED_KIND_NEEDS_INPUT,
        NOTIFICATION_FEED_KIND_BELL,
        NOTIFICATION_FEED_KIND_CUSTOM,
        NOTIFICATION_FEED_KIND_SEND_FAILED,
        NOTIFICATION_FEED_KIND_FAILED,
        NOTIFICATION_FEED_KIND_RESET_EXPIRING,
        NOTIFICATION_FEED_KIND_RESET_REDEEMED,
        NOTIFICATION_FEED_KIND_SECURITY,
    ] {
        assert!(kinds.contains(&kind), "{kind} missing from {kinds:?}");
    }
}

#[test]
fn every_kind_gxserver_writes_is_listed_by_every_client() {
    let ts = ts_kinds();
    let mut missing = Vec::new();
    for kind in server_kinds() {
        // gx-core: the row survives the normalizer, so the bell both lists and counts it.
        let message = ghostex_gx_core::notification_feed_state_message(&json!({
            "items": [{
                "id": "n1", "projectId": "P1", "sessionId": "S1", "kind": kind,
                "createdAt": "2026-11-08T00:00:00.000Z", "read": false,
            }],
        }));
        if message["items"].as_array().map(Vec::len) != Some(1) || message["unreadCount"] != 1 {
            missing.push(format!(
                "{kind}: gx-core KINDS (packages/gx-core/src/notification_feed.rs)"
            ));
        }
        if !DESKTOP_FEED_RS.contains(&format!("\"{kind}\" => Some(Self::")) {
            missing.push(format!(
                "{kind}: desktop GpuiNotificationFeedKind::from_wire (apps/desktop/src/notification_feed/mod.rs)"
            ));
        }
        if !ts.contains(&kind) {
            missing.push(format!(
                "{kind}: NOTIFICATION_FEED_KINDS (packages/shared/notification-feed/notification-feed-contract.ts)"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "gxserver writes feed kinds a client drops, so the bell would count rows its panel never \
         lists:\n{}",
        missing.join("\n")
    );
}
