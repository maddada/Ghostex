//! The private secret request cards above the composer (`secretRequests` in the chat core's
//! document, `extras/secret_requests.rs`): an agent's `ghostex secret request`, answered in a
//! masked field this view owns.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! The typed value stays in the view's own masked `InputState` until Save, then crosses once in the `answerSecretRequest` action; it is never echoed through the core's document or the composer draft the way the question cards' text boxes are, and the field is dropped as soon as its card closes.
//!
//! CDXC:Secrets 2026-11-18 WHY:
//! #648: a secret typed into the card went to the chat box and was sent as a chat message. The card's field was missing from the chat's list of its own text fields (`chat_text_field_focused`), so every keystroke typed in it was taken by the chat's background typing (`composer_background_key`), which moves the keyboard to the chat box and types the key there, and every queued chat box focus (the pane frame queues one on each press inside the chat) took the keyboard back from it. The field is now in that list, and while a card is open the chat box takes no keystrokes at all: a key bound for it is typed into the oldest open card instead (`secret_card_key`), a queued or gained chat box focus moves to the card, and a card opening while the chat box holds the keyboard takes it.
//! SEE-ALSO: apps/mobile/app/src/chat/native/cards/SecretRequestCard.tsx (the phone's card), packages/gx-chat-core/src/extras/secret_requests.rs.

use std::collections::HashMap;

use super::tone_card::{ToneButton, ToneCardParts, ToneLook, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::{Value, json};

/// The masked field of each open card, by request id.
#[derive(Default)]
pub(super) struct SecretRequestInputs {
    fields: HashMap<String, (Entity<InputState>, Subscription)>,
}

impl NativeChatView {
    fn secret_request_field(
        &mut self,
        id: &str,
        placeholder: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some((input, _)) = self.secret_request_inputs.fields.get(id) {
            return input.clone();
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(placeholder)
        });
        // A card that opens while the chat box holds the keyboard takes it, so the next keystroke
        // lands in the card. A field already being typed in (another card's) keeps it.
        if self
            .input
            .as_ref()
            .is_some_and(|composer| composer.read(cx).focus_handle(cx).is_focused(window))
        {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        let request_id = id.to_string();
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this: &mut Self, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                    this.save_secret_request(&request_id, cx);
                }
            },
        );
        self.secret_request_inputs
            .fields
            .insert(id.to_string(), (input.clone(), subscription));
        input
    }

    /// Whether a secret request card of this chat is open (unanswered).
    pub(super) fn secret_request_open(&self) -> bool {
        self.snapshot["secretRequests"]
            .as_array()
            .is_some_and(|cards| cards.iter().any(|card| card["open"] == true))
    }

    /// The masked field of the oldest open card, once it has been drawn: where the chat box's
    /// keyboard goes while a card is open.
    pub(super) fn open_secret_field(&self) -> Option<Entity<InputState>> {
        self.snapshot["secretRequests"]
            .as_array()?
            .iter()
            .filter(|card| card["open"] == true)
            .find_map(|card| self.secret_request_inputs.fields.get(card["id"].as_str()?))
            .map(|(input, _)| input.clone())
    }

    /// The masked field of every open card.
    pub(super) fn secret_fields(&self) -> impl Iterator<Item = &Entity<InputState>> {
        self.secret_request_inputs
            .fields
            .values()
            .map(|(input, _)| input)
    }

    fn save_secret_request(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((input, _)) = self.secret_request_inputs.fields.get(id) else {
            return;
        };
        let secret = input.read(cx).value().to_string();
        if secret.trim().is_empty() {
            return;
        }
        self.invoke(
            json!({"type":"answerSecretRequest","requestId":id,"secret":secret}),
            cx,
        );
    }

    fn decline_secret_request(&mut self, id: &str, cx: &mut Context<Self>) {
        self.invoke(
            json!({"type":"answerSecretRequest","requestId":id,"decline":true}),
            cx,
        );
    }

    /// Every secret request card of this session, oldest first, or `None` when there is none.
    pub(super) fn render_secret_requests(
        &mut self,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let cards = self.snapshot["secretRequests"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        // A field outlives its card only until the next draw: a closed card's text is dropped.
        let open: Vec<String> = cards
            .iter()
            .filter(|card| card["open"] == true)
            .map(|card| text(card, "id"))
            .collect();
        self.secret_request_inputs
            .fields
            .retain(|id, _| open.contains(id));
        if cards.is_empty() {
            return None;
        }
        let s = p.scale;
        let mut column = div().flex().flex_col().gap(px(8.0 * s)).w_full();
        for card in &cards {
            column = column.child(self.secret_request_card(card, p, window, cx));
        }
        Some(column.into_any_element())
    }

    fn secret_request_card(
        &mut self,
        card: &Value,
        p: &ChatAppearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(card, "id");
        let look = ToneLook {
            icon: "titlebar/lock.svg".to_string(),
            ..tone_look("approval", p)
        };
        let label = text(card, "label");
        if card["open"] != true {
            return self.tone_card(
                ToneCardParts {
                    look,
                    title: label,
                    detail: text(card, "closedLabel"),
                    trailing: Vec::new(),
                    body: Vec::new(),
                    footer_start: Vec::new(),
                    footer_note: None,
                    footer_end: Vec::new(),
                },
                None,
                None,
                p,
            );
        }
        let busy = card["busy"] == true;
        let input = self.secret_request_field(&id, text(card, "placeholder"), window, cx);
        let has_text = !input.read(cx).value().trim().is_empty();
        let error = text(card, "error");
        let mut body = vec![
            // A reason can carry a long link; it breaks anywhere rather than widening the card.
            div()
                .w_full()
                .min_w_0()
                .text_color(p.foreground)
                .whitespace_normal()
                .child(text(card, "reason"))
                .into_any_element(),
            div()
                .id(gpui::SharedString::from(format!(
                    "secret-request-field:{id}"
                )))
                .w_full()
                .child(
                    Input::new(&input)
                        .disabled(busy)
                        .caret_color(p.foreground)
                        .w_full()
                        .text_size(px(14.0 * s)),
                )
                .into_any_element(),
        ];
        if !error.is_empty() {
            body.push(
                div()
                    .text_size(px(12.5 * s))
                    .text_color(gpui::rgb(0xef9999))
                    .child(error)
                    .into_any_element(),
            );
        }
        body.push(
            div()
                .flex()
                .items_start()
                .gap(px(6.0 * s))
                .text_size(px(12.0 * s))
                .line_height(px(16.0 * s))
                .text_color(p.muted)
                .child(
                    gpui::svg()
                        .path("titlebar/shield-check.svg")
                        .size(px(13.0 * s))
                        .mt(px(1.5 * s))
                        .flex_shrink_0()
                        .text_color(p.muted),
                )
                .child(text(card, "note"))
                .into_any_element(),
        );
        let decline_id = id.clone();
        let save_id = id.clone();
        let decline = self
            .tone_button(
                gpui::SharedString::from(format!("secret-request-decline:{id}")),
                &text(card, "declineLabel"),
                None,
                ToneButton::Secondary,
                None,
                busy,
                p,
            )
            .when(!busy, |button| {
                button.on_click(
                    cx.listener(move |this, _, _, cx| this.decline_secret_request(&decline_id, cx)),
                )
            })
            .into_any_element();
        let save = self
            .tone_button(
                gpui::SharedString::from(format!("secret-request-save:{id}")),
                &text(card, "saveLabel"),
                Some("titlebar/lock.svg"),
                ToneButton::Primary,
                None,
                busy || !has_text,
                p,
            )
            .when(!busy && has_text, |button| {
                button.on_click(
                    cx.listener(move |this, _, _, cx| this.save_secret_request(&save_id, cx)),
                )
            })
            .into_any_element();
        self.tone_card(
            ToneCardParts {
                look,
                title: label,
                detail: text(card, "title"),
                trailing: Vec::new(),
                body,
                footer_start: Vec::new(),
                footer_note: None,
                footer_end: vec![decline, save],
            },
            None,
            None,
            p,
        )
    }
}
