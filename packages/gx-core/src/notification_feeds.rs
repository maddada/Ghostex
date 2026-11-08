//! The bell's one list across computers: this computer's notification feed and every remote
//! computer's, merged into the state message the bell and its panel read. Each daemon still owns
//! its rows, their read state and its own jump order (notification_feed.rs normalizes one
//! answer); this decides what the merged list shows, how it counts, which row a jump opens and
//! which computers an all-rows command on a tab goes to.
//!
//! CDXC:Notifications 2026-10-18 DECISION:
//! User (GitHub issue #257, picked on docs/2026-10-18/remote-notifications/mockup.html): "one bell" for every connected computer, not a second bell or a bell per computer. "Which tabs?": "all, this computer, remote". "Where should a remote row name its computer?": "chip by the title". "What happens when a remote computer goes offline?": "hide until back". So the badge counts unread rows from this computer and every online remote one; an offline computer's rows and count leave the bell until it reconnects, with one line saying so; Mark all read and Clear all act on the open tab (All = every online computer); Next unread spans every computer, newest undeferred unread first as each daemon orders its own; and read state stays on the computer that owns the row.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/notifications/ (reads every computer's feed and sends
//! each command to the owning one), apps/desktop/src/app/window/titlebar_notifications_panel.rs
//! (the tabs, the computer chip and the offline line), server/src/notification_feed/store.rs
//! (`nextUnreadDeferredAt`).

use std::cmp::Ordering;

use serde_json::{Map, Value};

use crate::keys::MachineId;
use crate::notification_feed::NOTIFICATION_FEED_STATE_MESSAGE_TYPE;

/// One computer's feed as the host holds it.
pub struct MachineNotificationFeed<'a> {
    pub machine: &'a MachineId,
    /// The computer's name as the sidebar shows it. Unused for this computer.
    pub machine_name: &'a str,
    /// Whether the computer can be reached now. This computer always can.
    pub online: bool,
    /// Its last state message (`notification_feed_state_message`); `None` before its first read.
    pub message: Option<&'a Value>,
}

/// The panel's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotificationFeedTab {
    #[default]
    All,
    ThisComputer,
    Remote,
}

impl NotificationFeedTab {
    pub const TABS: [Self; 3] = [Self::All, Self::ThisComputer, Self::Remote];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::ThisComputer => "This computer",
            Self::Remote => "Remote",
        }
    }

    /// Whether this tab shows rows of `machine`.
    pub fn includes(self, machine: &MachineId) -> bool {
        match self {
            Self::All => true,
            Self::ThisComputer => machine.is_local(),
            Self::Remote => !machine.is_local(),
        }
    }

    /// Whether this tab shows the offline line.
    ///
    /// CDXC:Notifications 2026-11-07 DECISION: User (the offline banner showed on every open of
    /// the panel because a Windows PC was off): "don't show this all the time man". A computer
    /// that is simply off is normal, so the line lives on the Remote tab only, never on All or
    /// This computer.
    pub fn shows_offline_notice(self) -> bool {
        self == Self::Remote
    }

    /// The computers Mark all read and Clear all go to from this tab: the online ones it shows.
    pub fn command_machines(self, feeds: &[MachineNotificationFeed<'_>]) -> Vec<MachineId> {
        feeds
            .iter()
            .filter(|feed| feed.online && self.includes(feed.machine))
            .map(|feed| feed.machine.clone())
            .collect()
    }
}

/// The computer a merged row (or the merged jump target) belongs to: `machineId` names a remote
/// computer, and its absence this one.
pub fn notification_feed_item_machine(item: &Value) -> MachineId {
    match item.get("machineId").and_then(Value::as_str) {
        Some(machine_id) if !machine_id.is_empty() => MachineId::Remote(machine_id.to_string()),
        _ => MachineId::Local,
    }
}

/// A computer's jump target, ranked against the others'.
struct JumpCandidate<'a> {
    machine: &'a MachineId,
    id: &'a str,
    created_at: &'a str,
    deferred_at: Option<&'a str>,
}

/// gxserver's own order (`read_notification_feed_state`), across computers: undeferred rows
/// first, newest first; then deferred rows, oldest deferral first. Ties keep the feed order, this
/// computer first.
fn jump_order(left: &JumpCandidate<'_>, right: &JumpCandidate<'_>) -> Ordering {
    match (left.deferred_at, right.deferred_at) {
        (None, None) => right.created_at.cmp(left.created_at),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left_at), Some(right_at)) => left_at.cmp(right_at),
    }
}

fn unread_count(message: &Value) -> u64 {
    message
        .get("unreadCount")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// "Windows is offline. …", or the plural for several computers.
fn offline_notice(names: &[&str]) -> Option<String> {
    let (last, rest) = names.split_last()?;
    if rest.is_empty() {
        return Some(format!(
            "{last} is offline. Its notifications show again when it reconnects."
        ));
    }
    Some(format!(
        "{} and {last} are offline. Their notifications show again when they reconnect.",
        rest.join(", ")
    ))
}

/// Every computer's feed as one state message:
/// `{ items, unreadCount, localUnreadCount, remoteUnreadCount, nextUnreadId?, nextUnreadMachineId?,
/// offlineNotice?, hasRemoteMachines, type }`. `items` holds the online computers' rows, newest
/// first, a remote row carrying `machineId` and `machineName`. An offline computer's rows and
/// count are left out; it gets the offline line only when its last-read feed holds rows (so
/// notifications really are waiting to come back). An offline computer with nothing pending, or
/// one whose feed was never read, gets no line.
pub fn merged_notification_feed_message(feeds: &[MachineNotificationFeed<'_>]) -> Value {
    let mut items: Vec<Value> = Vec::new();
    let mut local_unread = 0u64;
    let mut remote_unread = 0u64;
    let mut candidates: Vec<JumpCandidate<'_>> = Vec::new();
    let mut offline_names: Vec<&str> = Vec::new();
    for feed in feeds {
        let Some(message) = feed.message else {
            continue;
        };
        if !feed.online {
            let has_pending_rows = message
                .get("items")
                .and_then(Value::as_array)
                .is_some_and(|rows| !rows.is_empty());
            if !feed.machine.is_local() && has_pending_rows {
                offline_names.push(if feed.machine_name.is_empty() {
                    "A remote computer"
                } else {
                    feed.machine_name
                });
            }
            continue;
        }
        if feed.machine.is_local() {
            local_unread += unread_count(message);
        } else {
            remote_unread += unread_count(message);
        }
        let rows = message
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for row in rows {
            let mut row = row.clone();
            if let (Some(machine_id), Some(object)) =
                (feed.machine.remote_id(), row.as_object_mut())
            {
                object.insert("machineId".into(), Value::from(machine_id));
                object.insert("machineName".into(), Value::from(feed.machine_name));
            }
            items.push(row);
        }
        // A target beyond the rows the daemon listed has nothing to open, as on one computer.
        let next = message
            .get("nextUnreadId")
            .and_then(Value::as_str)
            .and_then(|next| {
                rows.iter()
                    .find(|row| row.get("id").and_then(Value::as_str) == Some(next))
                    .map(|row| (next, row))
            });
        if let Some((next, next_row)) = next {
            candidates.push(JumpCandidate {
                machine: feed.machine,
                id: next,
                created_at: next_row
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                deferred_at: message.get("nextUnreadDeferredAt").and_then(Value::as_str),
            });
        }
    }
    // Stable, so rows with the same time keep this computer first.
    items.sort_by(|left, right| {
        let at = |row: &Value| {
            row.get("createdAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        at(right).cmp(&at(left))
    });
    let next = candidates
        .iter()
        .enumerate()
        .min_by(|(left_index, left), (right_index, right)| {
            jump_order(left, right).then(left_index.cmp(right_index))
        })
        .map(|(_, candidate)| candidate);
    let mut merged = Map::new();
    merged.insert("items".into(), Value::Array(items));
    merged.insert(
        "unreadCount".into(),
        Value::from(local_unread + remote_unread),
    );
    merged.insert("localUnreadCount".into(), Value::from(local_unread));
    merged.insert("remoteUnreadCount".into(), Value::from(remote_unread));
    if let Some(next) = next {
        merged.insert("nextUnreadId".into(), Value::from(next.id));
        if let Some(machine_id) = next.machine.remote_id() {
            merged.insert("nextUnreadMachineId".into(), Value::from(machine_id));
        }
    }
    if let Some(notice) = offline_notice(&offline_names) {
        merged.insert("offlineNotice".into(), Value::from(notice));
    }
    merged.insert(
        "hasRemoteMachines".into(),
        Value::Bool(feeds.iter().any(|feed| !feed.machine.is_local())),
    );
    merged.insert(
        "type".into(),
        Value::from(NOTIFICATION_FEED_STATE_MESSAGE_TYPE),
    );
    Value::Object(merged)
}

/// The row a jump opens, from a merged message: its `nextUnreadId` on `nextUnreadMachineId`,
/// unless that row is the session the user is deferring away from.
pub fn merged_notification_feed_jump_target<'a>(
    message: &'a Value,
    deferred: Option<(&MachineId, &str)>,
) -> Option<&'a Value> {
    let next = message.get("nextUnreadId")?.as_str()?;
    let machine = match message.get("nextUnreadMachineId").and_then(Value::as_str) {
        Some(machine_id) => MachineId::Remote(machine_id.to_string()),
        None => MachineId::Local,
    };
    let item = message.get("items")?.as_array()?.iter().find(|item| {
        item["id"].as_str() == Some(next) && notification_feed_item_machine(item) == machine
    })?;
    let deferring = deferred.is_some_and(|(deferred_machine, deferred_session_id)| {
        *deferred_machine == machine && item["sessionId"].as_str() == Some(deferred_session_id)
    });
    (!deferring).then_some(item)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn row(id: &str) -> Value {
        json!({ "id": id, "createdAt": "2026-11-07T10:00:00Z" })
    }

    fn message(rows: Vec<Value>) -> Value {
        json!({ "items": rows, "unreadCount": 0 })
    }

    fn notice(feeds: &[MachineNotificationFeed<'_>]) -> Option<String> {
        merged_notification_feed_message(feeds)
            .get("offlineNotice")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn windows() -> MachineId {
        MachineId::Remote("windows".into())
    }

    #[test]
    fn offline_line_shows_only_on_the_remote_tab() {
        assert!(!NotificationFeedTab::All.shows_offline_notice());
        assert!(!NotificationFeedTab::ThisComputer.shows_offline_notice());
        assert!(NotificationFeedTab::Remote.shows_offline_notice());
    }

    #[test]
    fn offline_computer_with_waiting_rows_gets_the_line() {
        let local = message(vec![]);
        let remote = message(vec![row("a")]);
        let machine = windows();
        let feeds = [
            MachineNotificationFeed {
                machine: &MachineId::Local,
                machine_name: "",
                online: true,
                message: Some(&local),
            },
            MachineNotificationFeed {
                machine: &machine,
                machine_name: "Windows",
                online: false,
                message: Some(&remote),
            },
        ];
        assert_eq!(
            notice(&feeds).as_deref(),
            Some("Windows is offline. Its notifications show again when it reconnects.")
        );
    }

    #[test]
    fn offline_computer_with_nothing_waiting_gets_no_line() {
        let empty = message(vec![]);
        let machine = windows();
        let feeds = [MachineNotificationFeed {
            machine: &machine,
            machine_name: "Windows",
            online: false,
            message: Some(&empty),
        }];
        assert_eq!(notice(&feeds), None);
    }

    #[test]
    fn offline_computer_never_read_gets_no_line() {
        let machine = windows();
        let feeds = [MachineNotificationFeed {
            machine: &machine,
            machine_name: "Windows",
            online: false,
            message: None,
        }];
        assert_eq!(notice(&feeds), None);
    }

    #[test]
    fn online_computer_gets_no_line_and_only_waiting_offline_ones_are_named() {
        let rows = message(vec![row("a")]);
        let empty = message(vec![]);
        let (windows, mac_mini, laptop) = (
            windows(),
            MachineId::Remote("mini".into()),
            MachineId::Remote("laptop".into()),
        );
        let feeds = [
            MachineNotificationFeed {
                machine: &windows,
                machine_name: "Windows",
                online: false,
                message: Some(&rows),
            },
            MachineNotificationFeed {
                machine: &mac_mini,
                machine_name: "Mac mini",
                online: false,
                message: Some(&empty),
            },
            MachineNotificationFeed {
                machine: &laptop,
                machine_name: "Laptop",
                online: true,
                message: Some(&rows),
            },
        ];
        assert_eq!(
            notice(&feeds).as_deref(),
            Some("Windows is offline. Its notifications show again when it reconnects.")
        );
    }

    #[test]
    fn the_bell_counts_exactly_the_unread_rows_it_lists_on_every_computer() {
        // Each daemon counts a row this client drops (a security row from an older client's
        // point of view is the same shape: an unknown kind): the merged dot must still equal
        // the merged Unread group.
        let daemon_row = |id: &str, kind: &str, read: bool| {
            json!({
                "id": id, "projectId": "P1", "sessionId": format!("S-{id}"), "kind": kind,
                "createdAt": "2026-11-08T02:58:38.000Z", "read": read,
            })
        };
        let local = crate::notification_feed::notification_feed_state_message(&json!({
            "items": [daemon_row("s1", "security", false), daemon_row("x1", "somethingNewer", false),
                      daemon_row("d1", "finished", true)],
            "unreadCount": 2,
            "nextUnreadId": "x1",
        }));
        let remote = crate::notification_feed::notification_feed_state_message(&json!({
            "items": [daemon_row("x2", "somethingNewer", false), daemon_row("d2", "finished", true)],
            "unreadCount": 1,
            "nextUnreadId": "x2",
        }));
        let machine = windows();
        let feeds = [
            MachineNotificationFeed {
                machine: &MachineId::Local,
                machine_name: "",
                online: true,
                message: Some(&local),
            },
            MachineNotificationFeed {
                machine: &machine,
                machine_name: "Windows",
                online: true,
                message: Some(&remote),
            },
        ];
        let merged = merged_notification_feed_message(&feeds);
        let listed_unread: Vec<&str> = merged["items"]
            .as_array()
            .expect("items")
            .iter()
            .filter(|item| item["read"] != Value::Bool(true))
            .filter_map(|item| item["id"].as_str())
            .collect();
        assert_eq!(listed_unread, ["s1"]);
        assert_eq!(merged["unreadCount"], 1);
        assert_eq!(merged["localUnreadCount"], 1);
        assert_eq!(merged["remoteUnreadCount"], 0);
        // Neither daemon's jump target is a row the panel lists, so Next unread opens nothing
        // hidden.
        assert!(merged.get("nextUnreadId").is_none());
    }
}
