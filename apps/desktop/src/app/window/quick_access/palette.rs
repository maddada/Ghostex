//! The Quick Access design tokens, ported from the `.quick-access-surface` block in
//! packages/core-ui/styles.css (plus its light overrides in styles/modals-light.css and the shared
//! `.raised-tab-rail` in packages/components/ui/raised-tab-rail.css).
//!
//! Quick Access sits one elevation step below the `.gx-app-modal` language that
//! `native_modal_kit::ModalPalette` encodes: its raised tone is `--settings-raised` (#161616), not
//! the modal kit's #1d1d1d, and its row text is the deliberate #b4b8bf rather than the theme
//! foreground. Those are the two reasons this is its own palette instead of a ModalPalette call.
use crate::app::window::native_modal_kit::{
    ModalPalette, css_mix, dark_theme_text_colors, modal_rgba, rgba_of,
};
use gpui::{Hsla, Rgba, rgb};

pub(crate) const QUICK_ACCESS_ITEM_FONT_SIZE: f32 = 13.0;
pub(crate) const QUICK_ACCESS_ROW_FONT_SIZE: f32 = 14.5;
pub(crate) const QUICK_ACCESS_META_FONT_SIZE: f32 = 12.5;
pub(crate) const QUICK_ACCESS_ROW_HEIGHT: f32 = 40.0;
pub(crate) const QUICK_ACCESS_ROW_RADIUS: f32 = 9.0;
pub(crate) const QUICK_ACCESS_ROW_PADDING_X: f32 = 10.0;
pub(crate) const QUICK_ACCESS_LIST_PADDING: f32 = 6.0;
pub(crate) const QUICK_ACCESS_GROUP_HEADING_HEIGHT: f32 = 26.0;
pub(crate) const QUICK_ACCESS_CONTROL_HEIGHT: f32 = 32.0;
pub(crate) const QUICK_ACCESS_RADIUS_CONTROL: f32 = 8.0;
pub(crate) const QUICK_ACCESS_RADIUS_MENU_ITEM: f32 = 6.0;
/// A row's actions menu and the Actions panel.
pub(crate) const QUICK_ACCESS_RADIUS_ACTIONS_MENU: f32 = 11.0;
pub(crate) const QUICK_ACCESS_SEARCH_BAR_HEIGHT: f32 = 54.0;
pub(crate) const QUICK_ACCESS_SEARCH_FONT_SIZE: f32 = 17.0;
pub(crate) const QUICK_ACCESS_FILTER_HEIGHT: f32 = 28.0;
pub(crate) const QUICK_ACCESS_FILTER_MAX_WIDTH: f32 = 150.0;
pub(crate) const QUICK_ACCESS_FOOTER_HEIGHT: f32 = 44.0;

#[derive(Clone, Copy)]
pub(crate) struct QuickAccessPalette {
    /// The child window's own fill; the React surface is transparent over it.
    pub(crate) window: Rgba,
    /// `window` as an opaque colour, for text drawn on a foreground fill. Equal to `window`
    /// except under window glass (`frosted`).
    pub(crate) solid_window: Rgba,
    /// `--settings-raised`: search field, row hover, select triggers.
    pub(crate) raised: Rgba,
    /// `--settings-raised-hover`.
    pub(crate) raised_hover: Rgba,
    /// `--settings-hairline`.
    pub(crate) hairline: Rgba,
    /// `--foreground` inside the surface.
    pub(crate) foreground: Rgba,
    /// `--quick-access-item-color`: every list row's text.
    pub(crate) item: Rgba,
    /// `--muted-foreground`: headings, metadata, shortcuts.
    pub(crate) muted: Rgba,
    /// `--settings-focus-border-color`.
    pub(crate) focus_border: Rgba,
    /// `--ghostex-accent`: checked filters only.
    pub(crate) accent: Rgba,
    pub(crate) destructive: Rgba,
    /// Portaled menu surfaces (tag filter, project picker, row menus).
    pub(crate) menu_background: Rgba,
    /// A menu's fill when it draws in a frosted window of its own (`frosted`); `menu_background`
    /// otherwise.
    pub(crate) hosted_menu_background: Rgba,
    pub(crate) menu_border: Rgba,
    pub(crate) menu_hover: Rgba,
    /// The footer bar's own tone, its selected tab, and the tab accelerators.
    pub(crate) footer: Rgba,
    pub(crate) footer_active: Rgba,
    pub(crate) rail_hotkey: Rgba,
    pub(crate) rail_active_hotkey: Rgba,
    /// Keycaps in the footer and the actions menu.
    pub(crate) keycap: Rgba,
    /// The selected row's fill.
    pub(crate) row_selected: Rgba,
    /// The session-row lifecycle dot, at rest and for a row open in the sidebar.
    pub(crate) status_dot: Rgba,
    pub(crate) status_dot_open: Rgba,
    /// The Saved Prompts persistent star.
    pub(crate) favorite: Rgba,
}

impl QuickAccessPalette {
    pub(crate) fn resolve(light: bool, sidebar_theme: Option<&str>, window: Rgba) -> Self {
        let modal = ModalPalette::resolve(light, sidebar_theme);
        if light {
            let foreground = rgb(0x262626);
            let hairline = modal_rgba(0x000000, 0.14);
            Self {
                window,
                solid_window: window,
                raised: rgb(0xf0f0f0),
                raised_hover: rgb(0xe5e5e5),
                hairline,
                foreground,
                item: rgb(0x404040),
                muted: rgb(0x626262),
                // The light sheet pins the focus border back to the hairline.
                focus_border: hairline,
                accent: rgb(0x262626),
                destructive: rgb(0xb91c1c),
                menu_background: rgb(0xffffff),
                hosted_menu_background: rgb(0xffffff),
                menu_border: modal_rgba(0x000000, 0.16),
                menu_hover: rgb(0xe9e9e9),
                footer: modal_rgba(0x000000, 0.03),
                footer_active: modal_rgba(0x000000, 0.08),
                rail_hotkey: rgb(0x8a8a8a),
                rail_active_hotkey: rgb(0x525252),
                keycap: modal_rgba(0x000000, 0.07),
                row_selected: modal_rgba(0x000000, 0.06),
                status_dot: rgb(0x737373),
                status_dot_open: rgb(0x26a7fc),
                favorite: rgb(0xe3b341),
            }
        } else {
            let (_, muted, _) = dark_theme_text_colors(sidebar_theme);
            let foreground = rgb(0xb4b8bf);
            let hairline = modal_rgba(0xffffff, 0.08);
            Self {
                window,
                solid_window: window,
                raised: rgb(0x161616),
                raised_hover: rgb(0x1d1d1d),
                hairline,
                foreground,
                item: rgb(0xb4b8bf),
                muted: rgb(muted),
                focus_border: css_mix(foreground, 0.58, hairline),
                accent: rgb(0x86d3f8),
                destructive: modal.destructive,
                menu_background: rgb(0x161616),
                hosted_menu_background: rgb(0x161616),
                menu_border: modal_rgba(0xffffff, 0.08),
                menu_hover: rgb(0x232323),
                footer: modal_rgba(0x000000, 0.18),
                footer_active: modal_rgba(0xffffff, 0.09),
                rail_hotkey: rgb(0x6f737a),
                rail_active_hotkey: rgb(0x92969d),
                keycap: modal_rgba(0xffffff, 0.07),
                row_selected: modal_rgba(0xffffff, 0.075),
                status_dot: modal_rgba(0xffffff, 0.20),
                status_dot_open: rgb(0xffffff),
                favorite: rgb(0xe3b341),
            }
        }
    }

    /// Under window glass Quick Access is frosted like the app modals (`ModalPalette::frosted`):
    /// its window blurs what is behind it, `fill` replaces the window colour, and the raised
    /// fills become ink washes. Its menus draw in frosted windows of their own on macOS
    /// (`QuickAccessMenuPaint`), with the app's frosted menu fill and the same soft ink wash on
    /// their highlighted row as the app's other frosted menus.
    pub(crate) fn frosted(mut self, light: bool, fill: Rgba) -> Self {
        let (ink, raised, raised_hover, menu_hover) = if light {
            (0x000000, 0.05, 0.09, 0.06)
        } else {
            (0xffffff, 0.05, 0.08, 0.08)
        };
        self.window = fill;
        self.raised = modal_rgba(ink, raised);
        self.raised_hover = modal_rgba(ink, raised_hover);
        self.menu_hover = modal_rgba(ink, menu_hover);
        self.hosted_menu_background = crate::app::helpers::frosted_menu_fill(
            crate::app::helpers::titlebar_popup_menu_background(),
        )
        .into();
        self
    }

    /// `bg-border/50`: the hairline the Commands list draws between groups.
    pub(crate) fn separator(&self) -> Rgba {
        rgba_of(self.hairline, self.hairline.a * 0.5)
    }

    /// The tag/session chip fill, `color-mix(in srgb, <tag> 15%, transparent)`.
    pub(crate) fn chip_background(&self, tag: Rgba) -> Rgba {
        rgba_of(tag, 0.15)
    }

    pub(crate) fn chip_text(&self, tag: Rgba) -> Rgba {
        css_mix(tag, 0.72, self.foreground)
    }
}

pub(crate) fn hsla(color: Rgba) -> Hsla {
    color.into()
}

/// `#rrggbb`, `#rgb` and `#rrggbbaa` as the daemon writes tag colors.
pub(crate) fn parse_css_color(value: &str, fallback: Rgba) -> Rgba {
    let text = value.trim().trim_start_matches('#');
    let expand = |slice: &str| u8::from_str_radix(slice, 16).ok().map(|v| v as f32 / 255.0);
    match text.len() {
        3 => {
            let channel = |index: usize| {
                let digit = &text[index..index + 1];
                expand(&format!("{digit}{digit}"))
            };
            match (channel(0), channel(1), channel(2)) {
                (Some(r), Some(g), Some(b)) => Rgba { r, g, b, a: 1.0 },
                _ => fallback,
            }
        }
        6 | 8 => {
            let channel = |index: usize| expand(&text[index..index + 2]);
            match (channel(0), channel(2), channel(4)) {
                (Some(r), Some(g), Some(b)) => Rgba {
                    r,
                    g,
                    b,
                    a: if text.len() == 8 {
                        channel(6).unwrap_or(1.0)
                    } else {
                        1.0
                    },
                },
                _ => fallback,
            }
        }
        _ => fallback,
    }
}
