//! The Remote page's look: the `--app-*` tokens its `.settings-remote-*` rules mix
//! (packages/core-ui/styles/modals.css (deleted 2026-11-06), the light tones in styles/settings-light.css (deleted 2026-11-06)) and the small
//! pieces every card shares (badges, section labels, spinners, the raised segmented control, the
//! small switch, compact buttons and inputs).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    icon, settings_icon, settings_switch, tooltip_text, wrapped_tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::RemoteTab;
use super::model::BadgeTone;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, ClickEvent, Context, Div, ElementId, Entity,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Rgba, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, Transformation, Window, div, px,
    radians, rgb,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::time::Duration;

pub(super) const ICON_ALERT: &str = "modals/settings/alert-triangle.svg";
pub(super) const ICON_ARROWS_MAXIMIZE: &str = "modals/settings/arrows-maximize.svg";
pub(super) const ICON_BRAND_APPLE: &str = "modals/settings/brand-apple.svg";
pub(super) const ICON_BRAND_UBUNTU: &str = "modals/settings/brand-ubuntu.svg";
pub(super) const ICON_BRAND_WINDOWS: &str = "modals/settings/brand-windows.svg";
pub(super) const ICON_CHECK: &str = "modals/settings/check.svg";
pub(super) const ICON_CIRCLE_CHECK_FILLED: &str = "modals/settings/circle-check-filled.svg";
pub(super) const ICON_COPY: &str = "modals/settings/copy.svg";
pub(super) const ICON_DEVICE_DESKTOP: &str = "modals/settings/device-desktop.svg";
pub(super) const ICON_DEVICE_FLOPPY: &str = "modals/settings/device-floppy.svg";
pub(super) const ICON_DEVICE_LAPTOP: &str = "modals/settings/device-laptop.svg";
pub(super) const ICON_DEVICE_MOBILE: &str = "modals/settings/device-mobile.svg";
pub(super) const ICON_LOADER: &str = "modals/settings/loader-2.svg";
pub(super) const ICON_POWER: &str = "modals/settings/power.svg";
pub(super) const ICON_QRCODE: &str = "modals/settings/qrcode.svg";
pub(super) const ICON_REFRESH: &str = "modals/settings/refresh.svg";
pub(super) const ICON_SHIELD: &str = "modals/settings/shield.svg";

/// The `--app-*` tokens of the Remote rules, resolved for the Settings palette.
#[derive(Clone, Copy)]
pub(crate) struct RemoteTokens {
    pub(super) p: SettingsPalette,
    /// `--app-card-active`.
    pub(super) card_active: Rgba,
    /// `--app-border`.
    pub(super) border: Rgba,
    /// `--app-muted`.
    pub(super) muted: Rgba,
    /// `--app-foreground`.
    pub(super) foreground: Rgba,
    /// `--app-background`.
    pub(super) background: Rgba,
    /// `--primary`.
    pub(super) primary: Rgba,
    pub(super) success: Rgba,
    pub(super) warn: Rgba,
    pub(super) failed: Rgba,
}

impl RemoteTokens {
    pub(super) fn new(p: &SettingsPalette) -> Self {
        let foreground = p.foreground;
        if p.light {
            return Self {
                p: *p,
                card_active: if p.glass {
                    modal_rgba(0x000000, 0.06)
                } else {
                    rgb(0xefefef)
                },
                border: modal_rgba(0x000000, 0.12),
                muted: rgb(0x717171),
                foreground,
                background: rgb(0xffffff),
                primary: p.primary,
                success: rgb(0x047857),
                warn: rgb(0x92400e),
                failed: rgb(0xb91c1c),
            };
        }
        Self {
            p: *p,
            card_active: if p.glass {
                modal_rgba(0xffffff, 0.10)
            } else {
                rgb(0x2a2a2a)
            },
            border: modal_rgba(0xffffff, 0.11),
            muted: p.muted,
            foreground,
            background: p.modal.background,
            primary: p.primary,
            success: css_mix(rgb(0x6ee7b7), 0.78, foreground),
            warn: css_mix(rgb(0xfacc15), 0.82, foreground),
            failed: css_mix(rgb(0xf87171), 0.82, foreground),
        }
    }

    /// `color-mix(in srgb, var(--app-card-active) <weight>, transparent)`.
    pub(super) fn card(&self, weight: f32) -> Rgba {
        css_fade(self.card_active, weight)
    }

    /// `color-mix(in srgb, var(--app-border) <weight>, transparent)`.
    pub(super) fn edge(&self, weight: f32) -> Rgba {
        css_fade(self.border, weight)
    }

    pub(super) fn tone(&self, tone: BadgeTone) -> Rgba {
        match tone {
            BadgeTone::Active => self.success,
            BadgeTone::Failed => self.failed,
            BadgeTone::NeedsSetup => self.warn,
            _ => self.muted,
        }
    }
}

/// `.settings-remote-status-badge` / `.settings-remote-tag`: 22px tall, 8px sides, 11px text.
pub(super) fn badge(
    t: &RemoteTokens,
    label: impl Into<SharedString>,
    tone: BadgeTone,
    tag: bool,
) -> AnyElement {
    let (background, border, color) = if tag {
        (
            css_fade(t.primary, 0.16),
            css_fade(t.primary, 0.30),
            t.primary,
        )
    } else {
        (t.card(0.58), t.edge(0.76), t.tone(tone))
    };
    div()
        .flex_shrink_0()
        .h(px(22.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(background))
        .text_size(px(11.0))
        .line_height(px(11.0))
        .text_color(hsla(color))
        .whitespace_nowrap()
        .child(label.into())
        .into_any_element()
}

/// `.settings-remote-section-label`: the uppercase 13px muted heading.
pub(super) fn section_label(t: &RemoteTokens, label: &str) -> AnyElement {
    div()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(t.muted))
        .child(label.to_uppercase())
        .into_any_element()
}

/// `.settings-management-detail`: 13px muted, one line with an ellipsis unless `wrap`.
pub(super) fn detail(t: &RemoteTokens, text: impl Into<SharedString>, wrap: bool) -> gpui::Div {
    div()
        .min_w_0()
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(t.muted))
        .when(!wrap, |this| {
            this.overflow_hidden().whitespace_nowrap().text_ellipsis()
        })
        .child(text.into())
}

/// `animate-spin` / `.settings-remote-spinner` on the Tabler loader.
pub(super) fn spinner(
    id: impl Into<ElementId>,
    size: f32,
    color: Rgba,
    period_ms: u64,
) -> AnyElement {
    settings_icon(ICON_LOADER, size, color)
        .flex_shrink_0()
        .with_animation(
            id,
            Animation::new(Duration::from_millis(period_ms))
                .repeat()
                .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

/// The shadcn `Switch size='sm'` in the Settings skin: a 24x16 track, 12px thumb.
pub(super) fn small_switch(p: &SettingsPalette, checked: bool) -> AnyElement {
    let thumb = if checked || p.light {
        p.surface
    } else {
        p.foreground
    };
    div()
        .flex_shrink_0()
        .w(px(24.0))
        .h(px(16.0))
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
        .flex()
        .items_center()
        .child(
            div()
                .size(px(12.0))
                .ml(px(if checked { 10.0 } else { 1.0 }))
                .rounded(px(4.0))
                .bg(hsla(thumb))
                .shadow_sm(),
        )
        .into_any_element()
}

/// One option of the raised `SegmentedControl`.
pub(super) struct Segment {
    pub(super) value: &'static str,
    pub(super) label: &'static str,
    pub(super) icon: Option<&'static str>,
}

/// `SegmentedControl variant='raised' stretch`: a filled tray (`#202020` dark) with a hairline
/// edge, 3px inset and gap, the pressed segment raised (`#363636`, a soft shadow and a 1px ring).
/// `small` is `size='sm'`.
#[allow(clippy::too_many_arguments)]
pub(super) fn raised_segmented<V: 'static>(
    p: &SettingsPalette,
    id: &str,
    segments: &[Segment],
    selected: &str,
    _small: bool,
    on_select: impl Fn(&mut V, &'static str, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let (tray, tray_border, pressed_bg, pressed_text, resting_text, hover_bg) = if p.light {
        (
            rgb(0xf0f0f0),
            modal_rgba(0x000000, 0.14),
            rgb(0xffffff),
            rgb(0x171717),
            rgb(0x525252),
            modal_rgba(0x000000, 0.04),
        )
    } else {
        (
            rgb(0x202020),
            modal_rgba(0xffffff, 0.14),
            rgb(0x363636),
            rgb(0xf5f5f5),
            rgb(0xb8b8b8),
            modal_rgba(0xffffff, 0.04),
        )
    };
    // Under window glass, and in a dialog whose palette carries the ink washes
    // (`SettingsPalette::nested_dialog`), the solid tray and pressed fills become the rail washes.
    let (tray, pressed_bg, hover_bg) = if p.glass || p.raised.a < 1.0 {
        let rail = raised_rail_colors(p.light, true);
        (rail.track, rail.pressed, rail.hover)
    } else {
        (tray, pressed_bg, hover_bg)
    };
    let ring = if p.light {
        modal_rgba(0x000000, 0.08)
    } else {
        modal_rgba(0xffffff, 0.08)
    };
    let segments = segments.iter().enumerate().map(|(index, segment)| {
        let pressed = segment.value == selected;
        let value = segment.value;
        let on_select = on_select.clone();
        let text = if pressed { pressed_text } else { resting_text };
        h_flex()
            .id(SharedString::from(format!("{id}-segment-{index}")))
            // A segment takes its label's width plus an equal share of the spare room; when the
            // picker is too narrow the labels shrink together and end in an ellipsis inside
            // their padding (the tab strip's rule, `modal_bordered_segmented_control`).
            .flex_auto()
            .min_w_0()
            .h_full()
            .px(px(12.0))
            .gap(px(6.0))
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .text_size(px(13.0))
            .line_height(px(18.57))
            .whitespace_nowrap()
            .text_color(hsla(text))
            .cursor_pointer()
            .when(pressed, |this| {
                this.bg(hsla(pressed_bg)).shadow(vec![
                    gpui::BoxShadow {
                        color: hsla(modal_rgba(0x000000, 0.14)),
                        offset: gpui::point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    gpui::BoxShadow {
                        color: hsla(ring),
                        offset: gpui::point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ])
            })
            .when(!pressed, |this| {
                this.hover(move |this| this.bg(hsla(hover_bg)))
            })
            .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                if !pressed {
                    on_select(page, value, window, cx);
                }
            }))
            .children(
                segment
                    .icon
                    .map(|path| settings_icon(path, 16.0, text).flex_shrink_0()),
            )
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(segment.label),
            )
    });
    h_flex()
        .id(SharedString::from(format!("{id}-segmented")))
        .w_full()
        .h(px(32.0))
        .p(px(3.0))
        .gap(px(3.0))
        .items_stretch()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(tray_border))
        .bg(hsla(tray))
        .children(segments)
        .into_any_element()
}

/// The shadcn `Button` looks the Remote page draws itself (spinners, inherited text colours,
/// the stock 12px `xs` popover triggers); the field library's `settings_button_sized` covers the
/// plain ones. Every look is filled (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Look {
    /// The neutral fill every Settings button wears.
    Filled,
    /// `variant='secondary'`: the stock zinc fill (the current OS among the SSH instruction
    /// buttons).
    Secondary,
    Destructive,
}

fn look_colors(p: &SettingsPalette, look: Look) -> ButtonFills {
    match look {
        Look::Filled => neutral_button_fills(p.light),
        Look::Secondary => {
            let (rest, foreground) = if p.light {
                (rgb(0xf4f4f5), rgb(0x18181b))
            } else {
                (rgb(0x27272a), rgb(0xfafafa))
            };
            let hover = css_mix(rest, 0.95, foreground);
            ButtonFills {
                rest,
                hover,
                pressed: css_mix(rest, 0.90, foreground),
            }
        }
        Look::Destructive => destructive_button_fills(p.destructive, p.light),
    }
}

fn look_text(p: &SettingsPalette, look: Look) -> Rgba {
    match look {
        Look::Destructive => p.destructive,
        Look::Secondary => {
            if p.light {
                rgb(0x18181b)
            } else {
                rgb(0xfafafa)
            }
        }
        Look::Filled => p.foreground,
    }
}

/// A compact button (`height` 24 or 28) with an optional leading element (an icon or a spinner),
/// its text colour inherited from the row when `color` is set, and the disabled reason tooltip.
#[allow(clippy::too_many_arguments)]
pub(super) fn compact_button<V: 'static>(
    p: &SettingsPalette,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    leading: Option<AnyElement>,
    look: Look,
    height: f32,
    text_size: f32,
    color: Option<Rgba>,
    disabled: bool,
    reason: Option<SharedString>,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let fills = look_colors(p, look);
    let text = color.unwrap_or_else(|| look_text(p, look));
    let side = if height <= 24.0 { 10.0 } else { 12.0 };
    let has_leading = leading.is_some();
    h_flex()
        .id(id)
        .flex_shrink_0()
        .h(px(height))
        .px(px(side))
        .when(has_leading, |this| this.pl(px(side - 2.0)))
        .gap(px(4.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(transparent())
        .bg(hsla(fills.rest))
        .text_size(px(text_size))
        .line_height(px(if text_size <= 12.0 { 16.0 } else { 18.67 }))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5))
        .when_some(reason.filter(|_| disabled), |this, reason| {
            this.tooltip(tooltip_text(reason))
        })
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(fills.hover)))
                .active(move |this| this.bg(hsla(fills.pressed)))
                .on_click(cx.listener(move |page, _: &ClickEvent, window, cx| {
                    on_click(page, window, cx);
                }))
        })
        .children(leading)
        .child(label.into())
        .into_any_element()
}

/// The Settings input skin at the Remote dialog's size (`.settings-remote-machine-input`: 34px,
/// 10px sides) or the page's (32px, 12px sides).
pub(super) fn remote_input(
    p: &SettingsPalette,
    state: &Entity<InputState>,
    height: f32,
    side: f32,
    width: Option<f32>,
    monospace: bool,
    disabled: bool,
    window: &Window,
    cx: &gpui::App,
) -> AnyElement {
    use gpui::Focusable as _;
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .when_some(width, |this, width| this.w(px(width)).flex_shrink_0())
        .when(width.is_none(), |this| this.flex_1().w_full())
        .min_w_0()
        .max_w_full()
        .h(px(height))
        .px(px(side))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.input_background()))
        .when(disabled, |this| this.opacity(0.5))
        .when(monospace, |this| this.font_family(MODAL_MONO_FONT))
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .disabled(disabled)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(14.0))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .into_any_element()
}

/// `.settings-remote-path-icon`: the 28px tile in front of a path card's title.
pub(super) fn path_icon(t: &RemoteTokens, path: &'static str, accent: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.76)))
        .bg(hsla(t.card(0.58)))
        .child(settings_icon(
            path,
            16.0,
            if accent { t.primary } else { t.muted },
        ))
        .into_any_element()
}

/// `.settings-remote-error`: the red line with its warning icon.
pub(super) fn error_line(t: &RemoteTokens, message: impl Into<SharedString>) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(8.0))
        .text_size(px(13.0))
        .line_height(px(18.85))
        .text_color(hsla(t.failed))
        .child(
            div()
                .mt(px(1.0))
                .child(settings_icon(ICON_ALERT, 16.0, t.failed)),
        )
        .child(div().flex_1().min_w_0().child(message.into()))
        .into_any_element()
}

/// Bold text inside copy (`<strong>`): the foreground at medium weight.
pub(super) fn strong(t: &RemoteTokens, text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_color(hsla(t.foreground))
        .font_weight(FontWeight::MEDIUM)
        .child(text.into())
}

/// The chevron of a disclosure (`IconChevronDown` open, `IconChevronRight` closed).
pub(super) fn chevron(open: bool, size: f32, color: Rgba) -> AnyElement {
    settings_icon(
        if open {
            icon::CHEVRON_DOWN
        } else {
            icon::CHEVRON_RIGHT
        },
        size,
        color,
    )
    .flex_shrink_0()
    .into_any_element()
}

/// How a run of copy is emphasised (`<strong>`, `<em>`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Emph {
    Plain,
    /// `<strong>`: the foreground colour (Settings draws its medium weight in the regular face).
    Strong,
    Em,
}

/// A paragraph of runs that wraps as one text (`<span>` copy with `<strong>` and `<em>` inside).
pub(super) fn rich_text(strong_color: Rgba, parts: &[(&str, Emph)]) -> gpui::StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for (part, emph) in parts {
        let start = text.len();
        text.push_str(part);
        let range = start..text.len();
        match emph {
            Emph::Plain => {}
            Emph::Strong => highlights.push((
                range,
                gpui::HighlightStyle {
                    color: Some(hsla(strong_color)),
                    ..Default::default()
                },
            )),
            Emph::Em => highlights.push((
                range,
                gpui::HighlightStyle {
                    font_style: Some(gpui::FontStyle::Italic),
                    ..Default::default()
                },
            )),
        }
    }
    gpui::StyledText::new(text).with_highlights(highlights)
}

/// The colours of a dialog portaled out of Settings (`DialogContent` on the page body): its surface,
/// edge (`--app-border` at 92%), text and muted text.
pub(super) struct DialogColors {
    pub(super) background: Rgba,
    pub(super) border: Rgba,
    pub(super) foreground: Rgba,
    pub(super) muted: Rgba,
}

pub(super) fn dialog_colors(t: &RemoteTokens) -> DialogColors {
    let p = &t.p;
    DialogColors {
        background: p.dialog_surface(),
        border: t.edge(0.92),
        foreground: if p.light { p.foreground } else { rgb(0xfafafa) },
        muted: if p.light { p.muted } else { rgb(0xa1a1a1) },
    }
}

/// The (i) that follows a dialog field's label while the pointer is over the field (`group`, set on
/// the field's wrapper): hovering it shows the field's explanation in Settings' wrapping tooltip,
/// the same icon and tooltip as the Extensions cards' (i). It sits in a click shield so a press on
/// it never toggles the switch row it may be inside.
///
/// CDXC:RemoteMachines 2026-11-09 DECISION:
/// User, of the machine dialog: "pls reduce what i see right away here by moving the descriptions
/// for each field to be in a i icon that appears when i hover on that field/heading so i can hover
/// on it to read what each one requires". A field's explanation is no longer a line under it;
/// errors and validation messages stay inline.
pub(super) fn info_hint(
    t: &RemoteTokens,
    id: impl Into<ElementId>,
    group: SharedString,
    text: impl Into<SharedString>,
) -> AnyElement {
    click_shield(
        info_icon(t, id, text)
            .opacity(0.0)
            .group_hover(group, |this| this.opacity(1.0)),
    )
}

/// The (i) itself, always shown: the 14px info-circle with the explanation as Settings' wrapping
/// tooltip. The Relay picker puts one inside each option; there the caller shields its clicks.
pub(super) fn info_icon(
    t: &RemoteTokens,
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .tooltip(wrapped_tooltip_text(text))
        .child(settings_icon(
            "modals/settings/info-circle.svg",
            14.0,
            t.muted,
        ))
}

/// A dialog field's label, with its (i) right after it when it has an explanation.
pub(super) fn dialog_label(
    t: &RemoteTokens,
    label: &'static str,
    info: Option<SharedString>,
    group: SharedString,
) -> AnyElement {
    h_flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(19.25))
                .text_color(hsla(t.muted))
                .child(label),
        )
        .children(info.map(|text| info_hint(t, format!("remote-dialog-info-{label}"), group, text)))
        .into_any_element()
}

/// A switch row of a Settings > Remote dialog: its title (with the (i) that holds the
/// explanation), the app's switch on the right, and the whole row toggles it (`SwitchRowExt` in
/// native_modal_kit/controls.rs).
pub(super) fn dialog_switch_row(
    t: &RemoteTokens,
    id: &'static str,
    label: &'static str,
    info: &'static str,
    value: bool,
    toggle: impl Fn(&mut RemoteTab) + 'static,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let p = t.p.nested_dialog();
    let group = SharedString::from(format!("{id}-group"));
    h_flex()
        .id(id)
        .group(group.clone())
        .role(gpui::Role::Switch)
        .aria_toggled(a11y_toggled(value))
        .aria_label(label)
        .w_full()
        .items_center()
        .gap(px(12.0))
        .switch_row_press(
            switch_row_wash(p.foreground),
            false,
            bind_press(cx, move |tab, _window, cx| {
                toggle(tab);
                cx.notify();
            }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(dialog_label(t, label, Some(info.into()), group)),
        )
        .child(
            div()
                .flex_shrink_0()
                .child(settings_switch(&p, value, false)),
        )
        .into_any_element()
}

/// The window-wide black/65 backdrop a nested Settings dialog lays over the rail, the search and
/// the page (`[data-slot='dialog-overlay']`), with `sheet` centered on it. A press on the backdrop
/// runs `on_dismiss`, as Base UI closes a dialog on an outside press.
pub(super) fn dialog_overlay<V: 'static>(
    id: &'static str,
    sheet: AnyElement,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let viewport = window.viewport_size();
    gpui::deferred(
        gpui::anchored()
            .position_mode(gpui::AnchoredPositionMode::Window)
            .position(gpui::point(px(0.0), px(0.0)))
            .child(
                div()
                    .id(id)
                    .occlude()
                    .w(viewport.width)
                    .h(viewport.height)
                    .bg(hsla(modal_rgba(0x000000, 0.65)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |page, _: &gpui::MouseDownEvent, window, cx| {
                            on_dismiss(page, window, cx);
                        }),
                    )
                    .child(sheet),
            ),
    )
    .with_priority(2)
    .into_any_element()
}

/// One of the page's bottom rows (This computer, Tailscale, Advanced): its title, one line under it
/// and a chevron; the whole row opens or closes what it holds in place.
pub(super) fn fold_row<V: 'static>(
    t: &RemoteTokens,
    id: &'static str,
    title: &'static str,
    line: String,
    open: bool,
    on_press: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let wash = switch_row_wash(t.foreground);
    h_flex()
        .id(SharedString::from(format!("remote-fold-{id}")))
        .role(gpui::Role::Button)
        .aria_label(title)
        .aria_expanded(open)
        .w_full()
        .min_w_0()
        .min_h(px(52.0))
        .px(px(12.0))
        .py(px(8.0))
        .gap(px(12.0))
        .items_center()
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(wash)))
        .on_press(cx, on_press)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(t.foreground))
                        .child(title),
                )
                .child(detail(t, line, false)),
        )
        .child(settings_icon(
            if open {
                ICON_FOLD_OPEN
            } else {
                ICON_FOLD_CLOSED
            },
            16.0,
            t.muted,
        ))
        .into_any_element()
}

const ICON_FOLD_CLOSED: &str = "modals/settings/chevron-right.svg";
const ICON_FOLD_OPEN: &str = "modals/settings/chevron-down.svg";
