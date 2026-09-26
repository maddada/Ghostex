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

const KINDS: [&str; 4] = ["finished", "needsInput", "bell", "custom"];

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
    if let Some(agent_name) = non_empty("agentName") {
        item.insert("agentName".into(), Value::from(agent_name));
    }
    item.insert("body".into(), Value::from(text(raw, "body").unwrap_or("")));
    item.insert("createdAt".into(), Value::from(created_at));
    item.insert("id".into(), Value::from(id));
    item.insert("kind".into(), Value::from(kind));
    item.insert("projectId".into(), Value::from(project_id));
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
/// `{ items, nextUnreadId?, type, unreadCount }`. Anything that is not an object is the empty
/// feed, as in the TypeScript, which posted it.
pub fn notification_feed_state_message(result: &Value) -> Value {
    let raw = result.as_object();
    let items: Vec<Value> = raw
        .and_then(|raw| raw.get("items"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(normalize_item).collect())
        .unwrap_or_default();
    let unread_count = raw
        .and_then(|raw| raw.get("unreadCount"))
        .and_then(Value::as_f64)
        .filter(|count| count.is_finite() && *count >= 0.0)
        .map(|count| count.floor() as u64)
        .unwrap_or_else(|| {
            items
                .iter()
                .filter(|item| item["read"] != Value::Bool(true))
                .count() as u64
        });
    let next_unread_id = raw
        .and_then(|raw| text(raw, "nextUnreadId"))
        .filter(|id| !id.is_empty());
    let mut message = Map::new();
    message.insert("items".into(), Value::Array(items));
    if let Some(next_unread_id) = next_unread_id {
        message.insert("nextUnreadId".into(), Value::from(next_unread_id));
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
