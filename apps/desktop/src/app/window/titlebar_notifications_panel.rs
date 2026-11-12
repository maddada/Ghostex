//! The Notifications dropdown: the reading-panel variant behind the titlebar
//! bell. It renders the cached feed the Rust store pushed (see
//! `crate::notification_feed`), edits its own copy optimistically so clicks
//! feel instant, and sends one command per click back through the main app.
//! The rows are every online computer's (gx-core `notification_feeds.rs`): with a remote
//! computer in the sidebar the panel shows All, This computer and Remote tabs, a remote row
//! names its computer in a chip by the title, and the Remote tab shows one dimmed line when an offline computer has rows waiting to come back.
use super::native_modal_kit::{ModalRailItem, modal_bordered_segmented_control};
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
const NOTIFICATION_CHIP_ICON: &str = "titlebar/device-desktop.svg";

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
                tab: NotificationFeedTab::All,
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

    /// The open tab; All when the panel draws no tabs (no remote computer in the sidebar).
    fn notifications_tab(&self) -> NotificationFeedTab {
        match &self.state {
            GpuiTitlebarReadingPanelState::Notifications { tab, feed, .. }
                if feed.has_remote_machines =>
            {
                *tab
            }
            _ => NotificationFeedTab::All,
        }
    }

    fn select_notifications_tab(
        &mut self,
        next: NotificationFeedTab,
        cx: &mut gpui::Context<Self>,
    ) {
        if let GpuiTitlebarReadingPanelState::Notifications {
            tab, hovered_id, ..
        } = &mut self.state
            && *tab != next
        {
            *tab = next;
            *hovered_id = None;
            cx.notify();
        }
    }

    fn send_notification_feed_command(
        &self,
        action: &'static str,
        row: Option<(MachineId, String)>,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab = self.notifications_tab();
        let _ = self.main_app.update_in(cx, move |app, _main_window, cx| {
            app.request_notification_feed_command(
                action,
                row.as_ref().map(|(machine, id)| (machine, id.as_str())),
                tab,
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
        let tab = self.notifications_tab();
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
            // Cards are grouped by read state, so the slot is filled by the next card of the same group on this tab.
            next_hovered_id = feed
                .items
                .iter()
                .skip(index)
                .find(|item| item.read == removed.read && tab.includes(&item.machine))
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

    fn toggle_notification_read(&mut self, key: String, cx: &mut gpui::Context<Self>) {
        let tab = self.notifications_tab();
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
            .find(|item| item.read == was_read && tab.includes(&item.machine))
            .map(GpuiNotificationFeedItem::key);
        if let GpuiTitlebarReadingPanelState::Notifications { hovered_id, .. } = &mut self.state
            && hovered_id.as_deref() == Some(key.as_str())
        {
            *hovered_id = next_hovered_id;
        }
        self.send_notification_feed_command(action, Some(row), cx);
        cx.notify();
    }

    /// Reads every row the open tab shows, on every online computer it covers.
    fn mark_all_notifications_read(&mut self, cx: &mut gpui::Context<Self>) {
        let tab = self.notifications_tab();
        if let Some(feed) = self.notifications_feed_mut() {
            for item in &mut feed.items {
                if tab.includes(&item.machine) && !item.awaiting_approval {
                    item.read = true;
                }
            }
            reset_tab_unread_counts(feed, tab);
        }
        self.send_notification_feed_command("markAllRead", None, cx);
        cx.notify();
    }

    /// Clears every row the open tab shows, on every online computer it covers.
    fn clear_all_notifications(&mut self, cx: &mut gpui::Context<Self>) {
        let tab = self.notifications_tab();
        if let Some(feed) = self.notifications_feed_mut() {
            feed.items
                .retain(|item| item.awaiting_approval || !tab.includes(&item.machine));
            reset_tab_unread_counts(feed, tab);
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
            feed, hovered_id, ..
        } = &self.state
        else {
            unreachable!();
        };
        // Without a remote computer there is only one tab, so none is drawn and everything shows.
        let tab = self.notifications_tab();
        let mut render_group = |read: bool| {
            feed.items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.read == read && tab.includes(&item.machine))
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
            .filter(|_| tab.shows_offline_notice())
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
            .size_full()
            .overflow_hidden()
            .child(self.render_notifications_header(feed, tab, cx))
            .when(feed.has_remote_machines, |this| {
                this.child(self.render_notifications_tabs(feed, tab, cx))
            })
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
            .into_any_element()
    }

    /// All · This computer · Remote, each with its own unread count, in the shared bordered
    /// segmented control (the mockup's tab A).
    fn render_notifications_tabs(
        &self,
        feed: &GpuiNotificationFeedState,
        tab: NotificationFeedTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let items: Vec<ModalRailItem> = NotificationFeedTab::TABS
            .iter()
            .map(|tab| ModalRailItem {
                label: tab.label().into(),
                trailing: Some(feed.tab_unread_count(*tab).to_string().into()),
                dot: None,
            })
            .collect();
        let selected = NotificationFeedTab::TABS
            .iter()
            .position(|candidate| *candidate == tab)
            .unwrap_or(0);
        let ink = chrome_ink();
        div()
            .w_full()
            .flex_shrink_0()
            .px(px(10.0))
            .pt(px(9.0))
            .child(
                modal_bordered_segmented_control(
                    ink,
                    gpui::Rgba { a: 0.14, ..ink },
                    gpui::Rgba { a: 0.55, ..ink },
                    "ghostex-gpui-titlebar-notifications-tabs",
                    &items,
                    selected,
                    |this: &mut Self, index, _window, cx| {
                        if let Some(tab) = NotificationFeedTab::TABS.get(index) {
                            this.select_notifications_tab(*tab, cx);
                        }
                    },
                    cx,
                )
                .w_full(),
            )
            .into_any_element()
    }

    /// "<Computer> is offline. Its notifications show again when it reconnects." as a plain dimmed
    /// line at the top of the Remote tab while a remote computer's rows are waiting to come back.
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
    /// CDXC:Notifications 2026-09-13 DECISION:
    /// User: keep the Notifications panel in native GPUI and match the Tips header buttons with full-height, equal-width, unfilled actions separated by thin borders, with an icon beside each label.
    /// 2026-11-12, user: the "Next unread" action is gone from this header; Mark all read and Clear all fill it. Neither has a hotkey, so neither has a tooltip (it would only repeat the label).
    /// User: do not show the notification count next to the dropdown title.
    ///
    /// CDXC:Notifications 2026-10-16 DECISION:
    /// User: "i wanted that 40px to be shown as gap to the right of the word "Notifications" instead of making the buttons wider in the header of notifications modal". The 40px the panel gained (TITLEBAR_POPUP_NOTIFICATIONS_WIDTH) is right padding on the title, so the actions keep their 480px-panel widths. 2026-10-17, user: "move the notification title at the top 4px to the right (add more margin from the word's left)": the 4px moves from the title's right padding to its left, so the actions stay where they were.
    fn render_notifications_header(
        &self,
        feed: &GpuiNotificationFeedState,
        tab: NotificationFeedTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        // Mark all read and Clear all act on the open tab.
        let tab_unread_count = feed.tab_unread_count(tab);
        let tab_has_rows = feed.items.iter().any(|item| tab.includes(&item.machine));
        resource_header()
            .border_color(chrome_ink().opacity(0.12))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .pl(px(8.0 + 4.0))
                    .pr(px(4.0 + 40.0))
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(chrome_ink().opacity(0.96))
                    .child("Notifications"),
            )
            .child(
                h_flex()
                    .h_full()
                    .min_w_0()
                    .flex_1()
                    .items_stretch()
                    .child(Self::render_notification_header_action(
                        "ghostex-gpui-titlebar-notifications-mark-all-read",
                        "Mark all read",
                        NOTIFICATION_PANEL_CHECK_ICON,
                        tab_unread_count > 0,
                        false,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.mark_all_notifications_read(cx);
                        }),
                    ))
                    .child(Self::render_notification_header_action(
                        "ghostex-gpui-titlebar-notifications-clear-all",
                        "Clear all",
                        "titlebar/xmark.svg",
                        tab_has_rows,
                        true,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.clear_all_notifications(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    /// CDXC:Notifications 2026-09-13 WHY:
    /// Fixed 144px actions consumed 432px of the 480px dropdown and squeezed the heading into the buttons; equal flex shares with compact padding leave room for the title.
    fn render_notification_header_action(
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        enabled: bool,
        last: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .when(last, |this| {
                this.rounded_tr(px(RESOURCE_PANEL_RADIUS - 1.0))
            })
            .h_full()
            .min_w_0()
            .flex_1()
            .px(px(4.0))
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .border_l_1()
            .border_color(chrome_ink().opacity(0.12))
            .whitespace_nowrap()
            .text_size(px(TITLEBAR_POPUP_READING_HEADER_BUTTON_TEXT_SIZE))
            .font_weight(FontWeight::NORMAL)
            .when(enabled, |this| {
                this.text_color(chrome_ink().opacity(0.78))
                    .cursor_pointer()
                    .hover(|this| {
                        this.bg(chrome_ink().opacity(0.14))
                            .text_color(chrome_ink().opacity(0.94))
                    })
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| {
                this.text_color(chrome_ink().opacity(0.38)).cursor_default()
            })
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_POPUP_READING_HEADER_BUTTON_ICON_SIZE,
                chrome_ink()
                    .opacity(if enabled { 0.78 } else { 0.38 })
                    .into(),
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
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(chrome_ink().opacity(0.94))
                                    .child(title),
                            )
                            .when_some(item.machine_name.clone(), |this, name| {
                                this.child(Self::render_notification_machine_chip(name))
                            })
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
    /// A remote row's computer, by the title (the mockup's label B). Rows from this computer have
    /// none.
    fn render_notification_machine_chip(name: String) -> AnyElement {
        h_flex()
            .flex_shrink_0()
            .max_w(px(150.0))
            .h(px(17.0))
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(chrome_ink().opacity(0.16))
            .bg(chrome_ink().opacity(0.07))
            .text_size(px(10.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(chrome_ink().opacity(0.80))
            .child(titlebar_svg_icon(
                NOTIFICATION_CHIP_ICON,
                11.0,
                chrome_ink().opacity(0.80).into(),
            ))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(name),
            )
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

/// The tab's unread rows were all read or cleared, except approvals still waiting for an answer
/// (they stay unread): its counts drop to those, and the jump target goes unless it is one of them.
fn reset_tab_unread_counts(feed: &mut GpuiNotificationFeedState, tab: NotificationFeedTab) {
    let waiting = |local: bool| {
        feed.items
            .iter()
            .filter(|item| item.machine.is_local() == local && !item.read)
            .count()
    };
    let (local_waiting, remote_waiting) = (waiting(true), waiting(false));
    match tab {
        NotificationFeedTab::All => {
            feed.local_unread_count = local_waiting;
            feed.remote_unread_count = remote_waiting;
        }
        NotificationFeedTab::ThisComputer => feed.local_unread_count = local_waiting,
        NotificationFeedTab::Remote => feed.remote_unread_count = remote_waiting,
    }
    feed.unread_count = feed.local_unread_count + feed.remote_unread_count;
    let next_still_unread = feed.items.iter().any(|item| {
        !item.read
            && item.machine == feed.next_unread_machine
            && feed.next_unread_id.as_deref() == Some(item.id.as_str())
    });
    if tab.includes(&feed.next_unread_machine) && !next_still_unread {
        feed.next_unread_id = None;
    }
}
