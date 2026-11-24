use gpui::{Hsla, Rgba, Styled as _, px, rgb, svg};

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

pub(crate) const ICON_SELECTOR: &str = "modals/kit/selector.svg";
pub(crate) const ICON_LOADER: &str = "modals/kit/loader-2.svg";

/// The `.gx-app-modal` tokens plus the shadcn theme tokens the modals read,
/// resolved for one appearance. Dark values come from modals.css (deleted 2026-11-06) and
/// shadcn.css, light values from modals-light.css (deleted 2026-11-06).
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

/// Linearized sRGB relative luminance (WCAG 2.1).
fn wcag_luminance(color: Rgba) -> f32 {
    let channel = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
}

/// High-contrast dark (`#141518`) or white (`#ffffff`) ink for text over a filled accent control.
pub(crate) fn contrast_ink_on(fill: Rgba) -> Rgba {
    // Perceptual threshold in linearized sRGB luminance: keeps crisp white ink on saturated
    // medium-dark blues/ochres/cinnabar (Solarized #268bd2, Tokyo Night Day #2e7de9, Gruvbox
    // Light #b47109, Inkstone #d8664f) and dark ink on pastel/bright accents (#95d7f6, #8da101,
    // Catppuccin, Dracula, Nord, Everforest).
    if wcag_luminance(fill) > 0.28 {
        rgb(0x141518)
    } else {
        rgb(0xffffff)
    }
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

/// The ink washes of a dialog or panel on the popover glass (`frosted_dialog_fill`): a section
/// panel, a field or raised control at rest and hovered (also a selected row), and the hairline.
/// One set for the form dialogs, Settings, the Agents Hub, Search by Prompt, Quick Access and the
/// New Thread picker, so their fields and rows read the same on the thinner glass.
#[derive(Clone, Copy)]
pub(crate) struct FrostedDialogWashes {
    pub(crate) panel: Rgba,
    pub(crate) raised: Rgba,
    pub(crate) raised_hover: Rgba,
    pub(crate) hairline: Rgba,
}

pub(crate) fn frosted_dialog_washes(light: bool) -> FrostedDialogWashes {
    let ink = if light { 0x000000 } else { 0xffffff };
    let (panel, raised, raised_hover, hairline) = if light {
        (0.04, 0.07, 0.11, 0.16)
    } else {
        (0.05, 0.09, 0.12, 0.12)
    };
    FrostedDialogWashes {
        panel: modal_rgba(ink, panel),
        raised: modal_rgba(ink, raised),
        raised_hover: modal_rgba(ink, raised_hover),
        hairline: modal_rgba(ink, hairline),
    }
}

/// A filled button's background at rest, under the pointer and while pressed.
#[derive(Clone, Copy)]
pub(crate) struct ButtonFills {
    pub(crate) rest: Rgba,
    pub(crate) hover: Rgba,
    pub(crate) pressed: Rgba,
}

/// The neutral filled button every secondary text button in the app modals uses: a wash of the
/// ink (white in dark, black in light) over whatever it sits on, so it reads as a solid button on
/// the window surface, a section panel, a tinted chrome or window glass alike.
///
/// CDXC:AppModal 2026-10-14 DECISION:
/// User: "I don't want any outline buttons all over the app in those modals. Users don't know these are clickable, and an outline button beside a filled button looks wrong, so always use filled buttons everywhere." Every text button in a modal, dialog or Settings page is filled: the primary keeps its own fill, secondary actions take this neutral fill and destructive ones [`destructive_button_fills`], never a border-only button. This supersedes the 2026-10-09 decision that every labelled button draws an outline and the 2026-09-09 Settings rule that the default button is the quiet bordered one (Settings still has no white buttons: its default button takes this neutral fill). Icon-only ghost buttons, menu rows, links in running text, segmented controls and switches keep their own look.
pub(crate) fn neutral_button_fills(light: bool) -> ButtonFills {
    let ink = if light { 0x000000 } else { 0xffffff };
    let (rest, hover, pressed) = if light {
        (0.07, 0.11, 0.15)
    } else {
        (0.10, 0.14, 0.18)
    };
    ButtonFills {
        rest: modal_rgba(ink, rest),
        hover: modal_rgba(ink, hover),
        pressed: modal_rgba(ink, pressed),
    }
}

/// The filled destructive button (Remove, Delete, Disable): a tint of the destructive colour under
/// the destructive label, one step stronger on hover and again while pressed.
pub(crate) fn destructive_button_fills(destructive: Rgba, light: bool) -> ButtonFills {
    let (rest, hover, pressed) = if light {
        (0.10, 0.16, 0.22)
    } else {
        (0.18, 0.26, 0.32)
    };
    ButtonFills {
        rest: rgba_of(destructive, rest),
        hover: rgba_of(destructive, hover),
        pressed: rgba_of(destructive, pressed),
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
    /// `.gx-app-modal` tokens in packages/core-ui/styles/modals.css (deleted 2026-11-06), which hold the user's decision.
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
        self.with_theme_roles()
    }

    /// [`Self::frosted`] for a dialog or panel on the thinner popover glass (CDXC:AppModal
    /// 2026-10-29 on `frosted_dialog_fill`): its fields, section panels and hairlines take the
    /// stronger [`frosted_dialog_washes`], so they still read as panes over a backdrop that shows
    /// through more.
    pub(crate) fn frosted_dialog(self, fill: Rgba) -> Self {
        let mut palette = self.frosted(fill);
        let washes = frosted_dialog_washes(palette.light);
        palette.panel = washes.panel;
        palette.raised = washes.raised;
        palette.raised_hover = washes.raised_hover;
        palette.hairline = washes.hairline;
        palette.with_theme_roles()
    }

    /// The theme roles over the derived colours (helpers/theme_roles.rs): Borders paints the
    /// hairlines and menu borders, Grey text the muted text, Accent the primary button and
    /// checked controls, Cards/Controls/Hover the opaque fills when glass is off (under glass
    /// panels and fields keep translucent ink washes so they never stack into an opaque slab),
    /// Menus the menus, Focus ring the focused field's border, and Success and Danger their
    /// colours. A role not set keeps the derived colour.
    pub(crate) fn with_theme_roles(mut self) -> Self {
        use crate::app::helpers::{theme_accent_rgba, theme_role_or_rgba, theme_role_rgba};
        let light = self.light;
        self.hairline = theme_role_or_rgba("border", light, self.hairline);
        self.menu_border = theme_role_or_rgba("border", light, self.menu_border);
        self.muted = theme_role_or_rgba("mutedText", light, self.muted);
        self.foreground = theme_role_or_rgba("text", light, self.foreground);
        if !self.glass {
            if let Some(surface) = theme_role_rgba("surface", light) {
                self.panel = css_mix(rgba_of(surface, 1.0), surface.a, self.solid_surface);
            }
            if let Some(control) = theme_role_rgba("control", light) {
                self.raised = css_mix(rgba_of(control, 1.0), control.a, self.solid_surface);
            }
            if let Some(hover) = theme_role_rgba("hover", light) {
                self.raised_hover = css_mix(rgba_of(hover, 1.0), hover.a, self.raised);
                self.accent = self.raised_hover;
            }
        }
        self.menu_background = theme_role_or_rgba("menu", light, self.menu_background);
        self.focus_border = theme_role_or_rgba("focusRing", light, self.focus_border);
        self.destructive = theme_role_or_rgba("danger", light, self.destructive);
        self.success = theme_role_or_rgba("success", light, self.success);
        let primary = theme_accent_rgba(light);
        self.primary = primary;
        self.primary_foreground = contrast_ink_on(primary);
        self
    }

    pub(crate) fn resolve(light: bool, sidebar_theme: Option<&str>) -> Self {
        Self::derive(light, sidebar_theme).with_theme_roles()
    }

    fn derive(light: bool, sidebar_theme: Option<&str>) -> Self {
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

    /// The main button's fill (Save, Create, Rename, Switch to Native, Connect): uses the active
    /// theme's primary/accent colour in both dark and light modes, shifting subtly on hover and
    /// press, paired with WCAG-contrasting ink from [`Self::primary_button_text`].
    pub(crate) fn primary_button_fills(&self) -> ButtonFills {
        let target = if self.light {
            rgb(0x000000)
        } else {
            rgb(0xffffff)
        };
        ButtonFills {
            rest: self.primary,
            hover: css_mix(self.primary, 0.88, target),
            pressed: css_mix(self.primary, 0.78, target),
        }
    }

    /// The label, icon and spinner colour on [`Self::primary_button_fills`].
    pub(crate) fn primary_button_text(&self) -> Rgba {
        self.primary_foreground
    }

    /// The Windows modal window's system border: 16% ink over the surface, so it stays visible against the same-coloured app chrome.
    pub(crate) fn window_border(&self) -> Rgba {
        let ink = if self.light { 0x000000 } else { 0xffffff };
        css_mix(rgb(ink), 0.16, self.solid_surface)
    }

    /// The lifted dialog fill: the theme's own colour (`solid_surface`) lifted toward white
    /// ([`LIFTED_DIALOG_LIFT_DARK`], [`LIFTED_DIALOG_LIFT_LIGHT`]), covering
    /// [`LIFTED_DIALOG_GLASS_ALPHA`] of what is behind it when `glass`. Every dialog inside
    /// Settings draws on it (`SettingsPalette::dialog_surface`, CDXC:Settings 2026-11-11), and so
    /// the notice window under glass lifts the same way a step further ([`Self::lifted_notice`]).
    pub(crate) fn lifted_dialog_fill(&self, glass: bool) -> Rgba {
        let base = rgba_of(self.solid_surface, 1.0);
        let lift = if self.light {
            LIFTED_DIALOG_LIFT_LIGHT
        } else {
            LIFTED_DIALOG_LIFT_DARK
        };
        let solid = css_mix(rgb(0xffffff), lift, base);
        if glass {
            rgba_of(solid, LIFTED_DIALOG_GLASS_ALPHA)
        } else {
            solid
        }
    }

    /// The fill of a menu, dropdown or popover drawn inside a dialog or panel window under glass:
    /// the theme's own colour (`solid_surface`) lifted toward white a step past the dialog
    /// ([`LIFTED_MENU_LIFT_DARK`], [`LIFTED_MENU_LIFT_LIGHT`]) at [`LIFTED_DIALOG_GLASS_ALPHA`], so
    /// the open menu reads as a raised pane over the dialog and the page never reads through it.
    ///
    /// CDXC:Theming 2026-11-20: such a window cannot blur its own content, so under glass its
    /// menus take this lifted fill, never `menu_background` (#161616), `solid_surface` (#0e0e0e) or
    /// a fixed near-black. Glass off, every menu keeps its colour.
    pub(crate) fn lifted_menu_fill(&self) -> Rgba {
        let base = rgba_of(self.solid_surface, 1.0);
        let lift = if self.light {
            LIFTED_MENU_LIFT_LIGHT
        } else {
            LIFTED_MENU_LIFT_DARK
        };
        rgba_of(
            css_mix(rgb(0xffffff), lift, base),
            LIFTED_DIALOG_GLASS_ALPHA,
        )
    }

    /// The notice window's palette under window glass: the dialogs' frost and washes over the
    /// lifted dialog fill rather than the thin theme tint, so a notice reads as a raised pane of
    /// the theme and never as a black box over a dark chat.
    ///
    /// CDXC:AppModal 2026-11-20 DECISION:
    /// User, of the "This terminal's folder is missing" notice drawn near-black (the theme colour
    /// covering 60% of a blur, `#100e0e` over their dark chat), answered Q4234 "Lift it": only the
    /// notices lift, to the fill of the dialogs inside Settings (the theme lifted 7.5% toward white
    /// in dark mode, 55% in light, at 96%). The other frosted dialogs, menus and tooltips keep
    /// `frosted_dialog_fill`; glass off, the notice keeps the solid dialog palette.
    ///
    /// CDXC:AppModal 2026-11-20 DECISION:
    /// User, after rebuilding: the lifted fills were "Too dark". A notice lifts further than the
    /// dialogs inside Settings ([`LIFTED_NOTICE_LIFT_DARK`], 15%: #323232 over #0e0e0e instead of
    /// #202020), which keep the 7.5% they were approved at; light mode keeps the dialogs' lift.
    pub(crate) fn lifted_notice(self) -> Self {
        let lift = if self.light {
            LIFTED_DIALOG_LIFT_LIGHT
        } else {
            LIFTED_NOTICE_LIFT_DARK
        };
        let solid = css_mix(rgb(0xffffff), lift, rgba_of(self.solid_surface, 1.0));
        self.frosted_dialog(rgba_of(solid, LIFTED_DIALOG_GLASS_ALPHA))
    }
}

/// How far the lifted dialog fill moves the theme colour toward white, dark and light
/// ([`ModalPalette::lifted_dialog_fill`]).
pub(crate) const LIFTED_DIALOG_LIFT_DARK: f32 = 0.075;
/// How far a notice's fill moves the theme colour toward white in dark mode
/// ([`ModalPalette::lifted_notice`]).
pub(crate) const LIFTED_NOTICE_LIFT_DARK: f32 = 0.15;
pub(crate) const LIFTED_DIALOG_LIFT_LIGHT: f32 = 0.55;

/// How far a menu inside a dialog or panel window lifts the theme colour toward white under glass
/// ([`ModalPalette::lifted_menu_fill`]): a bit lighter than the dialog, as the work area's lifted
/// surfaces (`GLASS_LIFT_DARK`, `GLASS_LIFT_LIGHT` in helpers/chrome_palette.rs).
pub(crate) const LIFTED_MENU_LIFT_DARK: f32 = 0.18;
pub(crate) const LIFTED_MENU_LIFT_LIGHT: f32 = 0.6;

/// How much of what is behind it the lifted dialog fill covers under glass: nearly all of it, so
/// text behind a dialog that cannot blur its own window never reads through.
pub(crate) const LIFTED_DIALOG_GLASS_ALPHA: f32 = 0.96;

#[cfg(test)]
mod tests {
    use super::*;

    /// A form dialog on the popover glass keeps the solid surface for its dropdowns and draws its
    /// fields, panels and hairlines stronger than a large panel's frost, in both appearances.
    #[test]
    fn frosted_dialog_strengthens_fields_on_the_thinner_glass() {
        for light in [false, true] {
            let solid = ModalPalette::resolve(light, None).tinted(rgb(0x181818));
            let fill = rgba_of(solid.surface, 0.32);
            let panel = solid.frosted(fill);
            let dialog = solid.frosted_dialog(fill);
            assert!(dialog.glass);
            assert_eq!(dialog.surface, fill);
            assert_eq!(dialog.solid_surface, solid.solid_surface);
            assert!(dialog.raised.a > panel.raised.a);
            assert!(dialog.raised_hover.a > dialog.raised.a);
            assert!(dialog.panel.a > panel.panel.a);
            assert!(dialog.hairline.a > panel.hairline.a);
            let washes = frosted_dialog_washes(light);
            assert_eq!(dialog.raised, washes.raised);
            assert_eq!(dialog.hairline, washes.hairline);
        }
    }

    /// A notice under glass is the theme lifted toward white at 96%: lighter than the theme in dark
    /// mode (never the near-black tint), and keeps the dialogs' washes and solid dropdown surface.
    #[test]
    fn lifted_notice_raises_the_theme_toward_white() {
        for light in [false, true] {
            let solid = ModalPalette::resolve(light, None).tinted(rgb(0x0e0e0e));
            let notice = solid.lifted_notice();
            assert!(notice.glass);
            assert_eq!(notice.surface.a, LIFTED_DIALOG_GLASS_ALPHA);
            assert_eq!(notice.solid_surface, solid.solid_surface);
            assert!(notice.surface.r > solid.solid_surface.r);
            assert_eq!(notice.raised, frosted_dialog_washes(light).raised);
        }
        let dark = ModalPalette::resolve(false, None).tinted(rgb(0x0e0e0e));
        let fill = dark.lifted_dialog_fill(false);
        assert_eq!(fill.a, 1.0);
        // 7.5% white over #0e0e0e: about #202020.
        assert!((fill.r * 255.0 - 32.0).abs() < 1.5, "{fill:?}");
    }

    /// A menu inside a dialog under glass is the theme lifted a step past the dialog, at 96%:
    /// never the near-black menu colour or the theme chrome.
    #[test]
    fn lifted_menu_fill_sits_above_the_dialog() {
        for light in [false, true] {
            let solid = ModalPalette::resolve(light, None).tinted(rgb(0x0e0e0e));
            let menu = solid.lifted_menu_fill();
            assert_eq!(menu.a, LIFTED_DIALOG_GLASS_ALPHA);
            assert!(menu.r > solid.lifted_dialog_fill(true).r);
            assert!(menu.r >= solid.solid_surface.r);
        }
        let dark = ModalPalette::resolve(false, None).tinted(rgb(0x0e0e0e));
        let menu = dark.lifted_menu_fill();
        // 18% white over #0e0e0e: about #393939.
        assert!((menu.r * 255.0 - 57.4).abs() < 1.5, "{menu:?}");
        // A notice: 15% white over #0e0e0e, about #323232, above the Settings dialogs' #202020.
        let notice = dark.lifted_notice();
        assert!(
            (notice.surface.r * 255.0 - 50.2).abs() < 1.5,
            "{:?}",
            notice.surface
        );
        assert!(notice.surface.r > dark.lifted_dialog_fill(true).r);
    }
}
