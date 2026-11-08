//! The card above the chat box while the session's computer is offline (`computerNotice`, decided
//! in packages/gx-chat-core/src/session/computer.rs): "Windows PC is offline · last seen 2h ago"
//! and Retry, or "Windows PC isn't answering · checking…" with a spinner while a message to it got
//! no answer.
//!
//! CDXC:RemoteMachines 2026-11-08 DECISION:
//! User, on the offline-notice page: the card above the chat box (Q1831 "card"), with the "Worked for …" bar swapped below it ("pls swap the placement of the working bar and this new card"), Retry as its only action (Q1835), and nothing red: a computer being off is expected. It is one line in the working card's own capsule, so the two read as one stack. The phone draws the same card in apps/mobile/app/src/chat/native/cards/ComputerCard.tsx.

use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    cursor::ChatCursor as _,
    state::NativeChatView,
    transcript::text,
};
use crate::app::helpers::ThrottledAnimationExt as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, relative, svg,
};
use serde_json::json;

impl NativeChatView {
    pub(super) fn render_computer_card(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let notice = &self.snapshot["computerNotice"];
        if !notice.is_object() {
            return None;
        }
        let s = p.scale;
        let title = text(notice, "title");
        let detail = text(notice, "detail");
        let checking = notice["checking"] == true;
        let label = if detail.is_empty() {
            title.clone()
        } else {
            format!("{title} \u{b7} {detail}")
        };
        let trailing = if checking {
            let glyph = svg()
                .path("titlebar/loader2.svg")
                .size(px(12.0 * s))
                .text_color(p.muted);
            if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                glyph.into_any_element()
            } else {
                glyph
                    .with_throttled_animation(
                        "computer-card-spinner",
                        std::time::Duration::from_millis(900),
                        |svg, delta| {
                            svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                delta * std::f32::consts::TAU,
                            )))
                        },
                    )
                    .into_any_element()
            }
        } else {
            div()
                .id("computer-card-retry")
                .role(gpui::Role::Button)
                .aria_label("Retry")
                .chat_cursor_pointer()
                .flex_shrink_0()
                .px(px(8.0 * s))
                .py(px(1.0 * s))
                .rounded(px(6.0 * s))
                .border_1()
                .chat_filled(p.neutral_button_fills(), true)
                .text_size(px(12.0 * s))
                .text_color(p.primary)
                .child("Retry")
                .on_click(
                    cx.listener(|this, _, _, cx| this.invoke(json!({"type":"computerRetry"}), cx)),
                )
                .into_any_element()
        };
        Some(
            div()
                .id("chat-computer-card")
                .role(gpui::Role::Status)
                .aria_label(label.clone())
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .min_h(px(34.0 * s))
                .px(px(12.0 * s))
                .py(px(6.0 * s))
                .border_1()
                .border_color(p.input_border)
                .rounded_full()
                .bg(p.composer_background)
                .child(
                    svg()
                        .path("titlebar/device-desktop.svg")
                        .size(px(14.0 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.0 * s))
                        .line_height(relative(1.5))
                        .text_color(p.card_muted)
                        .child(label),
                )
                .child(trailing)
                .into_any_element(),
        )
    }
}
