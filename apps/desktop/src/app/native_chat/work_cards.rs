//! A coordinator's work item cards under its replies (`workCards` in the chat core's document,
//! gx-chat-core extras/work_cards.rs, which holds the user's decision): a card per item the turn
//! changed, with its status, where it stands, who has the ball and how long ago, and a card per
//! ask with its answer buttons; an item's earlier cards and settled asks fold to one quiet line.
//! The phone draws the same cards in apps/mobile/app/src/chat/native/cards/WorkCards.tsx.
//!
//! Titles are the chat's body text (its prose size and Text strength); second lines, folds and the
//! Moving row's names are its muted text at the same size, so the cards read as the chat does and
//! follow Chat zoom and Text strength live.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, on the opened Moving list: "the font size must match the font size of the chat here please
//! and same text strength should apply". Chips and the status and age labels stay small: they
//! are labels, not text to read.
//!
//! The buttons are neutral fills, never the dark primary fill, because the card sits on the chat's
//! glass (the user's standing rule: no black fills on glass).

use super::chat_text_look::{PROSE_LINE_HEIGHT, PROSE_SIZE};
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
/// The dock's cards stop growing here (unscaled) and scroll inside it.
const DOCK_MAX_HEIGHT: f32 = 300.0;
/// The dock's cards fold by themselves while the draft is longer than this.
const DOCK_FOLDS_PAST_CHARS: usize = 280;
/// The fold key (`expanded`) of the dock's cards, folded by the user.
const DOCK_FOLD_KEY: &str = "work-dock-folded";

/// The opened Moving list draws as a table once the transcript is at least this wide (unscaled):
/// its Item, Who and Age columns plus a "Where it stands" column of at least 260 px, so neither
/// text column is squeezed below a few words a line.
const MOVING_TABLE_MIN_WIDTH: f32 = 640.0;
const MOVING_ITEM_COLUMN: f32 = 200.0;
const MOVING_WHO_COLUMN: f32 = 110.0;
const MOVING_AGE_COLUMN: f32 = 36.0;

/// Who has a line of work, as an icon: the thread's agent logo, GitHub's mark, the user, the
/// coordinator's hat, or a clock for something else outside.
fn who_icon(who: &Value, p: &ChatAppearance) -> AnyElement {
    let agent = text(who, "agentIcon");
    let (path, color) = match text(who, "kind").as_str() {
        "thread" if !agent.is_empty() => (
            format!("agent-icons/{agent}.svg"),
            super::new_session_welcome::brand_logo_color(&agent, p),
        ),
        "thread" => ("titlebar/robot.svg".to_string(), p.muted),
        "github" => ("titlebar/brand-github.svg".to_string(), p.muted),
        "you" => (
            "titlebar/user.svg".to_string(),
            gpui::rgb(crate::app::native_sidebar::status::QUESTION_COLOR).into(),
        ),
        "coordinator" => (
            "titlebar/coordinator-captain-hat-small.svg".to_string(),
            p.muted,
        ),
        _ => ("titlebar/clock.svg".to_string(), p.muted),
    };
    svg()
        .path(path)
        .size(px(14.0 * p.scale))
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

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

    /// The dock above a coordinator's chat box (gx-chat-core work_cards.rs `dock`, which holds the
    /// user's pick C): every open ask as its full card, newest first, in a stack capped at
    /// `DOCK_MAX_HEIGHT` with its own scroll, then the one status line that opens the Work panel.
    /// The line's chevron folds the cards, and they fold by themselves while a long message is
    /// typed, so the box never has to fight them for room.
    pub(super) fn render_work_dock(
        &self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dock = &self.snapshot["workDock"];
        let parts: Vec<Value> = dock["line"]["parts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let cards: Vec<Value> = dock["cards"].as_array().cloned().unwrap_or_default();
        if parts.is_empty() && cards.is_empty() {
            return None;
        }
        let s = p.scale;
        let long_draft = self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).value().len() > DOCK_FOLDS_PAST_CHARS);
        let folded = self.expanded.contains(DOCK_FOLD_KEY) || long_draft;
        let mut dock_column = div().w_full().min_w_0().flex().flex_col().gap(px(6.0 * s));
        if !cards.is_empty() && !folded {
            dock_column = dock_column.child(
                div()
                    .id("work-dock-cards")
                    .w_full()
                    .min_w_0()
                    .max_h(px(DOCK_MAX_HEIGHT * s))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(6.0 * s))
                    .children(cards.iter().map(|card| {
                        let key = text(card, "key");
                        self.open_question_frame(Some(key.as_str()), self.work_card(card, p, cx), p)
                    })),
            );
        }
        let pill = self
            .pill_bounds
            .cell(super::coordinator_board::BOARD_MENU)
            .get();
        let line = div()
            .id("work-dock-line")
            .role(gpui::Role::Button)
            .aria_label("Open the Work panel")
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .h(px(32.0 * s))
            .px(px(12.0 * s))
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .text_size(px(PROSE_SIZE * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.04)))
            .when(!cards.is_empty(), |line| {
                let label = if folded {
                    "Show the cards"
                } else {
                    "Fold the cards"
                };
                line.child(
                    div()
                        .id("work-dock-fold")
                        .role(gpui::Role::Button)
                        .aria_label(label)
                        .flex_shrink_0()
                        .size(px(20.0 * s))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(5.0 * s))
                        .hover(|style| style.bg(p.foreground.opacity(0.08)))
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                        })
                        .child(
                            svg()
                                .path(if folded {
                                    "titlebar/chevron-up.svg"
                                } else {
                                    "titlebar/chevron-down.svg"
                                })
                                .size(px(12.0 * s))
                                .text_color(p.muted),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            if !this.expanded.remove(DOCK_FOLD_KEY) {
                                this.expanded.insert(DOCK_FOLD_KEY.to_string());
                            }
                            cx.notify();
                        })),
                )
            })
            .child(
                svg()
                    .path("titlebar/layout-board-split.svg")
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(p.card_muted)
                    .children(parts.iter().enumerate().flat_map(|(index, part)| {
                        let tone = super::coordinator_board::tone_color(&text(part, "tone"))
                            .map_or(p.card_muted, |color| gpui::rgb(color).into());
                        [
                            (index > 0).then(|| {
                                div().flex_shrink_0().child(" \u{b7} ").into_any_element()
                            }),
                            Some(
                                div()
                                    .flex_shrink_0()
                                    .text_color(tone)
                                    .child(text(part, "text"))
                                    .into_any_element(),
                            ),
                        ]
                        .into_iter()
                        .flatten()
                    })),
            )
            .child(
                svg()
                    .path("titlebar/chevron-right.svg")
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.toggle_coordinator_board(pill, window, cx);
            }));
        Some(dock_column.child(line).into_any_element())
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
        let body = p.strengthened(p.prose);
        // On a narrow chat the status and age wrap under the title rather than squeeze it.
        let mut top = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.0 * s))
            .min_h(px(PROSE_LINE_HEIGHT * s))
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
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(body)
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
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .text_color(p.muted)
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
                    .text_size(px(PROSE_SIZE * s))
                    .line_height(px(PROSE_LINE_HEIGHT * s))
                    .text_color(p.card_muted)
                    .child(line),
            );
        }
        // Each part a quiet link at the chat's size: its icon (the thread's agent logo, GitHub's
        // mark) and muted words, brightening and underlined on hover; a GitHub PR or issue says its
        // state in that state's colour. Parts wrap to another line, never cut (Q2205 A).
        let chips: Vec<AnyElement> = card["chips"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, chip)| self.work_card_part(chip, &format!("{key}:{index}"), p, cx))
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

    /// One part of a card's bottom line (`chips` in gx-chat-core work_cards.rs `item_chips`).
    fn work_card_part(
        &self,
        chip: &Value,
        id: &str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let kind = text(chip, "kind");
        let label = text(chip, "label");
        let command = chip["command"].clone();
        let agent = text(chip, "agentIcon");
        let (icon, icon_color) = match kind.as_str() {
            "thread" if !agent.is_empty() => (
                format!("agent-icons/{agent}.svg"),
                super::new_session_welcome::brand_logo_color(&agent, p),
            ),
            "github" => ("titlebar/brand-github.svg".to_string(), p.muted),
            _ => (chip_icon(&kind).to_string(), p.muted),
        };
        let state = text(chip, "state");
        let state_color = super::coordinator_board::tone_color(&text(chip, "tone"))
            .map_or(p.muted, |color| gpui::rgb(color).into());
        let words = div()
            .min_w_0()
            .whitespace_normal()
            .child(label.clone())
            .when(command.is_object(), |words| {
                words.hover(|style| style.text_color(p.foreground).underline())
            });
        div()
            .id(gpui::SharedString::from(format!("work-chip:{id}")))
            .flex()
            .max_w_full()
            .min_w_0()
            .items_center()
            .gap(px(6.0 * s))
            .min_h(px(PROSE_LINE_HEIGHT * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s))
            .text_color(p.card_muted)
            .aria_label(if state.is_empty() {
                label.clone()
            } else {
                format!("{label}, {state}")
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(icon_color),
            )
            .child(words)
            .when(!state.is_empty(), |part| {
                part.child(div().flex_shrink_0().text_color(state_color).child(state))
            })
            .when(command.is_object(), |part| {
                part.chat_cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            })
            .into_any_element()
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
        let body = p.strengthened(p.prose);
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
            .h(px(34.0 * s))
            .px(px(12.0 * s))
            .text_size(px(PROSE_SIZE * s))
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
            // The same 14 px as the list's who-has-it icons under it (the user's "make this icon
            // bigger for the ones inside the list").
            .child(
                svg()
                    .path(icon)
                    .size(px(14.0 * s))
                    .flex_shrink_0()
                    .text_color(color),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(body)
                    .child(text(card, "title")),
            )
            // Closed, it says who has what ("1 with you · 1 on GitHub · 5 threads"), what is
            // with the user in the question colour.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .when(!open, |summary| {
                        summary
                            .children(
                                card["summaryParts"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .enumerate()
                                    .flat_map(|(index, part)| {
                                        let tone = super::coordinator_board::tone_color(&text(
                                            part, "tone",
                                        ))
                                        .map_or(p.card_muted, |color| gpui::rgb(color).into());
                                        [
                                            (index > 0)
                                                .then(|| " \u{b7} ".to_string().into_any_element()),
                                            Some(
                                                gpui::div()
                                                    .flex_shrink_0()
                                                    .text_color(tone)
                                                    .child(text(part, "text"))
                                                    .into_any_element(),
                                            ),
                                        ]
                                    })
                                    .flatten(),
                            )
                            .flex()
                            .gap(px(0.0))
                    }),
            )
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
            // The work item cards' own background, border and radius.
            .rounded(px(10.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .overflow_hidden()
            .child(head);
        if open {
            // The pane, capped at the transcript column. Not `transcript_text_width`: it asks the
            // transcript list for the minimap's gutter while that list is drawing this row.
            let width = self
                .bounds
                .get()
                .size
                .width
                .as_f32()
                .min(self.transcript_column_cap(p));
            let wide = width / s >= MOVING_TABLE_MIN_WIDTH;
            column = column.child(self.work_moving_list(card, wide, p, cx));
        }
        column.into_any_element()
    }

    /// The opened Moving list, grouped by who has each line of work (gx-chat-core `who_has`, which
    /// holds the user's Q2207): on a wide chat the compact table (Item, Where it stands, Who,
    /// Age), else rows with the full text; quiet group labels, no hairlines between rows.
    fn work_moving_list(
        &self,
        card: &Value,
        wide: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let body = p.strengthened(p.prose);
        let mut list = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .px(px(12.0 * s))
            .pb(px(8.0 * s))
            .text_size(px(PROSE_SIZE * s))
            .line_height(px(PROSE_LINE_HEIGHT * s));
        if wide {
            list = list.child(
                div()
                    .flex()
                    .gap(px(16.0 * s))
                    .pt(px(2.0 * s))
                    .text_color(p.muted)
                    .child(
                        div()
                            .w(px(MOVING_ITEM_COLUMN * s))
                            .flex_shrink_0()
                            .child("Item"),
                    )
                    .child(div().flex_1().min_w_0().child("Where it stands"))
                    .child(
                        div()
                            .w(px(MOVING_WHO_COLUMN * s))
                            .flex_shrink_0()
                            .child("Who"),
                    )
                    .child(
                        div()
                            .w(px(MOVING_AGE_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .child("Age"),
                    ),
            );
        }
        let groups: Vec<&Value> = card["groups"].as_array().into_iter().flatten().collect();
        let mut current = String::new();
        for (index, row) in card["rows"].as_array().into_iter().flatten().enumerate() {
            let group = text(row, "group");
            if group != current {
                current = group.clone();
                let count = groups
                    .iter()
                    .find(|entry| entry["key"] == group.as_str())
                    .and_then(|entry| entry["count"].as_u64())
                    .unwrap_or(0);
                list = list.child(
                    div()
                        .pt(px(8.0 * s))
                        .pb(px(2.0 * s))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.muted)
                        .child(format!("{} \u{b7} {count}", text(row, "groupLabel"))),
                );
            }
            let command = row["command"].clone();
            let line = text(row, "line");
            let who = &row["who"];
            let mut drawn = div()
                .id(gpui::SharedString::from(format!(
                    "work-moving-row:{key}:{index}"
                )))
                .role(gpui::Role::Button)
                .aria_label(format!("{}, {}", text(row, "title"), text(who, "label")))
                .w_full()
                .min_w_0()
                .flex()
                .items_start()
                .py(px(4.0 * s))
                .rounded(px(6.0 * s));
            drawn = if wide {
                drawn
                    .gap(px(16.0 * s))
                    .child(
                        div()
                            .w(px(MOVING_ITEM_COLUMN * s))
                            .flex_shrink_0()
                            .whitespace_normal()
                            .text_color(body)
                            .child(text(row, "title")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_normal()
                            .text_color(p.card_muted)
                            .child(line),
                    )
                    .child(
                        div()
                            .w(px(MOVING_WHO_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0 * s))
                            .h(px(PROSE_LINE_HEIGHT * s))
                            .text_color(p.card_muted)
                            .child(who_icon(who, p))
                            .child(div().min_w_0().truncate().child(text(who, "label"))),
                    )
                    .child(
                        div()
                            .w(px(MOVING_AGE_COLUMN * s))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .text_color(p.muted)
                            .child(text(row, "age")),
                    )
            } else {
                drawn
                    .gap(px(8.0 * s))
                    .child(
                        div()
                            .w(px(20.0 * s))
                            .h(px(PROSE_LINE_HEIGHT * s))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(who_icon(who, p)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .whitespace_normal()
                                    .text_color(body)
                                    .child(text(row, "title")),
                            )
                            .when(!line.is_empty(), |cell| {
                                cell.child(
                                    div()
                                        .whitespace_normal()
                                        .text_color(p.card_muted)
                                        .child(line.clone()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(p.muted)
                            .child(text(row, "age")),
                    )
            };
            list = list.child(drawn.when(command.is_object(), |row| {
                row.chat_cursor_pointer()
                    .hover(|style| style.bg(p.foreground.opacity(0.04)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(command.clone(), cx);
                    }))
            }));
        }
        list.into_any_element()
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
        let outcome = text(card, "foldOutcome");
        let tooltip = if outcome.is_empty() {
            words.clone()
        } else {
            format!("{words} \u{b7} {outcome}")
        };
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
            .h(px(34.0 * s))
            .px(px(12.0 * s))
            .rounded(px(8.0 * s))
            .border_1()
            .border_color(p.input_border.opacity(0.7))
            .text_size(px(PROSE_SIZE * s))
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
            // The question or title gives way; the outcome and time stay whole.
            .child(
                div()
                    .flex_1()
                    .min_w(px(48.0 * s))
                    .truncate()
                    .text_color(p.card_muted)
                    .child(words),
            )
            .when(!outcome.is_empty(), |line| {
                line.child(
                    div()
                        .flex_shrink_0()
                        .max_w(px(320.0 * s))
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.card_muted)
                        .child(outcome),
                )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.muted)
                    .child(text(card, "time")),
            )
            .into_any_element()
    }
}
