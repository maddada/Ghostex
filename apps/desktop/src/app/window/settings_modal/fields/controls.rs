//! The Settings skins of the shadcn primitives the fields build on: the switch
//! (`.ghostex-settings-shadcn [data-slot='switch']`), buttons in the Settings type scale (filled,
//! never white: CDXC:AppModal 2026-10-14 on `neutral_button_fills`), and `ToggleField`.
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::SettingsPage;
use super::row::{PageAction, RowSpec, reset_key, setting_row, settings_icon, tooltip_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use serde_json::json;
use std::rc::Rc;

/// CDXC:Settings 2026-09-09 DECISION:
/// User: never show On or Off text beside a toggle in Settings; the switch itself is the state. A view that cannot be turned off shows a locked-on switch instead of an "Always on" caption.
/// The Settings switch: a 32x20 track with a 6px radius and a 1px edge, a 16px thumb with a 4px
/// radius. Off: the raised-hover track with a hairline edge and a foreground thumb (dark) or
/// surface thumb (light). On: a foreground track and a surface thumb.
pub(crate) fn settings_switch(p: &SettingsPalette, checked: bool, disabled: bool) -> AnyElement {
    let thumb = if checked || p.light {
        p.surface
    } else {
        p.foreground
    };
    let thumb = if p.glass && !checked && !p.light {
        p.foreground
    } else if p.glass && !p.light {
        glass_checked_thumb(p)
    } else if p.glass {
        p.modal.solid_surface
    } else {
        thumb
    };
    div()
        .flex_shrink_0()
        .w(px(32.0))
        .h(px(20.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(if checked {
            transparent()
        } else {
            hsla(p.hairline)
        })
        .bg(hsla(if checked {
            p.foreground
        } else {
            p.raised_hover
        }))
        .when(disabled, |this| this.opacity(0.5))
        .flex()
        .items_center()
        .child(
            div()
                .size(px(16.0))
                .ml(px(if checked { 12.0 } else { 0.0 }))
                .rounded(px(4.0))
                .bg(hsla(thumb))
                .shadow_sm(),
        )
        .into_any_element()
}

/// The thumb of an on switch under dark glass: a light gray on the light track.
///
/// CDXC:Settings 2026-11-06 DECISION:
/// The on switch drew the theme chrome (`solid_surface`, near-black on the default theme) as its thumb, a black square on the Extensions cards under glass. The user does not want black or near-black fills on the glass design, so the thumb is a light gray that still stands apart from the light track.
pub(crate) fn glass_checked_thumb(p: &SettingsPalette) -> gpui::Rgba {
    p.muted
}

/// A clickable Settings switch, named for the accessibility tree by `label`: its row's visible
/// title, never its element id (screen readers read the id when a switch has no label).
#[allow(clippy::too_many_arguments)]
pub(crate) fn switch_control<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_change: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .role(gpui::Role::Switch)
        .aria_toggled(a11y_toggled(checked))
        .accessibility_id(id.to_string())
        .aria_label(label)
        .flex_shrink_0()
        .when(!disabled, |this| {
            this.cursor_pointer().on_press(cx, move |this, window, cx| {
                on_change(this, !checked, window, cx);
            })
        })
        .when_some(disabled_reason.filter(|_| disabled), |this, reason| {
            this.tooltip(tooltip_text(reason))
        })
        .child(settings_switch(p, checked, disabled))
        .into_any_element()
}

/// The shadcn button variants Settings uses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonVariant {
    /// `variant='outline'`: the neutral fill.
    Outline,
    /// `variant='default'`: the same neutral fill (Settings has no white buttons).
    Default,
    /// `variant='ghost'`: borderless and unfilled at rest, for icon-only buttons; a labelled
    /// ghost button draws as `Outline`.
    Ghost,
    /// The stock shadcn primary of a dialog portaled out of Settings (Pick Color's Done): the
    /// filled `--primary` with medium text and the 10px `rounded-lg` corners.
    Primary,
    /// `variant='destructive'` on a Settings page: the destructive colour on a 20% (dark) or 10%
    /// (light) tint of itself, one step stronger on hover.
    Destructive,
    /// `variant='destructive'` in a dialog portaled out of Settings (no `.dark` ancestor): the
    /// 10% tint in both appearances.
    DestructiveDialog,
    /// `variant='secondary'`: the neutral fill.
    Secondary,
}

/// The shadcn button sizes Settings uses (Settings keeps 14px text at every size).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonSize {
    /// `size='default'`: 32px, 12px sides, 6px icon gap.
    Default,
    /// `size='sm'`: 28px, 12px sides, 4px icon gap.
    Sm,
    /// `size='xs'`: 24px, 10px sides, 4px icon gap, 14/18.67 text.
    Xs,
}

impl ButtonSize {
    fn metrics(self) -> (f32, f32, f32, f32) {
        // (height, side padding, icon gap, line height)
        match self {
            ButtonSize::Default => (32.0, 12.0, 6.0, 20.0),
            ButtonSize::Sm => (28.0, 12.0, 4.0, 20.0),
            ButtonSize::Xs => (24.0, 10.0, 4.0, 18.67),
        }
    }
}

/// The label colour of a button variant.
fn button_text(p: &SettingsPalette, variant: ButtonVariant) -> gpui::Rgba {
    match variant {
        ButtonVariant::Primary => p.modal.primary_foreground,
        ButtonVariant::Destructive | ButtonVariant::DestructiveDialog => p.destructive,
        _ => p.foreground,
    }
}

fn button_colors(p: &SettingsPalette, variant: ButtonVariant) -> ButtonFills {
    match variant {
        ButtonVariant::Outline | ButtonVariant::Default | ButtonVariant::Secondary => {
            neutral_button_fills(p.light)
        }
        ButtonVariant::Ghost => {
            let hover = if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                css_fade(gpui::rgb(0x262626), 0.5)
            };
            ButtonFills {
                rest: modal_rgba(0x000000, 0.0),
                hover,
                pressed: hover,
            }
        }
        // `hover:bg-primary/80`.
        ButtonVariant::Primary => ButtonFills {
            rest: p.primary,
            hover: css_fade(p.primary, 0.8),
            pressed: css_fade(p.primary, 0.7),
        },
        ButtonVariant::Destructive => destructive_button_fills(p.destructive, p.light),
        // A dialog portaled out of Settings keeps the light tint in both appearances.
        ButtonVariant::DestructiveDialog => destructive_button_fills(p.destructive, true),
    }
}

/// A 32px Settings button: 12px sides, 14px/400 label, optional 16px leading icon, half opacity
/// while disabled (with the `DisabledSettingControlTooltip` reason on hover).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading_icon: Option<&'static str>,
    variant: ButtonVariant,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_button_bound(
        p,
        id.into(),
        label.into(),
        leading_icon,
        variant,
        ButtonSize::Default,
        disabled,
        disabled_reason,
        bind_press(cx, on_click),
    )
}

/// `settings_button` at a shadcn size (`sm` 28px, `xs` 24px; `data-icon='inline-start'` trims
/// the leading side by 2px).
///
/// A button with a text label is always filled (CDXC:AppModal 2026-10-14 on
/// `neutral_button_fills`): `Ghost` renders as `Outline` here and in the Agents Hub's
/// `hub_button`.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `settings_button_bound`; generic over the click handler it was copied into the release binary once per call site (90 copies, about 0.43 MB, plus 18 inlined into `settings_button`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_button_sized<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading_icon: Option<&'static str>,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_button_bound(
        p,
        id.into(),
        label.into(),
        leading_icon,
        variant,
        size,
        disabled,
        disabled_reason,
        bind_press(cx, on_click),
    )
}

#[allow(clippy::too_many_arguments)]
fn settings_button_bound(
    p: &SettingsPalette,
    id: ElementId,
    label: SharedString,
    leading_icon: Option<&'static str>,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: BoundPress,
) -> AnyElement {
    let variant = match variant {
        ButtonVariant::Ghost => ButtonVariant::Outline,
        other => other,
    };
    let fills = button_colors(p, variant);
    let primary = variant == ButtonVariant::Primary;
    let text = button_text(p, variant);
    let (height, side, gap, line_height) = size.metrics();
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .flex_shrink_0()
        .h(px(height))
        .px(px(side))
        // `has-data-[icon=inline-start]`: `pl-2.5` at the default size, `pl-2` at sm and xs.
        .when(leading_icon.is_some(), |this| {
            this.pl(px(if size == ButtonSize::Default {
                10.0
            } else {
                8.0
            }))
        })
        .gap(px(gap))
        .items_center()
        .justify_center()
        .rounded(px(if primary { 10.0 } else { MODAL_RADIUS_CONTROL }))
        .when(primary, |this| this.font_weight(gpui::FontWeight::MEDIUM))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .text_size(px(14.0))
        .line_height(px(line_height))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5))
        .when_some(disabled_reason.filter(|_| disabled), |this, reason| {
            this.tooltip(tooltip_text(reason))
        })
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_bound_press(on_click)
        })
        .children(leading_icon.map(|icon| settings_icon(icon, 16.0, text)))
        .child(label)
        .into_any_element()
}

/// A square icon button (`size='icon'` 32px, `icon-sm` 28px, `icon-xs` 24px) in any variant.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `settings_icon_button_bound`; generic over the click handler it was copied into the release binary once per call site (19 copies, about 60 KB).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_icon_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    icon: &'static str,
    icon_size: f32,
    size: f32,
    variant: ButtonVariant,
    tooltip: Option<SharedString>,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_icon_button_bound(
        p,
        id.into(),
        icon,
        icon_size,
        size,
        variant,
        tooltip,
        disabled,
        bind_press(cx, on_click),
    )
}

#[allow(clippy::too_many_arguments)]
fn settings_icon_button_bound(
    p: &SettingsPalette,
    id: ElementId,
    icon: &'static str,
    icon_size: f32,
    size: f32,
    variant: ButtonVariant,
    tooltip: Option<SharedString>,
    disabled: bool,
    on_click: BoundPress,
) -> AnyElement {
    let fills = button_colors(p, variant);
    let color = button_text(p, variant);
    div()
        .id(id.clone())
        .role(gpui::Role::Button)
        .aria_label(tooltip.clone().unwrap_or_else(|| id.to_string().into()))
        .flex_shrink_0()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .when(disabled, |this| this.opacity(0.5))
        .when_some(tooltip, |this, tooltip| this.tooltip(tooltip_text(tooltip)))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_bound_press(on_click)
        })
        .child(settings_icon(icon, icon_size, color))
        .into_any_element()
}

/// `ToggleField`: a row with the Settings switch; `key` saves straight away.
pub(crate) fn toggle_field<V: SettingsPage>(
    page: &V,
    p: &SettingsPalette,
    key: &'static str,
    spec: RowSpec,
    checked: bool,
    cx: &mut Context<V>,
) -> AnyElement {
    let _ = page;
    toggle_field_with(
        p,
        key,
        spec,
        checked,
        Some(reset_key::<V>(key)),
        move |page: &mut V, next, _window, cx| {
            let store = page.settings_store().clone();
            store.update(cx, |store, cx| store.update_setting(key, json!(next), cx));
        },
        cx,
    )
}

/// `ToggleField` with a caller-owned change (an inverted key, a side effect after saving).
pub(crate) fn toggle_field_with<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    spec: RowSpec,
    checked: bool,
    on_reset: Option<PageAction<V>>,
    on_change: impl Fn(&mut V, bool, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let control = switch_control(
        p,
        SharedString::from(format!("{id}-switch")),
        spec.label.clone(),
        checked,
        false,
        None,
        on_change,
        cx,
    );
    setting_row(p, id, spec, on_reset, control, cx)
}

/// `ActionButtonPairField`: a row of neutral filled buttons on the right.
pub(crate) fn action_buttons_field<V: SettingsPage>(
    p: &SettingsPalette,
    id: &'static str,
    spec: RowSpec,
    actions: Vec<(&'static str, PageAction<V>)>,
    cx: &mut Context<V>,
) -> AnyElement {
    let buttons: Vec<AnyElement> = actions
        .into_iter()
        .enumerate()
        .map(|(index, (label, action))| {
            let action = Rc::clone(&action);
            settings_button(
                p,
                SharedString::from(format!("{id}-action-{index}")),
                label,
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut V, window, cx| action(page, window, cx),
                cx,
            )
        })
        .collect();
    let control = h_flex()
        .flex_wrap()
        .justify_end()
        .gap(px(8.0))
        .children(buttons)
        .into_any_element();
    setting_row(p, id, spec, None, control, cx)
}

/// A muted icon (the Ghostty notice's info icon, list icons).
pub(crate) fn muted_icon(p: &SettingsPalette, path: &'static str, size: f32) -> AnyElement {
    settings_icon(path, size, p.muted)
        .flex_shrink_0()
        .into_any_element()
}
