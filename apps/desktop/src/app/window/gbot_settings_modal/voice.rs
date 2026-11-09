//! Profile's Voice row: the voice a GBot answers calls in, one of the Voice agent provider's
//! voices or the Voice agent's own. Saved with the rest of the Profile on Save (`voiceSet`).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (M14 brief): "Per-GBot voice choice in GBot Settings". The row lists the voices of the provider calls use now (Settings › Voice Agent) with the Voice agent's own voice first; a voice saved for the other provider shows as itself and calls fall back to the Voice agent's voice until that provider is picked again (server/src/voice_agent/gbot_call/mod.rs `voice_for`).
use super::super::native_modal_kit::*;
use super::parts::wide_row;
use super::window::GpuiGbotSettingsModalWindow;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
};
use gpui_component::h_flex;

impl GpuiGbotSettingsModalWindow {
    /// The picker's rows: the Voice agent's own voice, then the provider's voices, then the saved
    /// voice when the provider doesn't have it.
    pub(super) fn voice_labels(&self) -> Vec<String> {
        let choices = &self.original.voice_choices;
        let own = if choices.default_voice.is_empty() {
            "The Voice agent's voice".to_string()
        } else {
            format!(
                "The Voice agent's voice ({})",
                title_case(&choices.default_voice)
            )
        };
        let mut labels = vec![own];
        labels.extend(
            self.voice_ids()
                .into_iter()
                .skip(1)
                .map(|voice| title_case(&voice)),
        );
        labels
    }

    /// The voice each row saves; the first is "" (the Voice agent's own).
    fn voice_ids(&self) -> Vec<String> {
        let choices = &self.original.voice_choices;
        let mut ids = vec![String::new()];
        ids.extend(choices.voices.iter().cloned());
        if !self.original.voice.is_empty() && !choices.voices.contains(&self.original.voice) {
            ids.push(self.original.voice.clone());
        }
        ids
    }

    pub(super) fn voice_index(&self) -> Option<usize> {
        self.voice_ids()
            .iter()
            .position(|voice| *voice == self.draft.voice)
    }

    pub(crate) fn toggle_voice_select(&mut self, cx: &mut Context<Self>) {
        let selected = self.voice_index();
        self.agent_select.close();
        self.voice_select.toggle(selected);
        cx.notify();
    }

    pub(super) fn choose_voice(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(voice) = self.voice_ids().get(index) {
            self.draft.voice = voice.clone();
        }
        self.voice_select.close();
        cx.notify();
    }

    /// The Voice row of Profile's "Runs on" section.
    pub(super) fn render_voice_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let labels = self.voice_labels();
        let value = self
            .voice_index()
            .and_then(|index| labels.get(index).cloned());
        let trigger = modal_select_trigger(
            &self.kit,
            &self.voice_select,
            "gbot-settings-voice",
            value,
            "Pick a voice",
            self.saving || self.original.voice_choices.voices.is_empty(),
            |this: &mut Self, _window, cx| this.toggle_voice_select(cx),
            cx,
        );
        let choices = &self.original.voice_choices;
        let mut note = "The voice it answers in when you call it.".to_string();
        if !choices.provider_label.is_empty() {
            note.push_str(&format!(
                " These are {}'s voices, the Voice agent's provider in Settings › Voice Agent.",
                choices.provider_label
            ));
        }
        if !self.draft.voice.is_empty() && !choices.voices.contains(&self.draft.voice) {
            note.push_str(&format!(
                " {} isn't one of them, so calls use the Voice agent's voice for now.",
                title_case(&self.draft.voice)
            ));
        }
        wide_row(
            &p,
            "Voice",
            h_flex()
                .w_full()
                .on_children_prepainted(capture_child_bounds(
                    self.voice_select.trigger_bounds.clone(),
                    0,
                ))
                .child(trigger)
                .into_any_element(),
            Some(SharedString::from(note)),
        )
    }

    /// The open voice list, drawn over the window.
    pub(super) fn render_voice_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let labels = self.voice_labels();
        modal_select_menu(
            &self.kit,
            &self.voice_select,
            "gbot-settings-voice-menu",
            &labels,
            self.voice_index(),
            |this: &mut Self, index, _window, cx| this.choose_voice(index, cx),
            |this: &mut Self, _window, cx| {
                this.voice_select.close();
                cx.notify();
            },
            window,
            cx,
        )
    }
}

/// "eve" → "Eve".
fn title_case(voice: &str) -> String {
    let mut chars = voice.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
