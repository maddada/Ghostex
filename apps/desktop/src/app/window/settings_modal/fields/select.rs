//! `SelectField` / `SettingsSelect`: a 32px trigger on the raised tone (12rem in list rows) whose
//! popup opens with the selected row laid over the trigger, as Base UI's select does, in the
//! `.settings-select-content` skin (the `#161616` popup, 32px rows, `#2c2c2c` selected row).
//! Built on the app modal kit's `ModalSelect` state; the popup draws above the page with
//! `deferred(anchored())`.
use super::super::super::native_modal_kit::*;
use super::super::catalog::SettingOption;
use super::super::palette::{SETTINGS_FONT, SettingsPalette};
use super::row::{CONTROL_HEIGHT, PageAction, RowSpec, setting_row, settings_icon, tooltip_text};
use super::{FieldStates, SettingsPage, icon};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, App, Context, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseDownEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, anchored, deferred, div, point, px,
};
use gpui_component::v_flex;
use std::rc::Rc;

/// Row height inside the popup (`min-height: 2rem`).
const POPUP_ROW_HEIGHT: f32 = 32.0;

pub(crate) struct SelectState {
    pub(crate) select: ModalSelect,
    pub(crate) focus: FocusHandle,
}

impl FieldStates {
    pub(crate) fn select_state(&mut self, id: &SharedString, cx: &mut App) -> &mut SelectState {
        self.selects
            .entry(id.clone())
            .or_insert_with(|| SelectState {
                select: ModalSelect::new(),
                focus: cx.focus_handle().tab_stop(true),
            })
    }
}

fn toggle_select<V: SettingsPage>(
    page: &mut V,
    id: &SharedString,
    selected: Option<usize>,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    let states = page.field_states();
    if states.open_select.as_ref().is_some_and(|open| open != id) {
        states.close_select();
    }
    let state = states.select_state(id, cx);
    state.select.toggle(selected);
    let open = state.select.open;
    state.focus.focus(window, cx);
    states.open_select = open.then(|| id.clone());
    cx.notify();
}

fn close_select<V: SettingsPage>(page: &mut V, cx: &mut Context<V>) {
    page.field_states().close_select();
    cx.notify();
}

/// CDXC:DesignSystem 2026-09-09 DECISION:
/// User: select rows carry no check mark. The selected row is marked by a different background instead, so the popup can match the trigger width exactly.
/// The trigger and, while open, its popup. `on_change` runs with the chosen option's value.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_select<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    options: &[SettingOption],
    value: &str,
    width: Option<f32>,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_change: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_select_with_unavailable(
        page,
        p,
        id,
        options,
        &[],
        value,
        width,
        disabled,
        disabled_reason,
        on_change,
        window,
        cx,
    )
}

/// A select's change handler, shared by the trigger's keys and every popup row.
type SelectChange<V> = Rc<dyn Fn(&mut V, String, &mut Window, &mut Context<V>)>;

/// [`settings_select`] with options that show, dimmed, but cannot be picked (`unavailable` holds
/// their values): an editor app that is not installed, for example.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in `settings_select_shared`, generic over the page only; generic over the change handler too it was copied into the release binary once per call site (11 copies plus 12 inlined into `select_field`, about 0.29 MB).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_select_with_unavailable<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    options: &[SettingOption],
    unavailable: &[String],
    value: &str,
    width: Option<f32>,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_change: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_select_shared(
        page,
        p,
        id.into(),
        options,
        unavailable,
        None,
        value,
        width,
        disabled,
        disabled_reason,
        Rc::new(on_change),
        window,
        cx,
    )
}

/// [`settings_select`] whose open list ends with a quiet line that can't be picked (`footer`):
/// Settings › Dictation's machine picker says "Only connected shown" there.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_select_with_footer<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: impl Into<SharedString>,
    options: &[SettingOption],
    footer: SharedString,
    value: &str,
    width: Option<f32>,
    on_change: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_select_shared(
        page,
        p,
        id.into(),
        options,
        &[],
        Some(footer),
        value,
        width,
        false,
        None,
        Rc::new(on_change),
        window,
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
fn settings_select_shared<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: SharedString,
    options: &[SettingOption],
    unavailable: &[String],
    footer: Option<SharedString>,
    value: &str,
    width: Option<f32>,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_change: SelectChange<V>,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let unavailable: Vec<String> = unavailable.to_vec();
    let selected = options.iter().position(|option| option.value == value);
    if page.field_states().pending_open_select.as_ref() == Some(&id) {
        page.field_states().pending_open_select = None;
        toggle_select(page, &id, selected, window, cx);
    }
    let label = selected
        .map(|index| options[index].label.clone())
        .unwrap_or_else(|| value.to_string());
    let (open, focus, trigger_bounds, highlight) = {
        let state = page.field_states().select_state(&id, cx);
        (
            state.select.open,
            state.focus.clone(),
            state.select.trigger_bounds.clone(),
            state.select.highlight,
        )
    };
    let focused = focus.is_focused(window);
    let p = *p;
    let key_id = id.clone();
    let key_options: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
    let key_labels: Vec<String> = options
        .iter()
        .map(|option| option.label.to_lowercase())
        .collect();
    let key_change = on_change.clone();
    let key_unavailable = unavailable.clone();
    let trigger = div()
        .id(SharedString::from(format!("{id}-trigger")))
        .role(gpui::Role::ComboBox)
        .accessibility_id(format!("{id}-trigger"))
        .aria_value(label.clone())
        .aria_expanded(open)
        .track_focus(&focus)
        .flex_shrink_0()
        .when_some(width, |this, width| this.w(px(width)))
        .when(width.is_none(), |this| this.w_full())
        .max_w_full()
        .min_w_0()
        .h(px(CONTROL_HEIGHT))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused || open {
            p.focus_border
        } else {
            p.hairline
        }))
        .bg(hsla(p.raised))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .when(disabled, |this| this.opacity(0.5))
        .when_some(disabled_reason.filter(|_| disabled), |this, reason| {
            this.tooltip(tooltip_text(reason))
        })
        .when(!disabled, |this| {
            let click_id = id.clone();
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(p.raised_hover)))
                .on_press(cx, move |page, window, cx| {
                    toggle_select(page, &click_id, selected, window, cx);
                })
                .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    let count = key_options.len();
                    let state = page.field_states().select_state(&key_id, cx);
                    if !state.select.open {
                        if matches!(key, "enter" | "space" | "down" | "up") {
                            cx.stop_propagation();
                            toggle_select(page, &key_id, selected, window, cx);
                        }
                        return;
                    }
                    cx.stop_propagation();
                    // Typeahead: jump to the next option starting with the typed letter.
                    if let Some(typed) = event
                        .keystroke
                        .key_char
                        .as_deref()
                        .filter(|typed| typed.chars().count() == 1 && !typed.trim().is_empty())
                    {
                        let typed = typed.to_lowercase();
                        let start = state.select.highlight.map(|index| index + 1).unwrap_or(0);
                        if let Some(index) = (0..count)
                            .map(|offset| (start + offset) % count.max(1))
                            .find(|index| key_labels[*index].starts_with(&typed))
                        {
                            state.select.highlight = Some(index);
                        }
                        cx.notify();
                        return;
                    }
                    let key = if key == "space" { "enter" } else { key };
                    match state.select.handle_key(key, count) {
                        ModalSelectKey::Choose(index) => {
                            page.field_states().open_select = None;
                            if let Some(value) = key_options
                                .get(index)
                                .filter(|value| !key_unavailable.contains(value))
                                .cloned()
                            {
                                key_change(page, value, window, cx);
                            }
                        }
                        ModalSelectKey::Consumed => {
                            if !page.field_states().select_state(&key_id, cx).select.open {
                                page.field_states().open_select = None;
                            }
                        }
                        ModalSelectKey::Ignored => {}
                    }
                    cx.notify();
                }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
        .child(settings_icon(icon::SELECTOR, 16.0, p.muted).flex_shrink_0());
    let wrapper = div()
        .flex_shrink_0()
        .when_some(width, |this, width| this.w(px(width)))
        .when(width.is_none(), |this| this.w_full())
        .max_w_full()
        .min_w_0()
        .on_children_prepainted(capture_child_bounds(trigger_bounds.clone(), 0))
        .child(trigger);
    if !open {
        return wrapper.into_any_element();
    }
    let Some(bounds) = trigger_bounds.get() else {
        return wrapper.into_any_element();
    };
    let rows = options.iter().enumerate().map(|(index, option)| {
        let is_selected = selected == Some(index);
        let highlighted = highlight == Some(index);
        let pickable = !unavailable.contains(&option.value);
        let value = option.value.clone();
        let on_change = on_change.clone();
        div()
            .id((SharedString::from(format!("{id}-option")), index))
            .role(gpui::Role::ListBoxOption)
            .aria_label(option.label.clone())
            .aria_selected(is_selected)
            .w_full()
            .flex_shrink_0()
            .min_h(px(POPUP_ROW_HEIGHT))
            .px(px(8.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .cursor_default()
            .text_color(hsla(if is_selected {
                p.popup_selected_foreground
            } else {
                p.foreground
            }))
            .when(is_selected, |this| this.bg(hsla(p.popup_selected)))
            .when(!is_selected && highlighted, |this| {
                this.bg(hsla(p.popup_hover))
            })
            .when(!is_selected && pickable, |this| {
                this.hover(move |this| this.bg(hsla(p.popup_hover)))
            })
            .when(!pickable, |this| this.opacity(0.45))
            .when(pickable, |this| {
                this.on_press(cx, move |page, window, cx| {
                    close_select(page, cx);
                    on_change(page, value.clone(), window, cx);
                })
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(option.label.clone()),
            )
    });
    // Base UI lays the selected row over the trigger: the row's text sits on the trigger's text.
    let selected_offset = selected.unwrap_or(0) as f32 * POPUP_ROW_HEIGHT;
    let top = bounds.origin.y - px(6.0) - px(selected_offset);
    let max_height = (window.viewport_size().height - px(16.0)).max(px(0.0));
    let popup = v_flex()
        .id(SharedString::from(format!("{id}-popup")))
        .occlude()
        .w(bounds.size.width)
        .max_h(max_height)
        .overflow_y_scroll()
        .p(px(4.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.popup_border))
        .bg(hsla(p.popup_background))
        .shadow_md()
        .font_family(SETTINGS_FONT)
        .on_mouse_down_out(
            cx.listener(move |page, event: &MouseDownEvent, _window, cx| {
                if bounds.contains(&event.position) {
                    return;
                }
                close_select(page, cx);
            }),
        )
        .children(rows)
        .when_some(footer, |this, footer| {
            this.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .pt(px(6.0))
                    .pb(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(hsla(p.muted))
                    .child(footer),
            )
        });
    wrapper
        .child(
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Window)
                    .position(point(bounds.origin.x, top.max(px(8.0))))
                    .snap_to_window_with_margin(px(8.0))
                    .child(popup),
            )
            .with_priority(1),
        )
        .into_any_element()
}

/// `SelectField`: a setting row with a select; `width` is the row's trigger width (`None` for
/// the standard 12rem).
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_field<V: SettingsPage>(
    page: &mut V,
    p: &SettingsPalette,
    id: &'static str,
    spec: RowSpec,
    on_reset: Option<PageAction<V>>,
    options: &[SettingOption],
    value: &str,
    width: Option<f32>,
    on_change: impl Fn(&mut V, String, &mut Window, &mut Context<V>) + Clone + 'static,
    window: &mut Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let disabled = spec.disabled_reason.is_some();
    let control = settings_select(
        page,
        p,
        id,
        options,
        value,
        Some(width.unwrap_or(super::row::SELECT_WIDTH)),
        disabled,
        spec.disabled_reason.clone(),
        on_change,
        window,
        cx,
    );
    setting_row(p, id, spec, on_reset.filter(|_| !disabled), control, cx)
}
