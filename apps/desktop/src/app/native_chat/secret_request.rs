//! The private secret request cards above the composer (`secretRequests` in the chat core's
//! document, `extras/secret_requests.rs`): an agent's `ghostex secret request`, answered in a
//! masked field this view owns.
//!
//! CDXC:Secrets 2026-11-02 WHY:
//! The typed value stays in the view's own masked `InputState` until Save, then crosses once in the `answerSecretRequest` action; it is never echoed through the core's document or the composer draft the way the question cards' text boxes are, and the field is dropped as soon as its card closes.
//!
//! CDXC:Secrets 2026-11-18 WHY:
//! #648: a secret typed into the card went to the chat box and was sent as a chat message. The card's field was missing from the chat's list of its own text fields (`chat_text_field_focused`), so every keystroke typed in it was taken by the chat's background typing (`composer_background_key`), which moves the keyboard to the chat box and types the key there, and every queued chat box focus (the pane frame queues one on each press inside the chat) took the keyboard back from it. The field is now in that list, background typing goes to the oldest open card instead of the chat box (`secret_card_key`), a queued chat box focus goes to the card when no field of the chat holds the keyboard, and a card opening while the chat box holds the keyboard takes it.
//!
//! CDXC:Secrets 2026-11-19 DECISION:
//! User: "keep the chat box usable while a secret card is open. Typed text goes to the chat as normal; the secret is only ever taken from the card's own field." Supersedes the part of the 2026-11-18 fix that locked the chat box for as long as a card was open. Keystrokes still go only where the user put focus: the card's field has the keyboard by default, and the chat box takes typing only after the user clicks into it (`composer_chosen`). A focus the user did not click into the box for (a hotkey, a handoff, a queued focus, a dropped file) still moves on to the card, and a key that reaches the box before that move is typed into the card. A second card opening while the user types in the chat box they clicked into leaves the keyboard there. Text in the chat box that looks like a key gets a warning and a second Send instead (`secret_send_warning.rs`).
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
    /// The user clicked into the chat box while a card was open, and it still has the keyboard:
    /// the chat box takes their typing until it loses focus or the last card closes.
    pub(super) composer_chosen: bool,
    /// When the last press on the chat box began, so the focus it gives the box counts as chosen.
    pub(super) composer_pressed_at: Option<web_time::Instant>,
    /// The chat's window going inactive and active again (`watch_window_for_secret_cards`), and
    /// since when it is inactive.
    window_activation: Option<Subscription>,
    inactive_since: Option<web_time::Instant>,
    /// The last press on the chat box, kept after `composer_pressed_at` is spent: a window that
    /// becomes active through a click on the chat box is the user choosing it.
    last_composer_press: Option<web_time::Instant>,
}

/// How long the chat's window must have been inactive for its return to count as coming back to
/// the app: the chat's own popup windows (the suggestions list) take the key window for a moment.
const AWAY_AT_LEAST: std::time::Duration = std::time::Duration::from_millis(300);

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
        // lands in the card. A field already being typed in (another card's) keeps it, and so does
        // the chat box the user clicked into while a card was already open.
        if !self.secret_request_inputs.composer_chosen
            && self
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

    /// A press on the chat box began: the focus it gives the box is the user's choice. The press
    /// itself chooses the box while a card is open, so a box that still held the keyboard (and so
    /// reports no focus gain) is chosen too.
    pub(super) fn note_composer_press(&mut self) {
        self.secret_request_inputs.composer_pressed_at = Some(web_time::Instant::now());
        self.secret_request_inputs.last_composer_press = Some(web_time::Instant::now());
        if self.secret_request_open() {
            self.secret_request_inputs.composer_chosen = true;
        }
    }

    /// The chat box gained the keyboard. Answers whether it keeps it: always with no card open,
    /// and with a card open only when the user clicked into it.
    pub(super) fn composer_focus_chosen(&mut self) -> bool {
        let pressed = self
            .secret_request_inputs
            .composer_pressed_at
            .take()
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1));
        if !self.secret_request_open() {
            self.secret_request_inputs.composer_chosen = false;
            return true;
        }
        // A choice the press already made stands (`note_composer_press`): a focus report that
        // arrives late, after the press was spent, must not move typing to the card mid-message.
        let chosen = self.secret_request_inputs.composer_chosen || pressed;
        self.secret_request_inputs.composer_chosen = chosen;
        chosen
    }

    /// The chat box keeps the keyboard only until a message leaves it or the user comes back to the
    /// window: while a card is open it goes back to the card then (CDXC:Secrets 2026-11-19 below).
    ///
    /// CDXC:Secrets 2026-11-19 WHY:
    /// The chat box the user clicked into kept the keyboard through Send and app switches for as long as a card was open, so "send 'one sec', copy the key from a password manager, come back, Cmd+V" pasted the key into the chat box. The keyboard now goes back to the card after a Send from the chat box and when the window becomes active again after the user was away; the chat box takes typing again after another click.
    pub(super) fn return_keyboard_to_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.secret_request_open() || !self.secret_request_inputs.composer_chosen {
            return;
        }
        self.secret_request_inputs.composer_chosen = false;
        // At once, not at the next draw: a paste (GPUI's own shortcut on the field) can come before
        // it. Only off a chat box that holds the keyboard; state.rs `ensure_input` checks again.
        let composer_focused = self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window));
        if composer_focused && let Some(secret) = self.open_secret_field() {
            secret.update(cx, |secret, cx| secret.focus(window, cx));
        }
        cx.notify();
    }

    /// The chat is drawn in `window` from now on (the main window or the floating sessions
    /// panel's): a choice of the chat box made in another window does not carry over, and the
    /// window coming back after the user was away hands the keyboard back to the card.
    pub(super) fn watch_window_for_secret_cards(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.secret_request_inputs.composer_chosen = false;
        self.secret_request_inputs.composer_pressed_at = None;
        self.secret_request_inputs.inactive_since = None;
        self.secret_request_inputs.window_activation =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.secret_request_inputs.inactive_since = Some(web_time::Instant::now());
                    return;
                }
                let away = this
                    .secret_request_inputs
                    .inactive_since
                    .take()
                    .is_some_and(|since| since.elapsed() >= AWAY_AT_LEAST);
                // Coming back with a click on the chat box chooses it, whichever of the two
                // events arrives first.
                let clicked_in = this
                    .secret_request_inputs
                    .last_composer_press
                    .is_some_and(|at| at.elapsed() < AWAY_AT_LEAST);
                if away && !clicked_in {
                    this.return_keyboard_to_card(window, cx);
                }
            }));
    }

    /// The chat box lost the keyboard: the next focus it gets must be chosen again.
    pub(super) fn composer_focus_released(&mut self) {
        self.secret_request_inputs.composer_chosen = false;
        self.secret_request_inputs.composer_pressed_at = None;
    }

    /// Whether a card's field has the keyboard: Dictate stays off then (dictation.rs).
    ///
    /// CDXC:Secrets 2026-11-19 WHY:
    /// Dictate typed into the chat box even with the caret in a card's field, so words meant for the card were posted to the chat. Typing them into the card instead still left a plain copy: gxserver keeps every dictation's audio and words in Dictation history, and the words are transcribed by an outside service. So the microphone and its hotkey do nothing while a card's field has the keyboard; the secret is typed or pasted.
    pub(super) fn secret_field_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.secret_fields()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
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
        if open.is_empty() {
            self.secret_request_inputs.composer_chosen = false;
        }
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
