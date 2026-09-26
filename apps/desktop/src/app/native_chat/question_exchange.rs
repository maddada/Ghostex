//! Answered question cards in the transcript.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! The rule that lifts a turn's exchanges out of its collapsed work fold lives in
//! packages/gx-chat-core/src/questions/hoisting.rs. The live question card
//! (question.rs) and this settled one deliberately share their choice rows, so an answer reads the
//! same after the fact as it did while it was being given.

use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, div, px,
};
use serde_json::Value;

impl NativeChatView {
    /// Every answered exchange carried by one message or hoisted onto one completed turn.
    pub(super) fn question_exchange_cards(
        &self,
        key: &str,
        exchanges: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let exchanges = exchanges.as_array().filter(|list| !list.is_empty())?;
        let mut column = div()
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .gap(px(12.0 * p.scale))
            .py(px(6.0 * p.scale));
        for (index, exchange) in exchanges.iter().enumerate() {
            column = column.child(self.question_exchange_card(
                &format!("{key}:{index}"),
                exchange,
                p,
                cx,
            ));
        }
        Some(column.into_any_element())
    }

    fn question_exchange_card(
        &self,
        key: &str,
        exchange: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let questions = exchange["questions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let answers = exchange["answers"].as_array().cloned();
        let total = questions.len();
        let mut card = div()
            .flex()
            .flex_col()
            .min_w_0()
            .w_full()
            .rounded(px(16.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .overflow_hidden();
        for (index, question) in questions.iter().enumerate() {
            let answer = answers
                .as_ref()
                .and_then(|answers| answers.get(index))
                .cloned()
                .unwrap_or(Value::Null);
            card = card.child(self.question_exchange_section(
                &format!("{key}:{index}"),
                question,
                &answer,
                answers.is_some(),
                index,
                total,
                p,
                cx,
            ));
        }
        let fallback = text(exchange, "fallbackText");
        if !fallback.is_empty() {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0 * s))
                    .px(px(16.0 * s))
                    .py(px(14.0 * s))
                    .when(total > 0, |section| {
                        section.border_t_1().border_color(p.input_border)
                    })
                    .child(self.question_exchange_label("ANSWER", p))
                    .child(div().text_color(p.foreground).child(fallback)),
            );
        }
        card.into_any_element()
    }

    fn question_exchange_label(&self, label: &str, p: &ChatAppearance) -> AnyElement {
        div()
            .text_size(px(11.0 * p.scale))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(p.muted)
            .child(label.to_uppercase())
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn question_exchange_section(
        &self,
        key: &str,
        question: &Value,
        answer: &Value,
        parsed_answers: bool,
        index: usize,
        total: usize,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let header = question["header"]
            .as_str()
            .unwrap_or("Question")
            .to_string();
        let selected: Vec<usize> = answer["selectedIndices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_u64().map(|value| value as usize))
            .collect();
        let options = question["options"].as_array().cloned().unwrap_or_default();
        let other = text(answer, "otherText");
        let mut section = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(6.0 * s))
            .px(px(16.0 * s))
            .py(px(14.0 * s))
            .when(index > 0, |section| {
                section.border_t_1().border_color(p.input_border)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0 * s))
                    .child(self.question_exchange_label(&header, p))
                    .when(total > 1, |row| {
                        row.child(
                            div()
                                .flex_shrink_0()
                                .px(px(5.0 * s))
                                .rounded(px(5.0 * s))
                                .bg(p.border.opacity(0.6))
                                .text_size(px(10.0 * s))
                                .text_color(p.muted)
                                .child(format!("{}/{total}", index + 1)),
                        )
                    }),
            );
        let prompt = text(question, "question");
        if !prompt.is_empty() {
            section = section.child(div().text_color(p.foreground.opacity(0.9)).child(prompt));
        }
        let mut answered = div().flex().flex_col().gap(px(6.0 * s)).pt(px(6.0 * s));
        let mut has_answer = false;
        for option_index in &selected {
            let Some(option) = options.get(*option_index) else {
                continue;
            };
            has_answer = true;
            answered = answered.child(self.question_exchange_answer_row(
                text(option, "label"),
                text(option, "description"),
                None,
                p,
            ));
        }
        if !other.is_empty() {
            has_answer = true;
            answered = answered.child(self.question_exchange_answer_row(
                other,
                String::new(),
                Some(if selected.is_empty() {
                    "CUSTOM ANSWER"
                } else {
                    "ADDED NOTE"
                }),
                p,
            ));
        }
        // A question the reader walked away from still says so, rather than looking unanswered.
        if !has_answer {
            let unanswered = if answer["dismissed"] == true {
                Some("Dismissed without answering")
            } else if parsed_answers {
                Some("Skipped")
            } else {
                None
            };
            if let Some(label) = unanswered {
                has_answer = true;
                answered = answered.child(
                    div()
                        .px(px(12.0 * s))
                        .py(px(8.0 * s))
                        .rounded(px(8.0 * s))
                        .bg(p.foreground.opacity(0.045))
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .child(label),
                );
            }
        }
        if has_answer {
            section = section.child(answered);
        }
        if !options.is_empty() {
            let options_key = format!("question-exchange:{key}");
            let expanded = self.expanded.contains(&options_key);
            let motion = self.disclosure_frame(&options_key, expanded, cx);
            section = section.child(
                div()
                    .pt(px(4.0 * s))
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child(self.disclosure(
                        options_key.clone(),
                        if expanded {
                            "Hide options".to_string()
                        } else {
                            format!("Show all {} options", options.len())
                        },
                        expanded,
                        None,
                        p,
                        cx,
                    )),
            );
            if expanded || motion.is_some() {
                let mut rows = div().flex().flex_col().gap(px(6.0 * s)).pt(px(6.0 * s));
                for (option_index, option) in options.iter().enumerate() {
                    rows = rows.child(self.choice_row(
                        format!("{key}:option:{option_index}"),
                        text(option, "label"),
                        text(option, "description"),
                        selected.contains(&option_index),
                        None,
                        true,
                        // Settled answers are a record, never a control.
                        true,
                        Value::Null,
                        p,
                        cx,
                    ));
                }
                section = section.child(self.disclosure_body_motion(
                    &options_key,
                    motion,
                    0.0,
                    rows.into_any_element(),
                ));
            }
        }
        section.into_any_element()
    }

    fn question_exchange_answer_row(
        &self,
        label: String,
        description: String,
        micro_label: Option<&'static str>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .flex()
            .items_start()
            .gap(px(10.0 * s))
            .w_full()
            .px(px(12.0 * s))
            .py(px(8.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.control_primary.opacity(0.3))
            .bg(p.control_primary.opacity(0.1))
            .child(
                gpui::svg()
                    .path("titlebar/check.svg")
                    .size(px(14.0 * s))
                    .mt(px(3.0 * s))
                    .flex_shrink_0()
                    .text_color(p.control_primary),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0 * s))
                    .when_some(micro_label, |column, micro| {
                        column.child(
                            div()
                                .text_size(px(10.0 * s))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.muted)
                                .child(micro),
                        )
                    })
                    // A chosen option's title carries React's `font-medium`, so the answer reads
                    // ahead of its own description. The reader's own words in a custom answer or an
                    // added note are prose and stay at the row's weight, as they did in React.
                    .child(
                        div()
                            .text_color(p.foreground)
                            .when(micro_label.is_none(), |title| {
                                title.font_weight(FontWeight::MEDIUM)
                            })
                            .child(label.clone()),
                    )
                    .when(!description.is_empty() && description != label, |column| {
                        column.child(
                            div()
                                .text_size(px(12.0 * s))
                                .text_color(p.card_muted)
                                .child(description),
                        )
                    }),
            )
            .into_any_element()
    }
}
