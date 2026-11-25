//! Every computer's notification feed as this app holds it, and the one merged list the bell
//! draws (gx-core `merged_notification_feed_message` decides what it shows, counts and jumps to).
//!
//! A computer's feed is read when its stream goes live (`Effect::MachineLive`) and whenever its
//! daemon announces `notificationFeedChanged`; the list is merged again when a feed lands and
//! when a remote computer goes offline, comes back or is removed from the sidebar. An offline
//! computer's last feed is kept, so its rows come back as they were the moment it reconnects,
//! before the read that refreshes them answers.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

use ghostex_gx_core::{
    ConnectionPhase, MachineId, MachineNotificationFeed, NotificationFeedTab,
    merged_notification_feed_message,
};
use serde_json::Value;

use super::feed::read_notification_feed;
use crate::GhostexGpuiApp;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

#[derive(Default)]
pub(crate) struct NotificationFeedsHost {
    /// Each computer's last state message.
    feeds: BTreeMap<MachineId, Value>,
    /// The remote computers in the sidebar and whether each was online, as the last merge saw
    /// them: a store change that moves neither merges nothing.
    remote_machines: Vec<(MachineId, bool)>,
    /// Per-machine mutation epoch, bumped whenever a local optimistic seen/read mutation begins so
    /// an earlier in-flight `readNotificationFeed` or `markAllSeen` response cannot overwrite it.
    epochs: BTreeMap<MachineId, u64>,
    /// Rows `(id, kind)` already seen in the bell panel on each machine (removed on explicit
    /// `markUnread`, when the row disappears, or when the server changes its `kind` such as
    /// `finished` -> `needsInput`). Overlaid during merge so in-flight refreshes, multi-machine
    /// merges, or older daemons predating `markAllSeen` never flash the badge back.
    optimistically_seen: BTreeMap<MachineId, HashSet<(String, String)>>,
    /// Row ids currently being marked read on each machine (while `markShownRead` or its per-row
    /// `markRead` fallback loop is in flight), so intermediate `notificationFeedChanged` refreshes
    /// keep those rows marked `read: true, seen: true` with no count flash between rows.
    pending_mark_read: BTreeMap<MachineId, HashSet<String>>,
    /// Count of in-flight `markShownRead` / fallback `markRead` requests per machine.
    in_flight_mark_read: BTreeMap<MachineId, usize>,
}

impl NotificationFeedsHost {
    pub(crate) fn epoch(&self, machine: &MachineId) -> u64 {
        self.epochs.get(machine).copied().unwrap_or(0)
    }

    pub(crate) fn bump_epoch(&mut self, machine: &MachineId) -> u64 {
        let next = self.epoch(machine).wrapping_add(1);
        self.epochs.insert(machine.clone(), next);
        next
    }

    pub(crate) fn mark_item_seen(&mut self, machine: &MachineId, id: String, kind: &str) {
        self.optimistically_seen
            .entry(machine.clone())
            .or_default()
            .insert((id, kind.to_string()));
    }

    pub(crate) fn clear_item_seen(&mut self, machine: &MachineId, id: &str) {
        if let Some(seen) = self.optimistically_seen.get_mut(machine) {
            seen.retain(|(row_id, _)| row_id != id);
            if seen.is_empty() {
                self.optimistically_seen.remove(machine);
            }
        }
        if let Some(pending) = self.pending_mark_read.get_mut(machine) {
            pending.remove(id);
            if pending.is_empty() {
                self.pending_mark_read.remove(machine);
            }
        }
    }

    pub(super) fn begin_mark_read(&mut self, machine: &MachineId, ids: &[String]) -> u64 {
        let epoch = self.bump_epoch(machine);
        let pending = self.pending_mark_read.entry(machine.clone()).or_default();
        for id in ids {
            pending.insert(id.clone());
        }
        *self.in_flight_mark_read.entry(machine.clone()).or_insert(0) += 1;
        epoch
    }

    pub(super) fn finish_mark_read(&mut self, machine: &MachineId) {
        let remaining = match self.in_flight_mark_read.get_mut(machine) {
            Some(count) => {
                *count = count.saturating_sub(1);
                *count
            }
            None => 0,
        };
        if remaining == 0 {
            self.in_flight_mark_read.remove(machine);
            self.pending_mark_read.remove(machine);
        }
    }

    pub(super) fn has_in_flight_mark_read(&self, machine: &MachineId) -> bool {
        self.in_flight_mark_read.get(machine).copied().unwrap_or(0) > 0
    }

    /// Keeps a computer's fresh feed without merging; the caller merges once for several.
    pub(super) fn hold(&mut self, machine: MachineId, message: Value) {
        let in_flight = self.has_in_flight_mark_read(&machine);
        if let Some(seen) = self.optimistically_seen.get_mut(&machine) {
            if let Some(items) = message.get("items").and_then(Value::as_array) {
                seen.retain(|(id, kind)| {
                    items.iter().any(|item| {
                        item.get("id").and_then(Value::as_str) == Some(id.as_str())
                            && item.get("kind").and_then(Value::as_str) == Some(kind.as_str())
                            && (in_flight || item.get("read") != Some(&Value::Bool(true)))
                    })
                });
            }
            if seen.is_empty() {
                self.optimistically_seen.remove(&machine);
            }
        }
        self.feeds.insert(machine, message);
    }

    fn overlaid_feed(&self, machine: &MachineId) -> Option<Cow<'_, Value>> {
        let raw = self.feeds.get(machine)?;
        let seen = self.optimistically_seen.get(machine);
        let pending_read = self.pending_mark_read.get(machine);
        if seen.is_none_or(HashSet::is_empty) && pending_read.is_none_or(HashSet::is_empty) {
            return Some(Cow::Borrowed(raw));
        }
        let mut message = raw.clone();
        let next_unread_id = message
            .get("nextUnreadId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut counts: Option<(u64, u64, bool)> = None;
        if let Some(items) = message.get_mut("items").and_then(Value::as_array_mut) {
            let mut changed = false;
            for item in items.iter_mut() {
                let Some(object) = item.as_object_mut() else {
                    continue;
                };
                let id = object.get("id").and_then(Value::as_str).unwrap_or_default();
                let kind = object
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let awaiting_approval = object.get("awaitingApproval") == Some(&Value::Bool(true));
                if !awaiting_approval && pending_read.is_some_and(|ids| ids.contains(id)) {
                    if object.get("read") != Some(&Value::Bool(true)) {
                        object.insert("read".into(), Value::Bool(true));
                        changed = true;
                    }
                    if object.get("seen") != Some(&Value::Bool(true)) {
                        object.insert("seen".into(), Value::Bool(true));
                        changed = true;
                    }
                } else if seen.is_some_and(|rows| {
                    rows.iter()
                        .any(|(seen_id, seen_kind)| seen_id == id && seen_kind == kind)
                }) && object.get("seen") != Some(&Value::Bool(true))
                {
                    object.insert("seen".into(), Value::Bool(true));
                    changed = true;
                }
            }
            if changed {
                let unread = |item: &&Value| item.get("read") != Some(&Value::Bool(true));
                let unseen =
                    |item: &&Value| unread(item) && item.get("seen") != Some(&Value::Bool(true));
                let unread_count = items.iter().filter(unread).count() as u64;
                let unseen_count = items.iter().filter(unseen).count() as u64;
                let next_still_unread = next_unread_id.as_deref().is_some_and(|next| {
                    items
                        .iter()
                        .filter(unread)
                        .any(|item| item.get("id").and_then(Value::as_str) == Some(next))
                });
                counts = Some((unread_count, unseen_count, next_still_unread));
            }
        }
        if let Some((unread_count, unseen_count, next_still_unread)) = counts
            && let Some(object) = message.as_object_mut()
        {
            object.insert("unreadCount".into(), Value::from(unread_count));
            object.insert("unseenCount".into(), Value::from(unseen_count));
            if !next_still_unread {
                object.remove("nextUnreadId");
                object.remove("nextUnreadDeferredAt");
            }
        }
        Some(Cow::Owned(message))
    }
}

/// Where a computer's feed calls go: `Some(None)` is this computer, `Some(Some(_))` a remote one
/// through its tunnel, and `None` a computer that cannot be reached now.
pub(super) type NotificationFeedRoute = Option<Option<GpuiRemoteGxserverRequestTarget>>;

impl GhostexGpuiApp {
    pub(crate) fn gx_store_mark_notification_item_seen(
        &mut self,
        machine: &MachineId,
        id: String,
        kind: &str,
    ) {
        self.gx_store
            .notifications
            .mark_item_seen(machine, id, kind);
    }

    pub(super) fn gx_store_notification_feed_route(
        &self,
        machine: &MachineId,
    ) -> NotificationFeedRoute {
        let Some(machine_id) = machine.remote_id() else {
            return Some(None);
        };
        if !self.gx_store_notification_machine_online(machine) {
            return None;
        }
        self.remote_gxserver_connections
            .get(machine_id)
            .map(|connection| Some(connection.request_target()))
    }

    /// A remote computer is online while its stream is live and its tunnel is up: the same "live"
    /// that keeps its sidebar rows from drawing stale.
    fn gx_store_notification_machine_online(&self, machine: &MachineId) -> bool {
        let Some(machine_id) = machine.remote_id() else {
            return true;
        };
        self.remote_gxserver_connections.contains_key(machine_id)
            && self
                .gx_store
                .core
                .presentation()
                .machine(machine)
                .is_some_and(|entry| entry.connection().phase == ConnectionPhase::Live)
    }

    /// The remote computers the sidebar has tabs for, in its order, with their names.
    fn gx_store_notification_remote_machines(&self) -> Vec<(MachineId, String)> {
        self.gx_store
            .remote
            .tabs()
            .iter()
            .filter(|tab| !tab.is_local())
            .map(|tab| (MachineId::Remote(tab.machine_id.clone()), tab.label.clone()))
            .collect()
    }

    /// Every computer that can be reached now: this one first, then the online remote ones.
    pub(super) fn gx_store_online_notification_machines(&self) -> Vec<MachineId> {
        self.gx_store_notification_machines_on(NotificationFeedTab::All)
    }

    /// The computers an all-rows command on `tab` goes to (gx-core decides which).
    pub(super) fn gx_store_notification_machines_on(
        &self,
        tab: NotificationFeedTab,
    ) -> Vec<MachineId> {
        let machines = self.gx_store_notification_machine_list();
        let feeds: Vec<MachineNotificationFeed<'_>> = machines
            .iter()
            .map(|(machine, name, online)| MachineNotificationFeed {
                machine,
                machine_name: name,
                online: *online,
                message: None,
            })
            .collect();
        tab.command_machines(&feeds)
    }

    /// This computer, then every remote computer in the sidebar, with its name and whether it is
    /// online.
    fn gx_store_notification_machine_list(&self) -> Vec<(MachineId, String, bool)> {
        let mut machines = vec![(MachineId::Local, String::new(), true)];
        machines.extend(
            self.gx_store_notification_remote_machines()
                .into_iter()
                .map(|(machine, name)| {
                    let online = self.gx_store_notification_machine_online(&machine);
                    (machine, name, online)
                }),
        );
        machines
    }

    /// Reads one computer's feed and merges it into the bell. A computer that cannot be reached
    /// keeps what the bell has.
    pub(crate) fn gx_store_refresh_notification_feed(
        &mut self,
        machine: MachineId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(route) = self.gx_store_notification_feed_route(&machine) else {
            return;
        };
        let started_epoch = self.gx_store.notifications.epoch(&machine);
        cx.spawn(async move |this, cx| {
            let Some(message) = read_notification_feed(route).await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if this.gx_store.notifications.epoch(&machine) != started_epoch {
                    return;
                }
                this.gx_store_store_notification_feed(machine, message, cx);
            });
        })
        .detach();
    }

    /// One computer's fresh feed: kept, then the bell is merged again. Returns the merged message.
    pub(super) fn gx_store_store_notification_feed(
        &mut self,
        machine: MachineId,
        message: Value,
        cx: &mut gpui::Context<Self>,
    ) -> Value {
        self.gx_store.notifications.hold(machine, message);
        self.gx_store_publish_notification_feeds(cx)
    }

    /// Merges every held feed into the bell's state, dropping the feeds of computers removed from
    /// the sidebar. Returns the merged message.
    pub(crate) fn gx_store_publish_notification_feeds(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Value {
        let machines = self.gx_store_notification_machine_list();
        let host = &mut self.gx_store.notifications;
        let listed = |machine: &MachineId| machines.iter().any(|(listed, ..)| listed == machine);
        host.feeds.retain(|machine, _| listed(machine));
        host.epochs.retain(|machine, _| listed(machine));
        host.optimistically_seen
            .retain(|machine, _| listed(machine));
        host.pending_mark_read.retain(|machine, _| listed(machine));
        host.in_flight_mark_read
            .retain(|machine, _| listed(machine));
        host.remote_machines = machines
            .iter()
            .filter(|(machine, ..)| !machine.is_local())
            .map(|(machine, _, online)| (machine.clone(), *online))
            .collect();
        let overlaid: Vec<Option<Cow<'_, Value>>> = machines
            .iter()
            .map(|(machine, ..)| host.overlaid_feed(machine))
            .collect();
        let feeds: Vec<MachineNotificationFeed<'_>> = machines
            .iter()
            .zip(overlaid.iter())
            .map(
                |((machine, name, online), message)| MachineNotificationFeed {
                    machine,
                    machine_name: name,
                    online: *online,
                    message: message.as_deref(),
                },
            )
            .collect();
        let merged = merged_notification_feed_message(&feeds);
        self.receive_notification_feed_state_message(&merged, cx);
        merged
    }

    /// The sidebar list is being brought up to date: merge again when a remote computer went
    /// offline, came back, or joined or left the sidebar. A tab removed in Settings moves no
    /// connection, so this compares the machines themselves rather than reading the changes.
    pub(in crate::app::gx_store) fn gx_store_notification_feeds_store_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machines: Vec<(MachineId, bool)> = self
            .gx_store_notification_machine_list()
            .into_iter()
            .filter(|(machine, ..)| !machine.is_local())
            .map(|(machine, _, online)| (machine, online))
            .collect();
        if remote_machines != self.gx_store.notifications.remote_machines {
            self.gx_store_publish_notification_feeds(cx);
        }
    }
}
