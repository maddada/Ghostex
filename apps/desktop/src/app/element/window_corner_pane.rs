use gpui::{Element, Styled};

pub(crate) trait WindowCornerPaneExt: Element + Styled + Sized {
    #[cfg(target_os = "macos")]
    fn window_corner_pane(self) -> WindowCornerPane<Self> {
        WindowCornerPane(self, false)
    }

    #[cfg(not(target_os = "macos"))]
    fn window_corner_pane(self) -> Self {
        self
    }

    #[cfg(target_os = "macos")]
    fn window_corner_outline(self) -> WindowCornerPane<Self> {
        WindowCornerPane::outline(self)
    }

    #[cfg(not(target_os = "macos"))]
    fn window_corner_outline(self) -> Self {
        self
    }
}

/// The corner radius of the main window's system mask, in points.
///
/// CDXC:Workarea 2026-10-15 WHY:
/// Measured on macOS 27 from the alpha of a window capture at 2x: the mask is a continuous corner about 24pt long whose body follows a 16.5pt circle within half a device pixel, while the earlier 18pt arc sat a device pixel or two inside it near both ends and left a sliver of pane between the outline and the window's edge. GPUI only draws circular corners, so 16.5pt is the circle that hugs the mask.
/// SEE-ALSO: native/macos/GpuiWindowCorners.h clips native child content to the same radius.
/// SEE-ALSO: app/helpers/work_area_frame.rs, whose card radius (`WORK_AREA_FRAME_RADIUS`) is this radius less the frame's gutter.
#[cfg(target_os = "macos")]
pub(crate) const MACOS_WINDOW_CORNER_RADIUS: f32 = 16.5;

impl<E: Element + Styled> WindowCornerPaneExt for E {}

#[cfg(target_os = "macos")]
pub(crate) fn refresh_native_window_corner_clip(native_view: *mut std::ffi::c_void) {
    unsafe extern "C" {
        fn GhostexGpuiCEFRefreshNativeViewWindowCorners(native_view: *mut std::ffi::c_void);
    }
    unsafe { GhostexGpuiCEFRefreshNativeViewWindowCorners(native_view) }
}

#[cfg(target_os = "macos")]
use gpui::{
    App, Bounds, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels,
    Window, px,
};

/// CDXC:Workarea 2026-09-06 DECISION:
/// User: on macOS, round only pane bottom corners that meet the window's bottom corners, accounting for splits, either sidebar side, hidden sidebars, and bottom/right command panels in every state.
/// Use this frame's layout bounds, including pane Focus mode and panel reservations, so rounding cannot lag a resize or depend on a duplicated layout model.
#[cfg(target_os = "macos")]
pub(crate) struct WindowCornerPane<E>(E, bool);

#[cfg(target_os = "macos")]
impl<E> WindowCornerPane<E> {
    fn outline(element: E) -> Self {
        Self(element, true)
    }
}

#[cfg(target_os = "macos")]
impl<E: Element + Styled> IntoElement for WindowCornerPane<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

#[cfg(target_os = "macos")]
impl<E: Element + Styled> Element for WindowCornerPane<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.0.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.0.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.0.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // With the work area frame on, panes round to the card's inner corners instead of the
        // window's, top corners included and in full screen too (helpers/work_area_frame.rs).
        if let Some(radii) = super::work_area_frame::work_area_frame_corner_radii(bounds, window) {
            let style = &mut self.0.style().corner_radii;
            style.top_left = Some(radii.top_left.into());
            style.top_right = Some(radii.top_right.into());
            style.bottom_right = Some(radii.bottom_right.into());
            style.bottom_left = Some(radii.bottom_left.into());
            /*
            CDXC:Workarea 2026-11-07 WHY:
            A pane keeps a 1pt border on every side that does not face a resize rail (`rail_aware_pane_border`). Without the frame that side lies on the window's own edge; inside the card it lay right next to the card's 1pt border, so each edge of the work area showed two parallel lines, and in full screen, with no window edge beside them, they read as a double line along the screen's left and right edges (GitHub #432 follow-up). The card's border is the one line there: a side on the card's inner edge paints none. Its width stays in the layout, so the pane's content does not move. A focus or attention outline (`window_corner_outline`, a lone chat's) keeps every side: it is a coloured state, not a second hairline.
            */
            if !self.1
                && let Some(touching) =
                    super::work_area_frame::work_area_frame_touching_edges(bounds, window)
            {
                let widths = &mut self.0.style().border_widths;
                let none = || Some(gpui::AbsoluteLength::from(px(0.0)));
                if touching.left {
                    widths.left = none();
                }
                if touching.right {
                    widths.right = none();
                }
                if touching.top {
                    widths.top = none();
                }
                if touching.bottom {
                    widths.bottom = none();
                }
            }
            return self.0.prepaint(id, inspector_id, bounds, state, window, cx);
        }
        // The command group has a 1pt outer edge and a 1pt right reservation;
        // companion bodies may also sit inside their parent's 1pt border.
        let edge_inset = px(2.0);
        let viewport = window.viewport_size();
        let at_bottom =
            !window.is_fullscreen() && (bounds.bottom() - viewport.height).abs() <= edge_inset;
        // A focus or attention outline that reaches the window's top edge also follows the
        // window's top corners, where a straight line is cut off by the system mask.
        let at_top = self.1 && !window.is_fullscreen() && bounds.top().abs() <= edge_inset;
        let at_left = bounds.left().abs() <= edge_inset;
        let at_right = (bounds.right() - viewport.width).abs() <= edge_inset;
        let radius = |round: bool| {
            px(if round {
                MACOS_WINDOW_CORNER_RADIUS
            } else {
                0.0
            })
        };
        // SEE-ALSO: native/macos/GpuiWindowCorners.h clips native child content
        // to the same window corner, leaving the pane border visible.
        let radii = &mut self.0.style().corner_radii;
        radii.bottom_left = Some(radius(at_bottom && at_left).into());
        radii.bottom_right = Some(radius(at_bottom && at_right).into());
        if self.1 {
            radii.top_left = Some(radius(at_top && at_left).into());
            radii.top_right = Some(radius(at_top && at_right).into());
        }
        self.0.prepaint(id, inspector_id, bounds, state, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .paint(id, inspector_id, bounds, state, prepaint, window, cx);
    }
}
