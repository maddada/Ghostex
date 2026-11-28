//! The Name and Command inputs of an expanded row. Unlike Settings text fields, which save every
//! keystroke, these save once, on Enter or when focus leaves, because each save rewrites the
//! agent roster in every project.
use super::super::super::fields::settings_text_input_with_trailing;
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use gpui::Focusable as _;
use gpui::{AnyElement, AppContext as _, Context, SharedString, Window};
use gpui_component::input::{InputEvent, InputState};

impl AgentsTab {
    /// The input `id` showing `value`, calling `on_commit` with the trimmed text when it changed.
    /// The buffer follows `value` whenever the input is not focused.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn inline_input(
        &mut self,
        p: &SettingsPalette,
        id: SharedString,
        value: &str,
        placeholder: &str,
        width: f32,
        monospace: bool,
        on_commit: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.inline_input_with_trailing(
            p,
            id,
            value,
            placeholder,
            width,
            monospace,
            on_commit,
            None::<fn(&str) -> Option<AnyElement>>,
            window,
            cx,
        )
    }

    /// [`Self::inline_input`] with `trailing`, which maps the text in the box (typed or saved) to a
    /// mark drawn inside the field at its right end. The panel redraws as the user types, so the
    /// mark follows the text before it is saved.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn inline_input_with_trailing(
        &mut self,
        p: &SettingsPalette,
        id: SharedString,
        value: &str,
        placeholder: &str,
        width: f32,
        monospace: bool,
        on_commit: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
        trailing: Option<impl FnOnce(&str) -> Option<AnyElement>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // A value saved elsewhere replaces an unfocused buffer: rebuild the input so its commit
        // compares against the new value.
        if self.inline_committed.get(&id).map(String::as_str) == Some(value) {
            self.inline_committed.remove(&id);
        }
        if let Some(input) = self.inline_inputs.get(&id)
            && !input.read(cx).focus_handle(cx).is_focused(window)
            && input.read(cx).value().as_ref() != value
            && !self.inline_committed.contains_key(&id)
        {
            self.inline_inputs.remove(&id);
        }
        let input = match self.inline_inputs.get(&id) {
            Some(input) => input.clone(),
            None => {
                let placeholder = placeholder.to_string();
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value.to_string())
                });
                let shown = value.to_string();
                let input_id = id.clone();
                let subscription = cx.subscribe_in(
                    &input,
                    window,
                    move |page: &mut Self, input, event: &InputEvent, _window, cx| {
                        if matches!(event, InputEvent::Change) {
                            cx.notify();
                            return;
                        }
                        if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                            return;
                        }
                        let text = input.read(cx).value().trim().to_string();
                        if !text.is_empty() && text != shown {
                            // Keep the typed text on screen until the saved value comes back.
                            page.inline_committed.insert(input_id.clone(), text.clone());
                            on_commit(page, text, cx);
                        }
                    },
                );
                self.fields.subscriptions.push(subscription);
                self.inline_inputs.insert(id.clone(), input.clone());
                input
            }
        };
        let mark = trailing.and_then(|trailing| trailing(input.read(cx).value().as_ref()));
        settings_text_input_with_trailing(
            p,
            &input,
            Some(width),
            monospace,
            false,
            mark,
            window,
            cx,
        )
    }
}
