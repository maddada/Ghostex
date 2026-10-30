//! The card above the composer while a model change the agent refused for good holds messages:
//! how many, why, what releases them, and Pick model, which opens the composer's model menu the
//! way its model pill does. What it says and when it shows come from the chat core
//! (packages/gx-chat-core/src/menus/picker/hold_card.rs); this file only lays it out.

use super::tone_card::{ToneButton, ToneCardParts, tone_look};
use super::{appearance::ChatAppearance, state::NativeChatView, transcript::text};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div,
};

impl NativeChatView {
    pub(super) fn render_model_hold_card(
        &mut self,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = self.snapshot["modelHoldCard"].clone();
        if !card.is_object() {
            return None;
        }
        let title = text(&card, "title");
        let pick = self
            .tone_button(
                "model-hold-pick-model",
                &text(&card["action"], "label"),
                None,
                ToneButton::Primary,
                None,
                false,
                p,
            )
            .on_click(cx.listener(|this, _, window, cx| this.toggle_model_menu(None, window, cx)))
            .into_any_element();
        Some(
            div()
                .id("model-hold-card")
                .role(gpui::Role::Alert)
                .aria_label(title.clone())
                .w_full()
                .child(self.tone_card(
                    ToneCardParts {
                        look: tone_look(&text(&card, "tone"), p),
                        title,
                        detail: text(&card, "detail"),
                        trailing: Vec::new(),
                        body: Vec::new(),
                        footer_start: Vec::new(),
                        // The way out takes the room beside Pick model, as in the mockup.
                        footer_note: Some(text(&card, "note")),
                        footer_end: vec![pick],
                    },
                    None,
                    None,
                    p,
                ))
                .into_any_element(),
        )
    }
}
