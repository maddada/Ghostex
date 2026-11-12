//! The Notifications dropdown: the reading-panel variant behind the titlebar
//! bell. It renders the cached feed the Rust store pushed (see
//! `crate::notification_feed`), edits its own copy optimistically so clicks
//! feel instant, and sends one command per click back through the main app.
//! The rows are every online computer's (gx-core `notification_feeds.rs`): while more than one
//! computer is in the sidebar every row ends its title with its computer's icon, and the list
//! shows one dimmed line when an offline computer has rows waiting to come back.
//!
//! CDXC:Notifications 2026-11-12 DECISION:
//! User, of the panel's header and its All / This computer / Remote bar: "need these to be smaller width", "Mark all read <- needs to be shorter pls", "i dont like this bar here, i think let's remove it and just add a icon next to the title instead of the pc (like we have in app's sidebar)". On the options page they picked one "..." button whose click opens a menu with Mark all read and Clear all (Q2734 C), the computer's own icon right after the title as in a sidebar row, in place of the name chip (Q2735 B), and this computer's rows wearing its icon too, the sidebar's rule (Q2741 B). The tabs are gone, so both actions cover every row. The phone's list does the same (apps/mobile/app AttentionList.tsx).
use super::resources_style::*;
use crate::app::helpers::*;
use crate::notification_feed::{
    GpuiNotificationFeedItem, GpuiNotificationFeedState, notification_feed_relative_time,
};
use crate::*;
use ghostex_gx_core::{MachineId, NotificationFeedTab, SessionKey};

const NOTIFICATION_PANEL_BELL_ICON: &str = "titlebar/bell.svg";
const NOTIFICATION_PANEL_CHECK_ICON: &str = "titlebar/check.svg";
/// Same size as the Tips card check, floating in the card's bottom-right corner over the short footer line.
const NOTIFICATION_CARD_CHECK_SIZE: f32 = 22.0;
/// Title line height, so the line keeps one height whether or not it is hovered.
const NOTIFICATION_CARD_TITLE_LINE_HEIGHT: f32 = 18.0;
const NOTIFICATION_MORE_ICON: &str = "titlebar/dots.svg";
/// The sidebar row's computer icon size (native_sidebar/computer_mark.rs).
const NOTIFICATION_COMPUTER_ICON_SIZE: f32 = 12.0;
const NOTIFICATION_ACTIONS_MENU_WIDTH: f32 = 168.0;

impl GpuiTitlebarReadingPanel {
    pub(crate) fn notifications(
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        feed: GpuiNotificationFeedState,
    ) -> Self {
        Self {
            main_app,
            scroll_handle: ScrollHandle::new(),
            state: GpuiTitlebarReadingPanelState::Notifications {
                feed,
                hovered_id: None,
                actions_menu_open: false,
            },
        }
    }

    /// A fresh feed from the sidebar while the dropdown is open. The daemon's
    /// copy wins over any optimistic local edit, which is what keeps the list
    /// honest after a jump or an acknowledgement elsewhere.
    pub(super) fn update_notifications_feed(
        &mut self,
        next_feed: GpuiNotificationFeedState,
        cx: &mut gpui::Context<Self>,
    ) {
        let GpuiTitlebarReadingPanelState::Notifications { feed, .. } = &mut self.state else {
            return;
        };
        if *feed == next_feed {
            return;
        }
        *feed = next_feed;
        cx.notify();
    }

    fn notifications_feed_mut(&mut self) -> Option<&mut GpuiNotificationFeedState> {
        match &mut self.state {
            GpuiTitlebarReadingPanelState::Notifications { feed, .. } => Some(feed),
            _ => None,
        }
    }

    fn set_notifications_actions_menu_open(&mut self, open: bool, cx: &mut gpui::Context<Self>) {
        if let GpuiTitlebarReadingPanelState::Notifications {
            actions_menu_open, ..
        } = &mut self.state
            && *actions_menu_open != open
        {
            *actions_menu_open = open;
            cx.notify();
        }
    }

    fn send_notification_feed_command(
        &self,
        action: &'static str,
        row: Option<(MachineId, String)>,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update_in(cx, move |app, _main_window, cx| {
            app.request_notification_feed_command(
                action,
                row.as_ref().map(|(machine, id)| (machine, id.as_str())),
                NotificationFeedTab::All,
                cx,
            );
        });
    }

    fn open_notification(
        &mut self,
        key: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut reveal_session_id = None;
        let mut row = None;
        if let Some(feed) = self.notifications_feed_mut()
            && let Some(index) = feed.items.iter().position(|item| item.key() == key)
        {
            let machine = feed.items[index].machine.clone();
            if !feed.items[index].read && !feed.items[index].awaiting_approval {
                feed.items[index].read = true;
                feed.note_read(&machine);
            }
            let item = &feed.items[index];
            // An account row has no session to reveal; the store opens the account instead.
            reveal_session_id = item.account_id.is_none().then(|| match &item.machine {
                MachineId::Local => {
                    gpui_combined_presentation_session_id(&item.project_id, &item.session_id)
                }
                MachineId::Remote(machine_id) => {
                    SessionKey::remote(machine_id, &item.project_id, &item.session_id)
                        .to_sidebar_session_id()
                }
            });
            row = Some((machine, item.id.clone()));
        }
        let Some(row) = row else {
            return;
        };
        self.send_notification_feed_command("open", Some(row), cx);
        // The store (gx_store/notifications/) focuses and reveals the session; this only makes sure a collapsed sidebar opens so the reveal is visible, the same as the titlebar's reveal-active-session button.
        if let Some(session_id) = reveal_session_id {
            let _ = self.main_app.update_in(cx, move |app, _main_window, cx| {
                app.reveal_sidebar_session(&session_id, cx);
            });
        }
        self.close_popup(window, cx);
    }

    fn dismiss_notification(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        let mut next_hovered_id = None;
        let mut row = None;
        if let Some(feed) = self.notifications_feed_mut()
            && let Some(index) = feed.items.iter().position(|item| item.key() == key)
        {
            if feed.items[index].awaiting_approval {
                return;
            }
            let removed = feed.items.remove(index);
            if !removed.read {
                feed.note_read(&removed.machine);
            }
            if feed.next_unread_id.as_deref() == Some(removed.id.as_str())
                && feed.next_unread_machine == removed.machine
            {
                feed.next_unread_id = None;
            }
            // Hover only updates on mouse movement, so the card that slides into the removed slot is marked hovered here; the pointer was on the title line, which lands inside the next card's title line.
            // Cards are grouped by read state, so the slot is filled by the next card of the same group.
            next_hovered_id = feed
                .items
                .iter()
                .skip(index)
                .find(|item| item.read == removed.read)
                .map(GpuiNotificationFeedItem::key);
            row = Some((removed.machine, removed.id));
        }
        if let GpuiTitlebarReadingPanelState::Notifications { hovered_id, .. } = &mut self.state {
            *hovered_id = next_hovered_id;
        }
        if let Some(row) = row {
            self.send_notification_feed_command("dismiss", Some(row), cx);
        }
        cx.notify();
    }

    /// The row's Remove: the button goes at once, and the store removes the device on the row's
    /// computer, whose feed then comes back without it.
    fn remove_notification_device(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        let mut target = None;
        if let Some(feed) = self.notifications_feed_mut()
            && let Some(item) = feed.items.iter_mut().find(|item| item.key() == key)
            && let Some(device_id) = item.remove_device.take()
        {
            target = Some((item.machine.clone(), device_id));
        }
        let Some((machine, device_id)) = target else {
            return;
        };
        cx.notify();
        let _ = self.main_app.update_in(cx, move |app, _main_window, cx| {
            app.gx_store_remove_notification_device(machine, device_id, cx);
        });
    }

    fn toggle_notification_read(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        let Some(feed) = self.notifications_feed_mut() else {
            return;
        };
        let Some(index) = feed.items.iter().position(|item| item.key() == key) else {
            return;
        };
        if feed.items[index].awaiting_approval && !feed.items[index].read {
            return;
        }
        let machine = feed.items[index].machine.clone();
        let was_read = feed.items[index].read;
        feed.items[index].read = !was_read;
        let action = if was_read {
            feed.note_unread(&machine);
            "markUnread"
        } else {
            feed.note_read(&machine);
            "markRead"
        };
        let row = (machine, feed.items[index].id.clone());
        // The card leaves its group, so the next card of that group slides under the pointer; hand it the hover like a dismiss does.
        let next_hovered_id = feed
            .items
            .iter()
            .skip(index + 1)
            .find(|item| item.read == was_read)
            .map(GpuiNotificationFeedItem::key);
        if let GpuiTitlebarReadingPanelState::Notifications { hovered_id, .. } = &mut self.state
            && hovered_id.as_deref() == Some(key.as_str())
        {
            *hovered_id = next_hovered_id;
        }
        self.send_notification_feed_command(action, Some(row), cx);
        cx.notify();
    }

    /// Reads every row, on every online computer.
    fn mark_all_notifications_read(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(feed) = self.notifications_feed_mut() {
            for item in &mut feed.items {
                if !item.awaiting_approval {
                    item.read = true;
                }
            }
            reset_unread_counts(feed);
        }
        self.send_notification_feed_command("markAllRead", None, cx);
        cx.notify();
    }

    /// Clears every row, on every online computer.
    fn clear_all_notifications(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(feed) = self.notifications_feed_mut() {
            feed.items.retain(|item| item.awaiting_approval);
            reset_unread_counts(feed);
        }
        self.send_notification_feed_command("clearAll", None, cx);
        cx.notify();
    }

    fn set_hovered_notification(&mut self, id: &str, hovered: bool, cx: &mut gpui::Context<Self>) {
        let GpuiTitlebarReadingPanelState::Notifications { hovered_id, .. } = &mut self.state
        else {
            return;
        };
        let next = if hovered {
            Some(id.to_string())
        } else if hovered_id.as_deref() == Some(id) {
            None
        } else {
            return;
        };
        if *hovered_id != next {
            *hovered_id = next;
            cx.notify();
        }
    }

    pub(super) fn render_notifications(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let GpuiTitlebarReadingPanelState::Notifications {
            feed,
            hovered_id,
            actions_menu_open,
        } = &self.state
        else {
            unreachable!();
        };
        let mut render_group = |read: bool| {
            feed.items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.read == read)
                .map(|(index, item)| {
                    self.render_notification_row(
                        index,
                        item,
                        hovered_id.as_deref() == Some(item.key().as_str()),
                        cx,
                    )
                })
                .collect::<Vec<_>>()
        };
        let unread_rows = render_group(false);
        let read_rows = render_group(true);
        let has_unread = !unread_rows.is_empty();
        let has_read = !read_rows.is_empty();
        let offline_notice = feed
            .offline_notice
            .as_deref()
            .map(Self::render_notifications_offline_notice);
        let mut body = v_flex().w_full().p(px(10.0)).pt(px(8.0));
        if has_unread {
            body = body
                .child(Self::render_notifications_section_heading("UNREAD"))
                .child(v_flex().w_full().gap(px(7.0)).children(unread_rows));
        }
        if has_read {
            body = body
                .when(has_unread, |this| this.mt(px(10.0)))
                .child(Self::render_notifications_section_heading("READ"))
                .child(v_flex().w_full().gap(px(7.0)).children(read_rows));
        }
        v_flex()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(self.render_notifications_header(feed, *actions_menu_open, cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(if !has_unread && !has_read {
                        v_flex()
                            .size_full()
                            .when_some(offline_notice, |this, notice| {
                                this.child(div().w_full().px(px(10.0)).pt(px(8.0)).child(notice))
                            })
                            .child(
                                div()
                                    .w_full()
                                    .min_h_0()
                                    .flex_1()
                                    .child(Self::render_notifications_empty_state()),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .id("ghostex-gpui-titlebar-notifications-scroll-area")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(
                                v_flex()
                                    .w_full()
                                    .when_some(offline_notice, |this, notice| {
                                        this.child(
                                            div().w_full().px(px(10.0)).pt(px(8.0)).child(notice),
                                        )
                                    })
                                    .child(body),
                            )
                            .into_any_element()
                    })
                    .child(
                        Scrollbar::vertical(&self.scroll_handle)
                            .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                    ),
            )
            .when(*actions_menu_open, |this| {
                this.child(self.render_notifications_actions_menu(feed, cx))
            })
            .into_any_element()
    }

    /// "<Computer> is offline. Its notifications show again when it reconnects." as a plain dimmed
    /// line at the top of the list while a remote computer's rows are waiting to come back.
    ///
    /// CDXC:Notifications 2026-11-07 DECISION: User: "don't show this all the time man". A
    /// computer that is off is normal, so this is a quiet dimmed line in the list's style, not a
    /// yellow warning banner.
    fn render_notifications_offline_notice(notice: &str) -> AnyElement {
        div()
            .w_full()
            .px(px(2.0))
            .pb(px(2.0))
            .text_size(px(11.5))
            .text_color(chrome_ink().opacity(0.50))
            .child(notice.to_string())
            .into_any_element()
    }

    /// Same heading as the Tips dropdown's UNREAD / READ groups.
    fn render_notifications_section_heading(title: &'static str) -> AnyElement {
        h_flex()
            .h(px(24.0))
            .items_center()
            .px(px(2.0))
            .pt(px(4.0))
            .pb(px(7.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::BOLD)
            .text_color(chrome_ink().opacity(0.62))
            .child(title)
            .into_any_element()
    }
    /// The title, then one square "..." button at the right end whose click opens the actions menu
    /// (CDXC:Notifications 2026-11-12 at the top of this file).
    /// User: do not show the notification count next to the dropdown title.
    ///
    /// CDXC:Notifications 2026-10-17 DECISION:
    /// User: "move the notification title at the top 4px to the right (add more margin from the word's left)".
    fn render_notifications_header(
        &self,
        feed: &GpuiNotificationFeedState,
        menu_open: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let enabled = !feed.items.is_empty();
        let ink = chrome_ink();
        resource_header()
            .border_color(ink.opacity(0.12))
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_center()
                    .pl(px(8.0 + 4.0))
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ink.opacity(0.96))
                    .child("Notifications"),
            )
            .child(
                h_flex()
                    .id("ghostex-gpui-titlebar-notifications-actions")
                    .role(gpui::Role::Button)
                    .aria_label("Notification actions")
                    .flex_shrink_0()
                    .h_full()
                    .w(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT))
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(ink.opacity(0.12))
                    .rounded_tr(px(RESOURCE_PANEL_RADIUS - 1.0))
                    .when(menu_open, |this| this.bg(ink.opacity(0.14)))
                    .when(enabled, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.bg(ink.opacity(0.14)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.set_notifications_actions_menu_open(!menu_open, cx);
                                }),
                            )
                    })
                    .child(titlebar_svg_icon(
                        NOTIFICATION_MORE_ICON,
                        TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                        ink.opacity(if enabled { 0.78 } else { 0.38 }).into(),
                    )),
            )
            .into_any_element()
    }

    /// The "..." menu: Mark all read and Clear all, under the button at the header's right end.
    /// A click outside it (on the list or the header) closes it, as the sidebar's menus do.
    fn render_notifications_actions_menu(
        &self,
        feed: &GpuiNotificationFeedState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let has_unread = feed
            .items
            .iter()
            .any(|item| !item.read && !item.awaiting_approval);
        let has_rows = feed.items.iter().any(|item| !item.awaiting_approval);
        div()
            .id("ghostex-gpui-titlebar-notifications-actions-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.set_notifications_actions_menu_open(false, cx);
                }),
            )
            .child(
                v_flex()
                    .id("ghostex-gpui-titlebar-notifications-actions-menu")
                    .role(gpui::Role::Menu)
                    .aria_label("Notification actions")
                    .absolute()
                    .top(px(TITLEBAR_POPUP_READING_HEADER_HEIGHT + 4.0))
                    .right(px(6.0))
                    .w(px(NOTIFICATION_ACTIONS_MENU_WIDTH))
                    .p(px(4.0))
                    .rounded(px(POPUP_MENU_RADIUS))
                    .border_1()
                    .border_color(titlebar_popup_menu_border_color())
                    .bg(titlebar_popup_menu_background())
                    .shadow_lg()
                    .occlude()
                    .child(Self::render_notifications_menu_item(
                        "ghostex-gpui-titlebar-notifications-mark-all-read",
                        "Mark all read",
                        NOTIFICATION_PANEL_CHECK_ICON,
                        has_unread,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.set_notifications_actions_menu_open(false, cx);
                            this.mark_all_notifications_read(cx);
                        }),
                    ))
                    .child(Self::render_notifications_menu_item(
                        "ghostex-gpui-titlebar-notifications-clear-all",
                        "Clear all",
                        "titlebar/xmark.svg",
                        has_rows,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.set_notifications_actions_menu_open(false, cx);
                            this.clear_all_notifications(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_notifications_menu_item(
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let foreground = titlebar_popup_menu_foreground();
        let opacity = if enabled { 0.92 } else { 0.38 };
        h_flex()
            .id(id)
            .role(gpui::Role::MenuItem)
            .aria_label(label)
            .w_full()
            .h(px(28.0))
            .px(px(8.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(5.0))
            .text_size(px(13.0))
            .text_color(foreground.opacity(opacity))
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(titlebar_popup_menu_hover_color()))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .child(titlebar_svg_icon(
                icon,
                14.0,
                foreground.opacity(if enabled { 0.72 } else { 0.30 }),
            ))
            .child(label)
            .into_any_element()
    }

    /// CDXC:Notifications 2026-09-11 DECISION:
    /// User: notification cards look like the Tips dropdown cards (square icon tile, bold title, muted body, check mark bottom right, UNREAD and READ groups), keeping the blue accents for ordinary unread cards. The 2026-11-02 approval decision in server/src/notification_feed/store.rs supersedes blue for an approval card, which uses the sidebar's approval red.
    /// User: no dismiss gutter; the X floats where the time is and, while the card is hovered, replaces the time in that same right-aligned slot.
    fn render_notification_row(
        &self,
        index: usize,
        item: &GpuiNotificationFeedItem,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let key = item.key();
        let hover_id = key.clone();
        let open_id = key.clone();
        let toggle_id = key.clone();
        let read_id = key.clone();
        let remove_id = key.clone();
        let dismiss_id = key;
        let time_label = notification_feed_relative_time(item.created_at_epoch_secs);
        let unread = !item.read;
        let accent = if item.awaiting_approval {
            crate::app::native_sidebar::status::APPROVAL_COLOR
        } else {
            item.kind.accent()
        };
        let accent_text_light = if item.awaiting_approval {
            crate::notification_feed::NOTIFICATION_URGENT_RED_TEXT_LIGHT
        } else {
            item.kind.accent_text_light()
        };
        let title = if item.title.is_empty() {
            "Session".to_string()
        } else {
            item.title.clone()
        };
        // "Project · Finished" as the card footer; the kind alone when the project name is unknown.
        let meta_line = if item.subtitle.is_empty() {
            item.kind.label().to_string()
        } else {
            format!("{} · {}", item.subtitle, item.kind.label())
        };
        // CDXC:Notifications 2026-09-11 DECISION:
        // User: one square check button per card in the bottom-right corner, shown on hover only, no X and no separate column for it.
        // On an unread card it marks the card read; on a read card it dismisses the card from history.
        let check_button: Option<AnyElement> = (hovered && !item.awaiting_approval).then(|| {
            div()
                .id(format!("ghostex-gpui-titlebar-notification-check-{index}"))
                .absolute()
                .bottom(px(8.0))
                .right(px(8.0))
                .flex()
                .size(px(NOTIFICATION_CARD_CHECK_SIZE))
                .items_center()
                .justify_center()
                .cursor_pointer()
                .rounded(px(RESOURCE_CONTROL_RADIUS))
                .border_1()
                .border_color(if unread {
                    rgb(accent).opacity(0.45)
                } else {
                    chrome_ink().opacity(0.16)
                })
                .bg(if unread {
                    rgb(accent).opacity(0.18)
                } else {
                    chrome_ink().opacity(0.12)
                })
                .hover(move |this| {
                    this.bg(if unread {
                        rgb(accent).opacity(0.32)
                    } else {
                        chrome_ink().opacity(0.22)
                    })
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if unread {
                            this.toggle_notification_read(read_id.clone(), cx);
                        } else {
                            this.dismiss_notification(dismiss_id.clone(), cx);
                        }
                    }),
                )
                .child(titlebar_svg_icon(
                    NOTIFICATION_PANEL_CHECK_ICON,
                    14.0,
                    chrome_ink().opacity(0.92).into(),
                ))
                .into_any_element()
        });
        let time_slot = div()
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_size(px(11.0))
            .line_height(px(16.0))
            .text_color(chrome_ink().opacity(0.45))
            .child(time_label);
        // The agent's own logo when the session has one, else an icon for the kind.
        let tile_icon: AnyElement = if let Some(agent_name) = item.agent_name.as_deref()
            && let Some(icon_path) = workspace_tab_agent_icon_path(agent_name)
        {
            let mut icon_color: Hsla =
                rgb(workspace_tab_agent_icon_accent_color(agent_name)).into();
            // CDXC:Notifications 2026-09-13 WHY:
            // Agent accents target dark terminal tabs, including white logos; cap their lightness on light notification tiles while retaining the brand hue.
            // CDXC:Notifications 2026-09-15 DECISION:
            // User: Claude's notification icon uses its brand color #d97757 as-is, so it is exempt from the light-mode lightness cap.
            if CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed)
                && agent_name != "claude"
            {
                icon_color.l = icon_color.l.min(0.3);
            }
            svg()
                .path(icon_path)
                .size(px(16.0))
                .text_color(icon_color)
                .into_any_element()
        } else {
            titlebar_svg_icon(
                item.kind.icon_path(),
                16.0,
                chrome_ink().opacity(0.84).into(),
            )
            .into_any_element()
        };
        let detail = h_flex()
            .min_w_0()
            .flex_1()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .size(px(28.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(RESOURCE_CARD_RADIUS))
                    .bg(if unread {
                        rgb(accent).opacity(0.16)
                    } else {
                        chrome_ink().opacity(0.10)
                    })
                    .child(tile_icon),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(7.0))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .h(px(NOTIFICATION_CARD_TITLE_LINE_HEIGHT))
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(chrome_ink().opacity(0.94))
                                    .child(title),
                            )
                            .when_some(item.computer_icon, |this, asset| {
                                this.child(Self::render_notification_computer_icon(asset))
                            })
                            .child(div().flex_1())
                            .child(time_slot),
                    )
                    .when(!item.body.is_empty(), |this| {
                        this.child(
                            div()
                                .max_h(px(33.0))
                                .overflow_hidden()
                                .line_clamp(2)
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .line_height(px(16.2))
                                .text_color(chrome_ink().opacity(0.58))
                                .child(item.body.clone()),
                        )
                    })
                    .when(item.remove_device.is_some(), |this| {
                        this.child(Self::render_notification_remove_button(
                            index,
                            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.remove_notification_device(remove_id.clone(), cx);
                            }),
                        ))
                    })
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if unread {
                                if CHROME_LIGHT_APPEARANCE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                                {
                                    rgb(accent_text_light)
                                } else {
                                    rgb(accent).opacity(0.85)
                                }
                            } else {
                                chrome_ink().opacity(0.45)
                            })
                            .child(meta_line),
                    ),
            );
        h_flex()
            .id(format!("ghostex-gpui-titlebar-notification-row-{index}"))
            .relative()
            // The scroll column must not shrink cards to fit the panel; the column overflows and scrolls instead.
            .flex_shrink_0()
            .w_full()
            .min_h(px(72.0))
            .items_start()
            .rounded(px(RESOURCE_CARD_RADIUS))
            .border_1()
            .border_color(if unread {
                rgb(accent).opacity(0.30)
            } else {
                resource_card_border()
            })
            .bg(if unread {
                rgb(accent).opacity(0.05)
            } else if window_glass_active() {
                resource_glass_washes().panel
            } else {
                chrome_ink().opacity(0.025)
            })
            .p(px(8.0))
            .pt(px(9.0))
            .when(!unread, |this| this.opacity(0.72))
            .cursor_pointer()
            .when(hovered, |this| {
                this.opacity(1.0)
                    .bg(if unread {
                        rgb(accent).opacity(0.10)
                    } else if window_glass_active() {
                        resource_glass_washes().raised
                    } else {
                        chrome_ink().opacity(0.05)
                    })
                    .border_color(if unread {
                        rgb(accent).opacity(0.48)
                    } else {
                        chrome_ink().opacity(0.18)
                    })
            })
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                this.set_hovered_notification(&hover_id, *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_notification(open_id.clone(), window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_notification_read(toggle_id.clone(), cx);
                }),
            )
            .child(detail)
            .children(check_button)
            .into_any_element()
    }
    /// Remove on the row for a phone added for Easy Connect without a click: a compact neutral button
    /// (the shared neutral fills, as the app's secondary buttons), since the row is the click target.
    fn render_notification_remove_button(
        index: usize,
        on_press: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let fills = crate::app::window::native_modal_kit::palette::neutral_button_fills(
            CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
        );
        h_flex()
            .id(format!("ghostex-gpui-titlebar-notification-remove-{index}"))
            .role(gpui::Role::Button)
            .aria_label("Remove")
            .flex_shrink_0()
            .w_auto()
            .h(px(24.0))
            .px(px(10.0))
            .items_center()
            .cursor_pointer()
            .rounded(px(RESOURCE_CONTROL_RADIUS))
            .bg(crate::app::window::native_modal_kit::palette::hsla(
                fills.rest,
            ))
            .hover(move |this| {
                this.bg(crate::app::window::native_modal_kit::palette::hsla(
                    fills.hover,
                ))
            })
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(chrome_ink().opacity(0.92))
            .on_mouse_down(MouseButton::Left, on_press)
            .child("Remove")
            .into_any_element()
    }

    /// The row's computer, as its icon right after the title: the mark a sidebar row draws for it
    /// (native_sidebar/computer_mark.rs), in the row's muted ink.
    fn render_notification_computer_icon(asset: &'static str) -> AnyElement {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(titlebar_svg_icon(
                asset,
                NOTIFICATION_COMPUTER_ICON_SIZE,
                chrome_ink().opacity(0.55).into(),
            ))
            .into_any_element()
    }

    fn render_notifications_empty_state() -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .child(titlebar_svg_icon(
                NOTIFICATION_PANEL_BELL_ICON,
                32.0,
                chrome_ink().opacity(0.35).into(),
            ))
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(chrome_ink().opacity(0.86))
                    .child("No notifications yet"),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(chrome_ink().opacity(0.50))
                    .child("Agents that finish or need you will show up here."),
            )
            .into_any_element()
    }
}

/// Every unread row was read or cleared, except approvals still waiting for an answer (they stay
/// unread): the counts drop to those, and the jump target goes unless it is one of them.
fn reset_unread_counts(feed: &mut GpuiNotificationFeedState) {
    let waiting = |local: bool| {
        feed.items
            .iter()
            .filter(|item| item.machine.is_local() == local && !item.read)
            .count()
    };
    feed.local_unread_count = waiting(true);
    feed.remote_unread_count = waiting(false);
    feed.unread_count = feed.local_unread_count + feed.remote_unread_count;
    let next_still_unread = feed.items.iter().any(|item| {
        !item.read
            && item.machine == feed.next_unread_machine
            && feed.next_unread_id.as_deref() == Some(item.id.as_str())
    });
    if !next_still_unread {
        feed.next_unread_id = None;
    }
}
