//! `ColorField`, `WebColorPickerField` and the nested Pick Color dialog (the
//! `react-best-gradient-color-picker` the React field lazy-loads, with every control but the
//! saturation square, the hue bar and the HEX/R/G/B inputs hidden).
//!
//! The React `ColorField` swatch is an `input[type=color]`, which opens the operating system's
//! colour panel; here it opens the same Pick Color dialog the tint rows use.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::controls::{ButtonVariant, settings_button};
use super::row::{CONTROL_LANE_WIDTH, PageAction, RowSpec, setting_row, tooltip_text};
use super::text::{settings_text_input, settings_text_input_with_disabled};
use super::{FieldStates, SettingsPage, icon};
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, App, AppContext as _, Bounds, ClickEvent, Context,
    DragMoveEvent, Empty, Entity, FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, ParentElement as _, Pixels, Point, Render, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div,
    linear_color_stop, linear_gradient, point, px, rgb,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use serde_json::json;
use std::cell::Cell;
use std::rc::Rc;

/// `DEFAULT_CUSTOM_SIDEBAR_TITLEBAR_BACKGROUND_TINT_COLOR`, the tint rows' fallback.
pub(crate) const DEFAULT_TINT_COLOR: &str = "#808080";

/// `SIDEBAR_TITLEBAR_TINT_SWATCHES`.
pub(crate) const TINT_SWATCHES: &[(&str, &str)] = &[
    ("White", "#ffffff"),
    ("Neutral Gray", "#808080"),
    ("Black", "#000000"),
    ("Steel", "#4f6672"),
    ("Red", "#884444"),
    ("Orange", "#8a5330"),
    ("Amber", "#8a6a2f"),
    ("Olive", "#657a3f"),
    ("Green", "#3f7a5f"),
    ("Teal", "#2f7d66"),
    ("Cyan", "#287c7f"),
    ("Blue", "#336699"),
    ("Indigo", "#4f5f96"),
    ("Violet", "#6c4f8f"),
    ("Pink", "#854f7a"),
    ("Rose", "#8a4f5f"),
];

/// The square and the hue bar are 294px wide, as the React picker is given.
const PICKER_SIZE: f32 = 294.0;
const HUE_BAR_HEIGHT: f32 = 14.0;
const HANDLE_SIZE: f32 = 18.0;
/// `w-[22rem]`.
const DIALOG_WIDTH: f32 = 352.0;

fn is_hex_color(value: &str) -> bool {
    let value = value.trim();
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

/// `normalizeColorInputValue`.
pub(crate) fn normalize_color_input_value(value: &str, fallback: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if is_hex_color(&normalized) {
        normalized
    } else {
        fallback.to_string()
    }
}

/// `normalizePickerColorValue`: a hex, or an `rgb()`/`rgba()` string turned into a hex.
pub(crate) fn normalize_picker_color_value(value: &str, fallback: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if is_hex_color(&normalized) {
        return normalized;
    }
    let inner = normalized
        .strip_prefix("rgba(")
        .or_else(|| normalized.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'));
    let Some(inner) = inner else {
        return fallback.to_string();
    };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 || parts.len() > 4 {
        return fallback.to_string();
    }
    let channel = |text: &str| text.parse::<u32>().ok().filter(|value| *value <= 999);
    match (channel(parts[0]), channel(parts[1]), channel(parts[2])) {
        (Some(red), Some(green), Some(blue)) => {
            rgb_to_hex(red.min(255), green.min(255), blue.min(255))
        }
        _ => fallback.to_string(),
    }
}

/// `rgbToHexColor`.
pub(crate) fn rgb_to_hex(red: u32, green: u32, blue: u32) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

pub(crate) fn hex_to_rgb(hex: &str) -> Option<(u32, u32, u32)> {
    if !is_hex_color(hex) {
        return None;
    }
    let value = u32::from_str_radix(&hex.trim()[1..], 16).ok()?;
    Some(((value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff))
}

pub(crate) fn hex_rgba(hex: &str) -> Rgba {
    hex_to_rgb(hex)
        .map(|(red, green, blue)| rgb((red << 16) | (green << 8) | blue))
        .unwrap_or(rgb(0x000000))
}

fn rgb_to_hsv(red: u32, green: u32, blue: u32) -> (f32, f32, f32) {
    let (r, g, b) = (
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (u32, u32, u32) {
    let c = value * saturation;
    let h = (hue.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    let channel = |value: f32| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (channel(r), channel(g), channel(b))
}

/// The drag marker of the square and the hue bar.
#[derive(Clone)]
struct PickerDrag {
    field: SharedString,
    hue_bar: bool,
}

struct PickerDragView;

impl Render for PickerDragView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// The Pick Color dialog of one field.
pub(crate) struct PickColorState {
    pub(crate) open: bool,
    hue: f32,
    saturation: f32,
    value: f32,
    hex_input: Entity<InputState>,
    channel_inputs: [Entity<InputState>; 3],
    focus: FocusHandle,
    square_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    hue_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// A colour field's widgets: the hex text box on the row and its Pick Color dialog.
pub(crate) struct ColorFieldState {
    pub(crate) text_input: Entity<InputState>,
    pub(crate) picker: PickColorState,
    /// The saved value last shown.
    shown: String,
    /// `colorText`: what the row's hex box holds while typing.
    color_text: String,
}

/// What a colour field saves while the picker moves and when it closes.
#[derive(Clone, Copy)]
pub(crate) enum ColorFieldKind {
    /// `ColorField`: every change saves now, the text box holds any text.
    Plain,
    /// `WebColorPickerField`: picker moves save after the debounce, a commit saves now, the box
    /// only saves a complete `#rrggbb`.
    Tint,
}

fn save_color<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    value: &str,
    commit: bool,
    cx: &mut Context<V>,
) {
    let store = page.settings_store().clone();
    let value = json!(value);
    store.update(cx, |store, cx| {
        // A picker or text event can arrive after the shader switch changed.
        if cfg!(target_os = "macos")
            && key == "workspaceBackgroundColor"
            && store.bool("terminalShadersEnabled")
        {
            return;
        }
        if commit {
            store.update_setting(key, value, cx);
        } else {
            store.update_setting_debounced(key, value, cx);
        }
    });
}

impl FieldStates {
    pub(crate) fn color_state<V: SettingsPage>(
        page: &mut V,
        key: &'static str,
        value: &str,
        kind: ColorFieldKind,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> SharedString {
        let id = SharedString::from(key);
        if !page.field_states().colors.contains_key(&id) {
            let text_input = cx.new(|cx| {
                let input = InputState::new(window, cx).default_value(value.to_string());
                match kind {
                    ColorFieldKind::Tint => input.placeholder(DEFAULT_TINT_COLOR),
                    ColorFieldKind::Plain => input,
                }
            });
            let text_subscription = cx.subscribe_in(
                &text_input,
                window,
                move |page: &mut V, input, event: &InputEvent, window, cx| {
                    let text = input.read(cx).value().to_string();
                    match (kind, event) {
                        (ColorFieldKind::Plain, InputEvent::Change) => {
                            if let Some(state) = page.field_states().colors.get_mut(key) {
                                state.color_text = text.clone();
                            }
                            save_color(page, key, &text, true, cx);
                        }
                        (ColorFieldKind::Tint, InputEvent::Change) => {
                            if let Some(state) = page.field_states().colors.get_mut(key) {
                                state.color_text = text.clone();
                            }
                            if is_hex_color(text.trim()) {
                                save_color(page, key, &text.trim().to_lowercase(), false, cx);
                            }
                        }
                        (ColorFieldKind::Tint, InputEvent::Blur) => {
                            let saved = page
                                .field_states()
                                .colors
                                .get(key)
                                .map(|state| state.shown.clone())
                                .unwrap_or_default();
                            let fallback = normalize_color_input_value(&saved, DEFAULT_TINT_COLOR);
                            let committed = normalize_picker_color_value(&text, &fallback);
                            input.update(cx, |input, cx| {
                                input.set_value(committed.clone(), window, cx)
                            });
                            save_color(page, key, &committed, true, cx);
                        }
                        _ => {}
                    }
                },
            );
            let hex_input = cx.new(|cx| InputState::new(window, cx));
            let channel_inputs = [
                cx.new(|cx| InputState::new(window, cx)),
                cx.new(|cx| InputState::new(window, cx)),
                cx.new(|cx| InputState::new(window, cx)),
            ];
            let mut subscriptions = vec![text_subscription];
            subscriptions.push(cx.subscribe_in(
                &hex_input,
                window,
                move |page: &mut V, input, event: &InputEvent, window, cx| {
                    if !matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let text = input.read(cx).value().to_string();
                    let hex = format!("#{}", text.trim().trim_start_matches('#'));
                    if is_hex_color(&hex) {
                        picker_preview(page, key, kind, &hex.to_lowercase(), false, window, cx);
                    }
                },
            ));
            for (channel, input) in channel_inputs.iter().enumerate() {
                subscriptions.push(cx.subscribe_in(
                    input,
                    window,
                    move |page: &mut V, input, event: &InputEvent, window, cx| {
                        if !matches!(event, InputEvent::Change) {
                            return;
                        }
                        let Ok(amount) = input.read(cx).value().trim().parse::<u32>() else {
                            return;
                        };
                        let Some(state) = page.field_states().colors.get(key) else {
                            return;
                        };
                        let (red, green, blue) = hsv_to_rgb(
                            state.picker.hue,
                            state.picker.saturation,
                            state.picker.value,
                        );
                        let mut channels = [red, green, blue];
                        channels[channel] = amount.min(255);
                        let hex = rgb_to_hex(channels[0], channels[1], channels[2]);
                        picker_preview(page, key, kind, &hex, true, window, cx);
                    },
                ));
            }
            let states = page.field_states();
            states.subscriptions.extend(subscriptions);
            states.colors.insert(
                id.clone(),
                ColorFieldState {
                    text_input,
                    picker: PickColorState {
                        open: false,
                        hue: 0.0,
                        saturation: 0.0,
                        value: 0.0,
                        hex_input,
                        channel_inputs,
                        focus: cx.focus_handle(),
                        square_bounds: Rc::new(Cell::new(None)),
                        hue_bounds: Rc::new(Cell::new(None)),
                    },
                    shown: value.to_string(),
                    color_text: value.to_string(),
                },
            );
        }
        let state = page
            .field_states()
            .colors
            .get_mut(&id)
            .expect("color state");
        if state.shown != value {
            state.shown = value.to_string();
            state.color_text = value.to_string();
            let input = state.text_input.clone();
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            if !focused {
                let value = value.to_string();
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        id
    }
}

/// Moves the picker to `hex` and saves it (`previewColor`). `from_channels` keeps the R/G/B box
/// being typed in untouched.
fn picker_preview<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    kind: ColorFieldKind,
    hex: &str,
    from_channels: bool,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some((red, green, blue)) = hex_to_rgb(hex) else {
        return;
    };
    if let Some(state) = page.field_states().colors.get_mut(key) {
        let (hue, saturation, value) = rgb_to_hsv(red, green, blue);
        // A grey has no hue; keep the bar where it was.
        if saturation > 0.0 && value > 0.0 {
            state.picker.hue = hue;
        }
        state.picker.saturation = saturation;
        state.picker.value = value;
        state.color_text = hex.to_string();
        sync_picker_inputs(state, !from_channels, window, cx);
        let text_input = state.text_input.clone();
        let hex_text = hex.to_string();
        text_input.update(cx, |input, cx| input.set_value(hex_text, window, cx));
    }
    match kind {
        ColorFieldKind::Plain => save_color(page, key, hex, true, cx),
        ColorFieldKind::Tint => save_color(page, key, hex, false, cx),
    }
}

fn sync_picker_inputs(
    state: &mut ColorFieldState,
    channels: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let (red, green, blue) = hsv_to_rgb(
        state.picker.hue,
        state.picker.saturation,
        state.picker.value,
    );
    let hex = rgb_to_hex(red, green, blue);
    let hex_input = state.picker.hex_input.clone();
    if !hex_input.read(cx).focus_handle(cx).is_focused(window) {
        let text = hex.trim_start_matches('#').to_uppercase();
        hex_input.update(cx, |input, cx| input.set_value(text, window, cx));
    }
    if channels {
        for (input, amount) in state
            .picker
            .channel_inputs
            .clone()
            .iter()
            .zip([red, green, blue])
        {
            input.update(cx, |input, cx| {
                input.set_value(amount.to_string(), window, cx)
            });
        }
    }
}

pub(crate) fn open_picker<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    fallback: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(state) = page.field_states().colors.get_mut(key) else {
        return;
    };
    let current = normalize_picker_color_value(&state.color_text, fallback);
    let (red, green, blue) = hex_to_rgb(&current).unwrap_or((0x80, 0x80, 0x80));
    let (hue, saturation, value) = rgb_to_hsv(red, green, blue);
    state.picker.hue = hue;
    state.picker.saturation = saturation;
    state.picker.value = value;
    state.picker.open = true;
    sync_picker_inputs(state, true, window, cx);
    state.picker.focus.focus(window, cx);
    cx.notify();
}

/// `commitColorAfterClosingPicker`: closes the dialog, then saves the colour now.
fn close_picker<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    kind: ColorFieldKind,
    fallback: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(state) = page.field_states().colors.get_mut(key) else {
        return;
    };
    state.picker.open = false;
    let committed = match kind {
        ColorFieldKind::Tint => normalize_picker_color_value(&state.color_text, fallback),
        ColorFieldKind::Plain => {
            let (red, green, blue) = hsv_to_rgb(
                state.picker.hue,
                state.picker.saturation,
                state.picker.value,
            );
            rgb_to_hex(red, green, blue)
        }
    };
    let text_input = state.text_input.clone();
    let text = committed.clone();
    text_input.update(cx, |input, cx| input.set_value(text, window, cx));
    save_color(page, key, &committed, true, cx);
    cx.notify();
}

/// One of react-best-gradient-color-picker's inputs: a 32px `#363636` box with the value centered
/// in 15px white, and its 11px bold label (`#d4d4d4`) right under it.
fn picker_input(state: &Entity<InputState>, width: f32, label: &'static str) -> AnyElement {
    v_flex()
        .w(px(width))
        .flex_shrink_0()
        .items_center()
        .child(
            div()
                .w_full()
                .h(px(32.0))
                .px(px(2.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .bg(gpui::rgb(0x363636))
                .child(
                    Input::new(state)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .w_full()
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(15.0))
                        .text_align(gpui::TextAlign::Center)
                        .text_color(gpui::white()),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(13.0))
                .text_size(px(11.0))
                .line_height(px(13.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(gpui::rgb(0xd4d4d4))
                .text_center()
                .child(label),
        )
        .into_any_element()
}

fn picker_handle(fill: Option<Rgba>) -> gpui::Div {
    div()
        .absolute()
        .size(px(HANDLE_SIZE))
        .rounded_full()
        .border_2()
        .border_color(gpui::white())
        .shadow(vec![gpui::BoxShadow {
            color: gpui::hsla(0.0, 0.0, 0.0, 0.5),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(3.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .when_some(fill, |this, fill| this.bg(hsla(fill)))
}

/// Where the Pick Color dialog opens.
#[derive(Clone, Copy)]
enum PickerPlacement {
    /// Centered in the window, over the row that opened it.
    WindowCenter,
    /// Down and to the left of the square that opened it (the Theme page's "+" square, at the end
    /// of its row), so the dialog never covers the square; the dialog is a child of that square.
    BesideTrigger,
}

/// The dialog: title, the 294px saturation square and hue bar, HEX/R/G/B, and Done.
#[allow(clippy::too_many_arguments)]
fn pick_color_dialog<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    kind: ColorFieldKind,
    fallback: &'static str,
    placement: PickerPlacement,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let state = page.field_states().colors.get(key)?;
    if !state.picker.open {
        return None;
    }
    let (hue, saturation, value) = (
        state.picker.hue,
        state.picker.saturation,
        state.picker.value,
    );
    let hex_input = state.picker.hex_input.clone();
    let channels = state.picker.channel_inputs.clone();
    let focus = state.picker.focus.clone();
    let square_bounds = state.picker.square_bounds.clone();
    let hue_bounds = state.picker.hue_bounds.clone();
    let (hue_red, hue_green, hue_blue) = hsv_to_rgb(hue, 1.0, 1.0);
    let pure_hue = rgb((hue_red << 16) | (hue_green << 8) | hue_blue);
    let field: SharedString = key.into();
    let drag_field = field.clone();
    let square = div()
        .id(SharedString::from(format!("{key}-picker-square")))
        .relative()
        .size(px(PICKER_SIZE))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |page: &mut V, event: &MouseDownEvent, window, cx| {
                square_pick(page, key, kind, event.position, None, window, cx);
            }),
        )
        .on_drag(
            PickerDrag {
                field: drag_field.clone(),
                hue_bar: false,
            },
            |_, _, _, cx| cx.new(|_| PickerDragView),
        )
        .on_drag_move(cx.listener(
            move |page: &mut V, event: &DragMoveEvent<PickerDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.field.as_ref() != key || drag.hue_bar {
                    return;
                }
                square_pick(
                    page,
                    key,
                    kind,
                    event.event.position,
                    Some(event.bounds),
                    window,
                    cx,
                );
            },
        ))
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(gpui::white(), 0.0),
                    linear_color_stop(hsla(pure_hue), 1.0),
                ))
                .child(div().absolute().inset_0().bg(linear_gradient(
                    180.0,
                    linear_color_stop(gpui::transparent_black(), 0.0),
                    linear_color_stop(gpui::black(), 1.0),
                ))),
        )
        .child(
            picker_handle(None)
                .left(px(saturation * PICKER_SIZE - HANDLE_SIZE / 2.0))
                .top(px((1.0 - value) * PICKER_SIZE - HANDLE_SIZE / 2.0)),
        );
    const HUE_STOPS: [u32; 7] = [
        0xff0000, 0xffff00, 0x00ff00, 0x00ffff, 0x0000ff, 0xff00ff, 0xff0000,
    ];
    let hue_segments = HUE_STOPS.windows(2).map(|pair| {
        div().flex_1().h_full().bg(linear_gradient(
            90.0,
            linear_color_stop(hsla(rgb(pair[0])), 0.0),
            linear_color_stop(hsla(rgb(pair[1])), 1.0),
        ))
    });
    let hue_bar = div()
        .id(SharedString::from(format!("{key}-picker-hue")))
        .relative()
        .w(px(PICKER_SIZE))
        .h(px(HUE_BAR_HEIGHT))
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |page: &mut V, event: &MouseDownEvent, window, cx| {
                hue_pick(page, key, kind, event.position, None, window, cx);
            }),
        )
        .on_drag(
            PickerDrag {
                field: field.clone(),
                hue_bar: true,
            },
            |_, _, _, cx| cx.new(|_| PickerDragView),
        )
        .on_drag_move(cx.listener(
            move |page: &mut V, event: &DragMoveEvent<PickerDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.field.as_ref() != key || !drag.hue_bar {
                    return;
                }
                hue_pick(
                    page,
                    key,
                    kind,
                    event.event.position,
                    Some(event.bounds),
                    window,
                    cx,
                );
            },
        ))
        .child(
            h_flex()
                .absolute()
                .inset_0()
                .rounded(px(HUE_BAR_HEIGHT))
                .overflow_hidden()
                .children(hue_segments),
        )
        .child(
            picker_handle(Some(pure_hue))
                .left(px((hue / 360.0) * (PICKER_SIZE - HANDLE_SIZE)))
                .top(px(-2.0)),
        );
    let inputs = h_flex()
        .w(px(PICKER_SIZE))
        .justify_between()
        .child(picker_input(&hex_input, 76.0, "HEX"))
        .child(picker_input(&channels[0], 67.0, "R"))
        .child(picker_input(&channels[1], 67.0, "G"))
        .child(picker_input(&channels[2], 67.0, "B"));
    let done = settings_button(
        p,
        SharedString::from(format!("{key}-picker-done")),
        "Done",
        None,
        ButtonVariant::Primary,
        false,
        None,
        move |page: &mut V, window, cx| close_picker(page, key, kind, fallback, window, cx),
        cx,
    );
    let dialog_bg = if p.light {
        rgb(0xffffff)
    } else {
        rgb(0x0e0e0e)
    };
    let dialog = v_flex()
        .id(SharedString::from(format!("{key}-picker-dialog")))
        .track_focus(&focus)
        .occlude()
        .w(px(DIALOG_WIDTH))
        .p(px(16.0))
        .gap(px(16.0))
        .bg(hsla(dialog_bg))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(p.foreground_alpha(0.1)),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        }])
        .font_family(MODAL_UI_FONT)
        .text_color(hsla(p.foreground))
        .on_key_down(
            cx.listener(move |page: &mut V, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    close_picker(page, key, kind, fallback, window, cx);
                }
            }),
        )
        .on_mouse_down_out(
            cx.listener(move |page: &mut V, _: &MouseDownEvent, window, cx| {
                close_picker(page, key, kind, fallback, window, cx);
            }),
        )
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child("Pick Color"),
        )
        .child(
            // The picker's own body: `#202020` behind the square, the hue bar and the inputs.
            v_flex()
                .w(px(PICKER_SIZE))
                .mx_auto()
                .bg(gpui::rgb(0x202020))
                .gap(px(17.0))
                .child(
                    div()
                        .on_children_prepainted(capture_child_bounds(square_bounds, 0))
                        .child(square),
                )
                .child(
                    v_flex()
                        .gap(px(18.0))
                        .child(
                            div()
                                .on_children_prepainted(capture_child_bounds(hue_bounds, 0))
                                .child(hue_bar),
                        )
                        .child(inputs),
                ),
        )
        .child(h_flex().w_full().justify_end().child(done));
    if let PickerPlacement::BesideTrigger = placement {
        return Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::TopRight)
                    .offset(point(px(-8.0), px(0.0)))
                    .snap_to_window_with_margin(px(8.0))
                    .child(dialog),
            )
            .with_priority(2)
            .into_any_element(),
        );
    }
    let viewport = window.viewport_size();
    Some(
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(px(0.0), px(0.0)))
                .child(
                    div()
                        .w(viewport.width)
                        .h(viewport.height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(dialog),
                ),
        )
        .with_priority(2)
        .into_any_element(),
    )
}

/// The Pick Color dialog of a tint key whose trigger the page draws itself (the Theme page's "+"
/// square), opening beside that trigger. Render it as a child of the trigger.
pub(crate) fn trigger_tint_picker<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    value: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    let saved = normalize_color_input_value(value, DEFAULT_TINT_COLOR);
    FieldStates::color_state(page, key, &saved, ColorFieldKind::Tint, window, cx);
    pick_color_dialog(
        page,
        p,
        key,
        ColorFieldKind::Tint,
        DEFAULT_TINT_COLOR,
        PickerPlacement::BesideTrigger,
        window,
        cx,
    )
}

/// Opens a tint key's Pick Color dialog on its saved value.
pub(crate) fn open_tint_picker<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    value: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let saved = normalize_color_input_value(value, DEFAULT_TINT_COLOR);
    FieldStates::color_state(page, key, &saved, ColorFieldKind::Tint, window, cx);
    open_picker(page, key, DEFAULT_TINT_COLOR, window, cx);
}

fn square_pick<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    kind: ColorFieldKind,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(bounds) = bounds.or_else(|| {
        page.field_states()
            .colors
            .get(key)
            .and_then(|state| state.picker.square_bounds.get())
    }) else {
        return;
    };
    let x = ((f32::from(position.x) - f32::from(bounds.origin.x)) / f32::from(bounds.size.width))
        .clamp(0.0, 1.0);
    let y = ((f32::from(position.y) - f32::from(bounds.origin.y)) / f32::from(bounds.size.height))
        .clamp(0.0, 1.0);
    let Some(state) = page.field_states().colors.get_mut(key) else {
        return;
    };
    state.picker.saturation = x;
    state.picker.value = 1.0 - y;
    let (red, green, blue) = hsv_to_rgb(state.picker.hue, x, 1.0 - y);
    let hex = rgb_to_hex(red, green, blue);
    picker_after_move(page, key, kind, &hex, window, cx);
}

fn hue_pick<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    kind: ColorFieldKind,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let Some(bounds) = bounds.or_else(|| {
        page.field_states()
            .colors
            .get(key)
            .and_then(|state| state.picker.hue_bounds.get())
    }) else {
        return;
    };
    let x = ((f32::from(position.x) - f32::from(bounds.origin.x)) / f32::from(bounds.size.width))
        .clamp(0.0, 1.0);
    let Some(state) = page.field_states().colors.get_mut(key) else {
        return;
    };
    state.picker.hue = x * 360.0;
    let (red, green, blue) = hsv_to_rgb(
        state.picker.hue,
        state.picker.saturation,
        state.picker.value,
    );
    let hex = rgb_to_hex(red, green, blue);
    picker_after_move(page, key, kind, &hex, window, cx);
}

fn picker_after_move<V: SettingsPage>(
    page: &mut V,
    key: &'static str,
    kind: ColorFieldKind,
    hex: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if let Some(state) = page.field_states().colors.get_mut(key) {
        state.color_text = hex.to_string();
        sync_picker_inputs(state, true, window, cx);
        let text_input = state.text_input.clone();
        let text = hex.to_string();
        text_input.update(cx, |input, cx| input.set_value(text, window, cx));
    }
    match kind {
        ColorFieldKind::Plain => save_color(page, key, hex, true, cx),
        ColorFieldKind::Tint => save_color(page, key, hex, false, cx),
    }
    cx.notify();
}

/// `ColorField`: a 44px swatch (opens Pick Color) and the value's text box in the 17rem lane.
#[allow(clippy::too_many_arguments)]
pub(crate) fn color_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    FieldStates::color_state(page, key, value, ColorFieldKind::Plain, window, cx);
    let disabled = spec.disabled_reason.is_some();
    let swatch_color = normalize_color_input_value(value, "#121212");
    let text_input = page
        .field_states()
        .colors
        .get(key)
        .map(|state| state.text_input.clone());
    // The swatch opens the system colour panel like the React `<input type="color">`; the
    // library's Pick Color dialog shows here only in the preview binary's `pick-color` state.
    let dialog = pick_color_dialog(
        page,
        p,
        key,
        ColorFieldKind::Plain,
        "#121212",
        PickerPlacement::WindowCenter,
        window,
        cx,
    );
    let Some(text_input) = text_input else {
        return div().into_any_element();
    };
    // `SettingsInput type='color'` with `h-8 rounded-none p-1`: a square 44x32 box, and inside it
    // Chromium's colour swatch (`::-webkit-color-swatch-wrapper` padding 4px 2px, a 1px #777
    // `::-webkit-color-swatch` border), 30x14.
    let swatch = div()
        .id(SharedString::from(format!("{key}-swatch")))
        .flex_shrink_0()
        .w(px(44.0))
        .h(px(32.0))
        .p(px(4.0))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.input_background()))
        .when(!disabled, |swatch| {
            swatch.cursor_pointer().on_click({
                let initial = swatch_color.clone();
                cx.listener(move |page: &mut V, _: &ClickEvent, _window, cx| {
                    let store = page.settings_store().clone();
                    store.update(cx, |store, cx| store.pick_system_color(key, &initial, cx));
                })
            })
        })
        .child(
            div().size_full().px(px(2.0)).py(px(4.0)).child(
                div()
                    .size_full()
                    .border_1()
                    .border_color(gpui::rgb(0x777777))
                    .bg(hsla(hex_rgba(&swatch_color))),
            ),
        );
    let control = h_flex()
        .id(SharedString::from(format!("{key}-color-control")))
        .w(px(CONTROL_LANE_WIDTH))
        .max_w_full()
        .items_center()
        .gap(px(12.0))
        .child(swatch)
        .child(settings_text_input_with_disabled(
            p,
            &text_input,
            None,
            false,
            disabled,
            window,
            cx,
        ))
        .when(!disabled, |control| control.children(dialog))
        .when(disabled, |control| control.opacity(0.5))
        .when_some(spec.disabled_reason.clone(), |control, reason| {
            control.tooltip(tooltip_text(reason))
        })
        .into_any_element();
    setting_row(p, key, spec, on_reset.filter(|_| !disabled), control, cx)
}

/// `ToggleColorField`: a switch and, while it is on, the colour swatch beside it in the same row
/// (the swatch opens the colour panel like `ColorField`'s). The row is modified while either key
/// is, and its reset puts both back.
#[allow(clippy::too_many_arguments)]
pub(crate) fn toggle_color_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    toggle_key: &'static str,
    color_key: &'static str,
    spec: RowSpec,
    checked: bool,
    color: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let swatch = checked.then(|| {
        FieldStates::color_state(page, color_key, color, ColorFieldKind::Plain, window, cx);
        let dialog = pick_color_dialog(
            page,
            p,
            color_key,
            ColorFieldKind::Plain,
            "#95d7f6",
            PickerPlacement::WindowCenter,
            window,
            cx,
        );
        let initial = normalize_color_input_value(color, "#95d7f6");
        h_flex()
            .child(
                div()
                    .id(SharedString::from(format!("{color_key}-swatch")))
                    .flex_shrink_0()
                    .w(px(44.0))
                    .h(px(32.0))
                    .p(px(4.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.input_background()))
                    .cursor_pointer()
                    .tooltip(tooltip_text("Pick colour"))
                    .on_click({
                        let initial = initial.clone();
                        cx.listener(move |page: &mut V, _: &ClickEvent, _window, cx| {
                            let store = page.settings_store().clone();
                            store.update(cx, |store, cx| {
                                store.pick_system_color(color_key, &initial, cx)
                            });
                        })
                    })
                    .child(
                        div().size_full().px(px(2.0)).py(px(4.0)).child(
                            div()
                                .size_full()
                                .border_1()
                                .border_color(gpui::rgb(0x777777))
                                .bg(hsla(hex_rgba(&initial))),
                        ),
                    ),
            )
            .children(dialog)
    });
    let switch = super::controls::switch_control(
        p,
        SharedString::from(format!("{toggle_key}-switch")),
        spec.label.clone(),
        checked,
        false,
        None,
        move |page: &mut V, next, _window, cx| {
            let store = page.settings_store().clone();
            store.update(cx, |store, cx| {
                store.update_setting(toggle_key, json!(next), cx)
            });
        },
        cx,
    );
    let control = h_flex()
        .id(SharedString::from(format!("{toggle_key}-toggle-color")))
        .items_center()
        .gap(px(12.0))
        .children(swatch)
        .child(switch)
        .into_any_element();
    let on_reset: PageAction<V> = Rc::new(move |page: &mut V, _window, cx| {
        let store = page.settings_store().clone();
        store.update(cx, |store, cx| {
            store.reset_setting(toggle_key, cx);
            store.reset_setting(color_key, cx);
        });
    });
    setting_row(p, toggle_key, spec, Some(on_reset), control, cx)
}

/// `WebColorPickerField`: the tint swatches, the custom picker button and the hex box, wrapping
/// under the label.
#[allow(clippy::too_many_arguments)]
pub(crate) fn web_color_picker_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    key: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let saved = normalize_color_input_value(value, DEFAULT_TINT_COLOR);
    FieldStates::color_state(page, key, &saved, ColorFieldKind::Tint, window, cx);
    let (text_input, color_text) = page
        .field_states()
        .colors
        .get(key)
        .map(|state| (state.text_input.clone(), state.color_text.clone()))
        .expect("color state");
    let color_value = normalize_picker_color_value(&color_text, &saved);
    let dialog = pick_color_dialog(
        page,
        p,
        key,
        ColorFieldKind::Tint,
        DEFAULT_TINT_COLOR,
        PickerPlacement::WindowCenter,
        window,
        cx,
    );
    let label = spec.label.clone();
    let ring = hsla(css_fade(p.ring, 0.45));
    let swatches = TINT_SWATCHES
        .iter()
        .enumerate()
        .map(|(index, (name, hex))| {
            let selected = color_value == *hex;
            let hex = *hex;
            div()
                .id(SharedString::from(format!("{key}-swatch-{index}")))
                .flex_shrink_0()
                .size(px(28.0))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(if selected {
                    hsla(p.ring)
                } else {
                    hsla(css_fade(p.hairline, 0.8))
                })
                .when(selected, |this| {
                    this.shadow(vec![gpui::BoxShadow {
                        color: ring,
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(2.0),
                        inset: false,
                    }])
                })
                .bg(hsla(hex_rgba(hex)))
                .cursor_pointer()
                .tooltip(tooltip_text(*name))
                .on_click(
                    cx.listener(move |page: &mut V, _: &ClickEvent, window, cx| {
                        if let Some(state) = page.field_states().colors.get_mut(key) {
                            state.color_text = hex.to_string();
                            let input = state.text_input.clone();
                            input.update(cx, |input, cx| {
                                input.set_value(hex.to_string(), window, cx)
                            });
                        }
                        save_color(page, key, hex, true, cx);
                    }),
                )
        });
    let custom = div()
        .id(SharedString::from(format!("{key}-custom")))
        .flex_shrink_0()
        .h(px(32.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .cursor_pointer()
        .hover(|this| this.bg(hsla(css_fade(p.hairline, 0.3))))
        .tooltip(tooltip_text("Pick custom tint color"))
        .on_click(
            cx.listener(move |page: &mut V, _: &ClickEvent, window, cx| {
                open_picker(page, key, DEFAULT_TINT_COLOR, window, cx);
            }),
        )
        .child(
            div()
                .size(px(16.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(hex_rgba(&color_value))),
        )
        .child(super::row::settings_icon(icon::PALETTE, 16.0, p.foreground));
    let _ = label;
    let control = h_flex()
        .w_full()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .children(swatches)
        .child(custom)
        .child(
            div()
                .flex_1()
                .flex_basis(px(0.0))
                .min_w_0()
                .font_family(MODAL_MONO_FONT)
                .child(settings_text_input(p, &text_input, None, true, window, cx)),
        )
        .children(dialog)
        .into_any_element();
    setting_row(p, key, spec.wide(), on_reset, control, cx)
}
