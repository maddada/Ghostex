//! What a needs-you item offers, drawn once for every place it shows (the dock above the chat box,
//! the Work panel's and Columns' opened cards, cards under replies): its answers as the dock's
//! filled answer buttons, the recommended one starred, and the actions every item repeats (Reply,
//! Open, Show in chat, Dismiss, Snooze, Mention) as quiet icon buttons with a tooltip naming each.
//!
//! CDXC:Coordinators 2026-11-10 DECISION:
//! User, of an opened Work panel card (a "Yes | No" outlined pill, then outlined Reply, Show in
//! chat, Dismiss, Snooze and Mention buttons) beside the dock's filled answers: "bro these buttons
//! are ugly. please make the ones always repeated just icon buttons with a tooltip and also let's
//! please use the same look for the buttons in the list in the expanded cards and here. the buttons
//! look different between the 2 now". One component for each, so the places cannot drift apart
//! again; the fills are the neutral glass-safe ones (no black on glass).

use super::{
    appearance::ChatAppearance, chat_text_look::PROSE_SIZE, state::NativeChatView,
    tone_card::ToneButton,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px, svg,
};
use serde_json::{Value, json};

/// The button an ask's answers fold into when they do not fit on one line (`answers_dropdown`).
pub(super) const CHOOSE_ANSWER: &str = "Choose an answer";
/// Its dropdown grows to its longest answer up to this (unscaled), and the window's edge.
const ANSWERS_MENU_MAX: f32 = 560.0;

/// The answers dropdown's rows: each answer whole on its own line, the recommended one starred, a
/// click sending its command as its button does.
fn answer_menu_rows(answers: &[Value]) -> Vec<Value> {
    answers
        .iter()
        .filter(|answer| answer["command"].is_object() && answer["locked"] != true)
        .map(|answer| {
            let mut row = json!({ "label": text(answer, "label"), "command": answer["command"] });
            if answer["recommended"] == true {
                row["detail"] = json!("★");
            }
            row
        })
        .collect()
}

/// A closed card's answer button's width (unscaled), as `work_compact_answers` draws it: its
/// label, 8px sides and border, then its check, reply arrow, star or Draft (`mark`).
fn compact_answer_width(label: &str, mark: f32, p: &ChatAppearance, cx: &gpui::App) -> f32 {
    (super::work_cards::dock_text_width(label, PROSE_SIZE, p, cx) + 18.0 + mark).max(34.0)
}

/// An icon button's box, and the icon in it (unscaled).
const ICON_BOX: f32 = 28.0;
const ICON_SIZE: f32 = 14.0;

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

/// The quiet word after a Reply that holds words not yet sent (`has_kept_reply`): the button's own
/// muted ink, no colour of its own.
fn draft_sign(p: &ChatAppearance) -> gpui::Div {
    div()
        .flex_shrink_0()
        .font_weight(gpui::FontWeight::NORMAL)
        .text_color(p.muted)
        .child("Draft")
}

impl NativeChatView {
    /// The dock's answer button: `tone_button` at the dock's one text size (`PROSE_SIZE`, the row
    /// titles') in regular weight, not the ask cards' 13 medium.
    ///
    /// CDXC:Coordinators 2026-11-12 DECISION:
    /// User, of the phone's dock, which mixed sizes (header counts, row titles, answer buttons,
    /// ages): "ONE font size for all text in the dock", the secondary text (ages, "(2 questions)",
    /// "need you", descriptions) a little dimmer than the primary, the button labels at normal or
    /// medium weight, not bold. The desktop dock had the same mix, so it follows.
    /// SEE-ALSO: apps/mobile/app/src/chat/native/cards/workItemParts.tsx (`buttonLabel`).
    #[allow(clippy::too_many_arguments)]
    fn dock_tone_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        icon: Option<&'static str>,
        kind: ToneButton,
        hint: Option<String>,
        disabled: bool,
        p: &ChatAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        self.tone_button(id, label, icon, kind, hint, disabled, p)
            .text_size(px(PROSE_SIZE * p.scale))
            .font_weight(gpui::FontWeight::NORMAL)
    }

    /// An ask's answers, the dock's look everywhere: one filled button each, the recommended one
    /// ending in its star, the one picked here carrying a check. `busy`: an answer is being sent,
    /// so the others wait.
    pub(super) fn work_answer_buttons(
        &self,
        key: &str,
        answers: &[Value],
        busy: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let s = p.scale;
        answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let label = text(answer, "label");
                let picked = answer["picked"] == true;
                let locked = answer["locked"] == true;
                let command = answer["command"].clone();
                // Reply (`reply_answer`): the reply box at the press, its words the answer.
                let reply = answer["reply"].clone();
                let draft = reply.is_object() && !picked && self.has_kept_reply(&reply);
                let icon = if picked {
                    Some("titlebar/check.svg")
                } else if reply.is_object() {
                    Some("titlebar/arrow-back-up.svg")
                } else {
                    None
                };
                self.dock_tone_button(
                    gpui::SharedString::from(format!("work-answer:{key}:{index}")),
                    &label,
                    icon,
                    ToneButton::Secondary,
                    None,
                    (busy || locked) && !picked,
                    p,
                )
                .when(draft, |button| {
                    button
                        .aria_label(format!("{label}, draft kept"))
                        .child(draft_sign(p))
                })
                .when(answer["recommended"] == true, |button| {
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
                .on_click(
                    cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if busy || locked {
                            return;
                        }
                        if reply.is_object() {
                            let at = event.position();
                            let command = json!({
                                "question": reply.clone(),
                                "at": [f32::from(at.x), f32::from(at.y)],
                            });
                            this.open_reply_popover(&command, window, cx);
                        } else if command.is_object() {
                            this.invoke(command.clone(), cx);
                        }
                    }),
                )
                .into_any_element()
            })
            .collect()
    }

    /// An ask's answers folded into one button, for a line too short for them all (the dock's rows,
    /// an opened Work panel card): "Choose an answer" with its arrow opens the app's dropdown, one
    /// answer a line and each whole, the recommended one starred; a click on one answers at once,
    /// as its button does. `busy`: an answer is being sent, so it waits.
    pub(super) fn answers_dropdown(
        &self,
        key: &str,
        answers: &[Value],
        busy: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let locked = busy || answers.iter().all(|answer| answer["locked"] == true);
        let rows = answer_menu_rows(answers);
        self.dock_tone_button(
            gpui::SharedString::from(format!("work-answers-choose:{key}")),
            CHOOSE_ANSWER,
            None,
            ToneButton::Secondary,
            None,
            locked,
            p,
        )
        .aria_label(format!("{CHOOSE_ANSWER}, {} answers", rows.len()))
        .child(
            svg()
                .path("titlebar/chevron-down.svg")
                .size(px(14.0 * s))
                .flex_shrink_0()
                .text_color(p.foreground),
        )
        .on_click(
            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                if !locked {
                    this.show_answers_menu(rows.clone(), event.position(), window, cx);
                }
            }),
        )
        .into_any_element()
    }

    /// The answers dropdown (`answers_dropdown`) at `at`, inside the window it was pressed in (the
    /// chat's, or the floating Work card's), as wide as its longest answer.
    fn show_answers_menu(
        &mut self,
        rows: Vec<Value>,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if rows.is_empty() {
            return;
        }
        let in_main = self
            .open_main_window(cx)
            .is_none_or(|main| main == window.window_handle());
        if in_main {
            self.show_chat_menu_at(rows, at, ANSWERS_MENU_MAX, window, cx);
        } else {
            self.show_chat_menu_in_window_at(rows, at, ANSWERS_MENU_MAX, window, cx);
        }
    }

    /// Reply as one more of an ask's answers, last after them in their look (`work_answer_buttons`,
    /// `work_compact_answers`): it opens the reply box, and the words sent are the ask's answer.
    /// `replied` is those words once sent from here (the button then shows them with a check);
    /// `locked` when another answer was picked.
    ///
    /// CDXC:Coordinators 2026-11-11 DECISION:
    /// User: "when there's options for a needs you question in the cards then we also need to
    /// show Reply as a button option (not a small button below) so i can click it and write
    /// whatever and it should be like picking an option (mark that ask as answered)". The small
    /// Reply icon under the row goes where this shows.
    pub(super) fn reply_answer(reply: &Value, replied: &str, locked: bool) -> Value {
        // The words it sent, cut short so the button stays a button.
        let label = match replied.chars().count() {
            0 => "Reply".to_string(),
            count if count > 32 => format!(
                "{}…",
                replied.chars().take(31).collect::<String>().trim_end()
            ),
            _ => replied.to_string(),
        };
        json!({
            "label": label,
            "reply": reply,
            "picked": !replied.is_empty(),
            "locked": locked || !replied.is_empty(),
        })
    }

    /// The one button a dock row with no answers shows in their place, in their look: Open for a
    /// page or form (`open`), else Reply for a question with no preset answers (`reply`, the
    /// Reply popover at the press). `None` when it offers neither.
    pub(super) fn work_row_action(
        &self,
        key: &str,
        row: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let open = row["open"].clone();
        let reply = row["reply"].clone();
        let draft = !open.is_object() && reply.is_object() && self.has_kept_reply(&reply);
        // Each carries its icon, so it reads as an action and not as an answer.
        let (label, icon, id) = if open.is_object() {
            (
                "Open",
                "titlebar/arrow-up-right.svg",
                format!("work-row-open:{key}"),
            )
        } else if reply.is_object() {
            (
                "Reply",
                "titlebar/arrow-back-up.svg",
                format!("work-row-reply:{key}"),
            )
        } else {
            return None;
        };
        Some(
            self.dock_tone_button(
                gpui::SharedString::from(id),
                label,
                Some(icon),
                ToneButton::Secondary,
                None,
                false,
                p,
            )
            .when(draft, |button| {
                button
                    .aria_label(format!("{label}, draft kept"))
                    .child(draft_sign(p))
            })
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    if open.is_object() {
                        this.invoke(open.clone(), cx);
                        return;
                    }
                    let at = event.position();
                    let command = json!({
                        "question": reply.clone(),
                        "at": [f32::from(at.x), f32::from(at.y)],
                    });
                    this.open_reply_popover(&command, window, cx);
                }),
            )
            .into_any_element(),
        )
    }

    /// An ask's answers at a closed Work panel card's size: the same neutral filled buttons as
    /// [`Self::work_answer_buttons`], with less padding so the card keeps its two lines. Each gives
    /// way with an ellipsis on a narrow card rather than running past its edge; the one sent from
    /// here carries a check, and the others fade once it is answered.
    ///
    /// `room` (unscaled) is the line's width for them: past it the answers fold into one Choose an
    /// answer dropdown beside Reply, as on the dock's rows (`answers_dropdown`), so none is cut.
    pub(super) fn work_compact_answers(
        &self,
        key: &str,
        answers: &[Value],
        room: f32,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        use super::appearance::ChatFilledButton as _;
        let s = p.scale;
        let widths: Vec<f32> = answers
            .iter()
            .map(|answer| {
                let picked = answer["picked"] == true;
                let reply = answer["reply"].is_object();
                let mut mark = 0.0;
                if picked || reply {
                    mark += 15.0;
                }
                if answer["recommended"] == true {
                    mark += 13.0;
                }
                if reply && !picked && self.has_kept_reply(&answer["reply"]) {
                    mark += super::work_cards::dock_text_width("Draft", PROSE_SIZE, p, cx) + 4.0;
                }
                compact_answer_width(&text(answer, "label"), mark, p, cx)
            })
            .collect();
        let gaps = |count: usize| 4.0 * count.saturating_sub(1) as f32;
        let choices: Vec<usize> = (0..answers.len())
            .filter(|index| !answers[*index]["reply"].is_object())
            .collect();
        let choices_width: f32 =
            choices.iter().map(|index| widths[*index]).sum::<f32>() + gaps(choices.len());
        let choose_width = compact_answer_width(CHOOSE_ANSWER, 15.0, p, cx);
        let fold = choices_width > choose_width
            && widths.iter().sum::<f32>() + gaps(answers.len()) > room
            && !answers.iter().any(|answer| answer["picked"] == true);
        let mut row = div()
            .flex_shrink(1.0)
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.0 * s));
        if fold {
            let choices: Vec<Value> = choices
                .iter()
                .map(|index| answers[*index].clone())
                .collect();
            let rows = answer_menu_rows(&choices);
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "work-answers-choose-compact:{key}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(format!("{CHOOSE_ANSWER}, {} answers", rows.len()))
                    .h(px(24.0 * s))
                    .px(px(8.0 * s))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .rounded(px(7.0 * s))
                    .border(px(s))
                    .chat_filled(p.neutral_button_fills(), !rows.is_empty())
                    .text_size(px(PROSE_SIZE * s))
                    .text_color(p.foreground)
                    .chat_cursor_pointer()
                    .child(div().whitespace_nowrap().child(CHOOSE_ANSWER))
                    .child(
                        svg()
                            .path("titlebar/chevron-down.svg")
                            .size(px(11.0 * s))
                            .flex_shrink_0()
                            .text_color(p.foreground),
                    )
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.show_answers_menu(rows.clone(), event.position(), window, cx);
                        }),
                    ),
            );
        }
        for (index, answer) in answers.iter().enumerate() {
            if fold && !answer["reply"].is_object() {
                continue;
            }
            let label = text(answer, "label");
            let picked = answer["picked"] == true;
            let recommended = answer["recommended"] == true;
            let locked = answer["locked"] == true;
            let command = answer["command"].clone();
            let reply = answer["reply"].clone();
            let draft = reply.is_object() && !picked && self.has_kept_reply(&reply);
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!(
                        "work-answer-compact:{key}:{index}"
                    )))
                    .role(gpui::Role::Button)
                    .aria_label(if recommended {
                        format!("{label}, Recommended")
                    } else {
                        label.clone()
                    })
                    .h(px(24.0 * s))
                    .px(px(8.0 * s))
                    .flex_shrink(1.0)
                    .min_w(px(34.0 * s))
                    .flex()
                    .items_center()
                    .gap(px(4.0 * s))
                    .rounded(px(7.0 * s))
                    .border(px(s))
                    .chat_filled(p.neutral_button_fills(), !locked)
                    .text_size(px(PROSE_SIZE * s))
                    .text_color(p.foreground)
                    .when(locked && !picked, |this| this.opacity(0.4))
                    .when(picked, |this| {
                        this.child(
                            svg()
                                .path("titlebar/check.svg")
                                .size(px(11.0 * s))
                                .text_color(p.foreground),
                        )
                    })
                    .when(reply.is_object() && !picked, |this| {
                        this.child(
                            svg()
                                .path("titlebar/arrow-back-up.svg")
                                .size(px(11.0 * s))
                                .flex_shrink_0()
                                .text_color(p.foreground),
                        )
                    })
                    .child(div().min_w_0().truncate().child(label.clone()))
                    .when(draft, |this| {
                        this.aria_label(format!("{label}, draft kept"))
                            .child(draft_sign(p))
                    })
                    .when(recommended, |this| {
                        this.child(
                            svg()
                                .path("titlebar/star-filled.svg")
                                .size(px(9.0 * s))
                                .flex_shrink_0()
                                .text_color(super::ask_card::ask_color(p).opacity(0.75)),
                        )
                        .tooltip(|window, cx| {
                            gpui_component::tooltip::Tooltip::new("Recommended").build(window, cx)
                        })
                    })
                    .when(!locked, |this| {
                        this.chat_cursor_pointer().on_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if reply.is_object() {
                                    let at = event.position();
                                    let command = json!({
                                        "question": reply.clone(),
                                        "at": [f32::from(at.x), f32::from(at.y)],
                                    });
                                    this.open_reply_popover(&command, window, cx);
                                } else {
                                    this.invoke(command.clone(), cx);
                                }
                            },
                        ))
                    })
                    // A locked answer still swallows the click, so the card behind it does not
                    // take it as a click on the card.
                    .when(locked, |this| {
                        this.on_click(|_, _, cx| cx.stop_propagation())
                    }),
            );
        }
        row.into_any_element()
    }

    /// One quiet icon button: the icon in the muted colour, brighter with a soft fill on hover, its
    /// name in the tooltip and to a screen reader.
    fn work_icon_button(
        &self,
        id: String,
        icon: &'static str,
        label: &'static str,
        on: bool,
        p: &ChatAppearance,
    ) -> gpui::Stateful<gpui::Div> {
        let s = p.scale;
        div()
            .id(gpui::SharedString::from(id))
            .group("work-action")
            .role(gpui::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .size(px(ICON_BOX * s))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0 * s))
            .when(on, |this| this.bg(p.foreground.opacity(0.08)))
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.foreground.opacity(0.08)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(label).build(window, cx)
            })
            .child(
                svg()
                    .path(icon)
                    .size(px(ICON_SIZE * s))
                    .text_color(if on { p.foreground } else { p.muted })
                    .group_hover("work-action", |style| style.text_color(p.foreground)),
            )
    }

    /// The actions an item repeats, as one row of icon buttons: Reply (`actions.reply`, the reply
    /// bubble), Open, Show in chat, Dismiss, Snooze (a dropdown of its three lengths) and
    /// Mention (`mention`, the item's reference for the chat box). Only those it offers show;
    /// `None` when it offers none. `own_menu`: the item sits in the Work panel (docked, floating or
    /// in columns), which is itself one of the chat's menus, so its Snooze lengths open as a page
    /// of the card's own menu (frosted under window glass) instead of a second chat menu that
    /// would close it.
    pub(super) fn work_action_icons(
        &self,
        key: &str,
        actions: &Value,
        mention: &str,
        own_menu: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let s = p.scale;
        let mut row = div().flex().flex_wrap().items_center().gap(px(2.0 * s));
        let mut any = false;
        let reply = actions["reply"].clone();
        if reply.is_object() {
            any = true;
            row = row.child(
                // Holding words not yet sent, it keeps the soft "on" fill and says so.
                self.work_icon_button(
                    format!("work-reply:{key}"),
                    "titlebar/arrow-back-up.svg",
                    if self.has_kept_reply(&reply) {
                        "Reply (draft kept)"
                    } else {
                        "Reply"
                    },
                    self.has_kept_reply(&reply),
                    p,
                )
                .on_click(cx.listener(
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let at = event.position();
                        let command = json!({
                            "question": reply.clone(),
                            "at": [f32::from(at.x), f32::from(at.y)],
                        });
                        this.open_reply_popover(&command, window, cx);
                    },
                )),
            );
        }
        for (field, icon, label) in [
            ("open", "titlebar/arrow-up-right.svg", "Open"),
            ("showInChat", "titlebar/focus-2.svg", "Show in chat"),
            ("dismiss", "titlebar/x.svg", "Dismiss"),
        ] {
            let command = actions[field].clone();
            if command.is_object() {
                any = true;
                row = row.child(
                    self.work_icon_button(format!("work-{field}:{key}"), icon, label, false, p)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.invoke(command.clone(), cx);
                        })),
                );
            }
        }
        // Snooze opens the app's dropdown at the button with its lengths (1 hour, 6 hours, 24
        // hours); picking one snoozes, and the row of buttons never changes.
        //
        // CDXC:Coordinators 2026-11-10 DECISION:
        // User, of the row turning into "Snooze for · 1 hour · 6 hours · 24 hours": "i don't like
        // how snooze turns to \"Snooze for\" just please show a dropdown menu instead for the
        // choices".
        let snooze: Vec<Value> = actions["snooze"].as_array().cloned().unwrap_or_default();
        if !snooze.is_empty() {
            any = true;
            let rows: Vec<Value> = snooze
                .iter()
                .map(|choice| json!({ "label": text(choice, "label"), "command": choice["command"] }))
                .collect();
            let popover_key = key.to_string();
            row = row.child(
                self.work_icon_button(
                    format!("work-snooze:{key}"),
                    "titlebar/alarm.svg",
                    "Snooze",
                    false,
                    p,
                )
                .on_click(cx.listener(
                    move |this, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let at = event.position();
                        if own_menu {
                            // A page of the card's own menu, drawn by the card's window so the
                            // floating Work card stays open, frosted under window glass and
                            // opening upward at the window's bottom (`toggle_board_card_snooze`).
                            this.toggle_board_card_snooze(&popover_key, at, window, cx);
                            return;
                        }
                        // A card in one of the chat's own windows (the floating Work card) keeps
                        // its menu inside that window.
                        let in_main = this
                            .open_main_window(cx)
                            .is_none_or(|main| main == window.window_handle());
                        if in_main {
                            this.show_chat_menu_at(rows.clone(), at, 160.0, window, cx);
                        } else {
                            this.show_chat_menu_in_window_at(rows.clone(), at, 160.0, window, cx);
                        }
                    },
                )),
            );
        }
        if !mention.is_empty() {
            any = true;
            let mention = mention.to_string();
            row = row.child(
                self.work_icon_button(
                    format!("work-mention:{key}"),
                    "titlebar/message-plus.svg",
                    "Mention",
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.insert_board_reference(&mention, cx);
                })),
            );
        }
        any.then(|| row.into_any_element())
    }
}
