//! The shadcn `Checkbox` (16px, 6px radius, `border-input`, a filled `--primary` box with a check
//! when checked) and the small `Switch` (`size='sm'`: a 24x16 track with a 12px thumb) in the
//! Settings skin, for the management pages (extension cards, the custom view editor's spaces, the
//! account consent).
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::controls::SwitchControl;
use super::row::{settings_icon, tooltip_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

/// `IconCheck`.
pub(crate) const CHECK_ICON: &str = "modals/settings/check.svg";

/// The checkbox box.
pub(crate) fn settings_checkbox(p: &SettingsPalette, checked: bool, disabled: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(16.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(if checked { p.primary } else { p.hairline }))
        .bg(hsla(if checked {
            p.primary
        } else if p.light {
            gpui::rgba(0x00000000)
        } else {
            css_fade(p.hairline, 0.3)
        }))
        .when(disabled, |this| this.opacity(0.5))
        .when(checked, |this| {
            this.child(settings_icon(CHECK_ICON, 14.0, p.modal.primary_foreground))
        })
        .into_any_element()
}

/// A clickable checkbox followed by its label (`<label className='flex items-center gap-2'>`).
pub(crate) fn checkbox_control<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    checked: bool,
    label: Option<AnyElement>,
    gap: f32,
    on_change: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::CheckBox)
        .aria_toggled(a11y_toggled(checked))
        .flex()
        .items_center()
        .gap(px(gap))
        .cursor_pointer()
        .on_press(cx, move |this, window, cx| {
            on_change(this, !checked, window, cx);
        })
        .child(settings_checkbox(p, checked, false))
        .children(label)
        .into_any_element()
}

/// The small Settings switch: 24x16 with a 6px radius, a 12px thumb with a 4px radius.
pub(crate) fn settings_small_switch(
    p: &SettingsPalette,
    checked: bool,
    disabled: bool,
) -> AnyElement {
    draw_switch(
        SwitchShape::SMALL,
        super::controls::settings_switch_off(p),
        p.light,
        checked,
        disabled,
    )
    .into_any_element()
}

/// A clickable small switch with an optional reason tooltip while disabled, named for the
/// accessibility tree by `label` (its card or row title, as `switch_control`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn small_switch_control<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_change: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> SwitchControl {
    let id: ElementId = id.into();
    let press = (!disabled).then(|| {
        bind_press(cx, move |this: &mut V, window, cx| {
            on_change(this, !checked, window, cx);
        })
    });
    let own_press = press.clone();
    let element = div()
        .id(id.clone())
        .role(gpui::Role::Switch)
        .aria_toggled(a11y_toggled(checked))
        .accessibility_id(id.to_string())
        .aria_label(label)
        .flex_shrink_0()
        .when_some(own_press, |this, press| {
            this.cursor_pointer().on_bound_press(stopping_press(press))
        })
        .when_some(disabled_reason.filter(|_| disabled), |this, reason| {
            this.tooltip(tooltip_text(reason))
        })
        .child(settings_small_switch(p, checked, disabled))
        .into_any_element();
    SwitchControl::new(element, press)
}
