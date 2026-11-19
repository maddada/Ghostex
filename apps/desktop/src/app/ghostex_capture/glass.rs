//! The floating button's and its panel's colours, frosted under window glass.
//!
//! CDXC:GhostexCapture 2026-11-04 DECISION:
//! User, of the panel the floating button opens and of the count tab beside the button: "can we
//! make these glassy please?". Under window glass the panel is a frosted window like the
//! Notifications dropdown and the voice panel: its window blurs what is behind it at the frosted
//! surfaces' blur (`apply_frosted_menu_blur`, from the Menu blur setting), takes the dropdown
//! panels' fill and edge (`popup_window_panel_surface`, `titlebar_popup_menu_border_color`) and
//! follows the app's light or dark appearance, and its tiles, filter field, project cards and bottom
//! buttons are the dialogs' ink washes on that glass (`resource_glass_washes`) instead of solid
//! boxes. The count tray beside the button and the docked tab take the same fill and edge over a
//! blur confined to their own shape (`set_background_blur_region`), so the app icon and the clear
//! space around it stay unblurred; that confinement is macOS's, so elsewhere the button keeps its
//! solid tray. With window glass off both keep their solid dark look. The tooltips stay solid
//! (CDXC:GhostexCapture 2026-09-30 in `tooltip.rs`).
//! SEE-ALSO: app/voice_agent/panel.rs, app/window/titlebar_notifications_panel.rs,
//! app/window/toast.rs (the same blur region).

use gpui::{Bounds, Hsla, Pixels, WindowBackgroundAppearance, rgb, rgba};

use crate::app::helpers::{
    chrome_ink, chrome_uses_light_appearance, glass_lifted_opaque_from, popup_window_panel_surface,
    titlebar_background, titlebar_popup_menu_border_color, window_glass_active,
    workspace_background_color,
};
use crate::app::window::resources_style::resource_glass_washes;

/// The panel's colours: the frosted ones under window glass, the solid dark ones otherwise.
#[derive(Clone, Copy)]
pub(super) struct PanelPalette {
    pub(super) surface: Hsla,
    pub(super) edge: Hsla,
    /// The separators.
    pub(super) hairline: Hsla,
    /// The capture tiles at rest and under the pointer.
    pub(super) tile: Hsla,
    pub(super) tile_hover: Hsla,
    pub(super) tile_edge: Hsla,
    pub(super) tile_edge_hover: Hsla,
    /// A tile's key cap.
    pub(super) key: Hsla,
    pub(super) key_edge: Hsla,
    /// A project card and the filter field.
    pub(super) card: Hsla,
    pub(super) card_edge: Hsla,
    pub(super) field: Hsla,
    pub(super) field_edge: Hsla,
    /// A row or header under the pointer.
    pub(super) row_hover: Hsla,
    /// The bottom buttons and the Connect button.
    pub(super) button: Hsla,
    pub(super) button_hover: Hsla,
    pub(super) button_edge: Hsla,
    /// Tile labels, key caps, typed filter text, a hovered button's label.
    pub(super) strong: Hsla,
    /// Project and machine names.
    pub(super) title: Hsla,
    /// Session titles, the tiles' icons, the Connect label.
    pub(super) body: Hsla,
    /// The heading, chevrons, the bottom buttons' labels.
    pub(super) muted: Hsla,
    /// Counts, times, notes and placeholders.
    pub(super) faint: Hsla,
    /// A machine that is not connected.
    pub(super) offline: Hsla,
    /// A session that is idle.
    pub(super) idle_dot: Hsla,
    /// The Continue your draft badge.
    pub(super) accent: Hsla,
    pub(super) accent_fill: Hsla,
    pub(super) accent_fill_hover: Hsla,
    pub(super) accent_edge: Hsla,
}

impl PanelPalette {
    pub(super) fn resolve() -> Self {
        if window_glass_active() {
            Self::frosted()
        } else {
            Self::solid()
        }
    }

    /// The panel as it was before glass: a near-black panel with solid tiles.
    fn solid() -> Self {
        Self {
            surface: rgb(0x161616).into(),
            edge: rgb(0x3a3a3a).into(),
            hairline: rgba(0xffffff14).into(),
            tile: rgb(0x1d1d1d).into(),
            tile_hover: rgb(0x232323).into(),
            tile_edge: rgba(0xffffff14).into(),
            tile_edge_hover: rgba(0xffffff29).into(),
            key: rgb(0x222222).into(),
            key_edge: rgba(0xffffff29).into(),
            card: rgb(0x161616).into(),
            card_edge: rgba(0x3a3a3ab8).into(),
            field: rgb(0x1b1b1b).into(),
            field_edge: rgba(0xffffff14).into(),
            row_hover: rgb(0x202020).into(),
            button: gpui::transparent_black(),
            button_hover: rgb(0x202020).into(),
            button_edge: rgba(0xffffff14).into(),
            strong: rgb(0xe8e8e8).into(),
            title: rgb(0xcfd2d8).into(),
            body: rgb(0xb4b8c0).into(),
            muted: rgb(0x9a9a9a).into(),
            faint: rgb(0x6b6b6b).into(),
            offline: rgb(0x8a8a8a).into(),
            idle_dot: rgb(0xd9d9d9).into(),
            accent: rgb(0x86d3f8).into(),
            accent_fill: rgb(0x2a3a44).into(),
            accent_fill_hover: rgb(0x33495a).into(),
            accent_edge: rgba(0x86d3f866).into(),
        }
    }

    /// The Notifications dropdown's glass, with the dialogs' washes for everything on it; the ink
    /// is white in dark mode and black in light mode, so it reads either way.
    fn frosted() -> Self {
        let ink: Hsla = chrome_ink().into();
        let washes = resource_glass_washes();
        let light = chrome_uses_light_appearance();
        let accent: Hsla = if light {
            rgb(0x0369a1).into()
        } else {
            rgb(0x86d3f8).into()
        };
        Self {
            surface: popup_window_panel_surface(titlebar_background()),
            edge: titlebar_popup_menu_border_color(),
            hairline: washes.hairline.into(),
            tile: washes.raised.into(),
            tile_hover: washes.raised_hover.into(),
            tile_edge: washes.hairline.into(),
            tile_edge_hover: ink.opacity(0.24),
            key: washes.raised.into(),
            key_edge: ink.opacity(0.18),
            card: washes.panel.into(),
            card_edge: washes.hairline.into(),
            field: washes.panel.into(),
            field_edge: washes.hairline.into(),
            row_hover: washes.raised.into(),
            button: washes.raised.into(),
            button_hover: washes.raised_hover.into(),
            button_edge: washes.hairline.into(),
            strong: ink.opacity(0.94),
            title: ink.opacity(0.88),
            body: ink.opacity(0.78),
            muted: ink.opacity(0.62),
            faint: ink.opacity(0.48),
            offline: ink.opacity(0.55),
            idle_dot: ink.opacity(if light { 0.32 } else { 0.82 }),
            accent,
            accent_fill: accent.opacity(0.16),
            accent_fill_hover: accent.opacity(0.26),
            accent_edge: accent.opacity(0.45),
        }
    }
}

/// The colours of Capture's opaque windows and bubbles: the prompt box, its Send to picker, the
/// "sent" note and the tooltips.
///
/// CDXC:Theming 2026-11-20 DECISION:
/// These were fixed near-blacks in both appearances, ignoring the theme, and read as black slabs
/// on the glass design. Dark mode under window glass, they are the work area's colour lifted
/// toward white (`glass_lifted_opaque_from`, CDXC:Theming 2026-11-20 in
/// helpers/chrome_palette.rs), kept opaque per CDXC:GhostexCapture 2026-09-30 ("not transparent
/// please even if transparency enabled"); light mode, glass on or off, the same lift of the light
/// theme colour with dark text. Every fill on them is an opaque ink step over that surface. Dark
/// mode with glass off keeps the original solid colours.
#[derive(Clone, Copy)]
pub(crate) struct SolidPalette {
    pub(crate) light: bool,
    /// The prompt box.
    pub(crate) surface: Hsla,
    /// The Send to picker over the box, and the "sent" note.
    pub(crate) menu: Hsla,
    /// The tooltips.
    pub(crate) tooltip: Hsla,
    /// The box's, the picker's, the note's and the tooltips' edge.
    pub(crate) edge: Hsla,
    /// The quiet hairline (the search field, the closed trigger).
    pub(crate) hairline: Hsla,
    /// The text field's and the thumbnails' edge.
    pub(crate) edge_strong: Hsla,
    /// The open trigger's edge.
    pub(crate) edge_open: Hsla,
    /// The text field and the picker's search field.
    pub(crate) field: Hsla,
    pub(crate) search_field: Hsla,
    /// The Send to trigger at rest and under the pointer.
    pub(crate) control: Hsla,
    pub(crate) control_hover: Hsla,
    /// A picker row under the pointer and the chosen one.
    pub(crate) row_hover: Hsla,
    pub(crate) row_selected: Hsla,
    /// A capture icon button under the pointer.
    pub(crate) button_hover: Hsla,
    /// Typed text, the trigger's label, the note's text, the tooltips' text.
    pub(crate) strong: Hsla,
    /// Project names.
    pub(crate) title: Hsla,
    /// Session titles.
    pub(crate) body: Hsla,
    /// Labels, icons and chevrons.
    pub(crate) muted: Hsla,
    /// Counts and the empty message.
    pub(crate) faint: Hsla,
    /// A row's trailing note.
    pub(crate) fainter: Hsla,
    /// An idle session's dot, and a sleeping one's.
    pub(crate) idle_dot: Hsla,
    pub(crate) sleeping_dot: Hsla,
    /// The note's Open link.
    pub(crate) link: Hsla,
}

impl SolidPalette {
    pub(crate) fn resolve() -> Self {
        let light = chrome_uses_light_appearance();
        if light || window_glass_active() {
            Self::lifted(light)
        } else {
            Self::solid()
        }
    }

    /// The original solid dark colours.
    fn solid() -> Self {
        Self {
            light: false,
            surface: rgb(0x161616).into(),
            menu: rgb(0x1b1b1b).into(),
            tooltip: rgb(0x1f1f1f).into(),
            edge: rgb(0x3a3a3a).into(),
            hairline: rgba(0xffffff14).into(),
            edge_strong: rgba(0xffffff29).into(),
            edge_open: rgba(0xffffff40).into(),
            field: rgb(0x121212).into(),
            search_field: rgb(0x141414).into(),
            control: rgb(0x1d1d1d).into(),
            control_hover: rgb(0x232323).into(),
            row_hover: rgb(0x242424).into(),
            row_selected: rgb(0x2a2a2a).into(),
            button_hover: rgb(0x262626).into(),
            strong: rgb(0xe8e8e8).into(),
            title: rgb(0xcfd2d8).into(),
            body: rgb(0xc9ccd2).into(),
            muted: rgb(0x9a9a9a).into(),
            faint: rgb(0x6b6b6b).into(),
            fainter: rgb(0x5f5f5f).into(),
            idle_dot: rgb(0xd9d9d9).into(),
            sleeping_dot: rgb(0x5f5f5f).into(),
            link: rgb(0x86d3f8).into(),
        }
    }

    /// The theme colour lifted toward white, opaque, with opaque ink steps over it.
    fn lifted(light: bool) -> Self {
        let surface = glass_lifted_opaque_from(workspace_background_color(), light);
        let ink: Hsla = if light {
            gpui::black()
        } else {
            gpui::white()
        };
        let step = |amount: f32| surface.blend(ink.opacity(amount));
        let (strong, title, body, muted, faint, fainter, idle_dot, sleeping_dot, link): (
            Hsla,
            Hsla,
            Hsla,
            Hsla,
            Hsla,
            Hsla,
            Hsla,
            Hsla,
            Hsla,
        ) = if light {
            (
                rgb(0x1f1f1f).into(),
                rgb(0x262626).into(),
                rgb(0x333333).into(),
                rgb(0x5c5c5c).into(),
                rgb(0x707070).into(),
                rgb(0x7a7a7a).into(),
                rgb(0x6b6b6b).into(),
                rgb(0xb5b5b5).into(),
                rgb(0x0369a1).into(),
            )
        } else {
            let solid = Self::solid();
            (
                solid.strong,
                solid.title,
                solid.body,
                solid.muted,
                rgb(0x7a7a7a).into(),
                rgb(0x707070).into(),
                solid.idle_dot,
                solid.sleeping_dot,
                solid.link,
            )
        };
        Self {
            light,
            surface,
            menu: if light { step(0.0) } else { step(0.04) },
            tooltip: surface,
            edge: ink.opacity(if light { 0.16 } else { 0.18 }),
            hairline: ink.opacity(0.08),
            edge_strong: ink.opacity(0.16),
            edge_open: ink.opacity(0.25),
            field: step(0.04),
            search_field: step(0.05),
            control: step(0.07),
            control_hover: step(0.10),
            row_hover: step(0.08),
            row_selected: step(0.12),
            button_hover: step(0.10),
            strong,
            title,
            body,
            muted,
            faint,
            fainter,
            idle_dot,
            sleeping_dot,
            link,
        }
    }
}

/// The panel window's background: blurred under window glass.
pub(super) fn panel_background_appearance() -> WindowBackgroundAppearance {
    if window_glass_active() {
        WindowBackgroundAppearance::Blurred
    } else {
        WindowBackgroundAppearance::Transparent
    }
}

/// Whether the count tray and the docked tab are frosted: under window glass, where their blur can
/// be kept to their own shape (macOS; on Windows a blur region clips the window, which would cut the
/// app icon away).
pub(super) fn button_glass() -> bool {
    cfg!(target_os = "macos") && window_glass_active()
}

/// The count tray's and the docked tab's fill and edge.
#[derive(Clone, Copy)]
pub(super) struct TrayPalette {
    pub(super) fill: Hsla,
    pub(super) edge: Hsla,
    /// The docked tab's edge.
    pub(super) tab_edge: Hsla,
}

impl TrayPalette {
    pub(super) fn resolve(glass: bool) -> Self {
        if glass {
            let edge = titlebar_popup_menu_border_color();
            Self {
                fill: popup_window_panel_surface(titlebar_background()),
                edge,
                tab_edge: edge,
            }
        } else {
            Self {
                fill: rgba(0x080a0ee0).into(),
                edge: rgba(0xffffff1a).into(),
                tab_edge: rgba(0xffffff26).into(),
            }
        }
    }

    /// The docked tab's fill: the tray's, or its own slightly lighter black without glass.
    pub(super) fn tab_fill(&self, glass: bool) -> Hsla {
        if glass {
            self.fill
        } else {
            rgba(0x0a0c10e6).into()
        }
    }
}

/// A shape rounded on one side only, as a blur region: the rounded rectangle plus a square half on
/// the flat side, so their union is square where the shape is.
pub(super) fn one_side_rounded(
    bounds: Bounds<Pixels>,
    radius: Pixels,
    flat_left: bool,
) -> Vec<(Bounds<Pixels>, Pixels)> {
    let half = bounds.size.width / 2.0;
    let mut square = bounds;
    square.size.width = half;
    if !flat_left {
        square.origin.x += half;
    }
    vec![(bounds, radius), (square, gpui::px(0.0))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    #[test]
    fn a_one_side_rounded_region_keeps_its_flat_half_square() {
        let bounds = Bounds::new(point(px(10.0), px(7.0)), size(px(40.0), px(44.0)));
        let left = one_side_rounded(bounds, px(12.0), true);
        assert_eq!(left[0], (bounds, px(12.0)));
        assert_eq!(
            left[1],
            (
                Bounds::new(point(px(10.0), px(7.0)), size(px(20.0), px(44.0))),
                px(0.0)
            )
        );
        let right = one_side_rounded(bounds, px(12.0), false);
        assert_eq!(
            right[1],
            (
                Bounds::new(point(px(30.0), px(7.0)), size(px(20.0), px(44.0))),
                px(0.0)
            )
        );
    }
}
