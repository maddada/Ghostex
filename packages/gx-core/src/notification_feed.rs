//! The notification feed as the host keeps it: the daemon's answer normalized into the one message
//! the bell and its panel read, and the command vocabulary the panel and the jump keys send.
//!
//! CDXC:Notifications 2026-09-25 WHY:
//! gxserver owns the rows, their read state and the jump order; a client only reads, normalizes and
//! asks for updates. This is the normalizer the old QuickJS runtime ran on every read
//! (`normalizeNotificationFeedState` in packages/shared/notification-feed/notification-feed-contract.ts),
//! moved here so the desktop, the web build and mobile keep the same lenient reading: a row with a
//! missing id, project, session, time or an unknown kind is dropped rather than failing the feed.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/notifications/ (the desktop performs the reads),
//! apps/desktop/src/notification_feed/mod.rs (the bell, which parses this message strictly),
//! server/src/notification_feed/.

use serde_json::{Map, Value};

pub const NOTIFICATION_FEED_READ_ENDPOINT: &str = "/api/readNotificationFeed";
pub const NOTIFICATION_FEED_UPDATE_ENDPOINT: &str = "/api/updateNotificationFeed";

/// `type` of the state message the bell reads.
pub const NOTIFICATION_FEED_STATE_MESSAGE_TYPE: &str = "notificationFeedState";

/// `resetExpiring` and `resetRedeemed` are account rows (banked usage resets): they carry
/// `accountId`, and their `sessionId` names no session. `failed` is a session row for a run
/// that ended on an error (server/src/notification_feed/failure.rs). `security` is a security log
/// entry (server/src/audit/notify.rs): it carries `securityEntry`, and its `sessionId` names no
/// session either.
const KINDS: [&str; 9] = [
    "finished",
    "needsInput",
    "bell",
    "custom",
    "sendFailed",
    "failed",
    "resetExpiring",
    "resetRedeemed",
    "security",
];

/// What the panel and the jump keys ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationFeedCommand {
    Open,
    Dismiss,
    MarkRead,
    MarkUnread,
    MarkAllRead,
    ClearAll,
    JumpToLatestUnread,
    DeferAndJumpNext,
}

impl NotificationFeedCommand {
    pub fn from_action(action: &str) -> Option<Self> {
        Some(match action {
            "open" => Self::Open,
            "dismiss" => Self::Dismiss,
            "markRead" => Self::MarkRead,
            "markUnread" => Self::MarkUnread,
            "markAllRead" => Self::MarkAllRead,
            "clearAll" => Self::ClearAll,
            "jumpToLatestUnread" => Self::JumpToLatestUnread,
            "deferAndJumpNext" => Self::DeferAndJumpNext,
            _ => return None,
        })
    }

    /// The update endpoint's action for the commands that are one update and nothing else.
    pub fn update_action(self) -> Option<&'static str> {
        Some(match self {
            Self::Dismiss => "dismiss",
            Self::MarkRead => "markRead",
            Self::MarkUnread => "markUnread",
            Self::MarkAllRead => "markAllRead",
            Self::ClearAll => "clearAll",
            _ => return None,
        })
    }

    /// Whether the update names one row. `markAllRead` and `clearAll` take nothing, and a
    /// row command without a row id does nothing at all.
    pub fn takes_notification_id(self) -> bool {
        matches!(
            self,
            Self::Open | Self::Dismiss | Self::MarkRead | Self::MarkUnread
        )
    }
}

fn text<'a>(raw: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    raw.get(key).and_then(Value::as_str)
}

/// One row, or `None` when it lacks what a row needs. Key order is the TypeScript's.
fn normalize_item(value: &Value) -> Option<Value> {
    let raw = value.as_object()?;
    let non_empty = |key: &str| text(raw, key).filter(|value| !value.is_empty());
    let id = non_empty("id")?;
    let project_id = non_empty("projectId")?;
    let session_id = non_empty("sessionId")?;
    let created_at = non_empty("createdAt")?;
    let kind = non_empty("kind").filter(|kind| KINDS.contains(kind))?;
    let mut item = Map::new();
    if let Some(account_id) = non_empty("accountId") {
        item.insert("accountId".into(), Value::from(account_id));
    }
    if let Some(agent_name) = non_empty("agentName") {
        item.insert("agentName".into(), Value::from(agent_name));
    }
    item.insert("body".into(), Value::from(text(raw, "body").unwrap_or("")));
    item.insert("createdAt".into(), Value::from(created_at));
    item.insert("id".into(), Value::from(id));
    item.insert("kind".into(), Value::from(kind));
    if raw.get("awaitingApproval") == Some(&Value::Bool(true)) {
        item.insert("awaitingApproval".into(), Value::Bool(true));
    }
    item.insert("projectId".into(), Value::from(project_id));
    if let Some(entry) = raw.get("securityEntry").and_then(Value::as_u64) {
        item.insert("securityEntry".into(), Value::from(entry));
    }
    item.insert(
        "read".into(),
        Value::Bool(raw.get("read") == Some(&Value::Bool(true))),
    );
    item.insert("sessionId".into(), Value::from(session_id));
    item.insert(
        "subtitle".into(),
        Value::from(text(raw, "subtitle").unwrap_or("")),
    );
    item.insert(
        "title".into(),
        Value::from(text(raw, "title").unwrap_or("")),
    );
    Some(Value::Object(item))
}

/// The daemon's feed answer as the state message the bell reads:
/// `{ items, nextUnreadId?, nextUnreadDeferredAt?, type, unreadCount }`. Anything that is not an object is the empty
/// feed, as in the TypeScript, which posted it.
///
/// CDXC:Notifications 2026-11-08 DECISION:
/// User: "i'm seeing blue dot on the notification bell ... but when i open it none of the
/// notifications show blue border ... i want what's shown on the bell vs what's inside it to
/// match please!" The daemon's `unreadCount` counted its new `security` rows, which this
/// normalizer did not know and dropped, so the bell lit for rows the panel never listed. The
/// count is now the unread rows this message keeps, never the daemon's number: a row a client
/// cannot show (an unknown or newer kind, a malformed row) cannot light the bell, and reading or
/// clearing the rows the panel shows always clears it. The jump target is likewise dropped
/// unless it is one of those unread rows.
pub fn notification_feed_state_message(result: &Value) -> Value {
    let raw = result.as_object();
    let items: Vec<Value> = raw
        .and_then(|raw| raw.get("items"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(normalize_item).collect())
        .unwrap_or_default();
    let unread = |item: &&Value| item["read"] != Value::Bool(true);
    let unread_count = items.iter().filter(unread).count() as u64;
    let next_unread_id = raw
        .and_then(|raw| text(raw, "nextUnreadId"))
        .filter(|id| !id.is_empty())
        .filter(|id| {
            items
                .iter()
                .filter(unread)
                .any(|item| item["id"].as_str() == Some(id))
        });
    let mut message = Map::new();
    message.insert("items".into(), Value::Array(items));
    if let Some(next_unread_id) = next_unread_id {
        message.insert("nextUnreadId".into(), Value::from(next_unread_id));
        // Set when that row was pushed back; the merge across computers ranks it last
        // (notification_feeds.rs). Older daemons never send it.
        if let Some(deferred_at) = raw
            .and_then(|raw| text(raw, "nextUnreadDeferredAt"))
            .filter(|at| !at.is_empty())
        {
            message.insert("nextUnreadDeferredAt".into(), Value::from(deferred_at));
        }
    }
    message.insert(
        "type".into(),
        Value::from(NOTIFICATION_FEED_STATE_MESSAGE_TYPE),
    );
    message.insert("unreadCount".into(), Value::from(unread_count));
    Value::Object(message)
}

/// The row a jump opens, from a state message: the daemon's `nextUnreadId`, unless that row is
/// the session the user is deferring away from.
pub fn notification_feed_jump_target<'a>(
    message: &'a Value,
    deferred_session_id: Option<&str>,
) -> Option<&'a Value> {
    let next = message.get("nextUnreadId")?.as_str()?;
    let item = message
        .get("items")?
        .as_array()?
        .iter()
        .find(|item| item["id"].as_str() == Some(next))?;
    if deferred_session_id.is_some() && item["sessionId"].as_str() == deferred_session_id {
        return None;
    }
    Some(item)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn row(id: &str, kind: &str, read: bool) -> Value {
        json!({
            "id": id, "projectId": "P1", "sessionId": format!("S-{id}"), "kind": kind,
            "title": "Thread", "subtitle": "Project", "body": "The agent run ended with an error.",
            "agentName": "claude", "createdAt": "2026-10-31T12:00:00.000Z", "read": read,
        })
    }

    #[test]
    fn a_failed_row_survives_normalization_and_is_the_jump_target() {
        let message = notification_feed_state_message(&json!({
            "items": [row("f1", "failed", false), row("d1", "finished", true)],
            "unreadCount": 1,
            "nextUnreadId": "f1",
        }));
        let items = message["items"].as_array().expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["kind"], "failed");
        assert_eq!(items[0]["body"], "The agent run ended with an error.");
        assert_eq!(message["unreadCount"], 1);
        // The list, the count and the jump agree on the failed row.
        assert_eq!(
            notification_feed_jump_target(&message, None).map(|item| &item["id"]),
            Some(&json!("f1"))
        );
    }

    #[test]
    fn account_rows_keep_their_account_and_unknown_kinds_are_still_dropped() {
        let mut account = row("a1", "resetExpiring", false);
        account["accountId"] = json!("acct-1");
        let message = notification_feed_state_message(&json!({
            "items": [account, row("x1", "somethingNewer", false), row("r1", "resetRedeemed", true)],
        }));
        let items = message["items"].as_array().expect("items");
        assert_eq!(
            items
                .iter()
                .map(|item| item["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["resetExpiring", "resetRedeemed"]
        );
        assert_eq!(items[0]["accountId"], "acct-1");
        // The count is the kept unread rows: the unread row of an unknown kind is not one.
        assert_eq!(message["unreadCount"], 1);
    }

    /// The bell's dot and the panel's Unread group, from one daemon answer.
    fn dot_and_list(message: &Value) -> (u64, usize) {
        let unread_rows = message["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|item| item["read"] != Value::Bool(true))
            .count();
        (message["unreadCount"].as_u64().expect("count"), unread_rows)
    }

    #[test]
    fn a_security_row_is_listed_and_counted_so_the_dot_matches_the_list() {
        // The user's feed on 2026-11-08: the daemon counted two unread security rows that the
        // normalizer dropped, so the bell lit over a panel with nothing unread.
        let mut security = row("s1", "security", false);
        security["projectId"] = json!("security");
        security["sessionId"] = json!("security:12");
        security["securityEntry"] = json!(12);
        let mut older = row("s0", "security", false);
        older["sessionId"] = json!("security:11");
        let message = notification_feed_state_message(&json!({
            "items": [security, older, row("d1", "finished", true)],
            "unreadCount": 2,
            "nextUnreadId": "s1",
        }));
        let items = message["items"].as_array().expect("items");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["kind"], "security");
        assert_eq!(items[0]["securityEntry"], 12);
        assert_eq!(dot_and_list(&message), (2, 2));
        assert_eq!(
            notification_feed_jump_target(&message, None).map(|item| &item["id"]),
            Some(&json!("s1"))
        );
    }

    #[test]
    fn a_row_the_panel_cannot_show_never_lights_the_dot() {
        // A newer daemon's kind this client does not know: the daemon counts it unread and makes
        // it the jump target, but the panel cannot list it.
        let message = notification_feed_state_message(&json!({
            "items": [row("x1", "somethingNewer", false), row("d1", "finished", true)],
            "unreadCount": 1,
            "nextUnreadId": "x1",
        }));
        assert_eq!(dot_and_list(&message), (0, 0));
        assert!(message.get("nextUnreadId").is_none());
        // A daemon count above the rows it listed (rows past the read limit) is not trusted.
        let message = notification_feed_state_message(&json!({
            "items": [row("f1", "finished", false)],
            "unreadCount": 5,
            "nextUnreadId": "f1",
        }));
        assert_eq!(dot_and_list(&message), (1, 1));
        // Reading every listed row clears the dot whatever the daemon still says.
        let message = notification_feed_state_message(&json!({
            "items": [row("f1", "finished", true)],
            "unreadCount": 3,
        }));
        assert_eq!(dot_and_list(&message), (0, 0));
    }
}
