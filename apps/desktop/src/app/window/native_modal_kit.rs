//! The shared look and controls of the native GPUI app modals: the
//! `.gx-app-modal` design language from packages/core-ui/styles/modals.css
//! (window surface, section panels, raised controls, hairlines, 12px sections,
//! 8px controls, one 32px control height, full-width paired footer buttons)
//! plus the shadcn switch, select and input skins the React modals use.
//!
//! CDXC:AppModal 2026-09-15 DECISION:
//! User: the React app modals are being rebuilt in native GPUI one at a time, each one matching its React twin 1 to 1 in both appearances. Every native modal draws its chrome and controls from this one module, the same rule the React shell enforces through `AppModalShell`, so restyling the design language stays a single edit.
//! SEE-ALSO: packages/core-ui/app-modal-shell.tsx and the `.gx-app-modal` rules in packages/core-ui/styles/modals.css and modals-light.css (the tokens mirrored here), apps/desktop/src/app/native_app_modal_lifecycle.rs (window open, fit, close).
//!
//! This module depends only on gpui and gpui-component so the preview binaries can include it with `#[path]`.
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ClickEvent, Context, Div, FocusHandle,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Pixels, Render, Rgba, ScrollHandle, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Transformation, Window, anchored, deferred, div,
    point, px, radians, rgb, size, svg,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

pub(crate) const MODAL_WINDOW_PADDING: f32 = 24.0;
pub(crate) const MODAL_SECTION_GAP: f32 = 20.0;
pub(crate) const MODAL_CONTROL_HEIGHT: f32 = 32.0;
pub(crate) const MODAL_FOOTER_BUTTON_HEIGHT: f32 = 32.0;
pub(crate) const MODAL_RADIUS_CONTROL: f32 = 8.0;
pub(crate) const MODAL_RADIUS_SECTION: f32 = 12.0;
/// The React shell falls back to `system-ui`; gpui resolves this name to the platform UI font.
pub(crate) const MODAL_UI_FONT: &str = ".SystemUIFont";
pub(crate) const MODAL_MONO_FONT: &str = if cfg!(target_os = "macos") {
    ".AppleSystemUIFontMonospaced"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "monospace"
};
/// Header, body, and footer are 20px apart inside 24px window padding; the
/// footer is one 32px button row. Added to the measured header and body heights.
pub(crate) const MODAL_FIT_EXTRA_HEIGHT: f32 =
    MODAL_WINDOW_PADDING * 2.0 + MODAL_SECTION_GAP * 2.0 + MODAL_FOOTER_BUTTON_HEIGHT;

const ICON_SELECTOR: &str = "modals/kit/selector.svg";
const ICON_LOADER: &str = "modals/kit/loader-2.svg";

/// The `.gx-app-modal` tokens plus the shadcn theme tokens the modals read,
/// resolved for one appearance. Dark values come from modals.css and
/// shadcn.css, light values from modals-light.css.
#[derive(Clone, Copy)]
pub(crate) struct ModalPalette {
    pub(crate) surface: Rgba,
    pub(crate) panel: Rgba,
    pub(crate) raised: Rgba,
    pub(crate) raised_hover: Rgba,
    pub(crate) hairline: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    /// `--background`: the switch thumb color.
    pub(crate) background: Rgba,
    pub(crate) primary: Rgba,
    pub(crate) primary_foreground: Rgba,
    /// `bg-input/90`: the unchecked switch track.
    pub(crate) switch_off: Rgba,
    pub(crate) focus_border: Rgba,
    pub(crate) destructive: Rgba,
    pub(crate) success: Rgba,
    pub(crate) accent: Rgba,
    pub(crate) menu_background: Rgba,
    pub(crate) menu_border: Rgba,
    /// The dialog surface as an opaque colour. Equal to `surface`, except under window glass
    /// (`frosted`), where `surface` is thinned and the dropdowns drawn inside the modal's window,
    /// which nothing blurs, stay solid on this.
    pub(crate) solid_surface: Rgba,
    /// The modal's window blurs what is behind it (`frosted`).
    pub(crate) glass: bool,
    pub(crate) light: bool,
}

pub(crate) fn modal_rgba(hex: u32, alpha: f32) -> Rgba {
    let mut color = rgb(hex);
    color.a = alpha;
    color
}

pub(crate) fn rgba_of(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}

/// CSS `color-mix(in srgb, a <weight_a>, b)`: premultiplied interpolation.
pub(crate) fn css_mix(a: Rgba, weight_a: f32, b: Rgba) -> Rgba {
    let weight_b = 1.0 - weight_a;
    let alpha = a.a * weight_a + b.a * weight_b;
    if alpha <= 0.0 {
        return Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let channel = |ca: f32, cb: f32| (ca * a.a * weight_a + cb * b.a * weight_b) / alpha;
    Rgba {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: alpha,
    }
}

pub(crate) fn hsla(color: Rgba) -> Hsla {
    color.into()
}

pub(crate) fn transparent() -> Hsla {
    gpui::transparent_black()
}

pub(crate) fn modal_icon(path: &'static str, icon_size: f32, color: Rgba) -> gpui::Svg {
    svg().path(path).size(px(icon_size)).text_color(hsla(color))
}

/// The `--app-foreground`, `--app-muted` and `--app-background` triple of each
/// dark sidebar theme in packages/core-ui/styles/theme.css.
pub(crate) fn dark_theme_text_colors(sidebar_theme: Option<&str>) -> (u32, u32, u32) {
    match sidebar_theme {
        Some("dark-1") => (0xc8cdd5, 0x747b85, 0x191919),
        Some("dark-green") => (0xd8e3db, 0x8ea196, 0x0b120d),
        Some("dark-blue") => (0xdce6f8, 0x90a0b8, 0x0c1117),
        Some("dark-red") => (0xf1dde1, 0xb6939b, 0x140c0e),
        Some("dark-pink") => (0xf4deeb, 0xb99bad, 0x160d13),
        Some("dark-orange") => (0xf0dfcf, 0xbaa08c, 0x171008),
        _ => (0xc8cdd5, 0x747b85, 0x0e0e0e),
    }
}

impl ModalPalette {
    /// CDXC:Theming 2026-09-23 SEE-ALSO:
    /// The app tints these surfaces from the theme's chrome colour (`tinted`), like the
    /// `.gx-app-modal` tokens in packages/core-ui/styles/modals.css, which hold the user's decision.
    pub(crate) fn tinted(mut self, chrome: Rgba) -> Self {
        let ink = if self.light { 0x000000 } else { 0xffffff };
        let step = |amount: f32| css_mix(rgb(ink), amount, chrome);
        self.surface = chrome;
        self.solid_surface = chrome;
        self.panel = step(0.03);
        if self.light {
            self.raised = step(0.05);
            self.raised_hover = step(0.09);
        } else {
            self.raised = step(0.06);
            self.raised_hover = step(0.085);
        }
        self
    }

    /// CDXC:Theming 2026-09-26 DECISION:
    /// User, of the Rename Session dialog drawn as an opaque dark box over the glass window: "please make all these kinds of GPUI modals in the app match the glass look when transparency is enabled". Under window glass every native app modal is a frosted surface like the menus: its window blurs what is behind it, `fill` (the app's frosted menu fill of `surface`) replaces the solid surface, and the panels, raised controls and hovers become ink washes at the same steps `tinted` mixes into the chrome, so they read as lighter panes on the glass instead of solid slabs. Glass off, nothing changes.
    pub(crate) fn frosted(mut self, fill: Rgba) -> Self {
        let ink = if self.light { 0x000000 } else { 0xffffff };
        self.surface = fill;
        self.panel = modal_rgba(ink, 0.03);
        if self.light {
            self.raised = modal_rgba(ink, 0.05);
            self.raised_hover = modal_rgba(ink, 0.09);
            self.accent = modal_rgba(ink, 0.08);
        } else {
            self.raised = modal_rgba(ink, 0.06);
            self.raised_hover = modal_rgba(ink, 0.085);
            self.accent = modal_rgba(ink, 0.09);
        }
        self.glass = true;
        self
    }

    pub(crate) fn resolve(light: bool, sidebar_theme: Option<&str>) -> Self {
        if light {
            Self {
                surface: rgb(0xffffff),
                panel: rgb(0xf5f5f5),
                raised: rgb(0xf0f0f0),
                raised_hover: rgb(0xe5e5e5),
                hairline: modal_rgba(0x000000, 0.14),
                foreground: rgb(0x262626),
                muted: rgb(0x626262),
                background: rgb(0xffffff),
                primary: rgb(0x262626),
                primary_foreground: rgb(0xffffff),
                switch_off: modal_rgba(0x000000, 0.16 * 0.9),
                focus_border: rgb(0x525252),
                destructive: rgb(0xb91c1c),
                success: rgb(0x15803d),
                accent: rgb(0xe9e9e9),
                menu_background: rgb(0xffffff),
                menu_border: modal_rgba(0x000000, 0.16),
                solid_surface: rgb(0xffffff),
                glass: false,
                light: true,
            }
        } else {
            let (foreground, muted, background) = dark_theme_text_colors(sidebar_theme);
            Self {
                surface: rgb(0x0e0e0e),
                panel: rgb(0x161616),
                raised: rgb(0x1d1d1d),
                raised_hover: rgb(0x232323),
                hairline: modal_rgba(0xffffff, 0.08),
                foreground: rgb(foreground),
                muted: rgb(muted),
                background: rgb(background),
                primary: rgb(0xe5e5e5),
                primary_foreground: rgb(0x171717),
                switch_off: modal_rgba(0xffffff, 0.15 * 0.9),
                focus_border: rgb(0xffffff),
                destructive: rgb(0xf87171),
                success: rgb(0x4ade80),
                accent: rgb(0x262626),
                menu_background: rgb(0x161616),
                menu_border: modal_rgba(0xffffff, 0.08),
                solid_surface: rgb(0x0e0e0e),
                glass: false,
                light: false,
            }
        }
    }

    /// `color-mix(in srgb, var(--foreground) 6%, var(--gx-modal-raised))`.
    pub(crate) fn card_selected_background(&self) -> Rgba {
        css_mix(self.foreground, 0.06, self.raised)
    }

    /// `color-mix(in srgb, var(--foreground) 45%, var(--gx-modal-hairline))`.
    pub(crate) fn card_selected_border(&self) -> Rgba {
        css_mix(self.foreground, 0.45, self.hairline)
    }

    /// `color-mix(in srgb, var(--foreground) 10%, transparent)`: the icon chips.
    pub(crate) fn chip_background(&self) -> Rgba {
        rgba_of(self.foreground, 0.10)
    }

    /// The selected select row: `color-mix(in srgb, var(--popover-foreground) 12%, transparent)`.
    pub(crate) fn menu_selected_background(&self) -> Rgba {
        rgba_of(self.foreground, 0.12)
    }

    /// `color-mix(in srgb, var(--primary) 88%, white)`.
    pub(crate) fn primary_hover(&self) -> Rgba {
        css_mix(self.primary, 0.88, rgb(0xffffff))
    }

    /// The Windows modal window's system border: 16% ink over the surface, so it stays visible against the same-coloured app chrome.
    pub(crate) fn window_border(&self) -> Rgba {
        let ink = if self.light { 0x000000 } else { 0xffffff };
        css_mix(rgb(ink), 0.16, self.solid_surface)
    }
}

/// Dialog title (16px/400, line-height 1.3) and description (13px muted, line-height 1.55), 6px apart.
pub(crate) fn modal_header(
    p: &ModalPalette,
    title: impl Into<SharedString>,
    description: Option<impl Into<SharedString>>,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(20.8))
                .child(title.into()),
        )
        .children(description.map(|description| {
            div()
                .text_size(px(13.0))
                .line_height(px(20.15))
                .text_color(hsla(p.muted))
                .child(description.into())
        }))
        .into_any_element()
}

/// `[data-slot='field-label']` and the export dialog's section titles: 12px/500 muted.
pub(crate) fn modal_section_title(p: &ModalPalette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.0))
        .line_height(px(17.14))
        .font_weight(FontWeight::MEDIUM)
        .text_color(hsla(p.muted))
        .child(text.into())
}

/// 12px muted helper copy with the 1.45 line height the modals use for hints.
pub(crate) fn modal_hint(p: &ModalPalette, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.0))
        .line_height(px(17.4))
        .text_color(hsla(p.muted))
        .child(text.into())
}

/// `.export-transcript-error` and friends: 12px destructive copy, line-height 1.5.
pub(crate) fn modal_error(p: &ModalPalette, text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(12.0))
        .line_height(px(18.0))
        .text_color(hsla(p.destructive))
        .child(text.into())
        .into_any_element()
}

/// A section panel (`--gx-modal-panel`, hairline border, 12px radius), contents supplied by the caller.
pub(crate) fn modal_panel(p: &ModalPalette) -> Div {
    v_flex()
        .w_full()
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.panel))
        .overflow_hidden()
}

/// A raised control box (`--gx-modal-raised`, hairline border, 8px radius).
pub(crate) fn modal_raised_box(p: &ModalPalette) -> Div {
    div()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.raised))
}

/// The row inside a panel list: 9px 12px padding, label 13px/500 and 12px muted description, control on the right.
pub(crate) fn modal_panel_row(
    p: &ModalPalette,
    label: impl Into<SharedString>,
    description: Option<impl Into<SharedString>>,
    control: AnyElement,
    divider: bool,
) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(12.0))
        .py(px(9.0))
        .when(divider, |this| {
            this.border_t_1().border_color(hsla(p.hairline))
        })
        .child(
            v_flex()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.into()),
                )
                .children(description.map(|description| {
                    div()
                        .text_size(px(12.0))
                        .line_height(px(17.14))
                        .text_color(hsla(p.muted))
                        .child(description.into())
                })),
        )
        .child(control)
}

/// The app-wide toggle: 32x20 track with a 2px border and 6px radius, 16px thumb with 4px radius.
pub(crate) fn modal_switch(p: &ModalPalette, checked: bool, disabled: bool) -> AnyElement {
    div()
        .flex_shrink_0()
        .w(px(32.0))
        .h(px(20.0))
        .rounded(px(6.0))
        .border_2()
        .border_color(if checked {
            hsla(p.primary)
        } else {
            transparent()
        })
        .bg(hsla(if checked { p.primary } else { p.switch_off }))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div()
                .size(px(16.0))
                .ml(px(if checked { 12.0 } else { 0.0 }))
                .rounded(px(4.0))
                .bg(hsla(p.background))
                .shadow_sm(),
        )
        .into_any_element()
}

/// A rotating loader icon for busy primary buttons.
pub(crate) fn modal_spinner(color: Rgba) -> AnyElement {
    modal_icon(ICON_LOADER, 15.0, color)
        .with_animation(
            "native-modal-spinner",
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalButtonTone {
    Neutral,
    Primary,
    Danger,
}

/// A footer pill button (`.gx-app-modal-action-button`): 32px tall, 8px radius,
/// outline by default, filled for the primary tone, destructive-tinted outline
/// for the danger tone, half opacity when disabled.
pub(crate) fn modal_action_button<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    label: impl Into<SharedString>,
    leading: Option<AnyElement>,
    tone: ModalButtonTone,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let (background, border, text, hover) = match tone {
        ModalButtonTone::Neutral => (transparent(), p.hairline, p.foreground, p.raised_hover),
        ModalButtonTone::Primary => (
            hsla(p.primary),
            p.primary,
            p.primary_foreground,
            p.primary_hover(),
        ),
        // `.gx-app-modal-action-danger`: an outline tint, never a fill.
        ModalButtonTone::Danger => (
            transparent(),
            rgba_of(p.destructive, 0.45),
            p.destructive,
            rgba_of(p.destructive, 0.12),
        ),
    };
    h_flex()
        .id(id)
        .flex_1()
        .flex_basis(px(0.0))
        .min_w_0()
        .h(px(MODAL_FOOTER_BUTTON_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(background)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_click(this, window, cx);
                }))
        })
        .children(leading)
        .child(label.into())
        .into_any_element()
}

/// `.gx-app-modal-footer`: equal-width buttons filling the modal width, 8px apart.
pub(crate) fn modal_footer(buttons: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .w_full()
        .gap(px(8.0))
        .children(buttons)
        .into_any_element()
}

/// A 32px ghost icon button (shadcn `variant='ghost' size='icon'` inside the modal skin).
pub(crate) fn modal_icon_button<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    icon_path: &'static str,
    icon_size: f32,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let accent = p.accent;
    div()
        .id(id)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(32.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(accent)))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            on_click(this, window, cx);
        }))
        .child(modal_icon(icon_path, icon_size, p.foreground))
}

/// Captures one child's bounds from a prepaint pass, for anchoring popovers.
pub(crate) fn capture_child_bounds(
    cell: Rc<Cell<Option<Bounds<Pixels>>>>,
    index: usize,
) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
    move |bounds, _window, _cx| cell.set(bounds.get(index).copied())
}

/// The shadcn select, drawn in-window: a raised 32px trigger and an anchored
/// popover with 28px rows (selected row filled at 12% foreground, hovered or
/// keyboard-highlighted row on the accent). Owned by the modal entity.
pub(crate) struct ModalSelect {
    pub(crate) open: bool,
    pub(crate) highlight: Option<usize>,
    pub(crate) trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: ScrollHandle,
}

pub(crate) enum ModalSelectKey {
    /// The key moved the highlight or closed the menu.
    Consumed,
    /// Enter chose the highlighted row.
    Choose(usize),
    /// The menu is closed, so the key is the modal's to handle.
    Ignored,
}

impl ModalSelect {
    pub(crate) fn new() -> Self {
        Self {
            open: false,
            highlight: None,
            trigger_bounds: Rc::new(Cell::new(None)),
            scroll: ScrollHandle::new(),
        }
    }

    pub(crate) fn toggle(&mut self, selected: Option<usize>) {
        self.open = !self.open;
        self.highlight = self.open.then(|| selected.unwrap_or(0));
        if let Some(index) = self.highlight {
            self.scroll.scroll_to_item(index);
        }
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn handle_key(&mut self, key: &str, count: usize) -> ModalSelectKey {
        if !self.open {
            return ModalSelectKey::Ignored;
        }
        match key {
            "escape" => {
                self.open = false;
                ModalSelectKey::Consumed
            }
            "up" | "down" if count > 0 => {
                let delta: isize = if key == "up" { -1 } else { 1 };
                let current = self.highlight.unwrap_or(0) as isize;
                self.highlight = Some((current + delta).rem_euclid(count as isize) as usize);
                self.scroll.scroll_to_item(self.highlight.unwrap());
                ModalSelectKey::Consumed
            }
            "enter" => {
                self.open = false;
                match self.highlight {
                    Some(index) => ModalSelectKey::Choose(index),
                    None => ModalSelectKey::Consumed,
                }
            }
            _ => ModalSelectKey::Consumed,
        }
    }
}

pub(crate) fn modal_select_trigger<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    value: Option<String>,
    placeholder: &'static str,
    disabled: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = select.open;
    h_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_between()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if open { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .text_size(px(14.0))
        .line_height(px(20.0))
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .when(!open, |this| {
                    this.hover(move |this| this.bg(hsla(p.raised_hover)))
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_toggle(this, window, cx);
                }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(value.is_none(), |this| this.text_color(hsla(p.muted)))
                .child(value.unwrap_or_else(|| placeholder.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .child(modal_icon(ICON_SELECTOR, 16.0, p.muted)),
        )
        .into_any_element()
}

/// The open select popover, rendered at the modal root so it floats above everything.
pub(crate) fn modal_select_menu<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    items: &[String],
    selected: Option<usize>,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !select.open {
        return None;
    }
    let trigger = select.trigger_bounds.get()?;
    let p = *p;
    let highlight = select.highlight;
    // CDXC:AppModal 2026-09-16 WHY:
    // Anchoring only repositions the popup; it cannot make a long session list fit or scroll. Bound the list to the window and keep keyboard highlights in the same scroll container.
    let max_height = px(288.0).min((window.viewport_size().height - px(16.0)).max(px(0.0)));
    let position = point(
        trigger.origin.x,
        trigger.origin.y + trigger.size.height + px(4.0),
    );
    let rows = items.iter().enumerate().map(|(index, item)| {
        let is_selected = selected == Some(index);
        let highlighted = highlight == Some(index);
        let on_choose = on_choose.clone();
        h_flex()
            .id((id, index))
            .w_full()
            .flex_shrink_0()
            .min_h(px(28.0))
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_default()
            .when(is_selected, |this| {
                this.bg(hsla(p.menu_selected_background()))
            })
            .when(!is_selected && highlighted, |this| this.bg(hsla(p.accent)))
            .when(!is_selected, |this| {
                this.hover(move |this| this.bg(hsla(p.accent)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_choose(this, index, window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(item.clone()),
            )
    });
    Some(
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    v_flex()
                        .id(id)
                        .occlude()
                        .w(trigger.size.width)
                        .max_h(max_height)
                        .overflow_y_scroll()
                        .track_scroll(&select.scroll)
                        .p(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.menu_border))
                        .bg(hsla(p.menu_background))
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                // A mouse-down on the trigger is the trigger's own toggle-close.
                                if trigger.contains(&event.position) {
                                    return;
                                }
                                on_dismiss(this, window, cx);
                            },
                        ))
                        .children(rows),
                ),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// The shadcn input skin the modals use: raised background, hairline border
/// (white or #525252 when focused), 8px radius, 32px tall, 14px text. Wraps a
/// gpui-component `Input`, whose window root must be a gpui-component `Root`.
pub(crate) fn modal_text_input(
    p: &ModalPalette,
    state: &gpui::Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .when(disabled, |this| this.opacity(0.5))
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

/// The shadcn textarea skin: the input skin with 12px vertical padding.
/// `min_height` is the React `min-height`; `None` makes the editor fill the
/// remaining column height (the fixed-frame editors). The `InputState` must be
/// created with `multi_line(true)`.
pub(crate) fn modal_text_area(
    p: &ModalPalette,
    state: &gpui::Entity<InputState>,
    min_height: Option<f32>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .when_some(min_height, |this, min_height| this.min_h(px(min_height)))
        .when(min_height.is_none(), |this| this.flex_1().min_h_0())
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { p.focus_border } else { p.hairline }))
        .bg(hsla(p.raised))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div()
                .w_full()
                .min_w_0()
                .when(min_height.is_none(), |this| this.flex_1().min_h_0())
                .child(
                    Input::new(state)
                        .with_size(ComponentSize::Small)
                        .appearance(false)
                        .bordered(false)
                        .focus_bordered(false)
                        .disabled(disabled)
                        .w_full()
                        .when(min_height.is_none(), |this| this.h_full())
                        .px(px(0.0))
                        .py(px(0.0))
                        .text_size(px(14.0))
                        .text_color(hsla(p.foreground)),
                ),
        )
        .into_any_element()
}

/// Sizes the child window to the dialog's own layout.
///
/// The window opens at a per-modal first-frame estimate. The first prepaint
/// knows the real header and body heights, so the window is resized to fit
/// them exactly; later prepaints only grow it (a validation or failure line
/// appearing) and never shrink it, so a shorter follow-up state keeps the frame
/// the opening layout established, like the React fit-height pass did.
pub(crate) struct ModalFit {
    requested: Rc<Cell<Option<f32>>>,
    /// A fixed-frame modal (the editors that fill their window) never resizes.
    fixed: bool,
}

impl ModalFit {
    pub(crate) fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            fixed: false,
        }
    }

    /// For modals whose React twin is pinned to `100vh`: the window keeps the
    /// size it opened at and the body stretches to fill it.
    pub(crate) fn fixed() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            fixed: true,
        }
    }

    /// A prepaint listener for the column that holds the header and body.
    pub(crate) fn listener(
        &self,
        extra_height: f32,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let requested = self.requested.clone();
        let fixed = self.fixed;
        move |bounds, window, cx| {
            if fixed || bounds.is_empty() {
                return;
            }
            let content: f32 = bounds
                .iter()
                .map(|bounds| f32::from(bounds.size.height))
                .sum();
            let needed = (content + extra_height).round();
            let current = f32::from(window.viewport_size().height).round();
            let first = requested.get().is_none();
            if !first && needed <= current {
                return;
            }
            if (needed - current).abs() < 1.0 || requested.get() == Some(needed) {
                if first {
                    requested.set(Some(needed));
                }
                return;
            }
            requested.set(Some(needed));
            let handle = window.window_handle();
            let width = window.viewport_size().width;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_root, window, _cx| {
                    window.resize(size(width, px(needed)));
                });
            });
        }
    }
}

/// The dialog frame: the window surface, the platform UI font, 24px padding,
/// header and body 20px apart, the footer pinned to the bottom, keyboard focus
/// on the frame so Escape and Enter reach the modal's key handler.
pub(crate) fn modal_shell<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    content: Vec<AnyElement>,
    footer: AnyElement,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(MODAL_WINDOW_PADDING))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(MODAL_FIT_EXTRA_HEIGHT))
                        .children(content),
                )
                .child(footer),
        )
        .children(overlay)
}

/// `color-mix(in srgb, color <weight>, transparent)`: the color at `weight` of its alpha.
pub(crate) fn css_fade(color: Rgba, weight: f32) -> Rgba {
    css_mix(
        color,
        weight,
        Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    )
}

/// The `.ghostex-settings-shadcn command-config-modal-shadcn` skin of the
/// legacy dialogs (Delete Worktree, Rename Worktree, Configure action): the
/// shadcn tokens as the modal host resolves them for that class. Dark values
/// come from `.ghostex-settings-shadcn` in packages/core-ui/styles/shadcn.css,
/// re-tokened by the later `.ghostex-settings-shadcn[data-sidebar-theme]`
/// block in packages/core-ui/styles.css (`--background`, `--border` and
/// `--input` become the settings surface and hairline) and the per-theme
/// `--foreground` / `--muted-foreground` in styles/theme.css; light values
/// come from the `.ghostex-settings-shadcn` block in styles/modals-light.css.
#[derive(Clone, Copy)]
pub(crate) struct ModalLegacyPalette {
    /// `--app-modal-background`: the child window around the dialog.
    pub(crate) window: Rgba,
    /// `--settings-surface`, which is `--background` under this skin: the dialog itself.
    pub(crate) surface: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    pub(crate) primary: Rgba,
    pub(crate) primary_foreground: Rgba,
    pub(crate) border: Rgba,
    pub(crate) input: Rgba,
    /// `--settings-raised`: the text input fill (at 82%).
    pub(crate) raised: Rgba,
    /// `--settings-focus-border-color` (dark) or the light `#525252` focus rule.
    pub(crate) focus_border: Rgba,
    pub(crate) destructive: Rgba,
    /// `--muted`: the outline button's hover fill in light mode.
    pub(crate) muted_fill: Rgba,
    /// `--settings-raised-hover`: the default button's hover fill.
    pub(crate) raised_hover: Rgba,
    /// The modal's window is frosted (`ModalPalette::frosted`).
    pub(crate) glass: bool,
    pub(crate) light: bool,
}

impl ModalLegacyPalette {
    pub(crate) fn resolve(p: &ModalPalette) -> Self {
        let lp = if p.light {
            Self {
                window: p.surface,
                surface: p.surface,
                foreground: p.foreground,
                muted: p.muted,
                primary: p.primary,
                primary_foreground: p.primary_foreground,
                border: p.hairline,
                input: modal_rgba(0x000000, 0.16),
                raised: rgb(0xf0f0f0),
                focus_border: rgb(0x525252),
                destructive: p.destructive,
                muted_fill: rgb(0xf1f1f1),
                raised_hover: p.raised_hover,
                glass: false,
                light: true,
            }
        } else {
            // styles/theme.css publishes `--app-modal-background` as #191919 for
            // dark-1 and #0e0e0e for every other dark theme; the palette's
            // `background` is `--app-background`, which only coincides with it
            // for dark-1 and dark-2, so the colored dark themes keep the #0e0e0e window.
            let window = if p.background == rgb(0x191919) {
                p.background
            } else {
                p.surface
            };
            Self {
                window,
                surface: p.surface,
                foreground: p.foreground,
                muted: p.muted,
                primary: p.primary,
                primary_foreground: p.primary_foreground,
                border: p.hairline,
                input: p.hairline,
                raised: rgb(0x161616),
                // `color-mix(in srgb, var(--foreground) 58%, var(--border) 42%)`.
                focus_border: css_mix(p.foreground, 0.58, p.hairline),
                // shadcn's dark `--destructive: oklch(0.704 0.191 22.216)`, Tailwind red-400.
                destructive: rgb(0xff6467),
                muted_fill: p.raised,
                raised_hover: p.raised,
                glass: false,
                light: false,
            }
        };
        if !p.glass {
            return lp;
        }
        // Under window glass the window's frosted fill is the only layer: the dialog drawn on it
        // is clear, and its fixed raised tones become the kit's ink washes.
        Self {
            window: p.surface,
            surface: css_fade(p.surface, 0.0),
            raised: p.raised,
            muted_fill: p.raised,
            glass: true,
            ..lp
        }
    }

    /// `color-mix(in srgb, var(--foreground) <weight>, var(--muted-foreground))`: the skin's text blends.
    pub(crate) fn text(&self, foreground_weight: f32) -> Rgba {
        css_mix(self.foreground, foreground_weight, self.muted)
    }

    /// `color-mix(in srgb, var(--border) 82%, transparent)`: the header and footer dividers.
    pub(crate) fn divider(&self) -> Rgba {
        css_fade(self.border, 0.82)
    }
}

/// The legacy skin's square checkbox (`.worktree-*-branch-checkbox`): 16px,
/// no radius, `--input` border over a 30% `--input` fill; `--primary` fill with
/// a `--primary-foreground` tick when checked; half opacity when disabled.
pub(crate) fn modal_square_checkbox(
    lp: &ModalLegacyPalette,
    checked: bool,
    disabled: bool,
) -> AnyElement {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .size(px(16.0))
        .border_1()
        .border_color(hsla(if checked { lp.primary } else { lp.input }))
        .bg(hsla(if checked {
            lp.primary
        } else {
            css_fade(lp.input, 0.30)
        }))
        .when(disabled, |this| this.opacity(0.5))
        .when(checked, |this| {
            this.child(modal_icon(
                "modals/kit/legacy-check.svg",
                16.0,
                lp.primary_foreground,
            ))
        })
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalLegacyButtonTone {
    /// shadcn `variant='outline'`.
    Outline,
    /// shadcn `variant='default'`, which `.ghostex-settings-shadcn button[data-variant='default']`
    /// in packages/core-ui/styles.css turns into the quiet bordered button (no white buttons in
    /// this skin): transparent, hairline border, raised-hover fill on hover.
    Primary,
    /// shadcn `variant='destructive'`: the tinted destructive fill.
    Destructive,
}

/// A shadcn `Button` inside the legacy skin: 32px tall, 8px radius, 12px
/// side padding, 14px label at the skin's flat 400 weight (`.ghostex-settings-shadcn
/// .ghostex-settings-shadcn * { font-weight: 400 }`), `min-width: 112px` as the
/// worktree dialogs' footer buttons set it, content-sized and right-aligned by the footer.
pub(crate) fn modal_legacy_action_button<V: 'static>(
    lp: &ModalLegacyPalette,
    id: &'static str,
    label: impl Into<SharedString>,
    tone: ModalLegacyButtonTone,
    disabled: bool,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let lp = *lp;
    let (background, border, text, hover) = match tone {
        // `border-border bg-background hover:bg-muted dark:bg-transparent dark:hover:bg-input/30`.
        ModalLegacyButtonTone::Outline => (
            if lp.light {
                hsla(lp.surface)
            } else {
                transparent()
            },
            hsla(lp.border),
            lp.foreground,
            if lp.light {
                lp.muted_fill
            } else {
                css_fade(lp.input, 0.30)
            },
        ),
        // `background: transparent; border-color: var(--settings-hairline); color: var(--foreground)`,
        // `:hover { background: var(--settings-raised-hover) }`.
        ModalLegacyButtonTone::Primary => (
            transparent(),
            hsla(lp.border),
            lp.foreground,
            lp.raised_hover,
        ),
        // `bg-destructive/10 hover:bg-destructive/20` in light, `/20` and `/30` in dark.
        ModalLegacyButtonTone::Destructive => (
            hsla(css_fade(lp.destructive, if lp.light { 0.10 } else { 0.20 })),
            transparent(),
            lp.destructive,
            css_fade(lp.destructive, if lp.light { 0.20 } else { 0.30 }),
        ),
    };
    h_flex()
        .id(id)
        .flex_shrink_0()
        .min_w(px(112.0))
        .h(px(MODAL_FOOTER_BUTTON_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(text))
        .whitespace_nowrap()
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_click(this, window, cx);
                }))
        })
        .child(label.into())
        .into_any_element()
}

/// The shadcn `Input` inside the legacy skin (`.ghostex-settings-shadcn
/// [data-slot='input']`): 32px tall, 8px radius, the settings raised tone at
/// 82% with a hairline border that turns into the focus border while focused.
pub(crate) fn modal_legacy_text_input(
    lp: &ModalLegacyPalette,
    state: &gpui::Entity<InputState>,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused { lp.focus_border } else { lp.border }))
        .bg(hsla(css_fade(lp.raised, 0.82)))
        .when(disabled, |this| this.opacity(0.5))
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
                    .text_color(hsla(lp.foreground)),
            ),
        )
        .into_any_element()
}

/// The legacy dialog frame (`.command-config-modal-shadcn` in a native child
/// window): the dialog is narrower than the window and sits centered at its
/// top on the `--app-modal-background` window color, a 12px-radius surface
/// with a window-colored 1px edge, a 20px-padded header over a hairline
/// divider, the caller's body, and a right-aligned footer (10px gaps, padding
/// 14px 20px 20px) under another divider. The window fits itself to the
/// dialog's measured height, nothing added.
pub(crate) fn modal_legacy_shell<V: Render>(
    lp: &ModalLegacyPalette,
    id: &'static str,
    dialog_width: f32,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    header: Div,
    body: AnyElement,
    footer_buttons: Vec<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let divider = hsla(lp.divider());
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(lp.window))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(lp.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .items_center()
                .on_children_prepainted(fit.listener(0.0))
                .child(
                    v_flex()
                        .w(px(dialog_width))
                        .flex_shrink_0()
                        .rounded(px(MODAL_RADIUS_SECTION))
                        .border_1()
                        .border_color(if lp.glass {
                            transparent()
                        } else {
                            hsla(lp.window)
                        })
                        .bg(hsla(lp.surface))
                        .overflow_hidden()
                        .child(
                            header
                                .w_full()
                                .p(px(20.0))
                                .border_b_1()
                                .border_color(divider),
                        )
                        .child(body)
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .justify_end()
                                .gap(px(10.0))
                                .pt(px(14.0))
                                .px(px(20.0))
                                .pb(px(20.0))
                                .border_t_1()
                                .border_color(divider)
                                .children(footer_buttons),
                        ),
                ),
        )
}

// ---------------------------------------------------------------------------
// Added with the Add Worktree modal: the raised segmented control, the
// searchable select popup, per-modal field skins, and an inset shell.
// ---------------------------------------------------------------------------

/// One segment of [`modal_segmented_control`]: an optional 16px leading icon and the label.
pub(crate) struct ModalSegmentedItem {
    pub(crate) icon: Option<&'static str>,
    pub(crate) label: &'static str,
}

/// `SegmentedControl variant='raised' stretch` (packages/components/ui/segmented-control.tsx
/// skinned by packages/components/ui/raised-tab-rail.css): a 32px inset track
/// (`light-dark(#ededed, #202020)`, 14% hairline, 8px radius, 3px padding and
/// gap) whose segments split the width at 13px/400 in `light-dark(#525252, #b8b8b8)`,
/// hover on `light-dark(#e3e3e3, #292929)`, and the pressed segment raised on
/// `light-dark(#ffffff, #363636)` with a 1px 8% ring plus a `0 1px 3px` shadow.
/// gpui svgs take no inherited color, so the icon is painted in the segment's
/// resting or pressed text color and does not follow the hover tint.
pub(crate) fn modal_segmented_control<V: 'static>(
    p: &ModalPalette,
    id: &'static str,
    items: &[ModalSegmentedItem],
    selected: usize,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let (track, track_border, text, hover_bg, active_text, pressed_bg, ring) = if p.light {
        (
            rgb(0xededed),
            modal_rgba(0x000000, 0.14),
            rgb(0x525252),
            rgb(0xe3e3e3),
            rgb(0x262626),
            rgb(0xffffff),
            modal_rgba(0x000000, 0.08),
        )
    } else {
        (
            rgb(0x202020),
            modal_rgba(0xffffff, 0.14),
            rgb(0xb8b8b8),
            rgb(0x292929),
            rgb(0xf5f5f5),
            rgb(0x363636),
            modal_rgba(0xffffff, 0.08),
        )
    };
    let segments = items.iter().enumerate().map(|(index, item)| {
        let pressed = index == selected;
        let on_select = on_select.clone();
        h_flex()
            .id((id, index))
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .h_full()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .px(px(10.0))
            .rounded(px(5.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .whitespace_nowrap()
            .text_color(hsla(if pressed { active_text } else { text }))
            .cursor_pointer()
            .when(pressed, |this| {
                this.bg(hsla(pressed_bg)).shadow(vec![
                    gpui::BoxShadow {
                        color: hsla(modal_rgba(0x000000, 0.14)),
                        offset: point(px(0.0), px(1.0)),
                        blur_radius: px(3.0),
                        spread_radius: px(0.0),
                        inset: false,
                    },
                    gpui::BoxShadow {
                        color: hsla(ring),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(1.0),
                        inset: false,
                    },
                ])
            })
            .when(!pressed, |this| {
                this.hover(move |this| this.bg(hsla(hover_bg)).text_color(hsla(active_text)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                on_select(this, index, window, cx);
            }))
            .children(item.icon.map(|icon| {
                modal_icon(icon, 16.0, if pressed { active_text } else { text }).flex_shrink_0()
            }))
            .child(item.label)
    });
    h_flex()
        .id(id)
        .w_full()
        .h(px(MODAL_CONTROL_HEIGHT))
        .items_stretch()
        .gap(px(3.0))
        .p(px(3.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(track_border))
        .bg(hsla(track))
        .overflow_hidden()
        .children(segments)
        .into_any_element()
}

/// The React searchable `Select` filter: the query's whitespace-separated,
/// case-insensitive terms must all occur in the row's search text. An empty
/// query matches everything.
pub(crate) fn modal_select_filter_matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|term| text.contains(&term.to_lowercase()))
}

/// A row of [`modal_searchable_select_menu`]. A disabled row is the
/// `<SelectItem disabled>` placeholder a React list renders while it is empty.
pub(crate) struct ModalSearchableSelectRow {
    pub(crate) label: String,
    pub(crate) disabled: bool,
}

/// The searchable shadcn Select popup (`packages/components/ui/select.tsx`
/// with `searchable-dropdown.css`): trigger-wide, 8px radius, `0 12px 28px`
/// shadow, a filter `InputGroup` (32px, 6px radius, `--input` border, ring
/// colored with a 3px 20% halo while focused, search icon at half opacity or a
/// clear button once a query is typed) above a 4px-padded list of 32px rows
/// (13px, 6px radius, the highlighted or hovered row on 8% foreground), and the
/// empty message (13px, 24px 10px, centered) when nothing survives the filter.
/// The popup paints `bg-popover`, which is the modal surface color (`#0e0e0e` / `#ffffff`).
/// `rows` are the rows that survive the caller's filter; `select.highlight`
/// indexes into them. The caller creates the filter `InputState`
/// (single line, with the React `searchPlaceholder`), clears and focuses it
/// when the menu opens, and resets the highlight on `InputEvent::Change`.
pub(crate) fn modal_searchable_select_menu<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    filter: &gpui::Entity<InputState>,
    rows: &[ModalSearchableSelectRow],
    empty_message: &'static str,
    on_choose: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + Clone + 'static,
    on_clear_filter: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    on_dismiss: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if !select.open {
        return None;
    }
    let trigger = select.trigger_bounds.get()?;
    let p = *p;
    let highlight = select.highlight;
    let position = point(
        trigger.origin.x,
        trigger.origin.y + trigger.size.height + px(4.0),
    );
    // `--ghostex-tooltip-border` (theme.css): the popup edge and the filter divider.
    let popup_border = if p.light {
        modal_rgba(0x000000, 0.14)
    } else {
        modal_rgba(0xffffff, 0.12)
    };
    // `--input` and `bg-input/30` (theme.css, modals-light.css). The fill is
    // composited over the popup surface because gpui paints the focus-ring
    // shadow behind the element, where a translucent fill would let it through.
    let (input_border, input_background) = if p.light {
        (
            modal_rgba(0x000000, 0.16),
            css_mix(rgb(0x000000), 0.16 * 0.3, p.solid_surface),
        )
    } else {
        (
            modal_rgba(0xffffff, 0.15),
            css_mix(rgb(0xffffff), 0.15 * 0.3, p.solid_surface),
        )
    };
    // `--ring`: oklch(55.6% 0 0) dark, #737373 light.
    let ring = rgb(0x737373);
    let filter_focused = filter.read(cx).focus_handle(cx).is_focused(window);
    let query = filter.read(cx).value().to_string();
    let row_highlight = rgba_of(p.foreground, 0.08);
    let list_rows = rows.iter().enumerate().map(|(index, row)| {
        let highlighted = highlight == Some(index);
        let disabled = row.disabled;
        let on_choose = on_choose.clone();
        h_flex()
            .id((id, index))
            .w_full()
            .min_h(px(32.0))
            .px(px(10.0))
            .py(px(6.0))
            .gap(px(8.0))
            .items_center()
            .rounded(px(6.0))
            .text_size(px(13.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .cursor_default()
            .when(disabled, |this| this.opacity(0.5))
            .when(highlighted, |this| this.bg(hsla(row_highlight)))
            .when(!disabled, |this| {
                this.hover(move |this| this.bg(hsla(row_highlight)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_choose(this, index, window, cx);
                    }))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(row.label.clone()),
            )
    });
    let filter_group = h_flex()
        .w_full()
        .h(px(32.0))
        .pl(px(10.0))
        .pr(px(12.0))
        .gap(px(8.0))
        .items_center()
        .rounded(px(6.0))
        .border_1()
        .border_color(hsla(if filter_focused { ring } else { input_border }))
        .bg(hsla(input_background))
        .when(filter_focused, |this| {
            this.shadow(vec![gpui::BoxShadow {
                color: hsla(rgba_of(ring, 0.2)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            }])
        })
        .child(
            div().flex_1().min_w_0().child(
                Input::new(filter)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(13.0))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .child(if query.is_empty() {
            div()
                .flex_shrink_0()
                .child(modal_icon(
                    "modals/kit/search.svg",
                    16.0,
                    rgba_of(p.muted, 0.5),
                ))
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("{id}-clear-filter")))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_clear_filter(this, window, cx);
                }))
                .child(modal_icon("modals/kit/x.svg", 16.0, p.muted))
                .into_any_element()
        });
    let mut popup = v_flex()
        .id(id)
        .occlude()
        .w(trigger.size.width)
        .overflow_hidden()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(popup_border))
        .bg(hsla(p.solid_surface))
        .text_color(hsla(p.foreground))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(modal_rgba(0x000000, 0.35)),
            offset: point(px(0.0), px(12.0)),
            blur_radius: px(28.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .on_mouse_down_out(
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if trigger.contains(&event.position) {
                    return;
                }
                on_dismiss(this, window, cx);
            }),
        )
        .child(
            div()
                .w_full()
                .p(px(8.0))
                .border_b_1()
                .border_color(hsla(popup_border))
                .child(filter_group),
        );
    if rows.is_empty() {
        popup = popup.child(
            div()
                .w_full()
                .px(px(10.0))
                .py(px(24.0))
                .text_center()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .child(empty_message),
        );
    }
    popup = popup.child(
        v_flex()
            .id(SharedString::from(format!("{id}-list")))
            .w_full()
            .p(px(4.0))
            .max_h(px(288.0))
            .overflow_y_scroll()
            .children(list_rows),
    );
    Some(
        deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(popup),
        )
        .with_priority(1)
        .into_any_element(),
    )
}

/// A modal-specific field skin for select triggers and text areas whose
/// stylesheet restates the shared shadcn look (Add Worktree: `#161616` fields
/// on a 8% hairline with a 28% focus edge at 13px).
#[derive(Clone, Copy)]
pub(crate) struct ModalFieldSkin {
    pub(crate) background: Rgba,
    pub(crate) border: Rgba,
    pub(crate) focus_border: Rgba,
    pub(crate) text_size: f32,
}

/// [`modal_select_trigger`] with a [`ModalFieldSkin`] and a caller-chosen
/// chevron (the searchable React trigger draws `IconChevronDown`, the plain one
/// `IconSelector`). Full width, no hover fill, the focus edge while open.
pub(crate) fn modal_select_trigger_skinned<V: 'static>(
    p: &ModalPalette,
    select: &ModalSelect,
    id: &'static str,
    value: Option<String>,
    placeholder: &'static str,
    skin: ModalFieldSkin,
    chevron: &'static str,
    disabled: bool,
    on_toggle: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let p = *p;
    let open = select.open;
    h_flex()
        .id(id)
        .w_full()
        .flex_shrink_0()
        .h(px(MODAL_CONTROL_HEIGHT))
        .px(px(12.0))
        .gap(px(6.0))
        .items_center()
        .justify_between()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if open { skin.focus_border } else { skin.border }))
        .bg(hsla(skin.background))
        .text_size(px(skin.text_size))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .when(disabled, |this| this.opacity(0.5).cursor_default())
        .when(!disabled, |this| {
            this.cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    on_toggle(this, window, cx);
                }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(value.is_none(), |this| this.text_color(hsla(p.muted)))
                .child(value.unwrap_or_else(|| placeholder.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .child(modal_icon(chevron, 16.0, p.muted)),
        )
        .into_any_element()
}

/// [`modal_text_area`] with a [`ModalFieldSkin`]. The box keeps `min_height`
/// and grows with the editor, so create the `InputState` with
/// `.multi_line(true).auto_grow(min_rows, max_rows)` to cap the growth the way
/// the React `max-height` does.
pub(crate) fn modal_text_area_skinned(
    p: &ModalPalette,
    state: &gpui::Entity<InputState>,
    skin: ModalFieldSkin,
    min_height: f32,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .min_h(px(min_height))
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(if focused {
            skin.focus_border
        } else {
            skin.border
        }))
        .bg(hsla(skin.background))
        .when(disabled, |this| this.opacity(0.5))
        .child(
            div().w_full().min_w_0().child(
                Input::new(state)
                    .with_size(ComponentSize::Small)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .disabled(disabled)
                    .w_full()
                    .px(px(0.0))
                    .py(px(0.0))
                    .text_size(px(skin.text_size))
                    .text_color(hsla(p.foreground)),
            ),
        )
        .into_any_element()
}

/// [`modal_shell`] with a caller-chosen window inset for the React dialogs that
/// own their own edge padding in the child window (Add Worktree: 17px). The fit
/// pass adds `padding * 2` plus the two 20px gaps and the 32px footer row.
pub(crate) fn modal_shell_inset<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalFit,
    padding: f32,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    content: Vec<AnyElement>,
    footer: AnyElement,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let extra_height = padding * 2.0 + MODAL_SECTION_GAP * 2.0 + MODAL_FOOTER_BUTTON_HEIGHT;
    div()
        .id(id)
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(padding))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(extra_height))
                        .children(content),
                )
                .child(footer),
        )
        .children(overlay)
}

/// Space kept between a scrolling modal's bottom edge and the display's
/// visible bottom edge (above the Dock or taskbar) when the window grows.
pub(crate) const MODAL_SCROLL_FIT_SCREEN_MARGIN: f32 = 16.0;

/// The window fit for a modal whose body scrolls (Remote Setup): the window is
/// sized once, on open, to the header plus the body's own content and never
/// past the display's visible bottom. Anything taller later (the Android
/// popover, an error line) scrolls inside the body, as in the React twin.
pub(crate) struct ModalScrollFit {
    requested: Rc<Cell<Option<f32>>>,
    content_height: Rc<Cell<f32>>,
}

impl ModalScrollFit {
    pub(crate) fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(None)),
            content_height: Rc::new(Cell::new(0.0)),
        }
    }

    /// A prepaint listener for the scroll container, whose only child is the body column.
    fn content_listener(&self) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let content_height = self.content_height.clone();
        move |bounds, _window, _cx| {
            content_height.set(
                bounds
                    .iter()
                    .map(|bounds| f32::from(bounds.size.height))
                    .sum(),
            );
        }
    }

    /// A prepaint listener for the column holding the header and the scroll container.
    fn listener(
        &self,
        extra_height: f32,
    ) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let requested = self.requested.clone();
        let content_height = self.content_height.clone();
        move |bounds, window, cx| {
            let Some(header) = bounds.first() else {
                return;
            };
            let content = f32::from(header.size.height) + content_height.get();
            let mut needed = (content + extra_height).round();
            // gpui can resize a window but not move it, and the resize keeps
            // the top edge, so the cap is the room below the window's top.
            if let Some(display) = window.display(cx) {
                let visible = display.visible_bounds();
                let visible_bottom = f32::from(visible.origin.y + visible.size.height);
                let top = f32::from(window.bounds().origin.y);
                let limit = (visible_bottom - top - MODAL_SCROLL_FIT_SCREEN_MARGIN).round();
                if limit > 0.0 {
                    needed = needed.min(limit);
                }
            }
            let current = f32::from(window.viewport_size().height).round();
            // CDXC:AppModal 2026-09-16 DECISION:
            // User: a scrolling modal must not grow when a section expands (Remote Setup's "How to install"); it keeps the height it opened with and the body scrolls instead.
            if requested.get().is_some() {
                return;
            }
            requested.set(Some(needed));
            if (needed - current).abs() < 1.0 {
                return;
            }
            let handle = window.window_handle();
            let width = window.viewport_size().width;
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_root, window, _cx| {
                    window.resize(size(width, px(needed)));
                });
            });
        }
    }
}

/// [`modal_shell`] for a dialog whose body is a scroll container: the header
/// stays put, `body` scrolls inside the remaining height, and the optional
/// footer keeps its row. Sized by a [`ModalScrollFit`]. `body_id` names the
/// scroll container.
pub(crate) fn modal_shell_scrolling<V: Render>(
    p: &ModalPalette,
    id: &'static str,
    body_id: &'static str,
    focus_handle: &FocusHandle,
    fit: &ModalScrollFit,
    on_key_down: impl Fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<V>) + 'static,
    header: AnyElement,
    body: AnyElement,
    footer: Option<AnyElement>,
    overlay: Option<AnyElement>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let extra_height = MODAL_WINDOW_PADDING * 2.0
        + MODAL_SECTION_GAP
        + if footer.is_some() {
            MODAL_SECTION_GAP + MODAL_FOOTER_BUTTON_HEIGHT
        } else {
            0.0
        };
    div()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(hsla(p.surface))
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(p.foreground))
        .track_focus(focus_handle)
        .on_key_down(cx.listener(on_key_down))
        .child(
            v_flex()
                .size_full()
                .p(px(MODAL_WINDOW_PADDING))
                .gap(px(MODAL_SECTION_GAP))
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .gap(px(MODAL_SECTION_GAP))
                        .on_children_prepainted(fit.listener(extra_height))
                        .child(header)
                        .child(
                            div()
                                .on_children_prepainted(fit.content_listener())
                                .id(body_id)
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .overflow_y_scroll()
                                .child(body),
                        ),
                )
                .children(footer),
        )
        .children(overlay)
}
