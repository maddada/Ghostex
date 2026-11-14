//! Whether Ghostex draws a main window's caption itself: the drag strip along the top band and
//! the minimize, maximize and close buttons in its top right.

/// The app draws this window's caption: always on Windows, and on Linux where the window manager
/// draws no titlebar for the window (`linux_app_draws_window_caption`).
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) fn app_draws_window_caption(window: &gpui::Window) -> bool {
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        true
    }
    #[cfg(target_os = "linux")]
    {
        linux_app_draws_window_caption(window)
    }
}

/// CDXC:Titlebar 2026-11-15 DECISION:
/// User, on Omarchy: "please on arch show the close/minimize/maximise buttons on the top right like on windows". On Hyprland Ghostex draws the three buttons at the top right of the window, where Windows has them, as it does for a client-decorated window elsewhere; where the window manager draws a titlebar of its own (GNOME and KDE with server decorations) Ghostex adds none, so the buttons never show twice.
///
/// CDXC:Titlebar 2026-11-15 WHY:
/// Hyprland's XWayland does not advertise `_GTK_FRAME_EXTENTS`, so GPUI reports the main window as server-decorated, yet Hyprland honours the window's no-decorations hint (asked in workspace_windows/open.rs) and draws no titlebar for it, and Ghostex turns off the titlebar Omarchy's floating workspaces give other windows (`titlebar_requests` in linux_hyprland.rs). So on Hyprland the app owns the caption: its buttons and, in render/window_drag_region.rs, its drag.
#[cfg(target_os = "linux")]
pub(crate) fn linux_app_draws_window_caption(window: &gpui::Window) -> bool {
    matches!(
        window.window_decorations(),
        gpui::Decorations::Client { .. }
    ) || crate::linux_hyprland::running_on_hyprland()
}
