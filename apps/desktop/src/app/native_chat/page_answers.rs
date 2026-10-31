//! The answers a reader sent from an agent's HTML page, drawn as a card instead of the JSON message
//! gxserver typed into the session (`pageAnswers`, packages/gx-chat-core/src/transcript/page_answers.rs).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The phone draws the same card in apps/mobile/app/src/chat/native/transcript/PageAnswersCard.tsx; the page's own card reads Answered in visual.rs. Change them together.

use super::ask_card::{answer_aside, answered_ask_head, plain_answer, receipt_row};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb, svg,
};
use serde_json::{Value, json};

/// The green the chat's charts use for good news, which also marks an answered page.
pub(super) fn answered_color(p: &ChatAppearance) -> Hsla {
    rgb(if p.light { 0x059669 } else { 0x34d399 }).into()
}

impl NativeChatView {
    /// CDXC:SessionChat 2026-10-12 DECISION: User, on the answer card under an answered form: "please make sure the selected answer doesn't have that green thing around it, make it more simple should just say the answer and question there with bit different color/fontweight etc (like we have in other cards) but no background behind the answer like you're showing / also plase remove "1 answer" no need for it". The card keeps its green check tile and numbered rows; each answer is plain foreground text at medium weight under its muted question, with no chip, border or fill, and the head has no answer count. Supersedes the 2026-10-11 green answer chips on this card. The agent's own question receipts (question_exchange.rs) take the same plain answer (`plain_answer`, ask_card.rs).
    ///
    /// `delivery` is the echo's `startupDelivery` while the answers have not reached the
    /// transcript yet (gx-chat-core `session/answer_echoes.rs`): the card shows at once with a
    /// spinner at its bottom left, or the delivery's error with Retry and Remove.
    pub(super) fn page_answers_card(
        &self,
        answers: &Value,
        delivery: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let rows: Vec<Value> = answers["rows"].as_array().cloned().unwrap_or_default();
        let more = answers["more"].as_u64().unwrap_or(0) as usize;
        let head = answered_ask_head(format!("You answered “{}”", text(answers, "title")), p)
            .into_any_element();
        let s = p.scale;
        let mut body = div().mt(px(-6.0 * s)).flex().flex_col().w_full().min_w_0();
        for (index, row) in rows.iter().enumerate() {
            let value = text(row, "value");
            let note = text(row, "note");
            // CDXC:SessionChat 2026-10-11 DECISION: User: "make it show the Q above the Answer so they both have more width area": each row is the question (muted) on its own line, the answer below it at the card's full width, then its note. The phone (PageAnswersCard.tsx) stacks the same way.
            let answer = plain_answer(value, p);
            let below = if note.is_empty() {
                Vec::new()
            } else {
                vec![answer_aside(note, false, p)]
            };
            body = body.child(receipt_row(
                index + 1,
                text(row, "label"),
                vec![answer],
                below,
                index == 0,
                p,
            ));
        }
        if more > 0 {
            body = body.child(div().pt(px(4.0 * s)).child(answer_aside(
                format!("and {more} more"),
                false,
                p,
            )));
        }
        if delivery.is_object() {
            body = body.child(self.answers_delivery(answers, delivery, p, cx));
        }
        self.lined_ask_card(head, Some(body.into_any_element()), p)
    }

    /// The bottom-left line of answers on their way: a spinner while the queue holds or types
    /// them, with Remove while they are only held; why they could not be delivered with Retry (the
    /// queue keeps the answers) and Remove; or, once they left the queue and the conversation never
    /// showed them, that fact with Remove and no spinner
    /// (CDXC:SessionChat 2026-10-31 in packages/gx-chat-core/src/session/startup_sends.rs).
    fn answers_delivery(
        &self,
        answers: &Value,
        delivery: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let failed = delivery["state"] == "failed";
        let prompt_id = text(delivery, "promptId");
        let key = text(answers, "pageId");
        let line = div()
            .id(gpui::SharedString::from(format!("answers-delivery:{key}")))
            .pt(px(10.0 * s))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0 * s))
            .text_size(px(12.0 * s))
            .line_height(px(18.0 * s));
        let action = |id: String, label: &'static str, command: Value| {
            div()
                .id(gpui::SharedString::from(id))
                .role(gpui::Role::Button)
                .aria_label(label)
                .chat_cursor_pointer()
                .px(px(6.0 * s))
                .py(px(2.0 * s))
                .rounded(px(5.0 * s))
                .text_color(p.primary)
                .hover(|style| style.bg(p.border))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.invoke(command.clone(), cx)))
        };
        let remove = || {
            action(
                format!("answers-remove:{key}"),
                "Remove",
                json!({"type":"removeQueue","promptId":prompt_id.clone()}),
            )
        };
        if delivery["state"] == "unconfirmed" {
            return line
                .role(gpui::Role::Status)
                .text_color(p.muted)
                .child(text(delivery, "waitReason"))
                .when(!prompt_id.is_empty(), |line| line.child(remove()))
                .into_any_element();
        }
        if !failed {
            let reason = text(delivery, "waitReason");
            let removable = delivery["state"] == "queued"
                && !prompt_id.is_empty()
                && self.snapshot["queue"]["capabilities"]["canRemove"] == true;
            let label: gpui::SharedString = if delivery["state"] == "queued" && !reason.is_empty() {
                // Why gxserver still holds the answers, once they have waited about a minute.
                reason.into()
            } else if delivery["state"] == "queued" {
                "Waiting for the agent to be free…".into()
            } else if delivery["state"] == "reconnecting" {
                "Reconnecting to Ghostex… your answers go out as soon as it's back.".into()
            } else {
                "Sending to the agent…".into()
            };
            let glyph = svg()
                .path("titlebar/loader2.svg")
                .size(px(12.0 * s))
                .text_color(p.muted);
            let glyph = if crate::app::helpers::gpui_macos_reduce_motion_enabled() {
                glyph.into_any_element()
            } else {
                glyph
                    .with_throttled_animation(
                        gpui::SharedString::from(format!("answers-spinner:{key}")),
                        std::time::Duration::from_millis(900),
                        |svg, delta| {
                            svg.with_transformation(gpui::Transformation::rotate(gpui::radians(
                                delta * std::f32::consts::TAU,
                            )))
                        },
                    )
                    .into_any_element()
            };
            return line
                .role(gpui::Role::Status)
                .aria_label(label.clone())
                .text_color(p.muted)
                .child(glyph)
                .child(label)
                .when(removable, |line| line.child(remove()))
                .into_any_element();
        }
        let message = text(delivery, "errorMessage");
        let message = if message.is_empty() {
            "Your answers could not be delivered.".to_string()
        } else {
            format!("Your answers could not be delivered: {message}")
        };
        let error: Hsla = rgb(if p.light { 0xb91c1c } else { 0xfb7185 }).into();
        line.role(gpui::Role::Alert)
            .child(div().text_color(error).child(message))
            .when(!prompt_id.is_empty(), |line| {
                line.child(action(
                    format!("answers-retry:{key}"),
                    "Retry",
                    json!({"type":"retryQueue","promptId":prompt_id.clone()}),
                ))
                .child(remove())
            })
            .into_any_element()
    }
}
