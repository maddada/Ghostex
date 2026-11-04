//! Search by Prompt's colours: the tokens the React page resolved (`.ghostex-find-scope` in
//! packages/core-ui/styles/find.css, deleted 2026-10-01), laid over the theme's own window colour like the other native
//! modals, and frosted under window glass.
//!
//! CDXC:Theming 2026-09-29 DECISION:
//! User: "make the find by prompt modal also get same transparency as other modals like the cmd + n
//! modal", and "it should take the colors of the theme applied". The window is the theme's chrome
//! colour (white in light mode), and under glass it takes the app's frosted dialog fill (`frosted_panel_fill` since 2026-10-29, `frosted_modal_fill` before) with ink washes
//! for its raised fills, the recipe Quick Access and the New Thread picker use. Supersedes the fixed
//! React greys and the lifted #2b2b2b this window first shipped with.
use crate::app::window::native_modal_kit::MODAL_UI_FONT;
use crate::app::window::native_modal_kit::{css_mix, frosted_dialog_washes, modal_rgba, rgba_of};
use gpui::{Rgba, SharedString};

#[derive(Clone, Copy)]
pub(crate) struct FindPalette {
    pub(crate) light: bool,
    pub(crate) background: Rgba,
    pub(crate) foreground: Rgba,
    pub(crate) muted: Rgba,
    /// `--border` and the `border/60` hairlines between the toolbar, list and bottom pane.
    pub(crate) border: Rgba,
    pub(crate) hairline: Rgba,
    pub(crate) accent: Rgba,
    /// The query chevron (`--primary`).
    pub(crate) primary: Rgba,
    pub(crate) popover: Rgba,
    pub(crate) popover_border: Rgba,
    /// A filter trigger's fill at rest and on hover.
    pub(crate) button: Rgba,
    pub(crate) button_hover: Rgba,
    /// The filter menu's search field (`input/30`) and its focus border and ring.
    pub(crate) field: Rgba,
    pub(crate) field_border: Rgba,
    pub(crate) field_ring: Rgba,
    pub(crate) destructive: Rgba,
    /// The matched/total pill.
    pub(crate) pill: Rgba,
    pub(crate) pill_outline: Rgba,
    pub(crate) pill_highlight: Rgba,
    /// Matched characters in a row's prompt.
    pub(crate) matched: Rgba,
    /// `text-amber-400` for the favorite star.
    pub(crate) favorite: Rgba,
}

impl FindPalette {
    /// `light` is the app appearance and `glass` window glass. The window is the theme's chrome
    /// colour; under glass it is the app's frosted fill of it, with ink washes for raised fills.
    pub(crate) fn resolve(light: bool, glass: bool) -> Self {
        let chrome: Rgba = if light {
            modal_rgba(0xffffff, 1.0)
        } else {
            Rgba::from(crate::app::helpers::titlebar_background())
        };
        let base = Self::react(light, chrome);
        if !glass {
            return base;
        }
        // Menus draw inside this window, over rows nothing blurs, so they stay solid.
        let solid_menu = if light {
            chrome
        } else {
            css_mix(modal_rgba(0xffffff, 1.0), 0.06, chrome)
        };
        // The dialogs' washes on the popover glass (CDXC:AppModal 2026-10-29 on
        // `frosted_dialog_fill`): the selected row at the hover wash, fields and the filter
        // buttons' hover at the raised wash, the lines at the hairline.
        let washes = frosted_dialog_washes(light);
        Self {
            background: crate::app::helpers::frosted_panel_fill(chrome.into()).into(),
            accent: washes.raised_hover,
            button_hover: washes.raised,
            field: washes.raised,
            pill: washes.raised,
            pill_outline: washes.hairline,
            popover: solid_menu,
            hairline: washes.hairline,
            ..base
        }
    }

    /// The React page's tokens with `window` as its background.
    fn react(light: bool, window: Rgba) -> Self {
        if light {
            let foreground = modal_rgba(0x262626, 1.0);
            return Self {
                light,
                background: window,
                foreground,
                muted: modal_rgba(0x636363, 1.0),
                border: modal_rgba(0x000000, 0.14),
                hairline: modal_rgba(0x000000, 0.084),
                accent: modal_rgba(0xe9e9e9, 1.0),
                primary: foreground,
                popover: window,
                popover_border: modal_rgba(0x000000, 0.12),
                button: modal_rgba(0xffffff, 1.0),
                button_hover: modal_rgba(0xf1f1f1, 1.0),
                field: modal_rgba(0x000000, 0.042),
                field_border: modal_rgba(0x9d9d9d, 1.0),
                field_ring: modal_rgba(0x9d9d9d, 0.2),
                destructive: modal_rgba(0xb91c1c, 1.0),
                pill: modal_rgba(0xffffff, 1.0),
                pill_outline: modal_rgba(0x000000, 0.08),
                pill_highlight: modal_rgba(0x000000, 0.0),
                matched: modal_rgba(0x8a5800, 1.0),
                favorite: modal_rgba(0xffb900, 1.0),
            };
        }
        Self {
            light,
            background: window,
            foreground: modal_rgba(0xc8cdd5, 1.0),
            muted: modal_rgba(0x747b85, 1.0),
            border: modal_rgba(0xffffff, 0.11),
            hairline: modal_rgba(0xffffff, 0.066),
            accent: modal_rgba(0x262626, 1.0),
            primary: modal_rgba(0xe5e5e5, 1.0),
            popover: window,
            popover_border: modal_rgba(0xffffff, 0.12),
            button: modal_rgba(0x000000, 0.0),
            button_hover: modal_rgba(0xffffff, 0.045),
            field: modal_rgba(0xffffff, 0.045),
            field_border: modal_rgba(0x737373, 1.0),
            field_ring: modal_rgba(0x737373, 0.2),
            destructive: modal_rgba(0xff6467, 1.0),
            pill: css_mix(modal_rgba(0xffffff, 1.0), 0.025, window),
            pill_outline: modal_rgba(0xffffff, 0.05),
            pill_highlight: modal_rgba(0xffffff, 0.03),
            matched: modal_rgba(0xe3b341, 1.0),
            favorite: modal_rgba(0xffb900, 1.0),
        }
    }

    /// `color-mix(in srgb, var(--foreground) N%, transparent)`.
    pub(crate) fn foreground_at(&self, alpha: f32) -> Rgba {
        rgba_of(self.foreground, alpha)
    }

    /// `accent/N`: scales the accent's own alpha. Under glass the accent is already a faint ink wash,
    /// and replacing its alpha turned the selected row into a 70% white slab behind white text.
    pub(crate) fn accent_at(&self, alpha: f32) -> Rgba {
        rgba_of(self.accent, self.accent.a * alpha)
    }
}

/// Search by Prompt's family, which a picked app font replaces through `crate::ui_fonts`.
pub(crate) fn find_prompts_font_family() -> SharedString {
    SharedString::new_static(MODAL_UI_FONT)
}
