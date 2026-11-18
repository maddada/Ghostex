//! The card at the top of the chat box while a new coordinator's first message waits in its
//! agent's input box: the message (two lines, Show all for the rest), Edit and Start. What it says
//! and does comes from the chat core (packages/gx-chat-core/src/composer/first_message.rs, the
//! `firstMessageCard` key); this file only lays it out, quiet like a queued message's row.
//!
//! CDXC:Coordinators 2026-11-18 SEE-ALSO: the user's decision (Q3950 A: a card inside the chat
//! box) is recorded in server/src/session_chat_first_message.rs; the phone draws the same card in
//! apps/mobile/app/src/chat/native/composer/FirstMessageCard.tsx.

use super::tone_card::ToneButton;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use serde_json::json;

/// A message longer than this many characters gets Show all even on two lines (they wrap).
const LONG_TEXT_CHARS: usize = 140;

impl NativeChatView {
    pub(super) fn render_first_message_card(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = self.snapshot["firstMessageCard"].clone();
        if !card.is_object() {
            return None;
        }
        let s = p.scale;
        let busy = card["busy"] == true;
        let expanded = card["expanded"] == true;
        let collapsed_lines = card["collapsedLines"].as_u64().unwrap_or(2) as usize;
        let message = text(&card, "text");
        let preview = text(&card, "preview");
        let expandable = card["lines"].as_u64().unwrap_or(0) as usize > collapsed_lines
            || message.chars().count() > LONG_TEXT_CHARS;
        let edit = self
            .tone_button(
                "first-message-edit",
                &text(&card, "editLabel"),
                Some("titlebar/pencil.svg"),
                ToneButton::Secondary,
                None,
                busy,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.invoke(json!({ "type": "firstMessageEdit" }), cx);
            }));
        let start = self
            .tone_button(
                "first-message-start",
                &text(&card, "startLabel"),
                Some("titlebar/player-play.svg"),
                ToneButton::Primary,
                None,
                busy,
                p,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.invoke(json!({ "type": "firstMessageStart" }), cx);
            }));
        let header = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(6.0 * s))
            .text_size(px(12.0 * s))
            .line_height(px(18.0 * s))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.foreground)
                    .child(text(&card, "title")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .text_color(p.muted)
                    .child(format!("· {}", text(&card, "note"))),
            )
            .child(edit)
            .child(start);
        let body = div()
            .id("first-message-text")
            .min_w_0()
            .text_size(px(13.0 * s))
            .line_height(px(19.0 * s))
            .text_color(p.primary)
            // GPUI clamps only when the text also asks for an ellipsis (system_cards.rs).
            .when(!expanded, |body| {
                body.line_clamp(collapsed_lines).text_ellipsis()
            })
            .when(expanded, |body| {
                body.max_h(px(220.0 * s)).overflow_y_scroll()
            })
            .child(if expanded || preview.is_empty() {
                message.clone()
            } else {
                preview
            });
        let toggle = expandable.then(|| {
            let label = if expanded {
                text(&card, "showLessLabel")
            } else {
                text(&card, "showAllLabel")
            };
            div()
                .id("first-message-toggle")
                .role(gpui::Role::Button)
                .aria_label(label.clone())
                .chat_cursor_pointer()
                .flex()
                .items_center()
                .gap(px(3.0 * s))
                .text_size(px(12.0 * s))
                .line_height(px(18.0 * s))
                .text_color(p.muted)
                .hover(|style| style.text_color(p.foreground))
                .child(label)
                .child(
                    gpui::svg()
                        .path(if expanded {
                            "titlebar/chevron-up.svg"
                        } else {
                            "titlebar/chevron-down.svg"
                        })
                        .size(px(11.0 * s))
                        .text_color(p.muted),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invoke(json!({ "type": "firstMessageToggle" }), cx);
                }))
        });
        Some(
            div()
                .id("first-message-card")
                .role(gpui::Role::Group)
                .aria_label(text(&card, "title"))
                .flex()
                .flex_col()
                .min_w_0()
                .gap(px(4.0 * s))
                .mb(px(6.0 * s))
                .pl(px(12.0 * s))
                .pr(px(8.0 * s))
                .py(px(8.0 * s))
                .rounded(px(12.0 * s))
                .border_1()
                .border_color(p.border.opacity(0.58))
                .bg(p.card_row)
                .when(busy, |this| this.opacity(0.7))
                .child(header)
                .child(body)
                .children(toggle)
                .into_any_element(),
        )
    }
}
