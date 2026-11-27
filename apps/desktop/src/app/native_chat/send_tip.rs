//! The one-time "two more ways to send" tip on the Send button: a breathing dot on Send's corner
//! that opens a small popup about Tab to queue and Compact & Send.
//!
//! CDXC:SessionChat 2026-11-27 DECISION:
//! User: "i want the user to be told about these by putting a flashing circle on this button when
//! he clicks on it he sees explanation small pop up that tells him about tab to queue and compact
//! and send". From the mockup page the user picked (Q5197 to Q5200): everyone sees the dot once,
//! from this update on; only opening the popup puts the dot away for good (queueing with Tab or a
//! Compact & Send on their own does not); clicking the dot opens the popup while the rest of Send
//! still sends; the copy as mocked up, with ⌥-click / Alt-click drawn as key caps like ⌥ Enter.
//! The dot is the sidebar's "unseen" blue (native_sidebar/status.rs `ATTENTION_COLOR`); the
//! popup takes the chat's floating-card fill and the Send tooltip's key caps.
//!
//! Desktop only: the browser build has no state folder to remember that the tip was seen, so it
//! would come back on every load.

use super::appearance::{ChatAppearance, ChatFilledButton as _};
use super::state::NativeChatView;
use crate::app::helpers::{ThrottledAnimationExt as _, gpui_macos_reduce_motion_enabled};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, anchored, deferred, div, point, px,
};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

const UNKNOWN: u8 = 0;
const PENDING: u8 = 1;
const SEEN: u8 = 2;

/// Whether the tip was seen, read from disk once per process and shared by every chat.
static TIP_STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

fn tip_seen_path() -> std::path::PathBuf {
    crate::app::helpers::ghostex_state_root().join("gpui-send-tip-seen.json")
}

/// Whether Send still wears the tip's dot.
pub(super) fn send_tip_pending() -> bool {
    if cfg!(ghostex_web) {
        return false;
    }
    match TIP_STATE.load(Ordering::Relaxed) {
        PENDING => true,
        SEEN => false,
        _ => {
            let seen = tip_seen_path().exists();
            TIP_STATE.store(if seen { SEEN } else { PENDING }, Ordering::Relaxed);
            !seen
        }
    }
}

fn mark_send_tip_seen() {
    TIP_STATE.store(SEEN, Ordering::Relaxed);
    let path = tip_seen_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, r#"{"seen":true}"#);
}

/// The dot's blue: the sidebar's unseen dot in dark, a deeper blue that reads on the light Send.
fn dot_color(light: bool) -> gpui::Hsla {
    gpui::rgb(if light {
        0x2f8fd8
    } else {
        crate::app::native_sidebar::status::ATTENTION_COLOR
    })
    .into()
}

/// One key cap, the Send tooltip's look (gpui-component tooltip.rs `key_caps`).
fn key_cap(label: &str, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .min_w(px(17.0 * s))
        .px(px(4.0 * s))
        .rounded(px(4.0 * s))
        .bg(p.foreground.opacity(0.1))
        .border_1()
        .border_color(p.foreground.opacity(0.08))
        .text_center()
        .text_size(px(11.0 * s))
        .line_height(px(16.0 * s))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.foreground.opacity(0.82))
        .child(label.to_string())
        .into_any_element()
}

/// A chord's caps side by side, one per key.
fn chord_caps(keys: &[&str], p: &ChatAppearance) -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * p.scale))
        .children(keys.iter().map(|key| key_cap(key, p)))
        .into_any_element()
}

impl NativeChatView {
    /// Send with the tip's dot on its corner and, while open, the popup above it.
    pub(super) fn wrap_send_with_tip(
        &self,
        button: AnyElement,
        show_dot: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let color = dot_color(p.light);
        // The ring breathes out of the dot's centre (7px into the 14px click area) and fades.
        let ring_at = move |ring: gpui::Div, diameter: f32| {
            ring.left(px((7.0 - diameter / 2.0) * s))
                .top(px((7.0 - diameter / 2.0) * s))
                .size(px(diameter * s))
        };
        let ring = div().absolute().rounded_full().bg(color);
        let ring = if gpui_macos_reduce_motion_enabled() {
            ring_at(ring, 8.0).opacity(0.0).into_any_element()
        } else {
            ring_at(ring, 8.0)
                .with_throttled_animation(
                    "chat-send-tip-breathe",
                    Duration::from_millis(1_800),
                    move |ring, frame| {
                        let grow = (frame / 0.8).min(1.0);
                        ring_at(ring, 8.0 * (1.0 + 1.6 * grow)).opacity(0.7 * (1.0 - grow))
                    },
                )
                .into_any_element()
        };
        // The dot's click area is 14px, a little bigger than the 8px dot, over Send's corner; it
        // takes the mouse so a click on it never also sends.
        let dot = div()
            .id("chat-send-tip-dot")
            .role(gpui::Role::Button)
            .aria_label("New: two more ways to send")
            .absolute()
            .top(px(-6.0 * s))
            .right(px(-6.0 * s))
            .size(px(14.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .chat_cursor_pointer()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("New: two more ways to send")
                    .build(window, cx)
            })
            .child(ring)
            .child(
                div()
                    .size(px(10.0 * s))
                    .rounded_full()
                    .bg(color)
                    .border(px(1.5 * s))
                    .border_color(p.background),
            )
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                mark_send_tip_seen();
                this.send_tip_open = true;
                cx.stop_propagation();
                cx.notify();
                // Every other open chat drops its dot too.
                cx.refresh_windows();
            }));
        div()
            .relative()
            .flex_shrink_0()
            .child(button)
            .when(show_dot, |this| this.child(dot))
            .when(self.send_tip_open, |this| {
                this.child(self.render_send_tip_popup(p, cx))
            })
            .into_any_element()
    }

    fn render_send_tip_popup(&self, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let mac = super::keyboard::chat_mac_keys();
        let alt = if mac { "⌥" } else { "Alt" };
        let line = p.foreground.opacity(0.08);
        let row = |name: &'static str, keys: AnyElement, body: &'static str| {
            div()
                .px(px(12.0 * s))
                .py(px(8.0 * s))
                .border_t_1()
                .border_color(line)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0 * s))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(p.foreground)
                                .child(name),
                        )
                        .child(keys),
                )
                .child(
                    div()
                        .mt(px(3.0 * s))
                        .text_size(px(12.0 * s))
                        .line_height(px(17.0 * s))
                        .text_color(p.muted)
                        .child(body),
                )
        };
        let compact_keys = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(5.0 * s))
            .child(chord_caps(&[alt, "Enter"], p))
            .child(
                div()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child("or"),
            )
            .child(chord_caps(&[alt, "Click"], p))
            .into_any_element();
        let card = div()
            .id("chat-send-tip")
            .role(gpui::Role::Dialog)
            .aria_label("Two more ways to send")
            .w(px(300.0 * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.border)
            .bg(p.over_content_fill(p.menu_opaque()))
            .shadow_lg()
            .text_size(px(13.0 * s))
            .line_height(px(18.0 * s))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.send_tip_open = false;
                cx.notify();
            }))
            .child(
                div()
                    .px(px(12.0 * s))
                    .pt(px(10.0 * s))
                    .pb(px(6.0 * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child("Two more ways to send"),
            )
            .child(row(
                "Queue",
                chord_caps(&["Tab"], p),
                "Type your next message while the agent works. It waits in line and sends when the agent is done.",
            ))
            .child(row(
                "Compact & Send",
                compact_keys,
                "Shrinks the chat's memory first, then sends. Use it when the ring is nearly full. Right-click Send for it too.",
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .px(px(12.0 * s))
                    .pt(px(8.0 * s))
                    .pb(px(10.0 * s))
                    .border_t_1()
                    .border_color(line)
                    .child(
                        div()
                            .id("chat-send-tip-done")
                            .role(gpui::Role::Button)
                            .h(px(24.0 * s))
                            .px(px(12.0 * s))
                            .flex()
                            .items_center()
                            .rounded(px(6.0 * s))
                            .border_1()
                            .text_size(px(12.0 * s))
                            .chat_filled(p.neutral_button_fills(), true)
                            .text_color(p.primary)
                            .chat_cursor_pointer()
                            .child("Got it")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.send_tip_open = false;
                                cx.notify();
                            })),
                    ),
            );
        let send = self.composer_button_bounds.send.get();
        deferred(
            anchored()
                .position(point(send.right(), send.top() - px(8.0 * s)))
                .anchor(gpui::Anchor::BottomRight)
                .snap_to_window_with_margin(px(8.0 * s))
                .child(card),
        )
        .with_priority(1)
        .into_any_element()
    }
}
