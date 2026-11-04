//! Claude's `/btw` side question: the live card above the composer, and the folded row the
//! transcript keeps after Close.

use super::disclosure_motion::measured;
use super::thinking::estimated_lines;
use super::{
    appearance::{ChatAppearance, ChatFilledButton as _},
    state::NativeChatView,
    transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px, relative, svg,
};
use serde_json::{Value, json};

/// A long answer shows about this many lines before "Show all".
const CAP_LINES: usize = 12;
/// The header's line box: the pill, the question's first line and "Answering…" all take this
/// height from the same top, so their centres line up.
const HEADER_LINE: f32 = 20.0;
const LINE_HEIGHT: f32 = 22.75;
/// The live card's footer note; the phone's card says the same (`SideQuestionCard.tsx`).
const SIDE_CARD_NOTE: &str = "Can't reply to sidechat. Close it to message main agent.";

impl NativeChatView {
    /// CDXC:SessionChat 2026-09-27 DECISION:
    /// User approved the side question card from the 2026-09-27 mockup: the answer is rendered Markdown in the chat's font (not the panel's code-font text), the card holds the whole answer, a long one is capped with "Show all" rather than a scroll box, the panel's key hints become Copy (C), Fork (F) and Close (Esc) buttons, and a note says the answer is not added to the conversation.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/cards/SideQuestionCard.tsx, server/src/session_chat_claude_panel.rs.
    pub(super) fn side_question_card(
        &mut self,
        notice: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let dialog = &notice["dialog"];
        let card = &dialog["presentation"]["sideQuestion"];
        let dialog_id = text(dialog, "id");
        let answering = card["answering"] == true;
        let question = text(card, "question");
        let header = div()
            .flex()
            .items_start()
            .gap(px(8.0 * s))
            .child(side_badge(p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(p.foreground)
                    .line_height(px(HEADER_LINE * s))
                    .child(question.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0 * s))
                    .line_height(px(HEADER_LINE * s))
                    .text_color(p.muted)
                    .child(if answering { "Answering…" } else { "" }),
            )
            .into_any_element();
        let mut body = Vec::new();
        if answering {
            body.push(answering_placeholder(p));
        } else {
            let key = format!("side-question-card:{dialog_id}:{question}");
            body.push(self.side_answer_body(
                key,
                text(card, "answerMarkdown"),
                &card["answerReferences"],
                Some(window.viewport_size().height.as_f32() * 0.55),
                p,
                cx,
            ));
        }
        // CDXC:SessionChat 2026-09-30 DECISION: User: the side chat card's bottom left says "Can't reply to sidechat. Close it to message main agent.", cut short when there is no room, with the whole line on hover.
        let mut actions: Vec<AnyElement> = vec![
            div()
                .id("side-question-note")
                .flex_1()
                .min_w_0()
                .text_size(px(12.0 * s))
                .text_color(p.muted)
                .truncate()
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new(SIDE_CARD_NOTE).build(window, cx)
                })
                .child(SIDE_CARD_NOTE)
                .into_any_element(),
        ];
        let answer = text(card, "answer");
        actions.push(side_button(
            "side-question-copy",
            Some("titlebar/copy.svg"),
            "Copy",
            Some("C"),
            answering || answer.is_empty(),
            None,
            p,
            cx.listener(move |_, _, _, cx| {
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(answer.clone()),
                    cx,
                );
            }),
        ));
        let offered = |action: &str| {
            dialog["presentation"]["actions"]
                .as_array()
                .is_some_and(|actions| actions.iter().any(|entry| entry["action"] == action))
        };
        if offered("fork") {
            let id = dialog_id.clone();
            actions.push(side_button(
                "side-question-fork",
                Some("titlebar/git-fork.svg"),
                "Fork",
                Some("F"),
                false,
                Some("Continue this side question as a background Claude agent; its answer comes back to this chat."),
                p,
                cx.listener(move |this, _, _, cx| {
                    this.invoke(
                        json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":id,"dialogAction":"fork"}}),
                        cx,
                    );
                }),
            ));
        }
        if offered("cancel") {
            let id = dialog_id.clone();
            actions.push(side_button(
                "side-question-close",
                None,
                "Close",
                Some("Esc"),
                false,
                None,
                p,
                cx.listener(move |this, _, _, cx| {
                    this.invoke(
                        json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":id,"dialogAction":"cancel"}}),
                        cx,
                    );
                }),
            ));
        }
        self.status_card_with_header(header, body, actions, p)
    }

    /// The answer, capped at about `CAP_LINES` lines with "Show all" under it. Expanded, it grows to
    /// its full height; the card above the composer passes a `limit` past which it scrolls, so a
    /// long answer cannot push the transcript out of view, while the transcript row scrolls with
    /// the transcript.
    pub(super) fn side_answer_body(
        &self,
        key: String,
        markdown: String,
        references: &Value,
        limit: Option<f32>,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let expanded = self.expanded.contains(&key);
        let lines = estimated_lines(&markdown);
        let capped = lines > CAP_LINES;
        let cap = CAP_LINES as f32 * LINE_HEIGHT * s;
        let prose = self.markdown(format!("side-answer:{key}"), markdown, references, p, cx);
        let block = div()
            .id(SharedString::from(format!("side-answer-body:{key}")))
            .min_w_0()
            .map(|body| match limit {
                _ if capped && !expanded => body.max_h(px(cap)).overflow_hidden(),
                Some(limit) => body.max_h(px(limit.max(cap))).overflow_y_scroll(),
                None => body,
            })
            .child(prose)
            .into_any_element();
        let block = if capped && !expanded {
            measured(self.disclosure_floor(&key), block)
        } else {
            block
        };
        if !capped {
            return block;
        }
        let more = lines.saturating_sub(CAP_LINES);
        let toggle_key = key.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(6.0 * s))
            .child(block)
            .child(
                div()
                    .id(SharedString::from(format!("side-answer-toggle:{key}")))
                    .role(gpui::Role::Button)
                    .aria_label(if expanded { "Show less" } else { "Show all" })
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .text_size(px(13.0 * s))
                    .text_color(p.primary)
                    .chat_cursor_pointer()
                    .hover(|style| style.text_color(p.foreground))
                    .child(
                        svg()
                            .path(if expanded {
                                "titlebar/chevron-up.svg"
                            } else {
                                "titlebar/chevron-down.svg"
                            })
                            .size(px(14.0 * s))
                            .text_color(p.primary),
                    )
                    .child(if expanded {
                        "Show less".to_string()
                    } else {
                        format!("Show all · {more} more lines")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if expanded {
                            this.expanded.remove(&toggle_key);
                        } else {
                            this.expanded.insert(toggle_key.clone());
                        }
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// C copies and F forks while the side question card is up and the chat box is not taking
    /// typing. Returns whether the key was used.
    pub(super) fn side_question_key(
        &mut self,
        key: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return false;
        }
        let dialog = self.snapshot["terminalNotice"]["dialog"].clone();
        let card = &dialog["presentation"]["sideQuestion"];
        if self.snapshot["noticeVisible"] != true || card["complete"] != true {
            return false;
        }
        match key.key.as_str() {
            "c" => {
                crate::app::helpers::gpui_copy_to_clipboard(
                    gpui::ClipboardItem::new_string(text(card, "answer")),
                    cx,
                );
                true
            }
            "f" if card["canFork"] == true => {
                self.invoke(
                    json!({"type":"answer","answer":{"kind":"terminalDialog","dialogId":dialog["id"],"dialogAction":"fork"}}),
                    cx,
                );
                true
            }
            _ => false,
        }
    }

    /// CDXC:SessionChat 2026-09-27 DECISION:
    /// User: after Close the side question stays in the chat for good, folded to one line at the point it was asked, and opens again on click; it reads quieter than the conversation and says it was not added to it.
    pub(super) fn side_question_row(
        &self,
        id: &str,
        message: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let card = &message["sideQuestion"];
        let key = format!("side-question:{id}");
        let expanded = self.expanded.contains(&key);
        let answer = text(card, "answer");
        let toggle_key = key.clone();
        let header = div()
            .id(SharedString::from(format!("side-question-row:{id}")))
            .role(gpui::Role::Button)
            .aria_label(format!("Side question: {}", text(card, "question")))
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(12.0 * s))
            .py(px(8.0 * s))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.35)))
            .child(
                svg()
                    .path(if expanded {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(14.0 * s))
                    .text_color(p.muted)
                    .flex_shrink_0(),
            )
            .child(side_badge(p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0 * s))
                    .text_color(p.foreground)
                    .truncate()
                    .child(text(card, "question")),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0 * s))
                    .text_color(p.muted)
                    .child(text(message, "time")),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.anchor_disclosure_toggle(&toggle_key, !expanded);
                if expanded {
                    this.expanded.remove(&toggle_key);
                } else {
                    this.expanded.insert(toggle_key.clone());
                }
                cx.notify();
            }));
        let body = expanded.then(|| {
            let copy = answer.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(6.0 * s))
                .border_t_1()
                .border_color(p.border)
                .px(px(14.0 * s))
                .pt(px(10.0 * s))
                .pb(px(8.0 * s))
                .child(if answer.is_empty() {
                    div()
                        .text_size(px(13.0 * s))
                        .text_color(p.muted)
                        .child("No answer was saved for this side question.")
                        .into_any_element()
                } else {
                    self.side_answer_body(
                        format!("side-question-row:{id}"),
                        text(card, "answerMarkdown"),
                        &card["answerReferences"],
                        None,
                        p,
                        cx,
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0 * s))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.0 * s))
                                .text_color(p.muted)
                                .child("Not added to the conversation"),
                        )
                        .when(!answer.is_empty(), |row| {
                            row.child(side_button(
                                &format!("side-question-row-copy:{id}"),
                                Some("titlebar/copy.svg"),
                                "Copy",
                                None,
                                false,
                                None,
                                p,
                                cx.listener(move |_, _, _, cx| {
                                    crate::app::helpers::gpui_copy_to_clipboard(
                                        gpui::ClipboardItem::new_string(copy.clone()),
                                        cx,
                                    );
                                }),
                            ))
                        }),
                )
        });
        div()
            .w_full()
            .min_w_0()
            .pb(px(13.0 * s))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .rounded(px(10.0 * s))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.input.opacity(0.35))
                    .overflow_hidden()
                    .child(header)
                    .when_some(body, |row, body| row.child(body)),
            )
            .into_any_element()
    }
}

/// The "Side question" pill both the card and the transcript row lead with.
fn side_badge(p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex_shrink_0()
        .h(px(HEADER_LINE * s))
        .flex()
        .items_center()
        .gap(px(4.0 * s))
        .px(px(7.0 * s))
        .line_height(px(14.0 * s))
        .rounded(px(999.0))
        .border_1()
        .border_color(p.primary.opacity(0.35))
        .bg(p.primary.opacity(0.1))
        .text_size(px(11.5 * s))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.primary)
        .child(
            svg()
                .path("titlebar/message-circle.svg")
                .size(px(12.0 * s))
                .text_color(p.primary),
        )
        .child("Side question")
        .into_any_element()
}

/// Three muted bars where the answer will go while Claude is still answering.
pub(super) fn answering_placeholder(p: &ChatAppearance) -> AnyElement {
    let s = p.scale;
    div()
        .flex()
        .flex_col()
        .gap(px(9.0 * s))
        .py(px(4.0 * s))
        .children([0.92_f32, 0.78, 0.45].into_iter().map(|width| {
            div()
                .h(px(9.0 * s))
                .w(relative(width))
                .rounded(px(5.0 * s))
                .bg(p.muted.opacity(0.28))
        }))
        .into_any_element()
}

/// A neutral filled footer button with an optional icon and the key that does the same thing.
pub(super) fn side_button(
    id: &str,
    icon: Option<&'static str>,
    label: &'static str,
    key_hint: Option<&'static str>,
    disabled: bool,
    tooltip: Option<&'static str>,
    p: &ChatAppearance,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let s = p.scale;
    div()
        .id(SharedString::from(id.to_string()))
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex()
        .items_center()
        .gap(px(6.0 * s))
        .px(px(9.0 * s))
        .py(px(4.0 * s))
        .rounded(px(6.0 * s))
        .border_1()
        .chat_filled(p.neutral_button_fills(), !disabled)
        .text_color(p.primary)
        .when(disabled, |button| button.opacity(0.45))
        .when_some(tooltip, |button, tooltip| {
            button.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip).build(window, cx)
            })
        })
        .when(!disabled, |button| {
            button.chat_cursor_pointer().on_click(on_click)
        })
        .when_some(icon, |button, icon| {
            button.child(svg().path(icon).size(px(14.0 * s)).text_color(p.primary))
        })
        .child(label)
        .when_some(key_hint, |button, key| {
            button.child(
                div()
                    .px(px(4.0 * s))
                    .rounded(px(4.0 * s))
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(11.0 * s))
                    .line_height(px(15.0 * s))
                    .text_color(p.muted)
                    .child(key),
            )
        })
        .into_any_element()
}
