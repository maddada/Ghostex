//! Whether Ghostex draws a main window's caption itself: the drag strip along the top band and
//! the minimize, maximize and close buttons in its top right, and whether minimize is among them.

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
/// User, on Omarchy: "please on arch show the close/minimize/maximise buttons on the top right like on windows". On Hyprland Ghostex draws the three buttons at the top right of the window, where Windows has them, as it does for a client-decorated window elsewhere; where the window manager draws a titlebar of its own (GNOME and KDE with server decorations) Ghostex adds none, so the buttons never show twice. Minimize on Omarchy works only on floating workspaces, the ones with the dock that brings a window back; asked what the button should do elsewhere, the user chose "Hide it on tiled workspaces" (Q57): it shows only while the window is on a floating workspace, follows the window moving and the workspace's layout changing, and never shows on plain Hyprland (`window_minimize_available`).
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

/// Whether this window shows the minimize button: on Hyprland only while Omarchy's minimize would
/// move it to the dock (`linux_hyprland::omarchy_minimizes_window`, the 2026-11-15 decision on
/// `linux_app_draws_window_caption`); everywhere else always.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) fn window_minimize_available(window: &gpui::Window) -> bool {
    #[cfg(target_os = "linux")]
    if crate::linux_hyprland::running_on_hyprland() {
        return crate::linux_hyprland::omarchy_minimizes_window(
            gpui::Window::window_handle(window).window_id().as_u64(),
        );
    }
    let _ = window;
    true
}

/// Redraws this window's caption when `window_minimize_available` may have changed: the window's
/// Hyprland address is learned each time it becomes active (until known), and any window moving
/// or workspace changing layout redraws it.
#[cfg(target_os = "linux")]
pub(crate) fn follow_window_minimize_place(
    window: &mut gpui::Window,
    cx: &mut gpui::Context<crate::GhostexGpuiApp>,
) {
    use futures::StreamExt as _;
    if !crate::linux_hyprland::running_on_hyprland() {
        return;
    }
    let window_id = gpui::Window::window_handle(window).window_id().as_u64();
    if window.is_window_active() {
        crate::linux_hyprland::learn_active_window(window_id);
    }
    cx.observe_window_activation(window, move |_, window, _| {
        if window.is_window_active() {
            crate::linux_hyprland::learn_active_window(window_id);
        }
    })
    .detach();
    let mut changes = crate::linux_hyprland::watch_minimize_places();
    cx.spawn(async move |this, cx| {
        while changes.next().await.is_some() {
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        }
    })
    .detach();
}
