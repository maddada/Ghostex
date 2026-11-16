//! The native Docs view's colours and type. The solid values are the former React Docs page's
//! own tokens; under window glass the fills become light washes of
//! the text ink, the way the native Kanban's do, so the frosted work area shows through.

use crate::app::helpers::theme_role_or;
use gpui::{Hsla, rgb};

#[derive(Clone)]
pub(crate) struct DocsPalette {
    pub(crate) glass: bool,
    pub(crate) light: bool,
    pub(crate) font: String,
    pub(crate) mono_font: String,
    /// The document's fill; transparent under glass.
    pub(crate) page: Hsla,
    /// The files list, the search row and the header rows.
    pub(crate) chrome: Hsla,
    /// A floating files list covers the document, so it is always opaque.
    pub(crate) floating: Hsla,
    /// Menus and floating bars.
    pub(crate) raised: Hsla,
    pub(crate) text: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) subtle: Hsla,
    pub(crate) accent: Hsla,
    /// `--manage-row-surface`: a selected row, or the row whose menu is open.
    pub(crate) row_surface: Hsla,
    /// `--app-context-menu-hover-background`.
    pub(crate) row_hover: Hsla,
    /// File rows at rest, and hovered or selected.
    pub(crate) row_text: Hsla,
    pub(crate) row_text_strong: Hsla,
    /// A folder above the open file.
    pub(crate) ancestor_text: Hsla,
    pub(crate) toolbar_icon: Hsla,
    pub(crate) toolbar_disabled: Hsla,
    /// Header button hover and open state.
    pub(crate) control_hover: Hsla,
    /// `--manage-border`.
    pub(crate) border: Hsla,
    pub(crate) border_strong: Hsla,
    /// The search row's and the open-files list's bottom rule.
    pub(crate) rule: Hsla,
    /// The docked list's own left edge.
    pub(crate) divider: Hsla,
    pub(crate) danger: Hsla,
    pub(crate) green: Hsla,
    pub(crate) amber: Hsla,
}

fn ink(light: bool, alpha: f32) -> Hsla {
    Hsla::from(rgb(if light { 0x000000 } else { 0xffffff })).opacity(alpha)
}

fn pick(light: bool, light_value: u32, dark_value: u32) -> Hsla {
    rgb(if light { light_value } else { dark_value }).into()
}

impl DocsPalette {
    /// CDXC:Docs 2026-09-15 DECISION:
    /// User: in light mode the Docs files list, its search row, and the document and formatting toolbars use #f4f4f5, and the search row's bottom border is #e5e5e5. Dark mode keeps the 2026-09-07 decision (files sidebar, search row, and header rows use #0b0b0b) until the user shares dark colours.
    ///
    /// CDXC:Docs 2026-09-24 DECISION:
    /// User: after the move to GPUI, Docs has transparency in the background because it is GPUI-based, not web-based anymore. Under glass the page paints no fill and its rows are washes of the text ink, like Kanban and Automate; this supersedes the 2026-09-23 Theming decision that drew the page as one solid card below the tab strip.
    ///
    /// CDXC:Docs 2026-10-23 DECISION:
    /// User, in light mode with a tinted theme colour: "i see there's a pink color in the background in the files list this is wrong. pls fix." In light mode the opaque page behind a file is the neutral #f7f7f8 (the Graphite light work area, and the former Docs page's light fallback) instead of the theme's tinted work area colour; dark mode still takes the chat's work area colour, and under glass the page stays clear.
    ///
    /// The values for one appearance. `font` and `background` are the chat's (in dark mode Files
    /// follows the workarea theme the chat resolves); `current` in `render.rs` reads them from the app.
    pub(crate) fn resolve(glass: bool, light: bool, font: String, background: Hsla) -> Self {
        let solid_chrome = pick(light, 0xf4f4f5, 0x0b0b0b);
        let wash = |light_alpha: f32, dark_alpha: f32| {
            ink(light, if light { light_alpha } else { dark_alpha })
        };
        Self {
            glass,
            light,
            font,
            mono_font: super::fonts::DOCS_MONO.to_string(),
            page: if glass {
                gpui::transparent_black()
            } else if light {
                rgb(0xf7f7f8).into()
            } else {
                background
            },
            chrome: if glass {
                wash(0.03, 0.04)
            } else {
                solid_chrome
            },
            floating: solid_chrome,
            raised: pick(light, 0xffffff, 0x161616),
            text: theme_role_or("text", light, pick(light, 0x27272a, 0xe5e5e5)),
            muted: theme_role_or("mutedText", light, pick(light, 0x626269, 0xa3a3a3)),
            subtle: pick(light, 0x71717a, 0x808080),
            accent: theme_role_or("accent", light, pick(light, 0x315d88, 0x9bbce0)),
            row_surface: if glass {
                wash(0.08, 0.1)
            } else {
                pick(light, 0xe9e9eb, 0x202020)
            },
            row_hover: if glass {
                wash(0.05, 0.07)
            } else {
                pick(light, 0xefefef, 0x202020)
            },
            row_text: pick(light, 0x52525b, 0xb4b8c0),
            row_text_strong: pick(light, 0x3f3f46, 0xd8d8d8),
            ancestor_text: pick(light, 0x18181b, 0xffffff),
            toolbar_icon: pick(light, 0x3f3f46, 0xa3a3a3),
            toolbar_disabled: pick(light, 0xb5b5bd, 0x55555d),
            control_hover: theme_role_or("hover", light, ink(light, 0.08)),
            // The Borders, Dividers and Grey text theme roles, when set (helpers/theme_roles.rs).
            border: theme_role_or("border", light, ink(light, 0.11)),
            border_strong: ink(light, 0.12),
            rule: theme_role_or(
                "divider",
                light,
                if glass {
                    ink(light, 0.08)
                } else {
                    pick(light, 0xe5e5e5, 0x292929)
                },
            ),
            divider: theme_role_or(
                "divider",
                light,
                if glass {
                    ink(light, 0.08)
                } else {
                    pick(light, 0xe5e5e5, 0x212121)
                },
            ),
            danger: theme_role_or("danger", light, pick(light, 0xbe123c, 0xfda4af)),
            green: pick(light, 0x15803d, 0x86efac),
            amber: rgb(0xfbbf24).into(),
        }
    }
}
