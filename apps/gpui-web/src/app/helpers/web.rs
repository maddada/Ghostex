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

/// Window glass blurs the desktop behind a native window; a canvas has nothing behind it to blur, so glass is never on here.
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

/// Never reached with glass off; the sidebar's glass tint has no meaning without glass.
pub(crate) fn sidebar_glass_tint() -> gpui::Hsla {
    gpui::transparent_black()
}

pub(crate) fn sync_overlay_window_glass(_window: &gpui::Window, _main_origin: gpui::Point<gpui::Pixels>) {}

/// The sidebar's opaque fill, which is what the desktop draws with glass off.
pub(crate) fn sidebar_chrome_fill(_glass: bool, angle: f32) -> gpui::Background {
    crate::app::helpers::sidebar_chrome_gradient_fill(angle)
}

/// A page has no window glass, so a menu keeps its solid fill.
pub(crate) fn popup_window_surface(color: gpui::Hsla) -> gpui::Hsla {
    color
}

/// A page has no window glass, so a menu keeps its solid fill.
pub(crate) fn frosted_menu_fill(color: gpui::Hsla) -> gpui::Hsla {
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
