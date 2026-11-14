//! The "GBot added" card: Chief of Staff added a GBot on its own (`gbotCards` on a message or a
//! finished turn, `gbotCardStates` in the document; packages/gx-chat-core/src/transcript/gbot_cards.rs
//! words both). The bot's icon in its colour, "Researcher added", what it is and runs on, then
//! Undo and Open settings; after Undo, "Researcher removed" and "Undone".
//!
//! The "Routine set" card (`kind: routineSet`, packages/gx-chat-core/src/transcript/routine_cards.rs)
//! is drawn here too: the GBot's icon, "Routine set: Inbox watch", its schedule in words and its next
//! run, then Pause (Resume), Edit and Undo.
//!
//! CDXC:GBots 2026-11-09 SEE-ALSO: the user's decision is on packages/gx-chat-core/src/transcript/gbot_cards.rs and routine_cards.rs; the phone draws the same card in apps/mobile/app/src/chat/native/transcript/GbotCard.tsx.

use super::tone_card::{ToneButton, ToneCardParts, ToneLook, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Hsla, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, px,
};
use serde_json::{Value, json};

/// `#9fb0ff` as a colour.
fn hex_color(value: &str) -> Option<Hsla> {
    let digits = value.trim().trim_start_matches('#');
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
        .map(|rgb| gpui::rgb(rgb).into())
}

impl NativeChatView {
    /// The cards of one message or finished turn, or `None` when there are none.
    pub(super) fn gbot_cards(
        &self,
        cards: &[Value],
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let cards: Vec<AnyElement> = cards
            .iter()
            .filter(|card| !text(card, "changeId").is_empty())
            // Not drawn while GBots are off (CDXC:GBots 2026-11-09 in gx-chat-core gbot_cards.rs).
            .filter(|card| {
                self.snapshot["gbotCardStates"][text(card, "changeId").as_str()]["off"]
                    != Value::Bool(true)
            })
            .map(|card| self.gbot_card(card, p, cx))
            .collect();
        (!cards.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .gap(px(8.0 * p.scale))
                .children(cards)
                .into_any_element()
        })
    }

    fn gbot_card(&self, card: &Value, p: &ChatAppearance, cx: &Context<Self>) -> AnyElement {
        // A "Theme changed" card rides these cards' pipeline (theme_card.rs).
        if text(card, "kind") == "themeChanged" {
            return self.theme_card(card, p, cx);
        }
        let s = p.scale;
        let change_id = text(card, "changeId");
        let bot_id = text(card, "botId");
        let state = &self.snapshot["gbotCardStates"][change_id.as_str()];
        if !state.is_object() {
            // Asked on the next turn of the loop, because this runs while the row is rendering;
            // the core asks gxserver once however often this draws.
            let change_id = change_id.clone();
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.invoke(
                        json!({"type": "gbotCard", "op": "load", "changeId": change_id}),
                        cx,
                    )
                });
            })
            .detach();
        }
        let undone = state["undone"] == true;
        let icon = text(card, "icon");
        let look = ToneLook {
            hue: hex_color(&text(card, "colorHex")).or(tone_look("info", p).hue),
            icon: if icon.is_empty() {
                "titlebar/robot.svg".to_string()
            } else {
                format!("titlebar/{icon}.svg")
            },
        };
        let title = if undone {
            text(card, "removedTitle")
        } else {
            text(card, "title")
        };
        let mut body = Vec::new();
        let line = text(card, "body");
        if !line.is_empty() && !undone {
            body.push(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .text_color(p.foreground)
                    .child(line)
                    .into_any_element(),
            );
        }
        let is_routine = text(card, "kind") == "routineSet";
        for (key, strong) in [("next", true), ("checkNote", false)] {
            let line = text(state, key);
            if is_routine && !line.is_empty() {
                body.push(
                    div()
                        .w_full()
                        .min_w_0()
                        .whitespace_normal()
                        .text_size(px(12.5 * s))
                        .text_color(if strong { p.foreground } else { p.muted })
                        .child(line)
                        .into_any_element(),
                );
            }
        }
        let note = text(state, "note");
        if !note.is_empty() {
            body.push(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .text_size(px(12.5 * s))
                    .text_color(p.muted)
                    .child(note)
                    .into_any_element(),
            );
        }
        let error = text(state, "error");
        if !error.is_empty() {
            body.push(
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .text_size(px(12.5 * s))
                    .text_color(gpui::rgb(0xef9999))
                    .child(error)
                    .into_any_element(),
            );
        }
        let mut footer_end = Vec::new();
        if is_routine && !undone && state["routine"] == true {
            let routine_id = text(card, "routineId");
            let pause_enabled = state["pauseEnabled"] == true;
            let paused = state["paused"] == true;
            let (pause_change, pause_routine) = (change_id.clone(), routine_id.clone());
            let op = text(state, "pauseOp");
            footer_end.push(
                self.tone_button(
                    gpui::SharedString::from(format!("routine-card-pause:{change_id}")),
                    &text(state, "pauseLabel"),
                    Some(if paused {
                        "titlebar/player-play.svg"
                    } else {
                        "titlebar/player-pause.svg"
                    }),
                    ToneButton::Secondary,
                    None,
                    !pause_enabled,
                    p,
                )
                .when(pause_enabled, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.invoke(
                            json!({
                                "type": "gbotCard",
                                "op": op,
                                "changeId": pause_change,
                                "routineId": pause_routine,
                            }),
                            cx,
                        )
                    }))
                })
                .into_any_element(),
            );
            if state["editEnabled"] == true {
                let (edit_change, edit_bot) = (change_id.clone(), bot_id.clone());
                footer_end.push(
                    self.tone_button(
                        gpui::SharedString::from(format!("routine-card-edit:{change_id}")),
                        "Edit",
                        Some("titlebar/pencil.svg"),
                        ToneButton::Secondary,
                        None,
                        false,
                        p,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.invoke(
                            json!({
                                "type": "gbotCard",
                                "op": "edit",
                                "changeId": edit_change,
                                "botId": edit_bot,
                                "routineId": routine_id,
                            }),
                            cx,
                        )
                    }))
                    .into_any_element(),
                );
            }
        }
        if !undone {
            let enabled = state["undoEnabled"] == true;
            let undo_id = change_id.clone();
            let label = text(state, "undoLabel");
            footer_end.push(
                self.tone_button(
                    gpui::SharedString::from(format!("gbot-card-undo:{change_id}")),
                    if label.is_empty() { "Undo" } else { &label },
                    Some("titlebar/arrow-back-up.svg"),
                    ToneButton::Secondary,
                    None,
                    !enabled,
                    p,
                )
                .when(enabled, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.invoke(
                            json!({"type": "gbotCard", "op": "undo", "changeId": undo_id}),
                            cx,
                        )
                    }))
                })
                .into_any_element(),
            );
        }
        if state["settingsEnabled"] == true {
            let settings_id = change_id.clone();
            footer_end.push(
                self.tone_button(
                    gpui::SharedString::from(format!("gbot-card-settings:{change_id}")),
                    "Open settings",
                    Some("titlebar/settings.svg"),
                    ToneButton::Secondary,
                    None,
                    false,
                    p,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.invoke(
                        json!({
                            "type": "gbotCard",
                            "op": "openSettings",
                            "changeId": settings_id,
                            "botId": bot_id,
                        }),
                        cx,
                    )
                }))
                .into_any_element(),
            );
        }
        self.tone_card(
            ToneCardParts {
                look,
                title,
                detail: String::new(),
                trailing: Vec::new(),
                body,
                footer_start: Vec::new(),
                footer_note: None,
                footer_end,
            },
            None,
            None,
            p,
        )
    }
}
