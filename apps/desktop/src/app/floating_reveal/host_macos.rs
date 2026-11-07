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
        bottom_inset: f64,
        requested: bool,
        sticky: bool,
        slide_seconds: f64,
        window_controls: bool,
        radii: *const f64,
    ) -> bool;
    fn GhostexGpuiNativeSidebarRevealLeaving(popup: *mut c_void) -> bool;
    fn GhostexGpuiRevealEdgeSync(root: *mut c_void, visible: bool, width: f64);
    fn GhostexGpuiRevealEdgeTake(root: *mut c_void, y_fraction: *mut f64) -> i32;
    fn GhostexGpuiSetWindowControlsHidden(root: *mut c_void, hidden: bool);
}

/// What the native panel needs for its corners: whether it shows the main window's own traffic
/// lights in its top-left (native/macos/GpuiSidebarReveal.m finds them on the window), and its
/// corner radii (`floating_panel_corner_radii`).
struct NativeWindowControls {
    shown: bool,
    radii: [f64; 4],
}

/// The floating panel's corner radii: top-left, top-right, bottom-right, bottom-left. With the
/// work area frame on the full-height panel is a card inside the frame, rounded like the work area's
/// card, concentric with the window's corner. Otherwise a corner on the window's own rounded corner
/// takes the window's radius, so the panel never shows past the window's edge, and is square in
/// full screen, where the screen's corner is; every other corner keeps the floating panels' radius.
///
/// CDXC:Sidebar 2026-11-07 WHY:
/// The native host masks the panel with these four radii and GPUI draws its background square (window.rs): GPUI rounds all four corners alike and the mask can only cut more, so a corner smaller than the panel's 12pt (the card's 10.5pt, a square screen corner) could not be drawn otherwise. Supersedes the 2026-11-06 note that kept the window's 16.5pt corner with the frame on: the panel now sits inside the frame (CDXC:Sidebar 2026-11-07 WHY on `floating_reveal_full_height` in floating_reveal/model.rs).
fn floating_panel_corner_radii(
    full_height: bool,
    at_window_left: bool,
    framed: bool,
    fullscreen: bool,
) -> [f32; 4] {
    let panel = FLOATING_PANEL_CORNER_RADIUS;
    if full_height && framed {
        return [crate::app::helpers::WORK_AREA_FRAME_RADIUS; 4];
    }
    let window_corner = if fullscreen {
        0.0
    } else {
        crate::app::element::MACOS_WINDOW_CORNER_RADIUS
    };
    let top_left = if full_height { window_corner } else { panel };
    // The panel always runs down to the window's floor.
    let bottom_left = if at_window_left { window_corner } else { panel };
    [top_left, panel, panel, bottom_left]
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
        let radii = floating_panel_corner_radii(
            self.floating_reveal_full_height(content),
            self.floating_reveal_left_inset() <= 0.0,
            crate::app::helpers::work_area_frame_active(),
            self.main_window_fullscreen,
        );
        NativeWindowControls {
            shown: self.floating_reveal_carries_window_controls(content),
            radii: radii.map(f64::from),
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
                    self.floating_reveal_card_inset(panel.content) as f64,
                    requested,
                    false,
                    floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
                    controls.shown,
                    controls.radii.as_ptr(),
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
                self.floating_reveal_card_inset(content) as f64,
                requested,
                sticky,
                floating_reveal_slide_duration(cx.reduce_motion()).as_secs_f64(),
                controls.shown,
                controls.radii.as_ptr(),
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
                0.0,
                false,
                false,
                0.0,
                false,
                std::ptr::null(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corners_follow_the_window_or_the_frame() {
        let window = crate::app::element::MACOS_WINDOW_CORNER_RADIUS;
        let panel = FLOATING_PANEL_CORNER_RADIUS;
        let card = crate::app::helpers::WORK_AREA_FRAME_RADIUS;
        // (full height, at the window's left, frame on, full screen)
        // The hidden sidebar at the window's corners.
        assert_eq!(
            floating_panel_corner_radii(true, true, false, false),
            [window, panel, panel, window]
        );
        // In full screen the screen's corners are square.
        assert_eq!(
            floating_panel_corner_radii(true, true, false, true),
            [0.0, panel, panel, 0.0]
        );
        // With the frame on it is a card inside the frame, windowed and in full screen alike.
        for fullscreen in [false, true] {
            assert_eq!(
                floating_panel_corner_radii(true, false, true, fullscreen),
                [card; 4]
            );
        }
        // The sessions column alone, under the header at the window's left edge.
        assert_eq!(
            floating_panel_corner_radii(false, true, false, false),
            [panel, panel, panel, window]
        );
        // Beside a docked sidebar it touches no window corner.
        assert_eq!(
            floating_panel_corner_radii(false, false, true, false),
            [panel; 4]
        );
    }
}
