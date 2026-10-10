//! `SliderNumberField`: a slider and a number box in the 17rem control lane
//! (`grid-cols-[minmax(0,1fr)_4.75rem] gap-3`). Dragging or typing shows the value at once and
//! saves after the 180ms debounce; releasing the slider or leaving the box saves it now.
//!
//! The slider is GPUI-Kit's unstyled slider (`gpui_base`) in the shadcn skin: a 4px pill track on
//! `bg-input/90`, the primary range, and a 16px white thumb with a 1px 10% ring, placed with
//! Base UI's `thumbAlignment='edge'` (the thumb never leaves the track).
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::row::{CONTROL_LANE_WIDTH, PageAction, RowSpec, setting_row};
use super::{FieldStates, SettingsPage};
use gpui::Focusable as _;
use gpui::{
    AnyElement, AppContext as _, BoxShadow, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, SharedString, Styled as _, Window, div, point,
    px, relative,
};
use gpui_base::{Slider, SliderIndicator, SliderThumb, SliderTrack};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::slider::{SliderEvent, SliderState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex};
use serde_json::json;

/// `clampNumber`.
pub(crate) fn clamp_number(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn step_decimals(step: f64) -> usize {
    let text = super::super::catalog::js_number_string(step);
    text.split('.').nth(1).map(str::len).unwrap_or(0)
}

/// `snapNumberToStep`: the slider and the box persist the same increments the UI presents.
pub(crate) fn snap_number_to_step(value: f64, min: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    let decimals = step_decimals(step) as i32;
    let scaled = ((value - min) / step).round() * step + min;
    let factor = 10f64.powi(decimals);
    (scaled * factor).round() / factor
}

/// `formatSliderNumber`.
pub(crate) fn format_slider_number(value: f64, step: f64) -> String {
    if step.fract() == 0.0 {
        return format!("{}", value.round() as i64);
    }
    format!("{:.*}", step_decimals(step), value)
}

/// The Custom colour depth sliders move in half steps but show whole depths without a trailing
/// `.0` (96, 95.5); other half-step sliders such as the terminal font size keep it (13.0).
const WHOLE_NUMBERS_WITHOUT_DECIMALS: &[&str] = &[
    "customSidebarTitlebarBackgroundDarknessPercent",
    "customSidebarTitlebarLightBackgroundLightnessPercent",
];

/// `formatSliderNumber` for one field's number box.
fn format_field_number(key: &str, value: f64, step: f64) -> String {
    if WHOLE_NUMBERS_WITHOUT_DECIMALS.contains(&key) && value.fract() == 0.0 {
        return format!("{}", value as i64);
    }
    format_slider_number(value, step)
}

/// The slider and number box of one field.
pub(crate) struct SliderFieldState {
    pub(crate) slider: Entity<SliderState>,
    pub(crate) input: Entity<InputState>,
    pub(crate) thumb_focus: FocusHandle,
    min: f64,
    max: f64,
    step: f64,
    /// The value last shown, to refresh both widgets when the setting changes elsewhere.
    shown: f64,
    /// Saves the value its own way instead of writing `key` (see [`slider_number_field_with`]).
    saver: Option<SliderSaver>,
}

/// A slider's own save: the value, and whether it is committed (released, typed and left, a key)
/// or still being dragged.
pub(crate) type SliderSaver =
    std::rc::Rc<dyn Fn(&Entity<super::super::store::SettingsStore>, f64, bool, &mut gpui::App)>;

/// What a slider field saves: `key` through the store, or a caller's handlers.
#[derive(Clone)]
pub(crate) struct SliderBinding {
    pub(crate) key: &'static str,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) step: f64,
}

fn save_value<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    value: f64,
    commit: bool,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    if let Some(saver) = page
        .field_states()
        .sliders
        .get(key)
        .and_then(|state| state.saver.clone())
    {
        saver(&store, value, commit, cx);
        return;
    }
    let value = if value.fract() == 0.0 {
        json!(value as i64)
    } else {
        json!(value)
    };
    store.update(cx, |store, cx| {
        if commit {
            store.update_setting(key, value, cx);
        } else {
            store.update_setting_debounced(key, value, cx);
        }
    });
}

impl FieldStates {
    /// Creates the widgets of a slider field on first render and keeps them showing `value`.
    pub(crate) fn slider_state<V: SettingsPage>(
        page: &mut V,
        binding: &SliderBinding,
        value: f64,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> (Entity<SliderState>, Entity<InputState>, FocusHandle) {
        let id = SharedString::from(binding.key);
        if !page.field_states().sliders.contains_key(&id) {
            // `SliderState` starts at 0..100 and clamps on every setter, so widen `max` first:
            // a range such as 150..1000 would otherwise pass through min > max and panic.
            let slider = cx.new(|_| {
                SliderState::new()
                    .max(binding.max.max(binding.min) as f32)
                    .min(binding.min as f32)
                    .step(binding.step as f32)
                    .default_value(value as f32)
            });
            let input = cx.new(|cx| {
                InputState::new(window, cx).default_value(format_field_number(
                    binding.key,
                    value,
                    binding.step,
                ))
            });
            let key = binding.key;
            let (min, max, step) = (binding.min, binding.max, binding.step);
            let slider_subscription = cx.subscribe_in(
                &slider,
                window,
                move |page: &mut V, _slider, event: &SliderEvent, window, cx| {
                    let (raw, commit) = match event {
                        SliderEvent::Change(value) => (value.end() as f64, false),
                        SliderEvent::Release(value) => (value.end() as f64, true),
                    };
                    let snapped = clamp_number(snap_number_to_step(raw, min, step), min, max);
                    if let Some(state) = page.field_states().sliders.get_mut(key) {
                        state.shown = snapped;
                        let input = state.input.clone();
                        input.update(cx, |input, cx| {
                            input.set_value(format_field_number(key, snapped, step), window, cx);
                        });
                    }
                    save_value(page, key, snapped, commit, cx);
                },
            );
            let input_subscription = cx.subscribe_in(
                &input,
                window,
                move |page: &mut V, input, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let text = input.read(cx).value().to_string();
                        let Ok(next) = text.trim().parse::<f64>() else {
                            return;
                        };
                        if text.trim().is_empty() || !next.is_finite() || next < min || next > max {
                            return;
                        }
                        let snapped = clamp_number(snap_number_to_step(next, min, step), min, max);
                        if let Some(state) = page.field_states().sliders.get_mut(key) {
                            state.shown = snapped;
                            let slider = state.slider.clone();
                            slider.update(cx, |slider, cx| {
                                slider.set_value(snapped as f32, window, cx)
                            });
                        }
                        save_value(page, key, snapped, false, cx);
                    }
                    InputEvent::Focus => {
                        input.update(cx, |input, cx| {
                            let end = input.value().len();
                            input.set_selected_range(0..end, cx);
                        });
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            return;
                        }
                        let text = input.read(cx).value().to_string();
                        let current = page
                            .field_states()
                            .sliders
                            .get(key)
                            .map(|state| state.shown)
                            .unwrap_or(min);
                        let committed = match text.trim().parse::<f64>() {
                            Ok(next) if next.is_finite() && !text.trim().is_empty() => {
                                clamp_number(snap_number_to_step(next, min, step), min, max)
                            }
                            _ => current,
                        };
                        if let Some(state) = page.field_states().sliders.get_mut(key) {
                            state.shown = committed;
                            let slider = state.slider.clone();
                            slider.update(cx, |slider, cx| {
                                slider.set_value(committed as f32, window, cx)
                            });
                        }
                        input.update(cx, |input, cx| {
                            input.set_value(format_field_number(key, committed, step), window, cx);
                        });
                        save_value(page, key, committed, true, cx);
                    }
                },
            );
            let states = page.field_states();
            states.subscriptions.push(slider_subscription);
            states.subscriptions.push(input_subscription);
            states.sliders.insert(
                id.clone(),
                SliderFieldState {
                    slider,
                    input,
                    thumb_focus: cx.focus_handle().tab_stop(true),
                    min: binding.min,
                    max: binding.max,
                    step: binding.step,
                    shown: value,
                    saver: None,
                },
            );
        }
        let state = page
            .field_states()
            .sliders
            .get_mut(&id)
            .expect("slider state");
        let (slider, input, focus) = (
            state.slider.clone(),
            state.input.clone(),
            state.thumb_focus.clone(),
        );
        if (state.shown - value).abs() > f64::EPSILON {
            state.shown = value;
            let step = state.step;
            slider.update(cx, |slider, cx| slider.set_value(value as f32, window, cx));
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            if !focused {
                input.update(cx, |input, cx| {
                    input.set_value(format_field_number(&id, value, step), window, cx);
                });
            }
        }
        (slider, input, focus)
    }
}

/// The skinned slider: 184px of track in the standard lane.
pub(crate) fn settings_slider<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    state: &Entity<SliderState>,
    focus: &FocusHandle,
    binding: &SliderBinding,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let percentage = state.read(cx).percentage().end;
    let focused = focus.is_focused(window);
    let ring = hsla(css_fade(p.ring, 0.3));
    let thumb_shadow = |ring_width: f32| {
        let mut shadows = vec![
            BoxShadow {
                color: hsla(modal_rgba(0x000000, 0.1)),
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(6.0),
                spread_radius: px(-1.0),
                inset: false,
            },
            BoxShadow {
                color: hsla(modal_rgba(0x000000, 0.1)),
                offset: point(px(0.0), px(2.0)),
                blur_radius: px(4.0),
                spread_radius: px(-2.0),
                inset: false,
            },
            BoxShadow {
                color: hsla(modal_rgba(0x000000, 0.1)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(1.0),
                inset: false,
            },
        ];
        if ring_width > 0.0 {
            shadows.push(BoxShadow {
                color: ring,
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(ring_width),
                inset: false,
            });
        }
        shadows
    };
    let track = hsla(p.slider_track());
    let range = hsla(p.primary);
    let key = binding.key;
    let (min, max, step) = (binding.min, binding.max, binding.step);
    let focus_for_click = focus.clone();
    let slider = Slider::new(state).flex_1().min_w_0().h(px(16.0)).child(
        SliderTrack::new(state)
            .relative()
            .w_full()
            .h(px(16.0))
            .cursor_pointer()
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                focus_for_click.focus(window, cx);
            })
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(6.0))
                    .h(px(4.0))
                    .rounded_full()
                    .bg(track)
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(px(8.0))
                            .bg(range),
                    ),
            )
            .child(
                SliderIndicator::new(state)
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(8.0))
                    .right(px(8.0))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(6.0))
                            .h(px(4.0))
                            .w(relative(percentage))
                            .bg(range),
                    )
                    .child(
                        SliderThumb::new(state)
                            .absolute()
                            .top_0()
                            .left(relative(percentage))
                            .ml(px(-8.0))
                            .size(px(16.0))
                            .rounded_full()
                            .bg(gpui::white())
                            .shadow(thumb_shadow(if focused { 4.0 } else { 0.0 }))
                            .hover(move |this| this.shadow(thumb_shadow(4.0))),
                    ),
            ),
    );
    div()
        .id(SharedString::from(format!("{id}-slider")))
        .track_focus(focus)
        .flex_1()
        .min_w_0()
        .on_key_down(
            cx.listener(move |page: &mut V, event: &KeyDownEvent, window, cx| {
                let Some(state) = page.field_states().sliders.get(key) else {
                    return;
                };
                let current = state.shown;
                let next = match event.keystroke.key.as_str() {
                    "left" | "down" => current - step,
                    "right" | "up" => current + step,
                    "pageup" => current + step * 10.0,
                    "pagedown" => current - step * 10.0,
                    "home" => min,
                    "end" => max,
                    _ => return,
                };
                cx.stop_propagation();
                let next = clamp_number(snap_number_to_step(next, min, step), min, max);
                let slider = state.slider.clone();
                slider.update(cx, |slider, cx| slider.set_value(next as f32, window, cx));
                save_value(page, key, next, true, cx);
            }),
        )
        .child(slider)
        .into_any_element()
}

/// The Settings number box (`SettingsInput` at `h-8 px-3 tabular-nums`).
pub(crate) fn settings_number_input(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    width: f32,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .flex_shrink_0()
        .w(px(width))
        .h(px(32.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.input_background()))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
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
        .into_any_element()
}

/// `SliderNumberField`.
pub(crate) fn slider_number_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    binding: SliderBinding,
    value: f64,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let (slider_state, input_state, focus) =
        FieldStates::slider_state(page, &binding, value, window, cx);
    let slider = settings_slider(p, binding.key, &slider_state, &focus, &binding, window, cx);
    let input = settings_number_input(p, &input_state, 76.0, window, cx);
    let control = h_flex()
        .w(px(CONTROL_LANE_WIDTH))
        .max_w_full()
        .items_center()
        .gap(px(12.0))
        .child(slider)
        .child(input)
        .into_any_element();
    setting_row(p, binding.key, spec, on_reset, control, cx)
}

/// `SliderNumberField` whose value is saved by `saver` rather than written to `binding.key` (the
/// Theme page's Strength slider sets four tints at once). `binding.key` still names the field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn slider_number_field_with<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    binding: SliderBinding,
    value: f64,
    saver: SliderSaver,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    FieldStates::slider_state(page, &binding, value, window, cx);
    if let Some(state) = page.field_states().sliders.get_mut(binding.key) {
        state.saver = Some(saver);
    }
    slider_number_field(page, p, spec, on_reset, binding, value, window, cx)
}

/// A bare slider (no number box) whose value is saved by `saver`: the Theme page's five-step
/// Colourfulness slider. `width` is the track's width.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step_slider<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    binding: SliderBinding,
    value: f64,
    saver: SliderSaver,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let (slider_state, _input, focus) =
        FieldStates::slider_state(page, &binding, value, window, cx);
    if let Some(state) = page.field_states().sliders.get_mut(binding.key) {
        state.saver = Some(saver);
    }
    settings_slider(p, binding.key, &slider_state, &focus, &binding, window, cx)
}
