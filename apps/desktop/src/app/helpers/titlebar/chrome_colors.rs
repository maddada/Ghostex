use std::sync::atomic::Ordering;

use gpui::{App, Hsla, px, rgb};
use gpui_component::{Theme, ThemeMode};

use crate::app::helpers::*;
use crate::*;

pub(crate) static CHROME_LIGHT_APPEARANCE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

static SYSTEM_LIGHT_APPEARANCE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(crate) fn refresh_gpui_system_appearance(cx: &App) -> bool {
    let light = cef::system_page_color_scheme()
        .map(|scheme| scheme == "light")
        .unwrap_or(matches!(
            cx.window_appearance(),
            gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight
        ));
    SYSTEM_LIGHT_APPEARANCE.store(light, Ordering::Relaxed);
    light
}

pub(crate) fn gpui_system_uses_light_appearance() -> bool {
    cef::system_page_color_scheme()
        .map(|scheme| scheme == "light")
        .unwrap_or_else(|| SYSTEM_LIGHT_APPEARANCE.load(Ordering::Relaxed))
}

pub(crate) fn sidebar_uses_light_theme(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    match object
        .get("sidebarTheme")
        .and_then(serde_json::Value::as_str)
    {
        Some("plain-light") => true,
        Some("system") | None => gpui_system_uses_light_appearance(),
        _ => false,
    }
}

fn titlebar_uses_light_theme() -> bool {
    CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed)
}

fn titlebar_overlay_base() -> gpui::Rgba {
    rgb(if titlebar_uses_light_theme() {
        0x000000
    } else {
        0xffffff
    })
}

pub(crate) fn titlebar_background() -> Hsla {
    rgb(GPUI_TITLEBAR_BACKGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

/*
CDXC:Theming 2026-07-22:
The sidebar paints its shared gradient stops (darker top stop, lighter bottom stop) so the chrome
reads as one continuous surface. Solid consumers (popup borders, modal host fills) keep
`titlebar_background()`. The horizontal variant the titlebar strip painted went with the strip; the
workarea header that replaced it paints the workspace background so it has no edge against content.
*/
pub(crate) fn sidebar_chrome_gradient_fill(angle: f32) -> gpui::Background {
    gpui::linear_gradient(
        angle,
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_LEFT_RGB.load(Ordering::Relaxed) as u32),
            0.,
        ),
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.load(Ordering::Relaxed) as u32),
            1.,
        ),
    )
}

/// The colour the sidebar's chrome gradient starts from at its top edge, for the machine tabs' fade.
pub(crate) fn sidebar_chrome_gradient_top_color() -> Hsla {
    rgb(GPUI_TITLEBAR_GRADIENT_LEFT_RGB.load(Ordering::Relaxed) as u32).into()
}

/// The colour the sidebar's chrome gradient reaches at its bottom edge, for the list's fade ramp,
/// which has to fade into the sidebar there rather than sit on a flat fill.
pub(crate) fn sidebar_chrome_gradient_bottom_color() -> Hsla {
    rgb(GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.load(Ordering::Relaxed) as u32).into()
}

/// The line that shows where a dragged view-strip tab would land (see `view_strip_drop_line`).
pub(crate) fn view_strip_drop_line_color() -> Hsla {
    rgb(if titlebar_uses_light_theme() {
        0xb9d8fa
    } else {
        0xffffff
    })
    .into()
}

pub(crate) fn titlebar_button_border_color() -> Hsla {
    rgb(if titlebar_uses_light_theme() {
        0xd4d4d4
    } else {
        0x252525
    })
    .into()
}

pub(crate) fn titlebar_button_hover_color() -> Hsla {
    titlebar_overlay_base().opacity(0.08).into()
}

pub(crate) fn titlebar_active_segment_color() -> Hsla {
    titlebar_overlay_base().opacity(0.11).into()
}

/// The popup menu fill for the current appearance, derived from the resolved chrome background by
/// `refresh_gpui_visual_settings` (see `sidebar_titlebar_menu_background_for_chrome`).
pub(crate) static GPUI_MENU_BACKGROUND_RGB: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x171717);

pub(crate) fn titlebar_popup_menu_background() -> Hsla {
    rgb(GPUI_MENU_BACKGROUND_RGB.load(Ordering::Relaxed)).into()
}

/// The surface of a menu in a window of its own: the sidebar's dropdown menus' fill, frosted under
/// window glass (`popup_window_surface`).
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User, of the chat box's pill strip: "these panels in the pills bar above the composer are darker than this menu. please make those pill panels have same bg as the dropdown menu in the sidebar". The sidebar's menus and every menu the strip opens (Resources, Workers, Tasks, Subagents, Background and Links) take this one surface. Supersedes the 2026-10-29 decision that put the strip's menus on the Links list's chrome-coloured fill.
pub(crate) fn popup_menu_surface() -> Hsla {
    popup_window_surface(titlebar_popup_menu_background())
}

/// The corner radius of a header dropdown's panel and of the window that blurs behind it.
pub(crate) const POPUP_MENU_RADIUS: f32 = 8.0;

/// The whole frame of a header dropdown: [`popup_menu_surface`], the menus' hairline
/// ([`titlebar_popup_menu_border_color`]) and [`POPUP_MENU_RADIUS`]. The Commit dropdown and every
/// other titlebar menu draw on it (`titlebar_popup_menu_with_scroll_behavior`), and the Share and
/// Jobs popovers take it whole (CDXC:Collab 2026-11-11 in window/share_popover_view.rs).
pub(crate) fn popup_menu_frame() -> crate::app::window::share_popover_view::PopupSurface {
    crate::app::window::share_popover_view::PopupSurface {
        fill: popup_menu_surface(),
        border: titlebar_popup_menu_border_color(),
        radius: POPUP_MENU_RADIUS,
    }
}

pub(crate) fn titlebar_popup_menu_foreground() -> Hsla {
    rgb(if titlebar_uses_light_theme() {
        0x292929
    } else {
        0xfcfcfc
    })
    .into()
}

/// A menu's destructive red: the native modals' `destructive` colour (`ModalPalette`) for the
/// menu's light or dark theme.
pub(crate) fn titlebar_popup_menu_destructive() -> Hsla {
    rgb(if titlebar_uses_light_theme() {
        0xb91c1c
    } else {
        0xf87171
    })
    .into()
}

/// CDXC:Theming 2026-09-23 DECISION:
/// User: the hovered row of a menu read as a flat grey slab on the tinted menus ("hovered menu item color is ugly"). It is a wash of the menu's own ink instead, so it carries the theme's tint and works on frosted menus too.
pub(crate) fn titlebar_popup_menu_hover_color() -> Hsla {
    titlebar_overlay_base()
        .opacity(if titlebar_uses_light_theme() {
            0.06
        } else {
            0.08
        })
        .into()
}

/// A menu's or tooltip's outline: a faint ink line, fainter still on the frosted menus of window
/// glass, where it only has to catch the edge of the blur.
pub(crate) fn titlebar_popup_menu_border_color() -> Hsla {
    titlebar_overlay_base()
        .opacity(if window_glass_active() { 0.10 } else { 0.12 })
        .into()
}

pub(crate) fn apply_gpui_component_theme(cx: &mut App) {
    let mode = if titlebar_uses_light_theme() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
    let theme = Theme::global_mut(cx);
    theme.popover = titlebar_popup_menu_background();
    // CDXC:Theming 2026-09-23 DECISION: User: tooltips "dont fit the glass look". gpui-component's
    // tooltip paints `tokens.popover`, which kept the stock near-black, so it now takes the same
    // tinted menu colour as the app's menus.
    // Under glass tooltips draw in the frosted tooltip window, which paints this token at the
    // frosted alpha, so it takes the same lifted colour as the other frosted menus.
    // Without glass dark mode darkens it (`tooltip_background`); under glass it is the frosted
    // menus' material (CDXC:Tooltips 2026-10-15).
    theme.tokens.popover = if window_glass_active() {
        frosted_tooltip_fill(titlebar_popup_menu_background())
            .opacity(1.0)
            .into()
    } else {
        tooltip_background(titlebar_popup_menu_background()).into()
    };
    theme.popover_foreground = titlebar_popup_menu_foreground();
    theme.border = titlebar_popup_menu_border_color();
    gpui_component::tooltip::set_frosted_tooltip_alpha(frosted_tooltip_alpha());
    theme.radius = px(2.0);
    theme.scrollbar = gpui::transparent_black();
    theme.scrollbar_mode = gpui_component::scroll::ScrollbarMode::Hover;
    // CDXC:DesignSystem 2026-09-16 SEE-ALSO:
    // Exact app scrollbar colors are shared with packages/components/ui/scrollbar-theme.css.
    let thumb: Hsla = gpui::rgb(if titlebar_uses_light_theme() {
        0xbcbcbd
    } else {
        0x424346
    })
    .into();
    theme.tokens.scrollbar_thumb = thumb.into();
    theme.tokens.scrollbar_thumb_hover = thumb.into();
    crate::app::native_docs::data_table::apply_table_theme(theme, titlebar_uses_light_theme());
}

pub(crate) fn titlebar_popup_menu_disabled_text_color() -> Hsla {
    titlebar_overlay_base().opacity(0.42).into()
}

pub(crate) fn titlebar_popup_menu_preview_text_color() -> Hsla {
    titlebar_overlay_base().opacity(0.48).into()
}

pub(crate) fn titlebar_popup_git_section_label_color() -> Hsla {
    titlebar_overlay_base().opacity(0.55).into()
}

pub(crate) fn titlebar_popup_git_disabled_icon_color() -> Hsla {
    titlebar_overlay_base().opacity(0.42).into()
}

pub(crate) fn titlebar_popup_git_additions_color() -> Hsla {
    rgb(0x4ade80).into()
}

pub(crate) fn titlebar_popup_git_deletions_color() -> Hsla {
    rgb(0xf87171).into()
}

pub(crate) fn titlebar_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.84)
        .into()
}

pub(crate) fn titlebar_project_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.92)
        .into()
}

pub(crate) fn titlebar_active_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

pub(crate) fn titlebar_inactive_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.68)
        .into()
}

pub(crate) fn titlebar_disabled_text_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.30)
        .into()
}

pub(crate) fn titlebar_icon_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32)
        .opacity(0.84)
        .into()
}

pub(crate) fn titlebar_icon_hover_color() -> Hsla {
    rgb(GPUI_TITLEBAR_FOREGROUND_RGB.load(Ordering::Relaxed) as u32).into()
}

pub(crate) fn command_pane_titlebar_separator_color() -> Hsla {
    /*
    CDXC:CommandPane 2026-06-25-13:19:
    Native command titlebar separators use `calibratedWhite:0.54 alpha:0.24`, which is lighter and more translucent than the inactive command pane outline.
    */
    rgb(0x8a8a8a).opacity(0.24).into()
}
