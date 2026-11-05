//! One-click answers under an agent's question: the marked line `transcript/inline_answers.rs`
//! in gx-chat-core puts under the question, drawn as one compact segmented pill. The state is the
//! question's one-question form (`questionForms`): open, sending, queued, answered, and whether the
//! reader is changing the answer; this file only lays it out and sends the clicks.
//!
//! CDXC:SessionChat 2026-11-05 SEE-ALSO: The decision (Q1245 C, the segmented pill on the line under the question) is in packages/gx-chat-core/src/transcript/inline_answers.rs; the phone draws the same pill in apps/mobile/app/src/chat/native/transcript/markdown/InlineAnswers.tsx.

use super::{appearance::ChatAppearance, state::NativeChatView};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use serde_json::{Value, json};

impl NativeChatView {
    /// The answers of one question (`mark`, the marked line's JSON), at the list depth the
    /// question sits at.
    pub(super) fn inline_answers(
        &self,
        key: &str,
        mark: &Value,
        depth: usize,
        p: &ChatAppearance,
        cx: &gpui::Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let form_id = mark["form"].as_str().unwrap_or_default().to_string();
        let form = &self.snapshot["questionForms"][&form_id];
        let phase = form["state"].as_str().unwrap_or("open");
        let changing = form["changing"] == true;
        let withdrawn = form["tagWithdrawn"] == true || matches!(phase, "withdrawn" | "canceled");
        let picked = form["drafts"][0]["indices"][0].as_u64();
        let sent = matches!(phase, "sending" | "queued" | "answered");
        let clickable = !withdrawn && (phase == "open" || (changing && phase != "sending"));
        let edge = p.foreground.opacity(0.16);
        let ink = p.foreground;
        let ink_text: gpui::Hsla = if p.light {
            rgb(0xfafafa).into()
        } else {
            rgb(0x111113).into()
        };
        let choices = mark["choices"].as_array().cloned().unwrap_or_default();
        let count = choices.len();
        let segments = choices.iter().enumerate().map(|(index, choice)| {
            let label = choice["label"].as_str().unwrap_or_default().to_string();
            let chosen = sent && picked == Some(index as u64);
            let recommended = choice["recommended"] == true && !sent;
            let faded = (sent && !chosen && !changing) || withdrawn;
            let action = json!({"type":"inlineAnswer","form":form_id,"choice":index});
            div()
                .id(gpui::SharedString::from(format!(
                    "inline-answer:{key}:{index}"
                )))
                .flex()
                .items_center()
                .gap(px(5.0 * s))
                .h(px(24.0 * s))
                .px(px(11.0 * s))
                .flex_none()
                .text_size(px(12.5 * s))
                .font_weight(FontWeight::SEMIBOLD)
                .when(index > 0, |segment| segment.border_l_1().border_color(edge))
                .when(chosen, |segment| segment.bg(ink).text_color(ink_text))
                .when(!chosen, |segment| {
                    segment.text_color(ink).bg(p.foreground.opacity(0.035))
                })
                .when(faded, |segment| segment.opacity(0.4))
                .when(recommended, |segment| {
                    segment.child(
                        div()
                            .size(px(5.0 * s))
                            .rounded_full()
                            .bg(p.foreground.opacity(0.55)),
                    )
                })
                .child(label.clone())
                .when(chosen, |segment| segment.child("✓"))
                .when(clickable && !(chosen && !changing), |segment| {
                    segment
                        .chat_cursor_pointer()
                        .hover(|style| style.bg(p.foreground.opacity(0.09)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.invoke(action.clone(), cx);
                        }))
                })
                .when(recommended, |segment| {
                    segment.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                    })
                })
                .into_any_element()
        });
        let pill = div()
            .id(gpui::SharedString::from(format!("inline-answers:{key}")))
            .flex()
            .flex_none()
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(edge)
            .overflow_hidden()
            .children(segments);
        let status = if withdrawn {
            Some(("Withdrawn".to_string(), None))
        } else if changing {
            Some(("Pick another answer ·".to_string(), Some("Cancel")))
        } else {
            match phase {
                "sending" => Some(("Sending…".to_string(), None)),
                "queued" => Some(("Queued ·".to_string(), Some("Change"))),
                "answered" => Some(("Sent ·".to_string(), Some("Change"))),
                _ => None,
            }
        };
        let status = status.map(|(text, link)| {
            let toggle = json!({"type":"inlineAnswerChange","form":form_id});
            div()
                .flex()
                .items_center()
                .gap(px(4.0 * s))
                .text_size(px(11.5 * s))
                .text_color(p.card_muted)
                .child(text)
                .when_some(link, |row, link| {
                    row.child(
                        div()
                            .id(gpui::SharedString::from(format!(
                                "inline-answer-change:{key}"
                            )))
                            .underline()
                            .chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .child(link)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.invoke(toggle.clone(), cx);
                            })),
                    )
                })
        });
        let row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .pl(px(super::markdown_style::LIST_GUTTER * s * depth as f32))
            .when(count > 0, |row| row.child(pill))
            .children(status)
            .into_any_element();
        self.open_question_frame(Some(&form_id), row, p)
    }
}
