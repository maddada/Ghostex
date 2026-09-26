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

/// The glass flag alone, for the pages that keep their own theme (the React app-modal host and
/// Search by Prompt); `installWindowGlassFlag` in apps/desktop/views/workarea-theme.ts reads it.
pub(crate) fn window_glass_flag_script(glass: bool) -> String {
    format!(
        "window.ghostexGpui = window.ghostexGpui || {{}}; window.ghostexGpui.workareaGlass = {glass}; window.dispatchEvent(new CustomEvent('ghostex-workarea-theme-changed', {{detail: {{glass: {glass}}}}}));"
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
