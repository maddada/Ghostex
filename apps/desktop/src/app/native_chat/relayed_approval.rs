//! A coordinator's threads' approval cards, relayed to its own chat (`relayedApprovals` in the chat
//! core's snapshot, gx-chat-core extras/relayed_approvals.rs, which holds the DECISION): the card
//! under the thread's "waiting for an answer" report, the Approvals pill's menu, the buttons on the
//! board's Needs you row, and, in the thread's own chat, the "Approved from <coordinator>" line.
//!
//! The buttons are neutral fills, never the dark primary fill, because the card sits on the chat's
//! glass (the user's standing rule: no black fills on glass).

use super::tone_card::{ToneButton, ToneCardParts, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// The Approvals pill's id and its menu's trigger (menu_toggle.rs).
pub(super) const APPROVALS_MENU: &str =
    ghostex_gx_chat_core::extras::relayed_approvals::APPROVALS_PILL;
pub(super) const APPROVALS_TRIGGER: &str = "chat-pill-approvals";
pub(super) const APPROVALS_MENU_WIDTH: f32 = 480.0;
/// The tallest the menu grows before its list scrolls.
const APPROVALS_MENU_MAX: f32 = 560.0;

/// A settled card's mark and colour.
fn settled_mark(tone: &str, p: &ChatAppearance) -> (&'static str, gpui::Hsla) {
    match tone {
        "good" => ("titlebar/check.svg", gpui::rgb(0x3fb871).into()),
        "bad" => ("titlebar/x.svg", gpui::rgb(0xec5357).into()),
        "warn" => ("titlebar/clock.svg", gpui::rgb(0xf5a524).into()),
        _ => ("titlebar/arrow-right.svg", p.muted),
    }
}

/// "2:15 PM" for an ISO time, empty when it does not parse.
fn clock(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%-I:%M %p")
                .to_string()
        })
        .unwrap_or_default()
}

impl NativeChatView {
    fn relayed_cards(&self) -> &[Value] {
        self.snapshot["relayedApprovals"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
    }

    /// The card drawn under the report row `message_id`, open or ended.
    pub(super) fn relayed_card_for_report(&self, message_id: &str) -> Option<Value> {
        self.relayed_cards()
            .iter()
            .find(|card| card["anchorId"].as_str() == Some(message_id))
            .cloned()
    }

    /// The open card of the thread `session_id`, for its board row.
    pub(super) fn relayed_card_for_thread(&self, session_id: &str) -> Option<Value> {
        self.relayed_cards()
            .iter()
            .find(|card| card["open"] == true && card["sessionId"].as_str() == Some(session_id))
            .cloned()
    }

    /// One relayed card: the thread's question, its command or arguments (four lines, then the rest
    /// a click away) and its own buttons; once it ended, one line saying how. `place` keeps the
    /// element ids apart where the same card shows twice (under the report and in the menu).
    pub(super) fn relayed_approval_card(
        &self,
        card: &Value,
        place: &'static str,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        if card["open"] != true {
            return self.relayed_settled_row(card, place, p);
        }
        let thread = text(card, "threadTitle");
        let header = text(card, "header");
        let detail = [
            header.as_str(),
            thread.as_str(),
            text(card, "source").as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let mut body = Vec::new();
        let code = text(card, "code");
        if !code.is_empty() {
            let open_key = format!("relay-code:{key}");
            let expanded = self.expanded.contains(&open_key);
            let preview = text(card, "codePreview");
            let more = text(card, "codeMore");
            let shown = if expanded || preview.is_empty() {
                code.clone()
            } else {
                preview
            };
            let shell = text(card, "codeLabel") == "Command";
            let mut column = div()
                .flex()
                .flex_col()
                .gap(px(4.0 * s))
                .child(super::tone_card::command_box(shown, shell, p));
            if !more.is_empty() {
                let toggle_key = open_key.clone();
                column = column.child(
                    div()
                        .id(gpui::SharedString::from(format!(
                            "relay-code-toggle:{place}:{key}"
                        )))
                        .role(gpui::Role::Button)
                        .text_size(px(12.0 * s))
                        .text_color(p.muted)
                        .chat_cursor_pointer()
                        .hover(|style| style.text_color(p.foreground))
                        .child(if expanded {
                            "Show less".to_string()
                        } else {
                            format!("Show {more}")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !this.expanded.remove(&toggle_key) {
                                this.expanded.insert(toggle_key.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            body.push(column.into_any_element());
        }
        let reason = text(&card["reason"], "text");
        if !reason.is_empty() {
            body.push(
                div()
                    .text_size(px(12.5 * s))
                    .text_color(gpui::rgb(0xf5a524))
                    .child(reason)
                    .into_any_element(),
            );
        }
        let error = text(card, "error");
        if !error.is_empty() {
            body.push(
                div()
                    .text_size(px(12.5 * s))
                    .text_color(gpui::rgb(0xef9999))
                    .child(error)
                    .into_any_element(),
            );
        }
        let busy = card["busy"] == true;
        let footer_end = self.relayed_buttons(card, place, busy, p, cx);
        let open_thread = json!({
            "type": "openCoordinatorThread",
            "projectId": text(card, "projectId"),
            "sessionId": text(card, "sessionId"),
        });
        let footer_start = vec![
            div()
                .id(gpui::SharedString::from(format!(
                    "relay-open:{place}:{key}"
                )))
                .role(gpui::Role::Link)
                .text_size(px(12.0 * s))
                .text_color(p.muted)
                .chat_cursor_pointer()
                .hover(|style| style.text_color(p.foreground))
                .tooltip(|window, cx| {
                    gpui_component::tooltip::Tooltip::new("Open the thread").build(window, cx)
                })
                .child(text(card, "note"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.invoke(open_thread.clone(), cx);
                }))
                .into_any_element(),
        ];
        self.tone_card(
            ToneCardParts {
                look: super::tone_card::ToneLook {
                    icon: "titlebar/shield-check.svg".to_string(),
                    ..tone_look("approval", p)
                },
                title: text(card, "title"),
                detail,
                trailing: Vec::new(),
                body,
                footer_start,
                footer_end,
            },
            None,
            None,
            p,
        )
    }

    /// The card's buttons, in the thread card's order, each with its tooltip.
    fn relayed_buttons(
        &self,
        card: &Value,
        place: &'static str,
        busy: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let key = text(card, "key");
        card["choices"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, choice)| {
                let label = text(choice, "label");
                let tooltip = text(choice, "tooltip");
                let button = self.tone_button(
                    gpui::SharedString::from(format!("relay-choice:{place}:{key}:{index}")),
                    &label,
                    None,
                    ToneButton::Secondary,
                    None,
                    busy,
                    p,
                );
                let button = if text(choice, "tone") == "deny" {
                    button.text_color(gpui::rgb(0xec5357))
                } else {
                    button
                };
                let button = if tooltip.is_empty() {
                    button
                } else {
                    button.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                };
                if busy {
                    return button.into_any_element();
                }
                let action = json!({"type": "answerRelayedApproval", "key": key, "choice": index});
                button
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke(action.clone(), cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// An ended card: its mark, how it ended and when, on one line.
    fn relayed_settled_row(
        &self,
        card: &Value,
        place: &'static str,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let (icon, color) = settled_mark(&text(card, "settledTone"), p);
        let time = clock(&text(card, "settledAt"));
        let thread = text(card, "threadTitle");
        let tooltip = [thread.as_str(), text(card, "title").as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        div()
            .id(gpui::SharedString::from(format!(
                "relay-settled:{place}:{}",
                text(card, "key")
            )))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(8.0 * s))
            .px(px(10.0 * s))
            .py(px(6.0 * s))
            .rounded(px(9.0 * s))
            .border_1()
            .border_color(p.input_border)
            .bg(p.card_panel)
            .text_size(px(12.5 * s))
            .when(!tooltip.is_empty(), |row| {
                row.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
            })
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
                    .min_w_0()
                    .truncate()
                    .text_color(p.card_muted)
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0 * s))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.foreground)
                                    .child(text(card, "settledLead")),
                            )
                            .child(div().min_w_0().truncate().child(text(card, "settledText"))),
                    ),
            )
            .when(!time.is_empty(), |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0 * s))
                        .text_color(p.muted)
                        .child(time),
                )
            })
            .into_any_element()
    }

    /// The Approvals menu's height (unscaled): each open card's estimate, capped so the list
    /// scrolls.
    pub(super) fn approvals_menu_height(&self) -> Option<f32> {
        let open: Vec<Value> = self
            .relayed_cards()
            .iter()
            .filter(|card| card["open"] == true)
            .cloned()
            .collect();
        if open.is_empty() {
            return None;
        }
        let cards: f32 = open
            .iter()
            .map(|card| {
                let code = text(card, "codePreview");
                let code = if code.is_empty() {
                    text(card, "code")
                } else {
                    code
                };
                let lines = code.lines().count().min(4) as f32;
                let title_lines = (text(card, "title").chars().count() as f32 / 52.0)
                    .ceil()
                    .max(1.0);
                let mut height = 30.0 + title_lines * 20.0 + 48.0;
                if lines > 0.0 {
                    height += 22.0
                        + lines * 17.0
                        + if text(card, "codeMore").is_empty() {
                            0.0
                        } else {
                            20.0
                        };
                }
                if !text(&card["reason"], "text").is_empty() {
                    height += 22.0;
                }
                height + 12.0
            })
            .sum();
        Some((super::chat_pills::MENU_HEADER_HEIGHT + 8.0 + cards).min(APPROVALS_MENU_MAX))
    }

    /// The Approvals menu: every open card, oldest asked first.
    pub(super) fn approvals_menu_body(
        &self,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let open: Vec<Value> = self
            .relayed_cards()
            .iter()
            .filter(|card| card["open"] == true)
            .cloned()
            .collect();
        if open.is_empty() {
            return None;
        }
        let count = open.len();
        let header = super::chat_pills::pill_menu_header(
            "titlebar/shield-check.svg",
            "Approvals",
            format!(
                "{count} {} waiting for you",
                if count == 1 { "thread" } else { "threads" }
            ),
            None,
            p,
        );
        let list = div()
            .id("chat-approvals-list")
            .flex()
            .flex_col()
            .gap(px(8.0 * s))
            .max_h(px((APPROVALS_MENU_MAX
                - super::chat_pills::MENU_HEADER_HEIGHT
                - 8.0)
                * s))
            .overflow_y_scroll()
            .children(
                open.iter()
                    .map(|card| self.relayed_approval_card(card, "menu", p, cx)),
            )
            .into_any_element();
        Some(super::chat_pills::pill_menu_column(
            "chat-approvals-menu",
            "Approvals",
            header,
            Some(list),
            None,
            p,
        ))
    }

    /// The buttons a board row shows in place of its links while its thread's card waits: the
    /// thread card's own, small; each tooltip names what it answers.
    pub(super) fn board_relay_buttons(
        &self,
        card: &Value,
        chip_size: f32,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = text(card, "key");
        let question = [
            text(card, "title"),
            text(card, "codePreview"),
            text(card, "code"),
        ]
        .into_iter()
        .find(|part| !part.is_empty())
        .unwrap_or_default();
        let command = {
            let preview = text(card, "codePreview");
            if preview.is_empty() {
                text(card, "code")
            } else {
                preview
            }
        };
        let mut row = div().flex().items_center().gap(px(4.0 * s)).min_w_0();
        for (index, choice) in card["choices"].as_array().into_iter().flatten().enumerate() {
            let label = text(choice, "label");
            let deny = text(choice, "tone") == "deny";
            let tooltip = [
                question.clone(),
                if command == question {
                    String::new()
                } else {
                    command.clone()
                },
                text(choice, "tooltip"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
            let action = json!({"type": "answerRelayedApproval", "key": key, "choice": index});
            let busy = card["busy"] == true;
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "board-relay:{key}:{index}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{label}: {question}"))
                    .flex_shrink_0()
                    .h(px(22.0 * s))
                    .px(px(8.0 * s))
                    .flex()
                    .items_center()
                    .rounded(px(11.0 * s))
                    .border(px(s))
                    .border_color(p.foreground.opacity(0.12))
                    .bg(p.foreground.opacity(0.05))
                    .text_size(px(chip_size * s))
                    .text_color(if deny {
                        gpui::rgb(0xec5357).into()
                    } else {
                        p.foreground
                    })
                    .when(busy, |chip| chip.opacity(0.5))
                    .when(!busy, |chip| {
                        chip.chat_cursor_pointer()
                            .hover(|style| style.bg(p.foreground.opacity(0.1)))
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !busy {
                            this.invoke(action.clone(), cx);
                        }
                    })),
            );
        }
        row.into_any_element()
    }

    /// A thread's chat: the newest answer its coordinator's chat sent for it ("Approved from Main
    /// Coordinator"), where its card was, while it is recent (gxserver sends the last 12 hours).
    pub(super) fn render_relayed_answer_lines(&self, p: &ChatAppearance) -> Option<AnyElement> {
        let line = self.snapshot["relayedAnswerLines"]
            .as_array()
            .and_then(|lines| lines.last())
            .cloned()?;
        let s = p.scale;
        let (icon, color) = settled_mark(&text(&line, "tone"), p);
        let detail = text(&line, "detail");
        let time = clock(&text(&line, "at"));
        Some(
            div()
                .id("chat-relayed-answer")
                .w_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.0 * s))
                .px(px(10.0 * s))
                .py(px(5.0 * s))
                .rounded(px(9.0 * s))
                .border_1()
                .border_color(p.input_border)
                .bg(p.card_panel)
                .text_size(px(12.5 * s))
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(detail.clone()).build(window, cx)
                })
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
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.foreground)
                        .child(text(&line, "text")),
                )
                .when(!time.is_empty(), |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0 * s))
                            .text_color(p.muted)
                            .child(time),
                    )
                })
                .into_any_element(),
        )
    }
}
