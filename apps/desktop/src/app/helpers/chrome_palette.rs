use std::sync::atomic::Ordering;

use gpui::{Rgba, rgb};

use super::titlebar::CHROME_LIGHT_APPEARANCE;

/// CDXC:Theming 2026-09-13 DECISION:
/// User: titlebar dropdowns, the Browser header, companion pane chrome and command terminal tabs must work in light mode.
/// Use the resolved app appearance for both foregrounds and surfaces, independently of terminal content themes.
pub(crate) fn chrome_color(dark: u32, light: u32) -> Rgba {
    rgb(if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        light
    } else {
        dark
    })
}

pub(crate) fn chrome_ink() -> Rgba {
    chrome_color(0xffffff, 0x000000)
}

/// True while the app's chrome is on its light appearance. For a colour, prefer `chrome_color`;
/// this is for the places where light mode needs a different alpha, shadow or nothing at all rather
/// than a different hue.
pub(crate) fn chrome_uses_light_appearance() -> bool {
    CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed)
}

/// CDXC:Theming 2026-09-13 SEE-ALSO:
/// apps/desktop/views/workarea-theme.ts consumes this appearance-only event in Docs, Kanban and Automate.
/// CDXC:Theming 2026-09-22 DECISION:
/// User: the theme also colours the Docs, Kanban and Automate views. The event now carries the
/// resolved chrome colour and the content colour (the chat's step off it) next to the appearance,
/// and the pages publish them as `--app-chrome-background` / `--app-background`. `glass` says
/// whether the page is a card on window glass, which the pages publish as `data-window-glass`.
pub(crate) fn workarea_theme_script(light: bool, chrome: u32, content: u32, glass: bool) -> String {
    let theme = if light { "light" } else { "dark" };
    format!(
        "window.ghostexGpui = window.ghostexGpui || {{}}; window.ghostexGpui.workareaTheme = '{theme}'; window.ghostexGpui.workareaChrome = '#{chrome:06x}'; window.ghostexGpui.workareaContent = '#{content:06x}'; window.ghostexGpui.workareaGlass = {glass}; window.dispatchEvent(new CustomEvent('ghostex-workarea-theme-changed', {{detail: {{theme: '{theme}', chrome: '#{chrome:06x}', content: '#{content:06x}', glass: {glass}}}}}));"
    )
}

/// CDXC:Theming 2026-09-13 DECISION:
/// User: the background before a pane loads must be white in light mode.
/// CEF's initial paint and the native placeholder must agree to avoid a dark flash before page content arrives.
pub(crate) fn pane_prepaint_background_color() -> u32 {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        0xffffffff
    } else {
        crate::app::consts::CEF_DARK_PREPAINT_BACKGROUND_COLOR
    }
}

/// CDXC:Theming 2026-11-20 DECISION:
/// User, of the chat's Copy / Add to Chat / Reply bar drawn as a near-black pill over their glass
/// window: "i hate when ui elements in the app are this dark bg as you show here, doesn't fit the
/// rest of the glass aesthetic. need to make the bg of these less dark when the transparency i
/// enable isn't dark", then, shown 39 such spots side by side (dark-fills audit page, Q4219 to
/// Q4225, Q4247 "Lift the ticked ones"), lifted 38 of them; only the floating Commands pane keeps
/// its dark fill. Under window glass a surface drawn inside a window that cannot blur what is under
/// it (a floating bar, menu, popover, tooltip, sheet or card over content) takes the colour of the
/// window it sits on, lifted toward white ([`glass_lifted_from`]), at [`GLASS_LIFTED_ALPHA`] so text
/// under it never reads through (CDXC:Tooltips 2026-10-25). It is never the menu colour or a fixed
/// near-black. A fill with nothing under it (a box inside a card, a badge, a code box) takes the
/// ink wash of the fenced code blocks ([`glass_wash`]). Glass off, every spot keeps its colour.
/// SEE-ALSO: `glass_lifted_surface` and `glass_veil` in helpers/window_glass.rs (the work area's
/// own colour), ModalPalette::lifted_dialog_fill (the same lift inside dialogs and Settings).
///
/// CDXC:Theming 2026-11-20 DECISION:
/// User, after rebuilding with the first 10% lift (the chat selection bar, a dialog dropdown):
/// "Too dark". The lift is 18%: clearly lighter (about #5a6676 over their slate glass, #393939
/// over a neutral #0e0e0e), and the highest step that keeps the chat's #e5e5e5 body text at 4.5:1
/// or better on their slate (20% drops it to 4.4).
pub(crate) const GLASS_LIFT_DARK: f32 = 0.18;
/// [`GLASS_LIFT_DARK`] in light mode, where the window colour is already light.
pub(crate) const GLASS_LIFT_LIGHT: f32 = 0.6;
/// How much of what is under it a lifted surface covers: nearly all of it, as Settings' inner
/// dialogs (`LIFTED_DIALOG_GLASS_ALPHA`).
pub(crate) const GLASS_LIFTED_ALPHA: f32 = 0.96;

/// `base` (the colour of the window the surface sits on) lifted toward white by
/// [`GLASS_LIFT_DARK`] (or [`GLASS_LIFT_LIGHT`] in light mode), at [`GLASS_LIFTED_ALPHA`].
pub(crate) fn glass_lifted_from(base: gpui::Hsla, light: bool) -> gpui::Hsla {
    let lift = if light {
        GLASS_LIFT_LIGHT
    } else {
        GLASS_LIFT_DARK
    };
    // `alpha`, not `opacity`: `Hsla::opacity` multiplies the alpha, so `opacity(1.0)` left a
    // see-through base (and the "opaque" lift below at 96%).
    base.alpha(1.0)
        .blend(gpui::white().opacity(lift))
        .alpha(GLASS_LIFTED_ALPHA)
}

/// [`glass_lifted_from`] kept opaque, for a surface whose content must not show anything through
/// it (a × over a picture, Capture's windows over other apps).
pub(crate) fn glass_lifted_opaque_from(base: gpui::Hsla, light: bool) -> gpui::Hsla {
    glass_lifted_from(base, light).alpha(1.0)
}

/// The ink wash a box with nothing under it takes on glass (the fenced code blocks' 6% light wash
/// in dark mode, 4% dark in light mode), times `strength`.
pub(crate) fn glass_wash(light: bool, strength: f32) -> gpui::Hsla {
    if light {
        gpui::black().opacity(0.04 * strength)
    } else {
        gpui::white().opacity(0.06 * strength)
    }
}
