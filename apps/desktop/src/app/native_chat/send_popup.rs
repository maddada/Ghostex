//! The Send button's popup: Send, Queue, Interrupt & Send and Compact & Send, each with its icon
//! and its key on the right, drawn over the chat box above the toolbar. Pointing at Send opens it
//! after a moment and it closes once the pointer leaves both; a right-click opens it until a pick,
//! a click elsewhere or Escape.
//!
//! CDXC:SessionChat 2026-11-28 DECISION:
//! User, of Send's right-click menu (Interrupt & Send, Compact & Send): "add queue as an option
//! here"; of its hover hint (Send, Queue, Compact & Send): "i dont see interrupt and send here";
//! then "combine these 2 into 1 modal please", and "make this new menu that appears on hover/right
//! click on send button appear over the chat box like [the menu] does, not outside of the chat box
//! pls". So one popup replaces both, opened by hover and by right-click, drawn in the chat's own
//! window over the chat box (as History's card menu is, sent_history.rs) rather than as a menu
//! window or a tooltip. Its rows come from one list (`send_popup_rows`), so what it offers and the
//! keys it names stay in step. From the mockup page the user picked Send first (Send, Queue,
//! Interrupt & Send, Compact & Send) and dimming a row that does not apply right now rather than
//! hiding it (Interrupt & Send with nothing to interrupt), so the rows never move. A right-click
//! opens it with an empty box too, every row dimmed; pointing at Send then shows only its tooltip. Queue names Tab only while Settings >
//! Chat > Tab to queue is on (`keyboard::tab_queues`); off, it says Hold, since holding Send still
//! queues. Option-click on Send is still Compact & Send.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, px, svg,
};
use std::time::Duration;

/// How long the pointer rests on Send before the popup opens, and how long it may be away from
/// both Send and the popup (crossing the gap between them) before it closes.
const OPEN_DELAY: Duration = Duration::from_millis(350);
const CLOSE_DELAY: Duration = Duration::from_millis(220);

/// The popup's rows, as the chat's menus draw them (option_menu/render.rs `MenuMetrics::REGULAR`).
const PADDING: f32 = 6.0;
const ROW_GAP: f32 = 2.0;
const ROW_HEIGHT: f32 = 34.0;
const WIDTH: f32 = 240.0;

/// One row: what it says, its icon, its key, whether it can run now, and the `submit` mode it runs.
pub(super) struct SendPopupRow {
    pub(super) label: &'static str,
    pub(super) icon: &'static str,
    pub(super) keys: String,
    pub(super) enabled: bool,
    pub(super) mode: &'static str,
}

/// Whether the popup is open, and how: `pinned` by a right-click (it stays until a pick, a click
/// elsewhere or Escape), else by hover (it closes once the pointer leaves).
#[derive(Default)]
pub(crate) struct SendPopupState {
    open: bool,
    pinned: bool,
    over_button: bool,
    over_popup: bool,
    timer: Option<gpui::Task<()>>,
}

impl NativeChatView {
    /// The popup's rows, in order. The one list both the popup and its keys come from.
    pub(super) fn send_popup_rows(&self) -> Vec<SendPopupRow> {
        let chord = super::keyboard::chat_chord_label;
        let can_send = self.composer_ready
            && (!self.draft.trim().is_empty()
                || self.attachments_on_their_way()
                || self.draft_coordinator_mode);
        // The same rule as Tab (keyboard.rs `composer_key_down`) and holding Send.
        let can_queue = self.composer_ready
            && !self.draft.trim().is_empty()
            && self.snapshot["queue"]["capabilities"]["canQueue"] == true;
        let working = self.snapshot["working"] == true;
        vec![
            SendPopupRow {
                label: "Send",
                icon: "titlebar/arrow-up.svg",
                keys: chord("enter"),
                enabled: can_send,
                mode: "send",
            },
            SendPopupRow {
                label: "Queue",
                icon: "titlebar/clock.svg",
                keys: if super::keyboard::tab_queues() {
                    chord("tab")
                } else {
                    "Hold".to_owned()
                },
                enabled: can_queue,
                mode: "queue",
            },
            SendPopupRow {
                label: "Interrupt & Send",
                icon: "titlebar/bolt.svg",
                keys: chord("ctrl+enter"),
                enabled: can_send && !self.draft.trim().is_empty() && working,
                mode: "interrupt",
            },
            SendPopupRow {
                label: "Compact & Send",
                icon: "titlebar/arrows-diagonal-minimize.svg",
                keys: chord("alt+enter"),
                enabled: can_queue,
                mode: "compact",
            },
        ]
    }

    pub(super) fn send_popup_open(&self) -> bool {
        self.send_popup.open
    }

    /// Right-click on Send: opens the popup until a pick, a click elsewhere or Escape; a second
    /// right-click closes it.
    pub(super) fn toggle_send_popup_pinned(&mut self, cx: &mut Context<Self>) {
        let popup = &mut self.send_popup;
        popup.timer = None;
        if popup.open && popup.pinned {
            popup.open = false;
            popup.pinned = false;
        } else {
            popup.open = true;
            popup.pinned = true;
        }
        cx.notify();
    }

    /// Closes the popup; true when it was open (Escape closes it before anything else).
    pub(super) fn close_send_popup(&mut self, cx: &mut Context<Self>) -> bool {
        let popup = &mut self.send_popup;
        popup.timer = None;
        popup.pinned = false;
        if !std::mem::take(&mut popup.open) {
            return false;
        }
        cx.notify();
        true
    }

    /// The pointer came onto or left Send.
    pub(super) fn send_button_hovered(
        &mut self,
        hovered: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.send_popup.over_button = hovered;
        self.schedule_send_popup(window, cx);
    }

    fn schedule_send_popup(&mut self, window: &Window, cx: &mut Context<Self>) {
        let popup = &mut self.send_popup;
        if popup.pinned {
            return;
        }
        let pointed = popup.over_button || popup.over_popup;
        if pointed == popup.open {
            popup.timer = None;
            return;
        }
        let delay = if pointed { OPEN_DELAY } else { CLOSE_DELAY };
        popup.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                let popup = &mut this.send_popup;
                popup.timer = None;
                if popup.pinned {
                    return;
                }
                popup.open = popup.over_button || popup.over_popup;
                cx.notify();
            });
        }));
    }

    /// The popup, hung from the top right of Send (its parent draws this at Send's corner), over
    /// the chat box and unclipped by it.
    pub(super) fn render_send_popup(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let glass = super::option_menu::ChatMenuGlass::in_window(p);
        let rows: Vec<AnyElement> = self
            .send_popup_rows()
            .into_iter()
            .map(|row| {
                let enabled = row.enabled;
                let mode = row.mode;
                div()
                    .id(SharedString::from(format!("chat-send-popup:{mode}")))
                    .role(gpui::Role::MenuItem)
                    .aria_label(row.label)
                    .flex_shrink_0()
                    .w_full()
                    .h(px(ROW_HEIGHT * s))
                    .px(px(10.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(8.0 * s))
                    .rounded(px(6.0 * s))
                    .whitespace_nowrap()
                    .opacity(if enabled { 1.0 } else { 0.42 })
                    .when(enabled, |row| {
                        let hover = glass.hover;
                        row.hover(move |style| style.bg(hover))
                            .chat_cursor_pointer()
                            .on_click(cx.listener(move |chat, _, window, cx| {
                                cx.stop_propagation();
                                chat.close_send_popup(cx);
                                chat.submit(mode, window, cx);
                            }))
                    })
                    .child(
                        svg()
                            .path(row.icon)
                            .flex_shrink_0()
                            .size(px(14.0 * s))
                            .text_color(glass.ink),
                    )
                    .child(div().flex_1().min_w_0().child(row.label))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.0 * s))
                            .text_color(glass.ink.opacity(0.58))
                            .child(row.keys),
                    )
                    .into_any_element()
            })
            .collect();
        let popup = div()
            .id("chat-send-popup")
            .role(gpui::Role::Menu)
            .aria_label("Send")
            .w(px(WIDTH * s))
            .flex()
            .flex_col()
            .gap(px(ROW_GAP * s))
            .p(px(PADDING * s))
            .rounded(glass.radius)
            .border_1()
            .border_color(glass.edge)
            // Opaque, as every card drawn over the chat's own content is: the draft sits under it.
            .bg(p.over_content_fill(p.menu_opaque().alpha(1.0)))
            .text_color(glass.ink)
            .text_size(px(13.0 * s))
            .line_height(px(18.2 * s))
            .occlude()
            .on_hover(cx.listener(|chat, hovered: &bool, window, cx| {
                chat.send_popup.over_popup = *hovered;
                chat.schedule_send_popup(window, cx);
            }))
            .on_mouse_down_out(cx.listener(|chat, _: &gpui::MouseDownEvent, _, cx| {
                // Send's own presses decide for themselves (a right-click toggles it).
                if chat.send_popup.pinned && !chat.send_popup.over_button {
                    chat.close_send_popup(cx);
                }
            }))
            .children(rows);
        div()
            .absolute()
            .top(px(-8.0 * s))
            .right(px(0.0))
            .size_0()
            .child(
                deferred(
                    anchored()
                        .anchor(gpui::Anchor::BottomRight)
                        .snap_to_window_with_margin(px(8.0 * s))
                        .child(popup),
                )
                .with_priority(1),
            )
            .into_any_element()
    }
}
