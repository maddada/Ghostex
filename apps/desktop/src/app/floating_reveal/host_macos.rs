//! The AppKit host: the panel is a real child window of the main window, and AppKit runs the slide
//! and the pointer-leave dismissal inside `native/macos/GpuiSidebarReveal.m`.

use std::ffi::c_void;

use gpui::Pixels;
use gpui::Point;

use super::model::*;
use crate::*;

unsafe extern "C" {
    fn GhostexGpuiNativeSidebarRevealRequest(
        root: *mut c_void,
        width: f64,
        titlebar_height: f64,
        left_inset: f64,
        edge_hovered: bool,
        requested: bool,
        keep_under_pointer: bool,
    ) -> i32;
    fn GhostexGpuiNativeSidebarRevealUpdate(
        root: *mut c_void,
        popup: *mut c_void,
        enabled: bool,
        width: f64,
        titlebar_height: f64,
        left_inset: f64,
        requested: bool,
        sticky: bool,
        slide_seconds: f64,
        window_controls: bool,
        top_left_radius: f64,
        bottom_left_radius: f64,
    ) -> bool;
    fn GhostexGpuiNativeSidebarRevealLeaving(popup: *mut c_void) -> bool;
    fn GhostexGpuiRevealEdgeSync(root: *mut c_void, visible: bool, width: f64);
    fn GhostexGpuiRevealEdgeTake(root: *mut c_void, y_fraction: *mut f64) -> i32;
    fn GhostexGpuiSetWindowControlsHidden(root: *mut c_void, hidden: bool);
}

/// What the native panel needs for its window corner: whether it shows the main window's own
/// traffic lights in its top-left (native/macos/GpuiSidebarReveal.m finds them on the window).
struct NativeWindowControls {
    shown: bool,
    /// The panel's left corner radii (`floating_panel_left_corner_radii`).
    top_left_radius: f64,
    bottom_left_radius: f64,
}

/// The floating panel's (top-left, bottom-left) corner radii. A corner that sits on one of the
/// window's own rounded corners takes the window's radius, so the panel never shows past the
/// window's edge there; every other corner keeps the floating panels' radius. In full screen the
/// window has no rounded corners.
///
/// CDXC:Sidebar 2026-11-06 DECISION:
/// User (GitHub #432 follow-up): the floating panel's top-left and bottom-left corners match the window's 16.5pt corner where they meet it, so its 12pt corner no longer pokes past the window's. With the work area frame on the panel still starts at the window's edge (only the card moves in), so the concentric radius there is the window's own too. The right corners keep `FLOATING_PANEL_CORNER_RADIUS`.
fn floating_panel_left_corner_radii(
    at_window_top: bool,
    at_window_left: bool,
    fullscreen: bool,
) -> (f32, f32) {
    let on_corner = |at_window_edge: bool| {
        if at_window_edge && at_window_left && !fullscreen {
            crate::app::element::MACOS_WINDOW_CORNER_RADIUS
        } else {
            FLOATING_PANEL_CORNER_RADIUS
        }
    };
    // The panel always runs down to the window's floor.
    (on_corner(at_window_top), on_corner(true))
}

impl GhostexGpuiApp {
    /// Hides the main window's own traffic lights while the sidebar is hidden
    /// (`window_controls_shown`); an open floating sidebar shows them again in its top-left. Called on every render of the main window: AppKit can rebuild the
    /// standard buttons (full screen does), so the state is applied again rather than remembered;
    /// the native side only writes when it differs.
    pub(crate) fn sync_window_controls_visibility(&self) {
        unsafe {
            GhostexGpuiSetWindowControlsHidden(self.parent_ns_view, !self.window_controls_shown())
        };
    }

    fn native_window_controls(&self, content: FloatingRevealContent) -> NativeWindowControls {
        let (top_left, bottom_left) = floating_panel_left_corner_radii(
            self.floating_reveal_top(content) <= 0.0,
            self.floating_reveal_left_inset() <= 0.0,
            self.main_window_fullscreen,
        );
        NativeWindowControls {
            shown: self.floating_reveal_carries_window_controls(content),
            top_left_radius: f64::from(top_left),
            bottom_left_radius: f64::from(bottom_left),
        }
    }

    /// Keeps the native hot zone over the window's left edge while the sidebar is collapsed and
    /// hands what the pointer did there since the last sweep to the same arm and disarm the GPUI zone
    /// calls on the other platforms.
    pub(super) fn sync_floating_reveal_native_edge(&mut self) {
        let visible = self.floating_reveal_edge_strip_visible();
        unsafe {
            GhostexGpuiRevealEdgeSync(
                self.parent_ns_view,
                visible,
                FLOATING_REVEAL_EDGE_WIDTH as f64,
            )
        };
        if !visible {
            return;
        }
        let mut y_fraction = 0.0;
        match unsafe { GhostexGpuiRevealEdgeTake(self.parent_ns_view, &mut y_fraction) } {
            1 => {
                let want = self.floating_reveal_edge_want(y_fraction < 0.5);
                self.arm_floating_reveal_from_strip(want);
            }
            2 => self.disarm_floating_reveal_edge(),
            _ => {}
        }
    }

    pub(super) fn sync_floating_reveal_host(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(panel) = self.floating_reveal.panel.as_ref() {
            let controls = self.native_window_controls(panel.content);
            let visible = unsafe {
                GhostexGpuiNativeSidebarRevealUpdate(
                    self.parent_ns_view,
                    panel.native_view,
                    true,
                    panel.width as f64,
                    self.floating_reveal_top(panel.content) as f64,
                    self.floating_reveal_left_inset() as f64,
                    requested,
                    false,
                    floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
                    controls.shown,
                    controls.top_left_radius,
                    controls.bottom_left_radius,
                )
            };
            if !visible {
                self.close_floating_reveal(cx);
            } else if unsafe { GhostexGpuiNativeSidebarRevealLeaving(panel.native_view) } {
                self.dismiss_floating_reveal_chat_windows(cx);
            }
            return;
        }
        let content = self.floating_reveal_content();
        let width = self.floating_reveal_width_for(content);
        let request = unsafe {
            GhostexGpuiNativeSidebarRevealRequest(
                self.parent_ns_view,
                width as f64,
                self.floating_reveal_top(content) as f64,
                self.floating_reveal_left_inset() as f64,
                self.floating_reveal.edge_hovered,
                requested,
                keep_under_pointer,
            )
        };
        if request == 1 {
            self.open_floating_reveal(keep_under_pointer, cx);
        }
    }

    pub(super) fn attach_floating_reveal_panel(
        &mut self,
        window: gpui::WindowHandle<gpui_component::Root>,
        native_view: Option<*mut c_void>,
        anchor: Point<Pixels>,
        content: FloatingRevealContent,
        width: f32,
        sticky: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(native_view) = native_view else {
            let _ = window.update(cx, |_, window, _| window.remove_window());
            return false;
        };
        let requested = self.floating_reveal.requested_until.is_some();
        self.floating_reveal.panel = Some(FloatingRevealPanel {
            window,
            native_view,
            anchor,
            content,
            width,
        });
        // A links card opened from the docked sidebar does not stay up over the floating one.
        if content.sidebar {
            self.close_session_links_card(cx);
        }
        let controls = self.native_window_controls(content);
        let visible = unsafe {
            GhostexGpuiNativeSidebarRevealUpdate(
                self.parent_ns_view,
                native_view,
                true,
                width as f64,
                self.floating_reveal_top(content) as f64,
                self.floating_reveal_left_inset() as f64,
                requested,
                sticky,
                floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
                controls.shown,
                controls.top_left_radius,
                controls.bottom_left_radius,
            )
        };
        if !visible {
            self.close_floating_reveal(cx);
            return false;
        }
        // AppKit sized the panel from the frame it computed just now, from inside this update;
        // GPUI has to be told about that size the same way it is told about every other one.
        self.schedule_floating_reveal_bounds_refresh(cx);
        true
    }

    pub(super) fn dispose_floating_reveal_host(&self, panel: &FloatingRevealPanel) {
        unsafe {
            GhostexGpuiNativeSidebarRevealUpdate(
                std::ptr::null_mut(),
                panel.native_view,
                false,
                0.0,
                0.0,
                0.0,
                false,
                false,
                0.0,
                false,
                0.0,
                0.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_left_corners_follow_the_window_corners_they_sit_on() {
        let window = crate::app::element::MACOS_WINDOW_CORNER_RADIUS;
        let panel = FLOATING_PANEL_CORNER_RADIUS;
        // The hidden sidebar, carrying the lights from the window's top.
        assert_eq!(
            floating_panel_left_corner_radii(true, true, false),
            (window, window)
        );
        // The sessions column alone, under the header at the window's left edge.
        assert_eq!(
            floating_panel_left_corner_radii(false, true, false),
            (panel, window)
        );
        // Beside a docked sidebar it touches no window corner.
        assert_eq!(
            floating_panel_left_corner_radii(false, false, false),
            (panel, panel)
        );
        // Full screen has no rounded window corners.
        assert_eq!(
            floating_panel_left_corner_radii(true, true, true),
            (panel, panel)
        );
    }
}
