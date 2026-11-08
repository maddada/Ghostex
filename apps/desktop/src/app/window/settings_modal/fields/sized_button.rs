//! The shadcn `Button` sizes and variants the Settings management pages use besides the 32px
//! `SettingButton` of controls.rs: `xs` (24px, 10px sides, 12px icons: the extension card
//! actions), `sm` (28px, 12px sides: the account and detail-page actions) and the default 32px,
//! all in the Settings type scale (14px, weight 400). Every labelled variant is filled
//! (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
use super::super::super::native_modal_kit::*;
use super::super::palette::SettingsPalette;
use super::row::{CONTROL_HEIGHT, settings_icon, tooltip_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizedButtonSize {
    /// `size='xs'`: 24px tall.
    Xs,
    /// `size='sm'`: 28px tall.
    Sm,
    /// `size='default'`: 32px tall, the height of the Settings fields and selects.
    Default,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SizedButtonVariant {
    /// `variant='outline'`: the neutral fill.
    Outline,
    /// Borderless and unfilled at rest: only for icon-only buttons.
    Ghost,
    /// `variant='default'`: the neutral fill (Settings has no white buttons).
    Default,
    /// The neutral fill.
    Secondary,
    Destructive,
    /// The stock filled primary (a dialog's confirm button outside the Settings skin).
    Primary,
}

/// `bg-secondary` in the Settings skin.
pub(crate) fn secondary_fill(p: &SettingsPalette) -> gpui::Rgba {
    if p.glass && !p.light {
        css_mix(gpui::rgb(0xffffff), 0.06, p.modal.solid_surface)
    } else if p.light {
        gpui::rgb(0xf1f1f1)
    } else {
        gpui::rgb(0x27272a)
    }
}

/// `(fills, text)` of a variant.
pub(crate) fn sized_button_colors(
    p: &SettingsPalette,
    variant: SizedButtonVariant,
) -> (ButtonFills, gpui::Rgba) {
    match variant {
        SizedButtonVariant::Outline
        | SizedButtonVariant::Default
        | SizedButtonVariant::Secondary => (neutral_button_fills(p.light), p.foreground),
        SizedButtonVariant::Ghost => {
            let hover = if p.light {
                gpui::rgb(0xf1f1f1)
            } else {
                css_fade(gpui::rgb(0x262626), 0.5)
            };
            (
                ButtonFills {
                    rest: modal_rgba(0x000000, 0.0),
                    hover,
                    pressed: hover,
                },
                p.foreground,
            )
        }
        SizedButtonVariant::Destructive => (
            destructive_button_fills(p.destructive, p.light),
            p.destructive,
        ),
        SizedButtonVariant::Primary if p.modal.light => (
            p.modal.primary_button_fills(),
            p.modal.primary_button_text(),
        ),
        SizedButtonVariant::Primary => (
            ButtonFills {
                rest: p.primary,
                hover: css_fade(p.primary, 0.8),
                pressed: css_fade(p.primary, 0.7),
            },
            p.modal.primary_button_text(),
        ),
    }
}

/// A shadcn button of `size` and `variant`: optional leading and trailing icons (12px at `xs`,
/// 16px otherwise), half opacity and the `DisabledSettingControlTooltip` reason while disabled.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `settings_sized_button_bound`; generic over the click handler it was copied into the release binary once per call site (27 copies, about 0.13 MB).
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_sized_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading_icon: Option<&'static str>,
    trailing_icon: Option<&'static str>,
    variant: SizedButtonVariant,
    size: SizedButtonSize,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_sized_button_bound(
        p,
        id.into(),
        label.into(),
        leading_icon,
        trailing_icon,
        variant,
        size,
        disabled,
        disabled_reason,
        bind_press(cx, on_click),
    )
}

#[allow(clippy::too_many_arguments)]
fn settings_sized_button_bound(
    p: &SettingsPalette,
    id: ElementId,
    label: SharedString,
    leading_icon: Option<&'static str>,
    trailing_icon: Option<&'static str>,
    variant: SizedButtonVariant,
    size: SizedButtonSize,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: BoundPress,
) -> AnyElement {
    let (fills, text) = sized_button_colors(p, variant);
    let (height, side, gap, icon_size, line_height) = match size {
        SizedButtonSize::Xs => (24.0, 10.0, 4.0, 12.0, 18.67),
        SizedButtonSize::Sm => (28.0, 12.0, 4.0, 16.0, 20.0),
        SizedButtonSize::Default => (CONTROL_HEIGHT, 12.0, 6.0, 16.0, 20.0),
    };
    let icon_side = match size {
        SizedButtonSize::Xs | SizedButtonSize::Sm => 8.0,
        SizedButtonSize::Default => 10.0,
    };
    h_flex()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .flex_shrink_0()
        .h(px(height))
        .px(px(side))
        .when(leading_icon.is_some(), |this| this.pl(px(icon_side)))
        .when(trailing_icon.is_some(), |this| this.pr(px(icon_side)))
        .gap(px(gap))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
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
        .children(leading_icon.map(|icon| settings_icon(icon, icon_size, text)))
        .child(label)
        .children(trailing_icon.map(|icon| settings_icon(icon, icon_size, text)))
        .into_any_element()
}

/// A square icon button (`icon-xs` 24px with a 12px glyph, `icon-sm` 28px, `icon` 32px) with an
/// optional tooltip, in any variant.
///
/// CDXC:Release 2026-10-22 WHY: the body lives in the non-generic `settings_square_button_bound`; generic over the click handler it was copied into the release binary once per call site.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settings_square_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    icon: &'static str,
    icon_color: Option<gpui::Rgba>,
    variant: SizedButtonVariant,
    size: f32,
    tooltip: Option<SharedString>,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    settings_square_button_bound(
        p,
        id.into(),
        icon,
        icon_color,
        variant,
        size,
        tooltip,
        disabled,
        disabled_reason,
        bind_press(cx, on_click),
    )
}

#[allow(clippy::too_many_arguments)]
fn settings_square_button_bound(
    p: &SettingsPalette,
    id: ElementId,
    icon: &'static str,
    icon_color: Option<gpui::Rgba>,
    variant: SizedButtonVariant,
    size: f32,
    tooltip: Option<SharedString>,
    disabled: bool,
    disabled_reason: Option<SharedString>,
    on_click: BoundPress,
) -> AnyElement {
    let (fills, text) = sized_button_colors(p, variant);
    let icon_size = if size <= 24.0 { 12.0 } else { 16.0 };
    let tip = if disabled {
        disabled_reason.or(tooltip)
    } else {
        tooltip
    };
    let a11y_label: SharedString = tip.clone().unwrap_or_else(|| id.to_string().into());
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(a11y_label)
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
        .when_some(tip, |this, tip| this.tooltip(tooltip_text(tip)))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_bound_press(on_click)
        })
        .child(settings_icon(icon, icon_size, icon_color.unwrap_or(text)))
        .into_any_element()
}
