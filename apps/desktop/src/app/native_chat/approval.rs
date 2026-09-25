use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, svg,
};
use serde_json::{Value, json};

impl NativeChatView {
    pub(super) fn render_approval(
        &self,
        prompt: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let busy = self.snapshot["questionCard"]["busy"] == true;
        let header = div()
            .h(px(22.75 * s))
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .child(
                svg()
                    .path("titlebar/shield-check.svg")
                    .size(px(14.0 * s))
                    .mt(px(4.375 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_color(p.foreground)
                    .child("Approval request"),
            )
            // CDXC:SessionChat 2026-09-07 DECISION: User: card X buttons match Open terminal's color, outline and background, rather than a custom border and background of their own.
            .child(
                div()
                    .id("approval-cancel")
                    .role(gpui::Role::Button)
                    .aria_label("Dismiss")
                    .size(px(24.0 * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(p.control_border.opacity(0.65))
                    .bg(p.background.opacity(0.4))
                    .chat_cursor_pointer()
                    .hover(|style| style.bg(p.background.opacity(0.7)).text_color(p.foreground))
                    .child(
                        svg()
                            .path("titlebar/x.svg")
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.invoke(json!({"type":"questionCancel"}), cx)
                    })),
            )
            .into_any_element();
        let mut body = vec![
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(12.0 * s))
                .child(
                    div()
                        .min_w_0()
                        .text_color(p.card_muted)
                        .line_height(px(19.6 * s))
                        .child("Allow this command?"),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0 * s))
                        .line_height(px(18.0 * s))
                        .child(text(prompt, "tool")),
                )
                .into_any_element(),
        ];
        let summary = text(prompt, "summary");
        if !summary.is_empty() {
            body.push(
                div()
                    .min_w_0()
                    .p(px(12.0 * s))
                    .border_1()
                    .border_color(p.control_border.opacity(0.65))
                    .rounded(px(8.0 * s))
                    .bg(p.background.opacity(0.7))
                    .child(
                        div()
                            .id("approval-command")
                            .max_h(px(160.0 * s))
                            .overflow_y_scroll()
                            .font_family("Menlo")
                            .text_size(px(14.0 * s))
                            .line_height(px(22.75 * s))
                            .text_color(p.card_muted)
                            .child(summary),
                    )
                    .into_any_element(),
            );
        }
        let actions = [
            ("approval-deny", "Deny", ""),
            ("approval-allow", "Allow", "1"),
        ]
        .into_iter()
        .map(|(id, label, send)| {
            self.question_button(
                id,
                label,
                json!({"type":"answer","answer":{"kind":"approval","approvalSend":send}}),
                busy,
                false,
                false,
                p,
                cx,
            )
            .into_any_element()
        })
        .collect();
        self.status_card_with_header(header, body, actions, p)
    }
}
