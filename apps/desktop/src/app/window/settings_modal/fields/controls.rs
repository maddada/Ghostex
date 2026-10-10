//! The Settings skins of the shadcn primitives the fields build on: the switch
//! (`.ghostex-settings-shadcn [data-slot='switch']`), buttons in the Settings type scale
//! (CDXC:Settings 2026-09-09 DECISION: no white buttons), and `ToggleField`.
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
    /// `variant='outline'`.
    Outline,
    /// `variant='default'`, which Settings turns into the same quiet bordered button.
    Default,
    /// `variant='ghost'`.
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
    /// `variant='secondary'` (the stock zinc secondary: `#27272a` dark, `#f4f4f5` light).
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
        ButtonVariant::Secondary => {
            if p.light {
                gpui::rgb(0x18181b)
            } else {
                gpui::rgb(0xfafafa)
            }
        }
        _ => p.foreground,
    }
}

fn button_colors(
    p: &SettingsPalette,
    variant: ButtonVariant,
) -> (gpui::Hsla, gpui::Hsla, gpui::Rgba) {
    match variant {
        ButtonVariant::Outline => (
            if p.light {
                hsla(p.surface)
            } else {
                transparent()
            },
            hsla(p.hairline),
            if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                css_fade(p.hairline, 0.3)
            },
        ),
        ButtonVariant::Default => (transparent(), hsla(p.hairline), p.raised_hover),
        ButtonVariant::Ghost => (
            transparent(),
            transparent(),
            if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                css_fade(gpui::rgb(0x262626), 0.5)
            },
        ),
        // `hover:bg-primary/80`.
        ButtonVariant::Primary => (hsla(p.primary), transparent(), css_fade(p.primary, 0.8)),
        ButtonVariant::Destructive => {
            let (rest, hover) = if p.light { (0.1, 0.2) } else { (0.2, 0.3) };
            (
                hsla(css_fade(p.destructive, rest)),
                transparent(),
                css_fade(p.destructive, hover),
            )
        }
        ButtonVariant::DestructiveDialog => (
            hsla(css_fade(p.destructive, 0.1)),
            transparent(),
            css_fade(p.destructive, 0.2),
        ),
        // `hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)]`.
        ButtonVariant::Secondary => {
            let (rest, foreground) = if p.light {
                (gpui::rgb(0xf4f4f5), gpui::rgb(0x18181b))
            } else {
                (gpui::rgb(0x27272a), gpui::rgb(0xfafafa))
            };
            (hsla(rest), transparent(), css_mix(rest, 0.95, foreground))
        }
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
    settings_button_sized(
        p,
        id,
        label,
        leading_icon,
        variant,
        ButtonSize::Default,
        disabled,
        disabled_reason,
        on_click,
        cx,
    )
}

/// `settings_button` at a shadcn size (`sm` 28px, `xs` 24px; `data-icon='inline-start'` trims
/// the leading side by 2px).
///
/// CDXC:AppModal 2026-10-09 DECISION:
/// User: "why does this "Save" button (and other buttons in the app in settings and other places) not have an outline? I dont like this please make the buttons look the same." A button with a text label always draws the outline: `Ghost` renders as `Outline` here and in the Agents Hub's `hub_button`. Borderless ghost stays only for icon-only buttons (`settings_icon_button`, the modal corner X), menu rows, links inside running text, segmented controls and switches.
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
    let variant = match variant {
        ButtonVariant::Ghost => ButtonVariant::Outline,
        other => other,
    };
    let (background, border, hover) = button_colors(p, variant);
    let primary = variant == ButtonVariant::Primary;
    let text = button_text(p, variant);
    let (height, side, gap, line_height) = size.metrics();
    let label: SharedString = label.into();
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
        .border_color(border)
        .bg(background)
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
                .hover(move |this| this.bg(hsla(hover)))
                .on_press(cx, move |this, window, cx| {
                    on_click(this, window, cx);
                })
        })
        .children(leading_icon.map(|icon| settings_icon(icon, 16.0, text)))
        .child(label)
        .into_any_element()
}

/// A square icon button (`size='icon'` 32px, `icon-sm` 28px, `icon-xs` 24px) in any variant.
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
    let (background, border, hover) = button_colors(p, variant);
    let color = button_text(p, variant);
    let id: ElementId = id.into();
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
        .border_color(border)
        .bg(background)
        .when(disabled, |this| this.opacity(0.5))
        .when_some(tooltip, |this, tooltip| this.tooltip(tooltip_text(tooltip)))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_press(cx, move |this, window, cx| {
                    on_click(this, window, cx);
                })
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

/// `ActionButtonPairField`: a row of outline buttons on the right.
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
