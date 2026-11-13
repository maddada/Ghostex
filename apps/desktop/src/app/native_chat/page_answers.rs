//! The answers a reader sent from an agent's HTML page, drawn as a card instead of the JSON message
//! gxserver typed into the session (`pageAnswers`, packages/gx-chat-core/src/transcript/page_answers.rs).
//!
//! CDXC:SessionChat 2026-10-11 SEE-ALSO:
//! The phone draws the same card in apps/mobile/app/src/chat/native/transcript/PageAnswersCard.tsx; the page's own card reads Answered in visual.rs. Change them together.

use super::ask_card::{AskBodyMotion, RECEIPT_QUESTION_LINE, answer_aside, single_line_press_head};
use super::cards::SINGLE_LINE_CARD;
use super::link_words::LinkWords;
use super::startup_delivery::{DeliveryNote, NoteAction};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::helpers::ThrottledAnimationExt as _;
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, IntoElement, ParentElement as _, Styled as _, div, px, rgb, svg,
};
use gpui::{InteractiveElement as _, SharedString, StatefulInteractiveElement as _};
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::{Value, json};

/// About two lines of a note at the card's usual width: a longer note folds to two lines with More.
const NOTE_TWO_LINES: usize = 200;

/// The one-line cards' question tile with its green check: these answers were sent.
fn answered_tile(p: &ChatAppearance) -> AnyElement {
    let shape = &*SINGLE_LINE_CARD;
    shape
        .tile(super::ask_card::QUESTION_GLYPH, p)
        .child(shape.check_badge(p))
        .into_any_element()
}

/// A question's tag (or its number) in the muted mono type, kept whole.
fn tag_word(tag: String, p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .font_family(crate::app::native_chat::fonts::CHAT_MONO)
        .text_size(px(11.5 * s))
        .line_height(px(16.0 * s))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(p.muted)
        .child(tag)
        .into_any_element()
}

/// The green the chat's charts use for good news, which also marks an answered page.
pub(super) fn answered_color(p: &ChatAppearance) -> Hsla {
    rgb(if p.light { 0x059669 } else { 0x34d399 }).into()
}

impl NativeChatView {
    /// The reader's answers card is the very next row after this form or page, so its own Answered
    /// line is left out (gx-chat-core `transcript/page_answers.rs` `answers_below`, Q2995).
    pub(super) fn answers_right_below(&self, id: &str) -> bool {
        self.snapshot["answersBelow"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|known| known == id))
    }

    /// CDXC:SessionChat 2026-10-12 DECISION: User, on the answer card under an answered form: "please make sure the selected answer doesn't have that green thing around it, make it more simple should just say the answer and question there with bit different color/fontweight etc (like we have in other cards) but no background behind the answer like you're showing / also plase remove "1 answer" no need for it". The card keeps its green check tile and numbered rows; each answer is plain foreground text at medium weight under its muted question, with no chip, border or fill, and the head has no answer count. Supersedes the 2026-10-11 green answer chips on this card. The agent's own question receipts (question_exchange.rs) take the same plain answer (`plain_answer`, ask_card.rs).
    ///
    /// CDXC:SessionChat 2026-11-13 DECISION:
    /// User: "when I send answers please make them collapsed by default, and if I expand I don't like seeing the same text twice ('You answered "Settings: which section..."' and then the title again). Do better than this", then picked from ~/.local/share/agent-work/Ghostex/sent-answers/sent-answers.html: Q2990 A (tagged list), Q2991 (the link back to the page on the folded line), Q2992 (skipped questions on one line), Q2993 (a long note shows 2 lines, then More), Q2994 (each question one line beside its tag), Q2996 (several picks joined with commas), Q2997 (Quiet View keeps its line; the card it opens does not repeat it). The card is one 44px line, folded by default everywhere: the answered question tile, the page's or form's name (no "You answered"), the count ("3 answered · 1 skipped", gx-chat-core `answers_summary`) and, for a page, a button that opens it again. Opened, each question shows once: its tag (a number for answers sent before tags were carried) and its question on one line (the whole of it on hover), the answer under it, its note under that. Skipped questions share one line at the end. Under Quiet View's own line the card opens straight onto its rows (`body_only`). Supersedes the 2026-10-11 card that never folded and the numbered tiles. The phone matches (PageAnswersCard.tsx).
    ///
    /// `delivery` is the echo's `startupDelivery` while the answers have not reached the
    /// transcript yet (gx-chat-core `session/answer_echoes.rs`): the card shows its spinner, or
    /// the delivery's error with Retry and Remove, under its line whether or not it is open.
    pub(super) fn page_answers_card(
        &self,
        answers: &Value,
        delivery: &Value,
        body_only: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let page_id = text(answers, "pageId");
        // `by`: someone else's answers (a Ghostex Collab guest's, held for the owner).
        let by = text(answers, "by");
        let key = format!("answers-card:{page_id}:{by}");
        let open = body_only || self.is_expanded(&key, false);
        let motion = (!body_only)
            .then(|| self.disclosure_frame(&key, open, cx))
            .flatten();
        let rows =
            (open || motion.is_some()).then(|| self.answers_rows(answers, &key, !body_only, p, cx));
        let note = delivery
            .is_object()
            .then(|| self.answers_delivery(answers, delivery, p, cx));
        if body_only {
            let body = div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .pt(px(4.0 * p.scale))
                .children(rows)
                .children(note)
                .into_any_element();
            return self.lined_ask_card(div().into_any_element(), Some(body), p);
        }
        let head = self.answers_head(answers, &key, open, motion.is_some(), p, cx);
        let body = match (rows, note) {
            (None, None) => None,
            (rows, note) => Some(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .children(rows)
                    .children(note)
                    .into_any_element(),
            ),
        };
        self.single_line_ask_card(
            head,
            body,
            Some(AskBodyMotion {
                key: &key,
                frame: motion,
            }),
            p,
        )
    }

    /// The folded line: the answered tile, the name, the count and, for a page, its open button.
    /// A press anywhere else opens or folds the card.
    fn answers_head(
        &self,
        answers: &Value,
        key: &str,
        open: bool,
        moving: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let shape = &*SINGLE_LINE_CARD;
        let by = text(answers, "by");
        let summary = text(answers, "summary");
        let count = match (by.is_empty(), summary.is_empty()) {
            (true, _) => summary,
            (false, true) => by,
            (false, false) => format!("{by} \u{b7} {summary}"),
        };
        let mut trailing: Vec<AnyElement> = Vec::new();
        if !count.is_empty() {
            trailing.push(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_size(px(12.5 * s))
                    .line_height(px(18.0 * s))
                    .text_color(p.muted)
                    .child(count)
                    .into_any_element(),
            );
        }
        if answers["page"].is_object() {
            trailing.push(self.answers_open_page(&answers["page"], key, p, cx));
        }
        let head = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0 * s))
            .w_full()
            .min_w_0()
            .h(px(shape.header_height(s)))
            .px(px(shape.padding_x * s))
            .child(answered_tile(p))
            .child(
                shape
                    .title(div().flex_1().min_w_0().truncate(), p)
                    .text_color(p.foreground)
                    .child(text(answers, "title")),
            )
            .children(trailing);
        let toggle_key = key.to_string();
        single_line_press_head(
            head,
            SharedString::from(format!("{key}:head")),
            if open {
                "Hide your answers"
            } else {
                "Show your answers"
            },
            open || moving,
            p,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            let open = this.is_expanded(&toggle_key, false);
            this.anchor_disclosure_toggle(&toggle_key, !open);
            if open {
                this.expanded.remove(&toggle_key);
                this.collapsed.insert(toggle_key.clone());
            } else {
                this.collapsed.remove(&toggle_key);
                this.expanded.insert(toggle_key.clone());
            }
            cx.notify();
        }))
        .into_any_element()
    }

    /// The small button at the folded line's end that opens the page again, where Settings > Chat >
    /// Open Ghostex pages in says (the page card's own Open, page_card.rs).
    fn answers_open_page(
        &self,
        page: &Value,
        key: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let url = text(page, "url");
        let title = text(page, "title");
        let file = page["file"].clone();
        let hover = super::cards::card_hover_fill(p);
        div()
            .id(SharedString::from(format!("{key}:open-page")))
            .flex_shrink_0()
            .size(px(26.0 * s))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0 * s))
            .role(gpui::Role::Button)
            .aria_label("Open the page")
            .chat_cursor_pointer()
            .text_color(p.muted)
            .hover(move |style| style.bg(hover).text_color(p.foreground))
            .child(
                svg()
                    .path("titlebar/arrow-up-right.svg")
                    .size(px(15.0 * s))
                    .text_color(p.muted),
            )
            .managed_tooltip_with(
                Some(gpui_component::Placement::Top),
                Some(super::ask_card::QUICK_TOOLTIP_DELAY),
                |window, cx| {
                    gpui_component::tooltip::Tooltip::new("Open the page").build(window, cx)
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if super::visual_popup::FLOATING_PAGES {
                    cx.emit(super::state::NativeChatEvent::Host(json!({
                        "type": "open",
                        "modal": "visualPage",
                        "url": url,
                        "title": title,
                        "file": file,
                    })));
                } else {
                    this.invoke(
                        json!({"type":"openMarkdownLink","href":url,"external":false}),
                        cx,
                    );
                }
            }))
            .into_any_element()
    }

    /// The opened card's rows: each question once (its tag, one line of question, the answer, the
    /// note), the reader's own notes, then the skipped questions on one line.
    fn answers_rows(
        &self,
        answers: &Value,
        key: &str,
        under_title: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let rows: Vec<Value> = answers["rows"].as_array().cloned().unwrap_or_default();
        let more = answers["more"].as_u64().unwrap_or(0) as usize;
        let hairline = p.foreground.opacity(0.05);
        // Under the title, past the tile and its gap; under Quiet View's line there is no tile.
        let indent = if under_title {
            SINGLE_LINE_CARD.tile_size() + 10.0
        } else {
            0.0
        };
        let mut column = div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .pl(px(indent * s));
        let mut drawn = 0;
        for (index, row) in rows.iter().enumerate() {
            if row["skipped"] == true {
                continue;
            }
            let extra = row["extra"] == true;
            let tag = text(row, "tag");
            // Older answers carry no tags: their place in the list stands in.
            let marker = if extra {
                String::new()
            } else if tag.is_empty() {
                format!("{}.", index + 1)
            } else {
                tag
            };
            let label = if extra {
                "Your note".to_string()
            } else {
                text(row, "label")
            };
            let question = self.answers_question_line(answers, row, index, marker, label, p, cx);
            let answer = div()
                .min_w_0()
                .text_size(px(14.0 * s))
                .line_height(px(20.0 * s))
                .font_weight(if extra {
                    gpui::FontWeight::NORMAL
                } else {
                    gpui::FontWeight::MEDIUM
                })
                .text_color(p.foreground)
                .child(text(row, "value"));
            let note = text(row, "note");
            let note = (!note.is_empty())
                .then(|| self.answers_note(note, &format!("{key}:note:{index}"), p, cx));
            column = column.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0 * s))
                    .w_full()
                    .min_w_0()
                    .py(px(8.0 * s))
                    .when(drawn > 0, |row| row.border_t_1().border_color(hairline))
                    .child(question)
                    .child(answer)
                    .children(note),
            );
            drawn += 1;
        }
        let skipped: Vec<&Value> = rows.iter().filter(|row| row["skipped"] == true).collect();
        if !skipped.is_empty() {
            let mut line = div()
                .flex()
                .flex_wrap()
                .items_baseline()
                .gap_x(px(6.0 * s))
                .w_full()
                .min_w_0()
                .py(px(8.0 * s))
                .when(drawn > 0, |line| line.border_t_1().border_color(hairline))
                .text_size(px(12.5 * s))
                .line_height(px(18.0 * s))
                .text_color(p.muted)
                .child("Skipped");
            for (at, row) in skipped.iter().enumerate() {
                let tag = text(row, "tag");
                let comma = if at + 1 < skipped.len() { "," } else { "" };
                line = line.child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(6.0 * s))
                        .min_w_0()
                        .when(!tag.is_empty(), |item| item.child(tag_word(tag, p)))
                        .child(format!("{}{comma}", text(row, "label"))),
                );
            }
            column = column.child(line);
        }
        if more > 0 {
            column = column.child(div().pt(px(4.0 * s)).child(answer_aside(
                format!("and {more} more"),
                false,
                p,
            )));
        }
        column.into_any_element()
    }

    /// A row's first line: its tag (or number) and its question on one line, the whole question in
    /// a tooltip; a question's links open on a click (`labelRuns`, gx-chat-core
    /// questions/form_links.rs).
    #[allow(clippy::too_many_arguments)]
    fn answers_question_line(
        &self,
        answers: &Value,
        row: &Value,
        index: usize,
        marker: String,
        label: String,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let question: AnyElement = if row["labelRuns"].is_array() && row["extra"] != true {
            self.link_words(
                &format!("answers-question:{}:{index}", text(answers, "pageId")),
                LinkWords {
                    plain: label.clone(),
                    runs: &row["labelRuns"],
                    size: 13.0,
                    line_height: RECEIPT_QUESTION_LINE,
                    weight: gpui::FontWeight::NORMAL,
                    color: p.muted,
                },
                p,
                cx,
            )
        } else {
            div().child(label.clone()).into_any_element()
        };
        let hint: SharedString = label.into();
        div()
            .flex()
            .items_baseline()
            .gap(px(8.0 * s))
            .w_full()
            .min_w_0()
            .when(!marker.is_empty(), |line| line.child(tag_word(marker, p)))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "answers-question-line:{}:{index}",
                        text(answers, "pageId")
                    )))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0 * s))
                    .line_height(px(RECEIPT_QUESTION_LINE * s))
                    .text_color(p.muted)
                    .child(question)
                    .managed_tooltip_with(
                        Some(gpui_component::Placement::Top),
                        Some(super::ask_card::QUICK_TOOLTIP_DELAY),
                        move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
                        },
                    ),
            )
            .into_any_element()
    }

    /// A note under its answer: muted, behind a thin rule, two lines until More opens it.
    fn answers_note(
        &self,
        note: String,
        key: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let open = self.is_expanded(key, false);
        // About two lines of the card's width; a shorter note never needs More.
        let long = note.chars().count() > NOTE_TWO_LINES || note.lines().count() > 2;
        let toggle_key = key.to_string();
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(1.0 * s))
            .w_full()
            .min_w_0()
            .mt(px(3.0 * s))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .pl(px(8.0 * s))
                    .border_l_2()
                    .border_color(p.foreground.opacity(0.1))
                    .text_size(px(12.5 * s))
                    .line_height(px(18.0 * s))
                    .text_color(p.muted)
                    .when(long && !open, |text| text.line_clamp(2))
                    .child(note),
            )
            .when(long, |column| {
                column.child(
                    div()
                        .id(SharedString::from(format!("{key}:more")))
                        .role(gpui::Role::Button)
                        .aria_label(if open {
                            "Show less"
                        } else {
                            "Show the whole note"
                        })
                        .chat_cursor_pointer()
                        .text_size(px(12.5 * s))
                        .line_height(px(18.0 * s))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(p.foreground)
                        .hover(|style| style.opacity(0.8))
                        .child(if open { "Less" } else { "More" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.expanded.remove(&toggle_key) {
                                this.collapsed.insert(toggle_key.clone());
                            } else {
                                this.collapsed.remove(&toggle_key);
                                this.expanded.insert(toggle_key.clone());
                            }
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
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
        let remove = || NoteAction {
            id: format!("answers-remove:{key}").into(),
            label: "Remove",
            command: json!({"type":"removeQueue","promptId":prompt_id.clone()}),
        };
        // The card's bottom-left line, in the shape every undelivered note takes
        // (`delivery_note`, startup_delivery.rs).
        let note = |role, lead, status: gpui::SharedString, color, actions| {
            div()
                .pt(px(10.0 * s))
                .min_w_0()
                .child(self.delivery_note(
                    DeliveryNote {
                        id: format!("answers-delivery:{key}").into(),
                        role,
                        lead,
                        status,
                        color,
                        actions,
                        end: false,
                    },
                    p,
                    cx,
                ))
                .into_any_element()
        };
        if delivery["state"] == "unconfirmed" {
            let actions = if prompt_id.is_empty() {
                Vec::new()
            } else {
                vec![remove()]
            };
            return note(
                gpui::Role::Status,
                None,
                text(delivery, "waitReason").into(),
                p.muted,
                actions,
            );
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
                "Waiting for the agent to finish its turn…".into()
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
            let actions = if removable {
                vec![remove()]
            } else {
                Vec::new()
            };
            return note(gpui::Role::Status, Some(glyph), label, p.muted, actions);
        }
        let message = text(delivery, "errorMessage");
        let message = if message.is_empty() {
            "Your answers could not be delivered.".to_string()
        } else {
            format!("Your answers could not be delivered: {message}")
        };
        let error: Hsla = rgb(if p.light { 0xb91c1c } else { 0xfb7185 }).into();
        let mut actions = Vec::new();
        if !prompt_id.is_empty() {
            // The card that stands for this row has its own Retry (chat core's
            // `sendFailureCardPromptId`).
            if self.snapshot["sendFailureCardPromptId"] != delivery["promptId"] {
                actions.push(NoteAction {
                    id: format!("answers-retry:{key}").into(),
                    label: "Retry",
                    command: json!({"type":"retryQueue","promptId":prompt_id.clone()}),
                });
            }
            actions.push(remove());
        }
        note(gpui::Role::Alert, None, message.into(), error, actions)
    }
}
