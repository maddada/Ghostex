//! The Notifications dropdown: the reading-panel variant behind the titlebar
//! bell. It renders the cached feed the sidebar runtime pushed (see
//! `crate::notification_feed`), edits its own copy optimistically so clicks
//! feel instant, and sends one command per click back through the main app.
use super::resources_style::*;
use crate::app::helpers::*;
use crate::notification_feed::{
    GpuiNotificationFeedItem, GpuiNotificationFeedState, NOTIFICATION_ATTENTION_BLUE,
    notification_feed_badge_label, notification_feed_relative_time,
};
use crate::*;

const NOTIFICATION_PANEL_BELL_ICON: &str = "titlebar/bell.svg";
const NOTIFICATION_PANEL_CLOSE_ICON: &str = "titlebar/xmark.svg";
/// The hover-only dismiss button replaces the time in the title line's trailing slot, so line one keeps this height in both states.
const NOTIFICATION_CARD_DISMISS_SIZE: f32 = 18.0;

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

    fn send_notification_feed_command(
        &self,
        action: &'static str,
        notification_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.main_app.update_in(cx, move |app, _main_window, cx| {
            app.request_notification_feed_command(action, notification_id.as_deref(), cx);
        });
    }

    fn open_notification(&mut self, id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if let Some(feed) = self.notifications_feed_mut()
            && let Some(item) = feed.items.iter_mut().find(|item| item.id == id)
            && !item.read
        {
            item.read = true;
            feed.unread_count = feed.unread_count.saturating_sub(1);
        }
        self.send_notification_feed_command("open", Some(id), cx);
        self.close_popup(window, cx);
    }

    fn dismiss_notification(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        let mut next_hovered_id = None;
        if let Some(feed) = self.notifications_feed_mut()
            && let Some(index) = feed.items.iter().position(|item| item.id == id)
        {
            let removed = feed.items.remove(index);
            if !removed.read {
                feed.unread_count = feed.unread_count.saturating_sub(1);
            }
            if feed.next_unread_id.as_deref() == Some(id.as_str()) {
                feed.next_unread_id = None;
            }
            // Hover only updates on mouse movement, so the card that slides into the removed slot is marked hovered here; the pointer was on the title line, which lands inside the next card's title line.
            // Cards are grouped by read state, so the slot is filled by the next card of the same group.
            next_hovered_id = feed
                .items
                .iter()
                .skip(index)
                .find(|item| item.read == removed.read)
                .map(|item| item.id.clone());
        }
        if let GpuiTitlebarReadingPanelState::Notifications { hovered_id, .. } = &mut self.state {
            *hovered_id = next_hovered_id;
        }
        self.send_notification_feed_command("dismiss", Some(id), cx);
        cx.notify();
    }

    fn toggle_notification_read(&mut self, id: String, cx: &mut gpui::Context<Self>) {
        let Some(feed) = self.notifications_feed_mut() else {
            return;
        };
        let Some(item) = feed.items.iter_mut().find(|item| item.id == id) else {
            return;
        };
        item.read = !item.read;
        let action = if item.read {
            feed.unread_count = feed.unread_count.saturating_sub(1);
            "markRead"
        } else {
            feed.unread_count += 1;
            "markUnread"
        };
        self.send_notification_feed_command(action, Some(id), cx);
        cx.notify();
    }

    fn mark_all_notifications_read(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(feed) = self.notifications_feed_mut() {
            for item in &mut feed.items {
                item.read = true;
            }
            feed.unread_count = 0;
            feed.next_unread_id = None;
        }
        self.send_notification_feed_command("markAllRead", None, cx);
        cx.notify();
    }

    fn clear_all_notifications(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(feed) = self.notifications_feed_mut() {
            feed.items.clear();
            feed.unread_count = 0;
            feed.next_unread_id = None;
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
        let GpuiTitlebarReadingPanelState::Notifications { feed, hovered_id } = &self.state else {
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
                        hovered_id.as_deref() == Some(item.id.as_str()),
                        cx,
                    )
                })
                .collect::<Vec<_>>()
        };
        let unread_rows = render_group(false);
        let read_rows = render_group(true);
        let has_unread = !unread_rows.is_empty();
        let has_read = !read_rows.is_empty();
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
            .bg(titlebar_popup_menu_background())
            .child(self.render_notifications_header(feed, cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_h_0()
                    .flex_1()
                    .child(if !has_unread && !has_read {
                        Self::render_notifications_empty_state()
                    } else {
                        div()
                            .id("ghostex-gpui-titlebar-notifications-scroll-area")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(body)
                            .into_any_element()
                    })
                    .child(
                        Scrollbar::vertical(&self.scroll_handle)
                            .thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH)),
                    ),
            )
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
            .text_color(rgb(0xffffff).opacity(0.62))
            .child(title)
            .into_any_element()
    }
    fn render_notifications_header(
        &self,
        feed: &GpuiNotificationFeedState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let unread_count = feed.unread_count;
        resource_header()
            .child(
                resource_heading()
                    .child("Notifications")
                    .when(unread_count > 0, |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .h(px(18.0))
                                .px(px(6.0))
                                .rounded_full()
                                .bg(rgb(NOTIFICATION_ATTENTION_BLUE))
                                .text_size(px(11.0))
                                .line_height(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(titlebar_popup_menu_background())
                                .child(notification_feed_badge_label(unread_count)),
                        )
                    }),
            )
            .child(
                h_flex()
                    .h_full()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.0))
                    .pr(px(10.0))
                    .child(Self::render_notification_header_chip(
                        "ghostex-gpui-titlebar-notifications-jump",
                        "Jump to unread",
                        unread_count > 0,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.send_notification_feed_command("jumpToLatestUnread", None, cx);
                            this.close_popup(window, cx);
                        }),
                    ))
                    .child(Self::render_notification_header_chip(
                        "ghostex-gpui-titlebar-notifications-mark-all-read",
                        "Mark all read",
                        unread_count > 0,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.mark_all_notifications_read(cx);
                        }),
                    ))
                    .child(Self::render_notification_header_chip(
                        "ghostex-gpui-titlebar-notifications-clear-all",
                        "Clear all",
                        !feed.items.is_empty(),
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.clear_all_notifications(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_notification_header_chip(
        id: &'static str,
        label: &'static str,
        enabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        h_flex()
            .id(id)
            .h(px(22.0))
            .px(px(8.0))
            .flex_shrink_0()
            .items_center()
            .rounded(px(5.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::MEDIUM)
            .when(enabled, |this| {
                this.bg(rgb(0xffffff).opacity(0.10))
                    .text_color(rgb(0xffffff).opacity(0.90))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0xffffff).opacity(0.16)))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when(!enabled, |this| {
                this.bg(rgb(0xffffff).opacity(0.04))
                    .text_color(rgb(0xffffff).opacity(0.38))
                    .cursor_default()
            })
            .child(label)
            .into_any_element()
    }

    /// CDXC:Notifications 2026-09-11 DECISION:
    /// User: notification cards look like the Tips dropdown cards (square icon tile, bold title, muted body, check mark bottom right, UNREAD and READ groups), keeping the blue accents for unread cards.
    /// User: no dismiss gutter; the X floats where the time is and, while the card is hovered, replaces the time in that same right-aligned slot.
    fn render_notification_row(
        &self,
        index: usize,
        item: &GpuiNotificationFeedItem,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover_id = item.id.clone();
        let open_id = item.id.clone();
        let toggle_id = item.id.clone();
        let read_id = item.id.clone();
        let dismiss_id = item.id.clone();
        let time_label = notification_feed_relative_time(item.created_at_epoch_secs);
        let unread = !item.read;
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
        let trailing_slot: AnyElement = if hovered {
            div()
                .id(format!(
                    "ghostex-gpui-titlebar-notification-dismiss-{index}"
                ))
                .flex()
                .flex_shrink_0()
                .size(px(NOTIFICATION_CARD_DISMISS_SIZE))
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(rgb(0xffffff).opacity(0.10))
                .cursor_pointer()
                .hover(|this| this.bg(rgb(0xffffff).opacity(0.20)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.dismiss_notification(dismiss_id.clone(), cx);
                    }),
                )
                .child(titlebar_svg_icon(
                    NOTIFICATION_PANEL_CLOSE_ICON,
                    8.0,
                    rgb(0xffffff).opacity(0.75).into(),
                ))
                .into_any_element()
        } else {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(rgb(0xffffff).opacity(0.45))
                .child(time_label)
                .into_any_element()
        };
        // The agent's own logo when the session has one, else an icon for the kind.
        let tile_icon: AnyElement = if let Some(agent_name) = item.agent_name.as_deref()
            && let Some(icon_path) = workspace_tab_agent_icon_path(agent_name)
        {
            svg()
                .path(icon_path)
                .size(px(16.0))
                .text_color(rgb(workspace_tab_agent_icon_accent_color(agent_name)))
                .into_any_element()
        } else {
            titlebar_svg_icon(
                item.kind.icon_path(),
                16.0,
                rgb(0xffffff).opacity(0.84).into(),
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
                    .bg(if unread {
                        rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.16)
                    } else {
                        rgb(0xffffff).opacity(0.10)
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
                            .h(px(NOTIFICATION_CARD_DISMISS_SIZE))
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
                                    .text_color(rgb(0xffffff).opacity(0.94))
                                    .child(title),
                            )
                            .child(trailing_slot),
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
                                .text_color(rgb(0xffffff).opacity(0.58))
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
                                rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.85)
                            } else {
                                rgb(0xffffff).opacity(0.45)
                            })
                            .child(meta_line),
                    ),
            );
        // Same check as the Tips cards: a button that reads an unread card, a muted mark on a read one.
        let read_mark = div()
            .id(format!("ghostex-gpui-titlebar-notification-read-{index}"))
            .flex_shrink_0()
            .flex()
            .size(px(24.0))
            .self_end()
            .items_center()
            .justify_center()
            .when(unread, |this| {
                this.cursor_pointer()
                    .border_1()
                    .border_color(rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.45))
                    .bg(rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.18))
                    .hover(|this| this.bg(rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.30)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.toggle_notification_read(read_id.clone(), cx);
                        }),
                    )
            })
            .child(titlebar_svg_icon(
                "titlebar/check.svg",
                15.0,
                if unread {
                    rgb(0xffffff).opacity(0.92).into()
                } else {
                    rgb(0xffffff).opacity(0.46).into()
                },
            ));

        h_flex()
            .id(format!("ghostex-gpui-titlebar-notification-row-{index}"))
            // The scroll column must not shrink cards to fit the panel; the column overflows and scrolls instead.
            .flex_shrink_0()
            .w_full()
            .min_h(px(72.0))
            .items_start()
            .gap(px(10.0))
            .border_1()
            .border_color(if unread {
                rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.30)
            } else {
                rgb(0xffffff).opacity(0.10)
            })
            .bg(if unread {
                rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.05)
            } else {
                rgb(0xffffff).opacity(0.025)
            })
            .p(px(8.0))
            .pt(px(9.0))
            .when(!unread, |this| this.opacity(0.72))
            .cursor_pointer()
            .when(hovered, |this| {
                this.opacity(1.0)
                    .bg(if unread {
                        rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.10)
                    } else {
                        rgb(0xffffff).opacity(0.05)
                    })
                    .border_color(if unread {
                        rgb(NOTIFICATION_ATTENTION_BLUE).opacity(0.48)
                    } else {
                        rgb(0xffffff).opacity(0.18)
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
            .child(read_mark)
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
                rgb(0xffffff).opacity(0.35).into(),
            ))
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff).opacity(0.86))
                    .child("No notifications yet"),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(0xffffff).opacity(0.50))
                    .child("Agents that finish or need you will show up here."),
            )
            .into_any_element()
    }
}
