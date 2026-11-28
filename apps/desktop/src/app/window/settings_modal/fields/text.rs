//! `TextField` and `SettingsInput`: a 32px input on the raised tone at 82% that saves on every
//! change and on blur, keeps its own buffer while focused (settings echoes never repaint it), and
//! may carry a Browse button.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_icon_button};
use super::row::{CONTROL_LANE_WIDTH, PageAction, RowSpec, setting_row};
use super::{FieldStates, SettingsPage, icon};
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex};

pub(crate) struct TextFieldState {
    pub(crate) input: Entity<InputState>,
    /// The saved value last shown, to refresh the buffer when the setting changes elsewhere.
    shown: String,
}

/// Turns typed text into the value a setting stores (`normalizeghostexSettings` for that key).
pub(crate) type TextNormalizer = fn(&str) -> String;

impl FieldStates {
    /// The input of a text field, created on first render with its change handler; the buffer
    /// follows `value` whenever the field is not focused.
    ///
    /// CDXC:Release 2026-10-22 WHY: the body lives in `text_state_shared`, generic over the page only; generic over the change handler too it was copied into the release binary once per call site (22 copies, about 0.1 MB).
    pub(crate) fn text_state<V: SettingsPage>(
        page: &mut V,
        id: &SharedString,
        value: &str,
        placeholder: Option<&str>,
        on_change: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + 'static,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<InputState> {
        Self::text_state_shared(
            page,
            id,
            value,
            placeholder,
            Box::new(on_change),
            window,
            cx,
        )
    }

    fn text_state_shared<V: SettingsPage>(
        page: &mut V,
        id: &SharedString,
        value: &str,
        placeholder: Option<&str>,
        on_change: Box<dyn Fn(&mut V, String, &mut Window, &mut Context<V>)>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<InputState> {
        if !page.field_states().texts.contains_key(id) {
            let placeholder = placeholder.map(str::to_string);
            let input = cx.new(|cx| {
                let input = InputState::new(window, cx).default_value(value.to_string());
                match placeholder {
                    Some(placeholder) => input.placeholder(placeholder),
                    None => input,
                }
            });
            let subscription = cx.subscribe_in(
                &input,
                window,
                move |page: &mut V, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change | InputEvent::Blur) {
                        let text = input.read(cx).value().to_string();
                        on_change(page, text, window, cx);
                    }
                },
            );
            let states = page.field_states();
            states.subscriptions.push(subscription);
            states.texts.insert(
                id.clone(),
                TextFieldState {
                    input,
                    shown: value.to_string(),
                },
            );
        }
        let state = page.field_states().texts.get_mut(id).expect("text state");
        let input = state.input.clone();
        if state.shown != value {
            state.shown = value.to_string();
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            if !focused && input.read(cx).value().as_ref() != value {
                let value = value.to_string();
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        input
    }
}

/// The Settings input skin (`.ghostex-settings-shadcn [data-slot='input']`).
pub(crate) fn settings_text_input(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    width: Option<f32>,
    monospace: bool,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    settings_text_input_with_disabled(p, state, width, monospace, false, window, cx)
}

/// The same input skin with interaction disabled; existing callers remain editable.
pub(crate) fn settings_text_input_with_disabled(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    width: Option<f32>,
    monospace: bool,
    disabled: bool,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    settings_text_input_with_trailing(p, state, width, monospace, disabled, None, window, cx)
}

/// The same input skin with `trailing` (a small mark such as a warning dot) inside the field at its
/// right end, vertically centred, after the text so the text never runs under it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_text_input_with_trailing(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    width: Option<f32>,
    monospace: bool,
    disabled: bool,
    trailing: Option<AnyElement>,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    let focused = !disabled && state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .when_some(width, |this, width| this.w(px(width)).flex_shrink_0())
        .when(width.is_none(), |this| this.flex_1().w_full())
        .min_w_0()
        .max_w_full()
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.input_background()))
        .when(monospace, |this| this.font_family(MODAL_MONO_FONT))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .disabled(disabled)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(14.0))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .when_some(trailing, |this, trailing| {
            this.gap(px(8.0))
                .child(div().flex_shrink_0().child(trailing))
        })
        .into_any_element()
}

/// `TextField`. `key` saves `normalize(text)` on every change; `browse` adds the folder button
/// (`onBrowse` with its `browseLabel` tooltip).
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &str,
    placeholder: Option<&str>,
    normalize: Option<TextNormalizer>,
    browse: Option<(SharedString, PageAction<V>)>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let id = SharedString::from(key);
    let input = FieldStates::text_state(
        page,
        &id,
        value,
        placeholder,
        move |page: &mut V, text, _window, cx| {
            let text = normalize.map(|normalize| normalize(&text)).unwrap_or(text);
            let store = page.settings_store().clone();
            store.update(cx, |store, cx| {
                if store.string(key) != text {
                    store.update_setting(key, serde_json::Value::String(text), cx);
                }
            });
        },
        window,
        cx,
    );
    let control = match browse {
        Some((label, on_browse)) => h_flex()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .items_center()
            .gap(px(8.0))
            .child(settings_text_input(p, &input, None, false, window, cx))
            .child(settings_icon_button(
                p,
                SharedString::from(format!("{key}-browse")),
                icon::FOLDER_OPEN,
                16.0,
                32.0,
                ButtonVariant::Outline,
                Some(label),
                false,
                move |page: &mut V, window, cx| on_browse(page, window, cx),
                cx,
            ))
            .into_any_element(),
        None => div()
            .w(px(CONTROL_LANE_WIDTH))
            .max_w_full()
            .flex()
            .child(settings_text_input(p, &input, None, false, window, cx))
            .into_any_element(),
    };
    setting_row(p, key, spec, on_reset, control, cx)
}
