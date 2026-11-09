//! Your messages under the opt-in chat look (`chatLook` in the chat document): Message layout's
//! Left aligned and Your messages' Band. At Default nothing here runs and your message is today's
//! right-aligned bubble (transcript.rs `message_row`).

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use ghostex_gx_chat_core::transcript::chat_look::{MessageLayout, UserStyle};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, px, relative};
use serde_json::Value;

impl ChatAppearance {
    /// Whether this chat draws your messages on the left (Message layout, or Your messages Band,
    /// which the core already folded into `messageLayout`).
    pub(crate) fn left_aligned(&self) -> bool {
        self.chat_look.effective_layout() == MessageLayout::Left
    }
}

impl NativeChatView {
    /// Your messages and Message layout on your rows. Left aligned keeps the bubble and moves it to the agent's left edge; Band draws your message across the column on a fill one step off the bubble's, in brighter text. Each style applies to every row you wrote (sent, queued, waiting or not sent); the delivery and queued icons sit at the message's right end and the hover actions under it on the left.
    ///
    /// CDXC:SessionChat 2026-11-07 SEE-ALSO:
    /// packages/gx-chat-core/src/transcript/chat_look.rs (the user decisions, the settings and the implied Left), apps/mobile/app/src/chat/native/transcript/userMessageLook.tsx (the phone draws the same looks).
    pub(super) fn left_user_message(
        &mut self,
        row: gpui::Stateful<gpui::Div>,
        message: &Value,
        body: String,
        focused: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = text(message, "id");
        let id = id.as_str();
        let s = p.scale;
        let style = p.chat_look.user_style;
        let mut text_look = p.clone();
        text_look.prose = match style {
            UserStyle::Default => p.strengthened(p.primary),
            UserStyle::Band => p.user_style_prose(),
        };
        text_look.code_text = p.primary;
        let indicator = self.delivery_indicator(message, p, cx);
        let prompt = div()
            .min_w_0()
            // A slash command reads as one, in the chat's monospace (gx-chat-core `is_slash_command_line`).
            .when(message["slashCommand"] == true, |prompt| {
                prompt.font_family(super::fonts::CHAT_MONO)
            })
            .child(self.with_loading_pictures(
                id,
                self.prompt_body(id, message, body.clone(), &text_look, p, cx),
                !body.trim().is_empty(),
                p,
            ));
        let frame = match style {
            UserStyle::Default => {
                // The bubble's indicator hangs off its right edge, level with its last line, at the
                // same 6px gap the right-aligned bubble keeps on its left.
                let indicator = indicator.map(|(icon, slots)| {
                    let offset = 31.0 + 26.0 * slots.saturating_sub(1) as f32;
                    div()
                        .absolute()
                        .bottom(px(11.0 * s))
                        .right(px(-offset * s))
                        .child(icon)
                });
                div()
                    .relative()
                    .max_w(relative(0.8))
                    .min_w_0()
                    .rounded(px(super::chat_text_look::USER_BUBBLE_RADIUS * s))
                    .border(px(1.0 * s))
                    .border_color(gpui::transparent_black())
                    .p(px(super::chat_text_look::USER_BUBBLE_PADDING * s))
                    .bg(p.input)
                    .child(prompt)
                    .children(indicator)
            }
            // Full width, so the icons take the row's right end inside it.
            UserStyle::Band => div()
                .w_full()
                .min_w_0()
                .flex()
                .items_end()
                .gap(px(8.0 * s))
                .child(prompt.flex_1())
                .children(indicator.map(|(icon, _)| icon))
                .rounded(px(8.0 * s))
                .px(px(12.0 * s))
                .py(px(8.0 * s))
                .bg(p.user_band),
        };
        let failed_delivery = self.render_startup_delivery(message, false, p, cx);
        let thumbnails = self.user_image_thumbnails(message, p, cx);
        let actions = self.user_actions(message, focused, p, cx);
        row.children(thumbnails)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(4.0 * s))
                    .child(frame)
                    .children(failed_delivery)
                    .children(actions),
            )
            .into_any_element()
    }
}
