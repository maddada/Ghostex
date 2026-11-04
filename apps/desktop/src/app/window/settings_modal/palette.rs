//! The Settings surface tokens (`.ghostex-settings-shadcn.settings-modal-dialog` in
//! packages/core-ui/styles.css, the light block in styles/settings-light.css and the glass block
//! in styles/modals-glass.css), resolved from the app modal palette.
use super::super::native_modal_kit::*;
use gpui::{Rgba, rgb};

/// The face Settings is drawn in. The React modal asks for `--font-sans`
/// (`'Inter Variable', sans-serif`) and the modal host page never loads Inter Variable, so CEF
/// draws Settings, its dropdowns and its tooltips in Chromium's default sans-serif face: Arial on
/// Windows, Helvetica on macOS. Its `font-weight: 500` falls back to the regular face there, so the
/// medium labels (active rail rows, section titles) read regular. The Pick Color dialog is the
/// exception: portaled to the page body, it uses the system UI stack (`MODAL_UI_FONT`).
pub(crate) const SETTINGS_FONT: &str = if cfg!(target_os = "windows") {
    "Arial"
} else if cfg!(target_os = "macos") {
    "Helvetica"
} else {
    MODAL_UI_FONT
};

/// Settings is not tinted by the theme chrome like the `.gx-app-modal` dialogs: its dark surface
/// is the fixed `#0d0d0d` (CDXC:Settings 2026-09-09 DECISION in styles.css) and its light surface
/// the fixed `#f5f5f5`. Under window glass the frosted palette of the other native modals applies.
#[derive(Clone, Copy)]
pub(crate) struct SettingsPalette {
    /// The palette the shared kit controls draw with.
    pub(crate) modal: ModalPalette,
    pub(crate) light: bool,
    pub(crate) glass: bool,
    /// `--settings-surface`: the window.
    pub(crate) surface: Rgba,
    /// `--settings-raised`: the rail, cards, inputs and select triggers.
    pub(crate) raised: Rgba,
    /// `--settings-raised-hover`: hovered rows, the unchecked switch track, `--accent`.
    pub(crate) raised_hover: Rgba,
    /// `--settings-hairline` (also `--border` and `--input`).
    pub(crate) hairline: Rgba,
    pub(crate) foreground: Rgba,
    /// `--muted-foreground`.
    pub(crate) muted: Rgba,
    /// `--primary`: the slider range.
    pub(crate) primary: Rgba,
    pub(crate) destructive: Rgba,
    /// `--settings-accent`: the advanced-row arrow.
    pub(crate) settings_accent: Rgba,
    /// `--settings-focus-border-color`.
    pub(crate) focus_border: Rgba,
    /// `--settings-popup-background`, `--settings-popup-border`, `--settings-popup-selected`.
    pub(crate) popup_background: Rgba,
    pub(crate) popup_border: Rgba,
    pub(crate) popup_selected: Rgba,
    pub(crate) popup_selected_foreground: Rgba,
    /// `--accent` inside the portaled select popup (its hovered row).
    pub(crate) popup_hover: Rgba,
    /// `--ring`.
    pub(crate) ring: Rgba,
}

/// `--ghostex-accent` for the default tint: the sky accent the React modal host publishes before
/// the settings arrive.
const DEFAULT_GHOSTEX_ACCENT: u32 = 0x86d3f8;

impl SettingsPalette {
    pub(crate) fn resolve(modal: &ModalPalette) -> Self {
        let modal = *modal;
        let foreground = modal.foreground;
        if modal.glass && !modal.light {
            // styles/modals-glass.css: every Settings surface is a white wash over the window, at the
            // dialogs' washes on the popover glass (CDXC:AppModal 2026-10-29 on `frosted_dialog_fill`).
            let surface = modal.surface;
            let hairline = modal.hairline;
            return Self {
                modal,
                light: false,
                glass: true,
                surface,
                raised: modal.raised,
                raised_hover: modal.raised_hover,
                hairline,
                foreground,
                muted: rgb(0x9ca2ab),
                primary: rgb(0xe5e5e5),
                destructive: modal.destructive,
                settings_accent: rgb(DEFAULT_GHOSTEX_ACCENT),
                focus_border: css_mix(foreground, 0.58, hairline),
                popup_background: modal.solid_surface,
                popup_border: hairline,
                popup_selected: css_mix(rgb(0xffffff), 0.14, modal.solid_surface),
                popup_selected_foreground: rgb(0xffffff),
                popup_hover: css_mix(rgb(0xffffff), 0.085, modal.solid_surface),
                ring: rgb(0x737373),
            };
        }
        if modal.light {
            let hairline = if modal.glass {
                modal.hairline
            } else {
                modal_rgba(0x000000, 0.14)
            };
            let (surface, raised, raised_hover) = if modal.glass {
                (modal.surface, modal.raised, modal.raised_hover)
            } else {
                (rgb(0xf5f5f5), rgb(0xffffff), rgb(0xe9e9e9))
            };
            return Self {
                modal,
                light: true,
                glass: modal.glass,
                surface,
                raised,
                raised_hover,
                hairline,
                foreground: rgb(0x262626),
                muted: rgb(0x626262),
                primary: rgb(0x262626),
                destructive: rgb(0xb91c1c),
                // The plain-light theme's `--ghostex-accent` is the neutral #262626.
                settings_accent: css_mix(rgb(0x262626), 0.5, rgb(0x174564)),
                focus_border: css_mix(rgb(0x262626), 0.58, hairline),
                popup_background: rgb(0xffffff),
                popup_border: modal_rgba(0x000000, 0.16),
                popup_selected: rgb(0xdedede),
                popup_selected_foreground: rgb(0x171717),
                popup_hover: rgb(0xe9e9e9),
                ring: rgb(0x737373),
            };
        }
        let hairline = modal_rgba(0xffffff, 0.08);
        Self {
            modal,
            light: false,
            glass: false,
            surface: rgb(0x0d0d0d),
            raised: rgb(0x161616),
            raised_hover: rgb(0x1d1d1d),
            hairline,
            foreground,
            muted: modal.muted,
            primary: rgb(0xe5e5e5),
            destructive: modal.destructive,
            settings_accent: rgb(DEFAULT_GHOSTEX_ACCENT),
            focus_border: css_mix(foreground, 0.58, hairline),
            popup_background: rgb(0x161616),
            popup_border: modal_rgba(0xffffff, 0.08),
            popup_selected: rgb(0x2c2c2c),
            popup_selected_foreground: rgb(0xffffff),
            popup_hover: rgb(0x232323),
            ring: rgb(0x737373),
        }
    }

    /// `color-mix(in srgb, var(--foreground) <weight>, transparent)`.
    pub(crate) fn foreground_alpha(&self, weight: f32) -> Rgba {
        css_fade(self.foreground, weight)
    }

    /// `color-mix(in srgb, var(--muted-foreground) 72%, var(--background))`: resting rail subsections.
    pub(crate) fn rail_dim(&self) -> Rgba {
        css_mix(self.muted, 0.72, self.surface)
    }

    /// `color-mix(in srgb, var(--muted-foreground) 82%, var(--foreground) 18%)`: the modified asterisk.
    pub(crate) fn modified_marker(&self) -> Rgba {
        css_mix(self.muted, 0.82, self.foreground)
    }

    /// The surface of a dialog drawn inside Settings over its backdrop: the theme's own colour (the
    /// tinted chrome, opaque even under glass) lifted a step toward the ink's opposite, so the sheet
    /// reads as a raised pane of the theme instead of a black box.
    ///
    /// CDXC:Settings 2026-11-03 DECISION:
    /// User, of the Remote > Add a machine dialog drawn solid black with near-black fields: "make this not black bg like this, should fit the theme better please with transparency when it's enabled". Every dialog drawn inside Settings takes this themed surface and [`Self::nested_dialog`]'s ink washes for its fields, switches and segmented controls. It stays opaque under glass: a GPUI window blurs only what is behind the window, so a see-through sheet here would show the Settings page's text unblurred through it, the read-through the user turned down for floating surfaces on 2026-10-25.
    pub(crate) fn dialog_surface(&self) -> Rgba {
        let base = Rgba {
            a: 1.0,
            ..self.modal.solid_surface
        };
        if self.light {
            css_mix(rgb(0xffffff), 0.6, base)
        } else {
            css_mix(rgb(0xffffff), 0.055, base)
        }
    }

    /// This palette for the body of a dialog drawn inside Settings: the dialog's surface
    /// ([`Self::dialog_surface`]) and the dialogs' ink washes (`frosted_dialog_washes`) for its
    /// inputs, raised controls and hairlines, so they follow the theme instead of the page's solid
    /// near-black fills.
    pub(crate) fn nested_dialog(&self) -> Self {
        let washes = frosted_dialog_washes(self.light);
        Self {
            surface: self.dialog_surface(),
            raised: washes.raised,
            raised_hover: washes.raised_hover,
            hairline: washes.hairline,
            focus_border: css_mix(self.foreground, 0.58, washes.hairline),
            ..*self
        }
    }

    /// `color-mix(in srgb, var(--settings-raised) 82%, transparent)`: text inputs.
    pub(crate) fn input_background(&self) -> Rgba {
        css_fade(self.raised, 0.82)
    }

    /// `bg-input/90`: the slider track.
    pub(crate) fn slider_track(&self) -> Rgba {
        css_fade(self.hairline, 0.9)
    }
}
