//! What the notification bell's panel and the three jump keys ask for, performed against gxserver
//! and then on the sidebar: mark read and focus the row's session, dismiss, jump to the next unread
//! one, or push the focused session's row to the back of the queue.
//!
//! CDXC:Notifications 2026-09-11 DECISION:
//! User: clicking a notification always takes you to that session and reveals it in the sidebar.
//! The row's raw ids are turned into the sidebar's project-scoped session id without requiring the row to be in the current list, so a session hidden by a Space, a filter, or a stale snapshot still gets focused; the reveal request then selects its Space, clears filters, and scrolls to it. The daemon drops rows for deleted sessions on read, so a missing session never strands a row.
//!
//! CDXC:Notifications 2026-09-25 WHY:
//! This was the QuickJS runtime's (`notification-feed.ts`), reached through a CustomEvent script;
//! the app runtime port moved it here unchanged: the same two endpoints, the same order (the read
//! state is written before the focus moves), the same focus path a sidebar row takes, and the same
//! reveal. The focus itself is the store's too since the runtime was deleted (focus_perform.rs).
//!
//! Every command goes to the gxserver that owns the row (feeds.rs routes it): this computer's or a
//! remote one's through its tunnel, so read state stays where the row lives. Mark all read and
//! Clear all go to every online computer the open tab shows; a jump reads every online computer's
//! feed and opens the merged next row (gx-core notification_feeds.rs).
//!
//! SEE-ALSO: packages/gx-core/src/notification_feed.rs, packages/gx-core/src/notification_feeds.rs,
//! apps/desktop/src/notification_feed/mod.rs.

use futures::future::join_all;
use ghostex_gx_core::{
    MachineId, NotificationFeedCommand, NotificationFeedTab, SessionKey,
    merged_notification_feed_jump_target, notification_feed_item_machine,
};
use serde_json::{Value, json};

use super::super::focus_perform::RowFocusOptions;
use super::feed::{read_notification_feed, update_notification_feed};
use crate::GhostexGpuiApp;

/// The row an `open` names, as the ids the focus and the reveal need.
#[derive(Clone)]
struct OpenTarget {
    machine: MachineId,
    notification_id: String,
    project_id: String,
    session_id: String,
    /// Set on account rows (banked reset warnings): open the account, not a session.
    account_id: Option<String>,
    /// Set on security rows: open Settings > Security, not a session.
    security: bool,
}

impl OpenTarget {
    fn from_message_item(item: &Value) -> Option<Self> {
        Some(Self {
            machine: notification_feed_item_machine(item),
            notification_id: item["id"].as_str()?.to_string(),
            project_id: item["projectId"].as_str()?.to_string(),
            session_id: item["sessionId"].as_str()?.to_string(),
            account_id: item["accountId"].as_str().map(str::to_string),
            security: item["securityEntry"].is_u64(),
        })
    }

    fn from_feed_item(item: &crate::notification_feed::GpuiNotificationFeedItem) -> Self {
        Self {
            machine: item.machine.clone(),
            notification_id: item.id.clone(),
            project_id: item.project_id.clone(),
            session_id: item.session_id.clone(),
            account_id: item.account_id.clone(),
            security: item.security_entry.is_some(),
        }
    }
}

impl GhostexGpuiApp {
    /// One command from the panel or a jump key. A row command names its row by computer and id,
    /// and without one does nothing; `tab` is the panel tab Mark all read and Clear all act on.
    pub(crate) fn gx_store_notification_feed_command(
        &mut self,
        command: NotificationFeedCommand,
        row: Option<(&MachineId, &str)>,
        tab: NotificationFeedTab,
        cx: &mut gpui::Context<Self>,
    ) {
        let row = row.filter(|(_, id)| !id.is_empty());
        if command.takes_notification_id() && row.is_none() {
            return;
        }
        match command {
            NotificationFeedCommand::Open => {
                let target = row.and_then(|(machine, id)| {
                    self.notification_feed_state
                        .items
                        .iter()
                        .find(|item| item.id == id && &item.machine == machine)
                        .map(OpenTarget::from_feed_item)
                });
                if let Some(target) = target {
                    self.gx_store_open_notification(target, cx);
                }
            }
            NotificationFeedCommand::JumpToLatestUnread => {
                let reads = self.gx_store_read_every_notification_feed();
                cx.spawn(async move |this, cx| {
                    let refreshed = join_all(reads).await;
                    let _ = this.update(cx, |this, cx| {
                        for (machine, message) in refreshed {
                            if let Some(message) = message {
                                this.gx_store.notifications.hold(machine, message);
                            }
                        }
                        this.gx_store_publish_notification_feeds(cx);
                        // The runtime opened from the state it held after the read, which is the
                        // previous one when a read failed.
                        if let Some(target) = this.gx_store_next_unread_notification() {
                            this.gx_store_open_notification(target, cx);
                        }
                    });
                })
                .detach();
            }
            NotificationFeedCommand::DeferAndJumpNext => self.gx_store_defer_notification(cx),
            NotificationFeedCommand::MarkAllRead | NotificationFeedCommand::ClearAll => {
                let Some(action) = command.update_action() else {
                    return;
                };
                for machine in self.gx_store_notification_machines_on(tab) {
                    self.gx_store_update_notification_feed(
                        machine,
                        json!({ "action": action }),
                        cx,
                    );
                }
            }
            _ => {
                let (Some(action), Some((machine, id))) = (command.update_action(), row) else {
                    return;
                };
                self.gx_store_update_notification_feed(
                    machine.clone(),
                    json!({ "action": action, "notificationId": id }),
                    cx,
                );
            }
        }
    }

    /// One read per online computer, each answering with its computer.
    fn gx_store_read_every_notification_feed(
        &self,
    ) -> Vec<impl std::future::Future<Output = (MachineId, Option<Value>)> + use<>> {
        self.gx_store_online_notification_machines()
            .into_iter()
            .filter_map(|machine| {
                let route = self.gx_store_notification_feed_route(&machine)?;
                Some(async move { (machine, read_notification_feed(route).await) })
            })
            .collect()
    }

    fn gx_store_update_notification_feed(
        &mut self,
        machine: MachineId,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(route) = self.gx_store_notification_feed_route(&machine) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let Some(message) = update_notification_feed(route, params).await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_store_notification_feed(machine, message, cx);
            });
        })
        .detach();
    }

    /// "I am not done with this one; come back to it last." The focused session's newest row goes
    /// to the back of its computer's unread queue, then the jump opens whatever is now next across
    /// every computer, unless that is the same session.
    fn gx_store_defer_notification(&mut self, cx: &mut gpui::Context<Self>) {
        let deferred = self
            .gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .and_then(|session| {
                let route = self.gx_store_notification_feed_route(&session.machine)?;
                Some((session.machine.clone(), session.session_id.clone(), route))
            });
        let Some((machine, session_id, route)) = deferred else {
            // Nothing to defer: jump from every computer's fresh feed.
            self.gx_store_notification_feed_command(
                NotificationFeedCommand::JumpToLatestUnread,
                None,
                NotificationFeedTab::All,
                cx,
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            let message = update_notification_feed(
                route,
                json!({ "action": "deferUnread", "sessionId": session_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                // A failed deferral does nothing more.
                let Some(message) = message else {
                    return;
                };
                let merged = this.gx_store_store_notification_feed(machine.clone(), message, cx);
                let target =
                    merged_notification_feed_jump_target(&merged, Some((&machine, &session_id)))
                        .and_then(OpenTarget::from_message_item);
                if let Some(target) = target {
                    this.gx_store_open_notification(target, cx);
                }
            });
        })
        .detach();
    }

    /// The row the merged feed names next, from the state the bell holds.
    fn gx_store_next_unread_notification(&self) -> Option<OpenTarget> {
        let state = &self.notification_feed_state;
        let next = state.next_unread_id.as_ref()?;
        state
            .items
            .iter()
            .find(|item| &item.id == next && item.machine == state.next_unread_machine)
            .map(OpenTarget::from_feed_item)
    }

    /// Marks the row read on its computer, then focuses its session and reveals it in the sidebar,
    /// the same way a sidebar click on that session does (a remote one opens on its computer). An
    /// account row on this computer opens its account instead; a remote computer's accounts are
    /// not this app's to open, so its account rows are only marked read.
    fn gx_store_open_notification(&mut self, target: OpenTarget, cx: &mut gpui::Context<Self>) {
        let Some(route) = self.gx_store_notification_feed_route(&target.machine) else {
            return;
        };
        let mark_read = json!({
            "action": "markRead",
            "notificationId": target.notification_id,
        });
        // A security row opens this computer's Settings > Security; a remote computer's is only
        // marked read (its log is read on that computer).
        if target.security {
            let machine = target.machine;
            cx.spawn(async move |this, cx| {
                let marked = update_notification_feed(route, mark_read).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(message) = marked {
                        this.gx_store_store_notification_feed(machine.clone(), message, cx);
                    }
                    if machine.is_local() {
                        this.open_gpui_settings_tab("security", None, cx);
                    }
                });
            })
            .detach();
            return;
        }
        if let Some(account_id) = target.account_id {
            let machine = target.machine;
            cx.spawn(async move |this, cx| {
                let marked = update_notification_feed(route, mark_read).await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(message) = marked {
                        this.gx_store_store_notification_feed(machine.clone(), message, cx);
                    }
                    if machine.is_local() {
                        this.open_account_notification(&account_id, cx);
                    }
                });
            })
            .detach();
            return;
        }
        // The raw ids become the sidebar's session id: the session's CURRENT project when the
        // daemon lists it, else the row's own, so a session the list does not draw still resolves.
        let session = self
            .gx_store
            .core
            .presentation()
            .loaded(&target.machine)
            .and_then(|loaded| {
                loaded
                    .server_sessions()
                    .find(|session| session.session_id == target.session_id)
                    .map(|session| SessionKey {
                        machine: target.machine.clone(),
                        project_id: session.project_id.clone(),
                        session_id: session.session_id.clone(),
                    })
            })
            .unwrap_or_else(|| SessionKey {
                machine: target.machine.clone(),
                project_id: target.project_id.clone(),
                session_id: target.session_id.clone(),
            });
        let machine = target.machine;
        cx.spawn(async move |this, cx| {
            let marked = update_notification_feed(route, mark_read).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(message) = marked {
                    this.gx_store_store_notification_feed(machine.clone(), message, cx);
                }
                // CDXC:Notifications 2026-10-18 DECISION:
                // User: "when i click on a notification from another machine please switch the sidebar to show that other machine". The switch, the focus and the reveal are one shared step (gx_store/activation_focus.rs) that an OS banner's click takes too (issue #401: the banner used to focus only).
                this.gx_store_show_session_on_its_machine(&session, RowFocusOptions::default(), cx);
            });
        })
        .detach();
    }
}
