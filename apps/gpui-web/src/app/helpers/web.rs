//! The browser's answers to helpers whose desktop versions call the operating system.
use std::path::PathBuf;

use crate::app::model::GpuiRemoteGxserverRequestTarget;
use serde_json::Value;

/// `prefers-reduced-motion`, which is where a browser exposes the system setting the desktop reads from AppKit.
pub(crate) fn gpui_macos_reduce_motion_enabled() -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .match_media("(prefers-reduced-motion: reduce)")
                .ok()
                .flatten()
        })
        .is_some_and(|query| query.matches())
}

/// The desktop plays a system sound on copy; a page may not play audio it was not asked for.
pub(crate) fn gpui_play_copy_sound() {}

pub(crate) fn gpui_random_uuid_string() -> Result<String, String> {
    web_sys::window()
        .and_then(|window| window.crypto().ok())
        .map(|crypto| crypto.random_uuid())
        .ok_or_else(|| "crypto.randomUUID is not available".to_string())
}

/// There is no state directory in a browser. The one caller keeps a small remembered flag there; its read fails softly and its write is dropped.
pub(crate) fn ghostex_state_root() -> PathBuf {
    PathBuf::from("/ghostex-web-state")
}

pub(crate) fn gpui_remote_install_unique_id() -> String {
    gpui_random_uuid_string().unwrap_or_default()
}

pub(crate) fn gpui_remote_gxserver_rpc_result(
    _target: &GpuiRemoteGxserverRequestTarget,
    _endpoint: &str,
    _params: &Value,
    _timeout: std::time::Duration,
) -> Result<Value, String> {
    Err("Remote machines are not available in the browser build yet.".to_string())
}

/// A remote machine's answer, shaped for the sidebar; the page refuses every remote request (`remote_conn/sidebar_rpc.rs`), so there is never one to shape.
pub(crate) fn gpui_remote_sidebar_response_payload(
    _path: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    result
}

pub(crate) struct GpuiExtensionViewPresentation {
    pub(crate) title: String,
}

/// Extension and custom views are read from installed payloads on disk, which the browser build does not have, so none is ever offered.
pub(crate) fn gpui_extension_view_presentation(
    _id: crate::app::model::ExtensionId,
) -> Option<GpuiExtensionViewPresentation> {
    None
}

/// The desktop also plays its copy feedback here; the page only writes the clipboard.
pub(crate) fn gpui_copy_to_clipboard(item: gpui::ClipboardItem, cx: &mut gpui::App) {
    cx.write_to_clipboard(item);
}

/// The desktop converts the picture to PNG and reads the clipboard back
/// (`helpers/copy_picture.rs`); the browser's clipboard write cannot be read back here, so the
/// page only writes it.
pub(crate) fn gpui_copy_picture_to_clipboard(picture: &gpui::Image, cx: &mut gpui::App) -> bool {
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(picture));
    true
}

/// The desktop shows the failure where "Copied!" would have shown; the page shows no copy
/// feedback.
pub(crate) fn gpui_copy_failed_feedback(_message: &'static str, _cx: &mut gpui::App) {}

/// Window glass blurs the desktop behind a native window; a canvas has nothing behind it to blur, so glass is never on here.
/// A link the app opens outside itself (`gpui_open_external_url` in the desktop's os_cli/open_target.rs):
/// the browser opens it in a new tab, so there is no Wayland display to restore.
pub(crate) fn gpui_open_external_url(url: &str, cx: &gpui::App) {
    cx.open_url(url);
}

pub(crate) fn window_glass_active() -> bool {
    false
}

pub(crate) fn window_glass_active_in(_window: &gpui::Window) -> bool {
    false
}

pub(crate) fn window_glass_active_for(_window: Option<gpui::AnyWindowHandle>) -> bool {
    false
}

pub(crate) const WINDOW_GLASS_MENU_ALPHA: f32 = 0.78;

/// A menu's frosted blur samples the desktop behind its native window; a canvas menu has none.
pub(crate) fn apply_frosted_menu_blur(_window: &gpui::Window) {}

/// A page has no window glass, so a tooltip bubble has no blur to set.
pub(crate) fn apply_frosted_tooltip_blur(_window: &gpui::Window) {}

/// Never reached with glass off; the sidebar's glass tint has no meaning without glass.
pub(crate) fn sidebar_glass_tint() -> gpui::Hsla {
    gpui::transparent_black()
}

pub(crate) fn sync_overlay_window_glass(
    _window: &gpui::Window,
    _main_origin: gpui::Point<gpui::Pixels>,
) {
}

/// The sidebar's opaque fill, which is what the desktop draws with glass off.
pub(crate) fn sidebar_chrome_fill(_glass: bool, angle: f32) -> gpui::Background {
    crate::app::helpers::sidebar_chrome_gradient_fill(angle)
}

/// A page has no window glass, so a menu keeps its solid fill.
pub(crate) fn popup_window_surface(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// The desktop's `popup_menu_surface` (chrome_colors.rs): the sidebar menus' fill, solid on a page.
pub(crate) fn popup_menu_surface() -> gpui::Hsla {
    crate::app::helpers::titlebar_popup_menu_background()
}

/// A page has no window glass, so a menu keeps its solid fill.
pub(crate) fn frosted_menu_fill(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// The desktop's tooltip colour: the menu colour darkened toward black in dark mode (CDXC:Tooltips 2026-09-30 in the desktop's `window_glass.rs`).
pub(crate) fn tooltip_background(menu: gpui::Hsla) -> gpui::Hsla {
    if crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
        menu
    } else {
        menu.blend(gpui::black().opacity(0.55))
    }
}

/// The desktop's `apply_gpui_component_theme` (chrome_colors.rs) without window glass, so
/// gpui-component's tooltips take the app's tooltip colours instead of the stock light theme.
pub(crate) fn apply_gpui_component_theme(cx: &mut gpui::App) {
    use gpui_component::{Theme, ThemeMode};
    let mode = if crate::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed) {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
    let menu = crate::app::helpers::titlebar_popup_menu_background();
    let theme = Theme::global_mut(cx);
    theme.popover = menu;
    theme.tokens.popover = tooltip_background(menu).into();
    theme.popover_foreground = crate::app::helpers::titlebar_popup_menu_foreground();
    theme.border = crate::app::helpers::titlebar_popup_menu_border_color();
}

/// A page has no window glass, so a tooltip keeps its solid fill.
pub(crate) fn frosted_tooltip_fill(menu: gpui::Hsla) -> gpui::Hsla {
    tooltip_background(menu)
}

/// The desktop's list-row tooltip (`list_row_tooltip` in its titlebar helpers), without the
/// titlebar tooltip metrics a page does not carry.
pub(crate) fn list_row_tooltip(
    text: impl Into<gpui_component::text::Text>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> gpui::AnyView {
    gpui_component::tooltip::Tooltip::new(text).build(window, cx)
}

/// A page has no window glass, so a dialog keeps its solid fill.
pub(crate) fn frosted_modal_fill(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// A page has no window glass, so a form dialog keeps its solid fill.
pub(crate) fn frosted_dialog_fill(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// A page has no window glass, so a large panel keeps its solid fill.
pub(crate) fn frosted_panel_fill(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// The desktop's blocking typed-operation call, which only its quit path still makes synchronously (a client document's last push). A page cannot block on `fetch` and has no quit path, so the answer is a refusal.
pub(crate) fn gpui_gxserver_rpc_result(
    _endpoint: &str,
    _params: &Value,
    _timeout: std::time::Duration,
) -> Result<Value, String> {
    Err("A blocking gxserver call is not available in the browser.".to_string())
}

/// An Open In target (an editor, Finder). A page cannot launch an app, so the list it offers is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiOpenTarget {
    pub(crate) id: String,
    pub(crate) label: String,
}

pub(crate) fn gpui_visible_open_targets_from_current_settings() -> Vec<GpuiOpenTarget> {
    Vec::new()
}

pub(crate) fn gpui_launch_open_target(
    _target: &GpuiOpenTarget,
    _project_path: &std::path::Path,
) -> Result<(), String> {
    Err("Open In needs the Ghostex app.".to_string())
}

/// The desktop's `refresh_gpui_visual_settings` (helpers/project/colors.rs) for the theme colours
/// only: the sidebar, title bar, menus and work area take the colours the page's `?settings=`
/// resolve to, so a theme can be checked in a browser. A page with no settings keeps the defaults.
pub(crate) fn apply_settings_theme_colours() {
    use crate::app::consts::{
        GPUI_TITLEBAR_BACKGROUND_RGB, GPUI_TITLEBAR_GRADIENT_LEFT_RGB,
        GPUI_TITLEBAR_GRADIENT_RIGHT_RGB, GPUI_WORKSPACE_BACKGROUND_RGB,
    };
    use crate::app::helpers::*;
    use std::sync::atomic::Ordering;
    let snapshot = crate::shared_settings::shared_sidebar_settings_snapshot();
    let object = snapshot.object();
    if object.is_empty() {
        return;
    }
    let light = sidebar_uses_light_theme(object);
    crate::CHROME_LIGHT_APPEARANCE.store(light, Ordering::Relaxed);
    let chrome = resolved_custom_sidebar_titlebar_background_for_variant(object, light);
    GPUI_TITLEBAR_BACKGROUND_RGB.store(u64::from(chrome), Ordering::Relaxed);
    crate::GPUI_MENU_BACKGROUND_RGB.store(
        sidebar_titlebar_menu_background_for_chrome(chrome),
        Ordering::Relaxed,
    );
    let (left, right) = if light {
        (chrome, chrome)
    } else {
        sidebar_titlebar_gradient_stops_toward_full(
            chrome,
            theme_full_colour_fade(object, "themeSidebarContrast"),
        )
    };
    GPUI_TITLEBAR_GRADIENT_LEFT_RGB.store(u64::from(left), Ordering::Relaxed);
    GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.store(u64::from(right), Ordering::Relaxed);
    GPUI_WORKSPACE_BACKGROUND_RGB.store(
        u64::from(work_area_background_for_variant(object, light)),
        Ordering::Relaxed,
    );
}
/// The browser has no remote machine settings to name in a hook-install toast.
pub(crate) fn gpui_remote_machine_name_from_settings(_machine_id: &str) -> Option<String> {
    None
}
/// The browser draws no work area frame and no traffic lights, so the sidebar toggle keeps the edge
/// strip's x (the desktop's `sidebar_toggle_leading_x` in helpers/work_area_frame.rs adds the
/// frame's gutter and the macOS lights' reserve).
pub(crate) fn sidebar_toggle_leading_x(_lights_reserved: bool) -> f32 {
    crate::app::consts::SIDEBAR_TOGGLE_LEADING_X
}
