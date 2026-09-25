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
//! SEE-ALSO: packages/gx-core/src/notification_feed.rs, apps/desktop/src/notification_feed/mod.rs.

use ghostex_gx_core::{
    MachineId, NotificationFeedCommand, SessionKey, notification_feed_jump_target,
};
use serde_json::{Value, json};

use super::feed::{read_notification_feed, update_notification_feed};
use crate::GhostexGpuiApp;

/// The row an `open` names, as the ids the focus and the reveal need.
#[derive(Clone)]
struct OpenTarget {
    notification_id: String,
    project_id: String,
    session_id: String,
}

impl OpenTarget {
    fn from_message_item(item: &Value) -> Option<Self> {
        Some(Self {
            notification_id: item["id"].as_str()?.to_string(),
            project_id: item["projectId"].as_str()?.to_string(),
            session_id: item["sessionId"].as_str()?.to_string(),
        })
    }
}

impl GhostexGpuiApp {
    /// Reads the feed and hands it to the bell. Runs on every `notificationFeedChanged` frame and
    /// whenever this computer's stream goes live (`Effect::MachineLive`).
    pub(crate) fn gx_store_refresh_notification_feed(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(message) = read_notification_feed().await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.receive_notification_feed_state_message(&message, cx);
            });
        })
        .detach();
    }

    /// One command from the panel or a jump key. A row command without a row id does nothing.
    pub(crate) fn gx_store_notification_feed_command(
        &mut self,
        command: NotificationFeedCommand,
        notification_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let notification_id = notification_id.filter(|id| !id.is_empty());
        if command.takes_notification_id() && notification_id.is_none() {
            return;
        }
        match command {
            NotificationFeedCommand::Open => {
                let target = self
                    .notification_feed_state
                    .items
                    .iter()
                    .find(|item| Some(item.id.as_str()) == notification_id)
                    .map(|item| OpenTarget {
                        notification_id: item.id.clone(),
                        project_id: item.project_id.clone(),
                        session_id: item.session_id.clone(),
                    });
                if let Some(target) = target {
                    self.gx_store_open_notification(target, cx);
                }
            }
            NotificationFeedCommand::JumpToLatestUnread => {
                cx.spawn(async move |this, cx| {
                    let refreshed = read_notification_feed().await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(message) = &refreshed {
                            this.receive_notification_feed_state_message(message, cx);
                        }
                        // The runtime opened from the state it held after the read, which is the
                        // previous one when the read failed.
                        if let Some(target) = this.gx_store_next_unread_notification() {
                            this.gx_store_open_notification(target, cx);
                        }
                    });
                })
                .detach();
            }
            NotificationFeedCommand::DeferAndJumpNext => self.gx_store_defer_notification(cx),
            _ => {
                let Some(action) = command.update_action() else {
                    return;
                };
                let params = match notification_id {
                    Some(id) => json!({ "action": action, "notificationId": id }),
                    None => json!({ "action": action }),
                };
                self.gx_store_update_notification_feed(params, cx);
            }
        }
    }

    fn gx_store_update_notification_feed(&mut self, params: Value, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(message) = update_notification_feed(params).await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.receive_notification_feed_state_message(&message, cx);
            });
        })
        .detach();
    }

    /// "I am not done with this one; come back to it last." The focused session's newest row goes
    /// to the back of the unread queue, then the jump opens whatever the daemon now says is next,
    /// unless that is the same session.
    fn gx_store_defer_notification(&mut self, cx: &mut gpui::Context<Self>) {
        let deferred = self
            .gx_store
            .core
            .focus()
            .focused_session
            .as_ref()
            .filter(|session| session.machine == MachineId::Local)
            .map(|session| session.session_id.clone());
        cx.spawn(async move |this, cx| {
            let message = match &deferred {
                Some(session_id) => {
                    update_notification_feed(
                        json!({ "action": "deferUnread", "sessionId": session_id }),
                    )
                    .await
                }
                None => read_notification_feed().await,
            };
            let _ = this.update(cx, |this, cx| {
                let target = match (&deferred, message) {
                    (_, Some(message)) => {
                        this.receive_notification_feed_state_message(&message, cx);
                        notification_feed_jump_target(&message, deferred.as_deref())
                            .and_then(OpenTarget::from_message_item)
                    }
                    // A failed deferral does nothing more; a failed read jumps from the state the
                    // bell already holds, as the runtime did.
                    (Some(_), None) => return,
                    (None, None) => this.gx_store_next_unread_notification(),
                };
                if let Some(target) = target {
                    this.gx_store_open_notification(target, cx);
                }
            });
        })
        .detach();
    }

    /// The row the daemon named next, from the state the bell holds.
    fn gx_store_next_unread_notification(&self) -> Option<OpenTarget> {
        let state = &self.notification_feed_state;
        let next = state.next_unread_id.as_ref()?;
        state
            .items
            .iter()
            .find(|item| &item.id == next)
            .map(|item| OpenTarget {
                notification_id: item.id.clone(),
                project_id: item.project_id.clone(),
                session_id: item.session_id.clone(),
            })
    }

    /// Marks the row read, then focuses its session and reveals it in the sidebar.
    fn gx_store_open_notification(&mut self, target: OpenTarget, cx: &mut gpui::Context<Self>) {
        // The raw ids become the sidebar's session id: the session's CURRENT project when the
        // daemon lists it, else the row's own, so a session the list does not draw still resolves.
        let session = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)
            .and_then(|loaded| {
                loaded
                    .server_sessions()
                    .find(|session| session.session_id == target.session_id)
                    .map(|session| SessionKey::local(&session.project_id, &session.session_id))
            })
            .unwrap_or_else(|| SessionKey::local(&target.project_id, &target.session_id));
        let sidebar_session_id = session.to_sidebar_session_id();
        cx.spawn(async move |this, cx| {
            let marked = update_notification_feed(json!({
                "action": "markRead",
                "notificationId": target.notification_id,
            }))
            .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(message) = marked {
                    this.receive_notification_feed_state_message(&message, cx);
                }
                // The same focus a sidebar row click ends in (gx_store/focus_perform.rs).
                this.gx_store_focus_activated_session(&sidebar_session_id, cx);
                let request_id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_micros() as u64);
                this.gx_store_note_local_sidebar_reveal(&sidebar_session_id, request_id);
                this.gx_store_note_sidebar_reveal(&sidebar_session_id, request_id, cx);
                this.gx_store_sidebar_state_changed(cx);
            });
        })
        .detach();
    }
}
