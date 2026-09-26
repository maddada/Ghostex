//! The composer's refusal card for `composerNotReady`, the one send failure the
//! user can fix: the agent CLI is sitting on a trust prompt, an auth screen or a
//! first-run step and never painted an input box.
//!
//! "Message could not be sent" is useless for that, so the card says what is
//! wrong and offers the two ways out React offered (session-chat-composer-not-
//! ready.tsx): a read-only excerpt of the CURRENT screen, re-read on every
//! expand, and a switch to the session's terminal surface.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

const NOT_READY_HEADLINE: &str = "Message not sent. Your draft was restored.";

impl NativeChatView {
    pub(super) fn composer_not_ready(&self) -> bool {
        self.snapshot["operationErrorCode"] == "composerNotReady"
            && self.snapshot["operationError"].is_string()
    }

    pub(super) fn render_composer_not_ready(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        if !self.composer_not_ready() {
            return None;
        }
        let s = p.scale;
        let notice = self.snapshot["terminalTail"]["notice"].clone();
        let open = notice["open"] == true;
        let mut body = Vec::new();
        let reason = text(&self.snapshot, "operationError");
        if !reason.is_empty() && reason != NOT_READY_HEADLINE {
            body.push(
                div()
                    .line_height(px(19.6 * s))
                    .text_color(p.card_muted)
                    .child(reason)
                    .into_any_element(),
            );
        }
        if open {
            let excerpt = text(&notice, "excerpt");
            let inner = if notice["loading"] == true {
                div()
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child("Reading the terminal…")
                    .into_any_element()
            } else if let Some(error) = notice["error"].as_str() {
                div()
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(error.to_owned())
                    .into_any_element()
            } else if !excerpt.is_empty() {
                div()
                    .id("composer-not-ready-tail")
                    .max_h(px(192.0 * s))
                    .overflow_y_scroll()
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .font_family("Menlo")
                    .text_size(px(11.0 * s))
                    .line_height(px(16.0 * s))
                    .text_color(p.foreground)
                    .child(excerpt)
                    .into_any_element()
            } else {
                div()
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.card_muted)
                    .child(text(&notice, "empty"))
                    .into_any_element()
            };
            body.push(
                div()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(px(8.0 * s))
                    .border_1()
                    .border_color(p.input_border)
                    .bg(p.background.opacity(0.7))
                    .child(inner)
                    .into_any_element(),
            );
        }
        let actions = vec![
            div()
                .id("composer-not-ready-toggle")
                .role(gpui::Role::Button)
                .aria_label(if open {
                    "Hide terminal"
                } else {
                    "Show terminal"
                })
                .chat_cursor_pointer()
                .flex()
                .items_center()
                .gap(px(6.0 * s))
                .px(px(8.0 * s))
                .py(px(4.0 * s))
                .rounded(px(6.0 * s))
                .border_1()
                .border_color(p.border)
                .text_color(p.primary)
                .hover(|style| style.bg(p.border))
                .child(
                    gpui::svg()
                        .path(if open {
                            "titlebar/chevron-down.svg"
                        } else {
                            "titlebar/chevron-right.svg"
                        })
                        .size(px(14.0 * s))
                        .text_color(p.primary),
                )
                .child(if open {
                    "Hide terminal"
                } else {
                    "Show terminal"
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invoke(json!({"type":"terminalTailToggle"}), cx)
                }))
                .into_any_element(),
            self.host_button("terminalView", "titlebar/terminal-2.svg", p, cx),
        ];
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .child(
                gpui::svg()
                    .path("titlebar/alert-circle.svg")
                    .size(px(14.0 * s))
                    .mt(px(4.0 * s))
                    .text_color(gpui::rgb(0xef9999))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex_1()
                    .text_color(p.foreground)
                    .child(NOT_READY_HEADLINE),
            )
            .into_any_element();
        Some(
            div()
                .id("composer-not-ready")
                .role(gpui::Role::Alert)
                .w_full()
                .child(self.status_card_with_header(header, body, actions, p))
                .into_any_element(),
        )
    }
}
