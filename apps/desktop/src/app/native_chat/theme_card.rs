//! The "Theme changed" card (`kind: themeChanged` in `gbotCards`, words from
//! packages/gx-chat-core/src/transcript/theme_cards.rs, state in `gbotCardStates`): the palette icon,
//! "Theme changed by Claude", one line per setting it changed, anything it made hard to read in the
//! warning colour, then Undo; after Undo, "Theme change undone" and who undid it.
//!
//! CDXC:Theming 2026-11-14 SEE-ALSO: the user's decision is on packages/gx-chat-core/src/transcript/theme_cards.rs; the phone draws the same card in apps/mobile/app/src/chat/native/transcript/GbotCard.tsx.

use super::tone_card::{ToneButton, ToneCardParts, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, px,
};
use serde_json::{Value, json};

impl NativeChatView {
    pub(super) fn theme_card(
        &self,
        card: &Value,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let change_id = text(card, "changeId");
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
        let mut look = tone_look("info", p);
        look.icon = "titlebar/palette.svg".to_string();
        let warn = tone_look("warning", p).hue.unwrap_or(p.foreground);
        let line = |content: String, color| {
            div()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .text_size(px(12.5 * s))
                .text_color(color)
                .child(content)
                .into_any_element()
        };
        let mut body = Vec::new();
        if !undone {
            // The settings it changed read as one block, a line each.
            let items: Vec<AnyElement> = card["lines"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|item| line(item.to_string(), p.foreground))
                .collect();
            if !items.is_empty() {
                body.push(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .min_w_0()
                        .gap(px(2.0 * s))
                        .children(items)
                        .into_any_element(),
                );
            }
            for warning in card["warnings"].as_array().into_iter().flatten() {
                if let Some(warning) = warning.as_str() {
                    body.push(line(warning.to_string(), warn));
                }
            }
        }
        let note = text(state, "note");
        if !note.is_empty() {
            body.push(line(note, p.muted));
        }
        let error = text(state, "error");
        if !error.is_empty() {
            body.push(line(error, p.error()));
        }
        let mut footer_end = Vec::new();
        if !undone {
            let enabled = state["undoEnabled"] == true;
            let label = text(state, "undoLabel");
            footer_end.push(
                self.tone_button(
                    gpui::SharedString::from(format!("theme-card-undo:{change_id}")),
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
                            json!({"type": "gbotCard", "op": "undo", "changeId": change_id}),
                            cx,
                        )
                    }))
                })
                .into_any_element(),
            );
        }
        self.tone_card(
            ToneCardParts {
                look,
                title: if undone {
                    text(card, "removedTitle")
                } else {
                    text(card, "title")
                },
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
