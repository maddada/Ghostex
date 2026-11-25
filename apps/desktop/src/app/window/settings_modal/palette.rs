//! The Settings surface tokens (`.ghostex-settings-shadcn.settings-modal-dialog` in
//! packages/core-ui/styles.css (deleted 2026-11-06), the light block in styles/settings-light.css (deleted 2026-11-06) and the glass block
//! in styles/modals-glass.css (deleted 2026-11-06)), resolved from the app modal palette.
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
/// is the fixed `#0d0d0d` (CDXC:Settings 2026-09-09 DECISION in styles.css (deleted 2026-11-06)) and its light surface
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

/// How much of what is behind it a Settings dialog's fill covers under glass
/// (CDXC:Settings 2026-11-11 on [`SettingsPalette::dialog_surface`]): nearly all of it, since a
/// dialog drawn inside the Settings window cannot blur that window's own page. Shared with the
/// notice window through the modal kit (`ModalPalette::lifted_dialog_fill`).
const GLASS_DIALOG_ALPHA: f32 = LIFTED_DIALOG_GLASS_ALPHA;

/// How far a Settings dialog's fill is lifted from the theme colour toward white, dark and light.
/// The see-through tint it replaces read as the popovers' fill ([`SettingsPalette::solid_dialog_surface`],
/// 5.5% and 60%) brightened by the page under it; this solid fill sits a smidge darker than that.
const DIALOG_LIFT_DARK: f32 = LIFTED_DIALOG_LIFT_DARK;
const DIALOG_LIFT_LIGHT: f32 = LIFTED_DIALOG_LIFT_LIGHT;

/// `--ghostex-accent` for the default tint: the sky accent the React modal host publishes before
/// the settings arrive.
const DEFAULT_GHOSTEX_ACCENT: u32 = 0x86d3f8;

impl SettingsPalette {
    pub(crate) fn resolve(modal: &ModalPalette) -> Self {
        let palette = Self::derive(modal).with_theme_roles();
        if crate::app::helpers::window_art_frosted().is_some() {
            palette.over_artwork()
        } else {
            palette
        }
    }

    /// Settings over an art theme's artwork: the window is the dark frosted glass the chat box and
    /// the sidebar card use (`frosted_glass_backdrop`, drawn by the window in shell.rs), and the
    /// rail, the section cards and the inputs are faint washes of the ink with hairline edges over
    /// it, instead of grey slabs.
    ///
    /// CDXC:Settings 2026-11-25 DECISION:
    /// User, of Settings while an art theme shows: "next the settings page make it like them respective add some bhlurs and all also make it look clean spacing and all accorning to our rest of the redesigns". Over an artwork Settings takes the frosted glass of the chat box and the floating sidebar, with see-through cards (4.5% ink, 3.5% in light mode), hovers (8%) and hairlines (8%); menus keep their own fills. Without an artwork it is unchanged.
    fn over_artwork(mut self) -> Self {
        let ink = |alpha: f32| {
            let value = if self.light { 0.0 } else { 1.0 };
            Rgba {
                r: value,
                g: value,
                b: value,
                a: alpha,
            }
        };
        self.surface = ink(0.0);
        self.raised = ink(if self.light { 0.035 } else { 0.045 });
        self.raised_hover = ink(0.08);
        self.hairline = ink(0.08);
        self
    }

    /// The theme roles over the derived colours (helpers/theme_roles.rs). Under glass, raised
    /// surfaces keep their translucent ink washes (`frosted_dialog_washes`) and dropdowns keep
    /// `lifted_menu_fill` so an opaque `control` role (`base+10%/80%`) never stacks over the
    /// frosted window into an opaque slab.
    fn with_theme_roles(mut self) -> Self {
        use crate::app::helpers::{theme_accent_rgba, theme_role_or_rgba};
        let light = self.light;
        self.hairline = theme_role_or_rgba("border", light, self.hairline);
        self.popup_border = theme_role_or_rgba("border", light, self.popup_border);
        self.muted = theme_role_or_rgba("mutedText", light, self.muted);
        self.foreground = theme_role_or_rgba("text", light, self.foreground);
        if !self.glass {
            self.raised = self.modal.panel;
            self.raised_hover = self.modal.raised;
            self.popup_hover = theme_role_or_rgba("hover", light, self.popup_hover);
            self.popup_background = theme_role_or_rgba("menu", light, self.popup_background);
        }
        self.settings_accent = theme_accent_rgba(light);
        self.focus_border = theme_role_or_rgba("focusRing", light, self.focus_border);
        self.ring = theme_role_or_rgba("focusRing", light, self.ring);
        self.destructive = theme_role_or_rgba("danger", light, self.destructive);
        self.popup_selected = theme_role_or_rgba("selection", light, self.popup_selected);
        self
    }

    fn derive(modal: &ModalPalette) -> Self {
        let modal = *modal;
        let foreground = modal.foreground;
        let ink = rgb(if modal.light { 0x000000 } else { 0xffffff });
        if modal.glass {
            let popup = modal.lifted_menu_fill();
            return Self {
                modal,
                light: modal.light,
                glass: true,
                surface: modal.surface,
                raised: modal.raised,
                raised_hover: modal.raised_hover,
                hairline: modal.hairline,
                foreground,
                muted: modal.muted,
                primary: modal.primary,
                destructive: modal.destructive,
                settings_accent: modal.primary,
                focus_border: css_mix(foreground, 0.58, modal.hairline),
                popup_background: popup,
                popup_border: modal.hairline,
                popup_selected: css_mix(ink, 0.14, popup),
                popup_selected_foreground: foreground,
                popup_hover: css_mix(ink, 0.085, popup),
                ring: modal.focus_border,
            };
        }
        // Glass off: use the theme's own opaque elevated surface (`modal.solid_surface`) and
        // stepped panels/controls rather than fixed #0d0d0d / #f5f5f5 slabs.
        let popup = modal.menu_background;
        Self {
            modal,
            light: modal.light,
            glass: false,
            surface: modal.solid_surface,
            raised: modal.panel,
            raised_hover: modal.raised,
            hairline: modal.hairline,
            foreground,
            muted: modal.muted,
            primary: modal.primary,
            destructive: modal.destructive,
            settings_accent: modal.primary,
            focus_border: css_mix(foreground, 0.58, modal.hairline),
            popup_background: popup,
            popup_border: modal.menu_border,
            popup_selected: modal.card_selected_background(),
            popup_selected_foreground: foreground,
            popup_hover: modal.raised_hover,
            ring: modal.focus_border,
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
    /// tinted chrome) lifted a step toward the ink's opposite, so the sheet reads as a raised pane
    /// of the theme instead of a black box. Under glass it covers [`GLASS_DIALOG_ALPHA`] of the page,
    /// which the dialog's backdrop veils in the theme's colour (fields/nested_glass.rs).
    ///
    /// CDXC:Settings 2026-11-03 DECISION:
    /// User, of the Remote > Add a machine dialog drawn solid black with near-black fields: "make this not black bg like this, should fit the theme better please with transparency when it's enabled". Every dialog drawn inside Settings takes this themed surface and [`Self::nested_dialog`]'s ink washes for its fields, switches and segmented controls.
    ///
    /// CDXC:Settings 2026-11-08 DECISION:
    /// User, of the Playbook overrides editor still drawn as an opaque near-black panel over the glass Settings window: "this isn't transparent like it should be look at other dialogs in settings please", next to the frosted New Coordinator dialog. This supersedes the 2026-11-03 rule that the sheet stays opaque under glass (a GPUI window blurs only what is behind the window, so a see-through sheet showed the page's text sharply through it). Under glass every dialog nested in Settings is a frosted tint like the app's own glass dialogs. Asked whether to hide the page behind an open dialog or keep it dimmed (Q1996), the user answered "dim page": the page stays in view behind the see-through sheet, and under glass the backdrop is a veil of the theme's own colour instead of the black dim (`nested_glass::nested_backdrop`), so its text never reads sharply through the tint and a light theme does not turn grey. Popovers and the Pick Color dialog, drawn without a backdrop, keep the opaque [`Self::solid_dialog_surface`].
    ///
    /// CDXC:Settings 2026-11-11 DECISION:
    /// User, of the Remote computer dialog with the page's rows reading straight through its form: "this one is too see through please make it more blurred bg behind it and make its color smidge darker. do same for all similar modals in settings pls they should all have 1 look basically". A dialog drawn inside the Settings window is a panel of that window, and GPUI can only blur what is behind a whole window (the frosted menus and tooltips get their blur from a window of their own, frosted_host.rs), so the panel cannot blur the page under it. It takes the solid reading of the app's frosted dialogs instead: the theme colour lifted toward white ([`DIALOG_LIFT_DARK`], [`DIALOG_LIFT_LIGHT`]: a smidge darker than the old tint read with the page under it, never black) covering [`GLASS_DIALOG_ALPHA`] of the page under glass, so no text reads through it. Every dialog a Settings page opens draws on this fill through `fields::settings_dialog_surface` (one border, radius and shadow). The scrim behind it is unchanged. Supersedes the 2026-11-08 see-through tint (42% dark, 55% light).
    pub(crate) fn dialog_surface(&self) -> Rgba {
        let base = Rgba {
            a: 1.0,
            ..self.modal.solid_surface
        };
        let solid = if self.light {
            css_mix(rgb(0xffffff), DIALOG_LIFT_LIGHT, base)
        } else {
            css_mix(rgb(0xffffff), DIALOG_LIFT_DARK, base)
        };
        if !self.glass {
            return solid;
        }
        Rgba {
            a: GLASS_DIALOG_ALPHA,
            ..solid
        }
    }

    /// [`Self::dialog_surface`] kept opaque under glass: a popover or picker drawn over the page
    /// without hiding it.
    pub(crate) fn solid_dialog_surface(&self) -> Rgba {
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
        .with_theme_roles()
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
