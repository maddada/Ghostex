//! A coordinator's work item cards under its replies (`workCards` in the chat core's document,
//! gx-chat-core extras/work_cards.rs, which holds the user's decision): a card per item the turn
//! changed, with its status, where it stands, who has the ball and how long ago, and a card per
//! ask with its answer buttons; an item's earlier cards and settled asks fold to one quiet line.
//! The phone draws the same cards in apps/mobile/app/src/chat/native/cards/WorkCards.tsx.
//!
//! The buttons are neutral fills, never the dark primary fill, because the card sits on the chat's
//! glass (the user's standing rule: no black fills on glass).

use super::tone_card::ToneButton;
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::Value;

/// A status's glyph and colour, the board's (coordinator_board.rs `group_look`), so a card reads as
/// its row in the Work panel does.
pub(super) fn status_look(status: &str, p: &ChatAppearance) -> (&'static str, gpui::Hsla) {
    let group = match status {
        "waiting" => "hold",
        "finished" => "done",
        other => other,
    };
    if status == "dropped" {
        return ("titlebar/circle-x.svg", p.muted);
    }
    let (icon, color) = super::coordinator_board::group_look(group);
    (icon, color.map_or(p.muted, |color| gpui::rgb(color).into()))
}

/// A chip's glyph by what it says.
fn chip_icon(kind: &str) -> &'static str {
    match kind {
        "thread" => "titlebar/robot.svg",
        // 12 px: the hat's thicker-lined copy for small sizes.
        "coordinator" => "titlebar/coordinator-captain-hat-small.svg",
        "steps" => "titlebar/checklist.svg",
        "item" => "titlebar/stack.svg",
        "question" => "titlebar/help-circle.svg",
        "link" => "titlebar/link.svg",
        "file" => "titlebar/file-text.svg",
        _ => "titlebar/clock.svg",
    }
}

impl NativeChatView {
    fn work_cards(&self) -> &[Value] {
        self.snapshot["workCards"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
    }

    /// The cards that sit under the reply `message_id`.
    pub(super) fn work_cards_under(&self, message_id: &str) -> Vec<Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["anchorId"].as_str() == Some(message_id))
            .cloned()
            .collect()
    }

    /// The cards of a summary-mode turn (`turnId`), drawn under the turn, outside its folds.
    pub(super) fn work_cards_of_turn(&self, turn_id: &str) -> Vec<Value> {
        self.work_cards()
            .iter()
            .filter(|card| card["turnId"].as_str() == Some(turn_id))
            .cloned()
            .collect()
    }

    /// A column of cards, or `None` when there are none.
    pub(super) fn work_card_stack(
        &self,
        cards: &[Value],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        (!cards.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap(px(6.0 * p.scale))
                .children(cards.iter().map(|card| {
                    let drawn = if card["kind"] == "moving" {
                        self.work_moving_row(card, p, cx)
                    } else if card["folded"] == true {
                        self.work_fold_line(card, p)
                    } else {
                        self.work_card(card, p, cx)
                    };
                    // The ? pill's jump lands on an ask's card and flashes it (open_questions.rs).
                    let key = text(card, "key");
                    self.open_question_frame(Some(key.as_str()), drawn, p)
                }))
                .into_any_element()
        })
    }

    fn work_card(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let (icon, color) = status_look(&text(card, "status"), p);
        let status = text(card, "statusLabel");
        let age = text(card, "age");
        let line = text(card, "line");
        // On a narrow chat the status and age wrap under the title rather than squeeze it.
        let mut top = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .min_h(px(20.0 * s))
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(140.0 * s))
                    .whitespace_normal()
                    .text_size(px(14.0 * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.foreground)
                    .child(text(card, "title")),
            );
        let right = [status.as_str(), age.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        if !right.is_empty() {
            top = top.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.5 * s))
                    .text_color(p.card_muted)
                    .child(right),
            );
        }
        let mut column = div()
            .id(gpui::SharedString::from(format!("work-card:{key}")))
            .role(gpui::Role::Group)
            .aria_label(format!("{}, {status}", text(card, "title")))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.0 * s))
            .px(px(12.0 * s))
            .py(px(10.0 * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .child(top);
        if !line.is_empty() {
            column = column.child(
                div()
                    .pl(px(22.0 * s))
                    .min_w_0()
                    .whitespace_normal()
                    .text_size(px(13.0 * s))
                    .text_color(p.card_muted)
                    .child(line),
            );
        }
        // An icon and muted words, no outline or fill (Q2167); a thread, link or file opens.
        let chips: Vec<AnyElement> = card["chips"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, chip)| {
                let command = chip["command"].clone();
                let label = text(chip, "label");
                div()
                    .id(gpui::SharedString::from(format!("work-chip:{key}:{index}")))
                    .flex()
                    .flex_shrink(1.0)
                    .min_w(px(40.0 * s))
                    .max_w(px(280.0 * s))
                    .items_center()
                    .gap(px(5.0 * s))
                    .h(px(22.0 * s))
                    .text_size(px(12.5 * s))
                    .text_color(p.card_muted)
                    .aria_label(label.clone())
                    .child(
                        svg()
                            .path(chip_icon(&text(chip, "kind")))
                            .size(px(12.0 * s))
                            .flex_shrink_0()
                            .text_color(p.muted),
                    )
                    .child(div().min_w_0().truncate().child(label))
                    .when(command.is_object(), |chip| {
                        chip.chat_cursor_pointer()
                            .hover(|style| style.text_color(p.foreground))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.invoke(command.clone(), cx);
                            }))
                    })
                    .into_any_element()
            })
            .collect();
        let busy = card["busy"] == true;
        let buttons: Vec<AnyElement> = card["buttons"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, button)| {
                let label = text(button, "label");
                let picked = button["picked"] == true;
                let recommended = button["recommended"] == true;
                let command = button["command"].clone();
                self.tone_button(
                    gpui::SharedString::from(format!("work-ask:{key}:{index}")),
                    &label,
                    if picked {
                        Some("titlebar/check.svg")
                    } else {
                        None
                    },
                    ToneButton::Secondary,
                    None,
                    busy && !picked,
                    p,
                )
                // The recommended answer carries the question forms' star after its label
                // (ask_card.rs `recommended_mark`), "Recommended" as its tooltip and name.
                .when(recommended, |button| {
                    button
                        .aria_label(format!("{label}, Recommended"))
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                        })
                        .child(
                            svg()
                                .path("titlebar/star-filled.svg")
                                .size(px(10.0 * s))
                                .flex_shrink_0()
                                .text_color(super::ask_card::ask_color(p).opacity(0.75)),
                        )
                })
                .when(!busy, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
                })
                .into_any_element()
            })
            .collect();
        if !chips.is_empty() || !buttons.is_empty() {
            column = column.child(
                div()
                    .pl(px(22.0 * s))
                    .pt(px(2.0 * s))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(14.0 * s))
                    .gap_y(px(4.0 * s))
                    .children(chips)
                    .when(!buttons.is_empty(), |row| {
                        row.child(
                            div()
                                .ml_auto()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.0 * s))
                                .children(buttons),
                        )
                    }),
            );
        }
        column.into_any_element()
    }

    /// Everything still running, as one quiet row ("Moving · 7" and their names) that opens to a
    /// list; a row of the list goes to that item's newest card (gx-chat-core work_cards.rs
    /// `moving_row`, which holds the user's Q2166).
    fn work_moving_row(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let open = self.expanded.contains(&key);
        let (icon, color) = status_look("moving", p);
        let toggle_key = key.clone();
        let waiting = text(card, "waiting");
        let head = div()
            .id(gpui::SharedString::from(format!("work-moving:{key}")))
            .role(gpui::Role::Button)
            .aria_label(format!(
                "{}, {}",
                text(card, "title"),
                if open { "open" } else { "closed" }
            ))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(30.0 * s))
            .px(px(12.0 * s))
            .text_size(px(12.5 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .child(
                svg()
                    .path(if open {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.card_muted)
                    .child(text(card, "title")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.muted)
                    .when(!open, |summary| summary.child(text(card, "summary"))),
            )
            .when(!waiting.is_empty(), |head| {
                head.child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.muted)
                        .child(waiting.clone()),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !this.expanded.remove(&toggle_key) {
                    this.expanded.insert(toggle_key.clone());
                }
                cx.notify();
            }));
        let mut column = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.input_border.opacity(0.7))
            .overflow_hidden()
            .child(head);
        if open {
            for (index, row) in card["rows"].as_array().into_iter().flatten().enumerate() {
                let (row_icon, row_color) = status_look(&text(row, "status"), p);
                let command = row["command"].clone();
                let line = text(row, "line");
                column = column.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "work-moving-row:{key}:{index}"
                        )))
                        .role(gpui::Role::Button)
                        .aria_label(text(row, "title"))
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_start()
                        .gap(px(8.0 * s))
                        .px(px(12.0 * s))
                        .py(px(6.0 * s))
                        .border_t_1()
                        .border_color(p.input_border.opacity(0.5))
                        .text_size(px(12.5 * s))
                        .child(
                            div()
                                .h(px(18.0 * s))
                                .flex()
                                .items_center()
                                .pl(px(20.0 * s))
                                .child(
                                    svg()
                                        .path(row_icon)
                                        .size(px(11.0 * s))
                                        .flex_shrink_0()
                                        .text_color(row_color),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .line_height(px(18.0 * s))
                                        .text_color(p.foreground)
                                        .child(text(row, "title")),
                                )
                                .when(!line.is_empty(), |cell| {
                                    cell.child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .line_height(px(17.0 * s))
                                            .text_color(p.muted)
                                            .child(line.clone()),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .line_height(px(18.0 * s))
                                .text_color(p.muted)
                                .child(text(row, "age")),
                        )
                        .when(command.is_object(), |row| {
                            row.chat_cursor_pointer()
                                .hover(|style| style.bg(p.foreground.opacity(0.04)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.invoke(command.clone(), cx);
                                }))
                        }),
                );
            }
        }
        column.into_any_element()
    }

    /// An item's earlier card, or a settled ask: one quiet line, its status's glyph, what that turn
    /// did, and when.
    fn work_fold_line(&self, card: &Value, p: &ChatAppearance) -> AnyElement {
        let s = p.scale;
        let status = match text(card, "foldStatus").as_str() {
            "" => text(card, "status"),
            status => status.to_string(),
        };
        let (icon, color) = status_look(&status, p);
        let words = text(card, "foldText");
        let tooltip = words.clone();
        div()
            .id(gpui::SharedString::from(format!(
                "work-fold:{}",
                text(card, "key")
            )))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(28.0 * s))
            .px(px(12.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.input_border.opacity(0.7))
            .text_size(px(12.5 * s))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(color.opacity(0.8)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.muted)
                    .child(words),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .child(text(card, "time")),
            )
            .into_any_element()
    }
}
