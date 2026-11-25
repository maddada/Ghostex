// Cluster: the shared grab strip, hover line, and pane outline segments used by every resize rail.

use gpui::Animation;
use gpui::AnimationExt as _;
use gpui::AnyElement;
use gpui::Div;
use gpui::ElementId;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement;
use gpui::Stateful;
use gpui::Styled;
use gpui::deferred;
use gpui::div;
use gpui::px;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// Which side of its rail a grab strip covers, named in layout order: `Leading` is the left or top
/// neighbour, `Trailing` the right or bottom one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResizeRailGrabSide {
    Straddle,
    Leading,
    Trailing,
    /// The rail is a frame's border whose trailing side may be a CEF page clipped inside it: the
    /// strip straddles the border like `Straddle`, so the hit area is centred on it, and the hover
    /// line's extra pixel goes on the leading side, as `Leading`'s does, so a page never hides it.
    FrameEdge,
}

/// The sides of a pane that touch a resize rail. The pane leaves its own border off those sides so the
/// rail is the only line there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RailFacingEdges {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) top: bool,
    pub(crate) bottom: bool,
}

impl RailFacingEdges {
    /// The edges the first and second child of a split inherit: each keeps its parent's outer edges and
    /// gains the one that touches the split's own rail.
    pub(crate) fn for_split_children(self, axis: WorkspaceSplitAxis) -> (Self, Self) {
        match axis {
            WorkspaceSplitAxis::Horizontal => (
                Self {
                    right: true,
                    ..self
                },
                Self { left: true, ..self },
            ),
            WorkspaceSplitAxis::Vertical => (
                Self {
                    bottom: true,
                    ..self
                },
                Self { top: true, ..self },
            ),
        }
    }
}

fn resize_rail_strip_leading_reach(side: ResizeRailGrabSide) -> f32 {
    match side {
        ResizeRailGrabSide::Straddle | ResizeRailGrabSide::FrameEdge => RESIZE_RAIL_GRAB_REACH,
        ResizeRailGrabSide::Leading => RESIZE_RAIL_GRAB_ONE_SIDED_REACH,
        ResizeRailGrabSide::Trailing => 0.0,
    }
}

fn resize_rail_strip_trailing_reach(side: ResizeRailGrabSide) -> f32 {
    match side {
        ResizeRailGrabSide::Straddle | ResizeRailGrabSide::FrameEdge => RESIZE_RAIL_GRAB_REACH,
        ResizeRailGrabSide::Leading => 0.0,
        ResizeRailGrabSide::Trailing => RESIZE_RAIL_GRAB_ONE_SIDED_REACH,
    }
}

/// CDXC:Workarea 2026-09-19 DECISION:
/// User: rails should be catchable without hovering exactly on the line, the way Waku's are, and the earlier rule that the visible divider must be the only grab target no longer applies to resize rails.
/// The strip is an invisible child of the 1px rail that reaches a few pixels into the neighbouring panes. It is drawn deferred so it sits above both panes and escapes the split containers' clipping, and it blocks the mouse so what is under it does not react to a pointer aiming for the rail. A CEF page is a native view above the GPUI scene, so the strip never receives the mouse over one; callers whose rail touches a CEF page on one side pass `Leading` or `Trailing` to put the whole strip on the GPUI-painted side, which is what Waku does next to its webview.
pub(crate) fn resize_rail_grab_strip(
    id: impl Into<ElementId>,
    axis: WorkspaceSplitAxis,
    side: ResizeRailGrabSide,
) -> Stateful<Div> {
    let leading = resize_rail_strip_leading_reach(side);
    let extent =
        leading + WORKSPACE_SPLIT_HANDLE_THICKNESS + resize_rail_strip_trailing_reach(side);
    let strip = div().id(id).absolute().occlude();
    match axis {
        WorkspaceSplitAxis::Horizontal => strip
            .top_0()
            .bottom_0()
            .left(px(-leading))
            .w(px(extent))
            .cursor_ew_resize(),
        WorkspaceSplitAxis::Vertical => strip
            .left_0()
            .right_0()
            .top(px(-leading))
            .h(px(extent))
            .cursor_ns_resize(),
    }
}

/// The accent line a hovered or dragged rail shows, as a child of its grab strip. It covers the rail
/// plus one pixel beside it, on the strip's own side when the strip is one-sided so the extra pixel is
/// never hidden under a CEF page.
pub(crate) fn resize_rail_hover_line(
    animation_id: impl Into<ElementId>,
    axis: WorkspaceSplitAxis,
    side: ResizeRailGrabSide,
) -> impl IntoElement {
    let rail_offset = resize_rail_strip_leading_reach(side);
    let extra = SIDEBAR_DIVIDER_HOVER_LINE_WIDTH - WORKSPACE_SPLIT_HANDLE_THICKNESS;
    let offset = match side {
        ResizeRailGrabSide::Leading | ResizeRailGrabSide::FrameEdge => rail_offset - extra,
        ResizeRailGrabSide::Straddle | ResizeRailGrabSide::Trailing => rail_offset,
    };
    let line = div().absolute().bg(sidebar_divider_hover_line_color());
    let line = match axis {
        WorkspaceSplitAxis::Horizontal => line
            .top_0()
            .bottom_0()
            .left(px(offset))
            .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH)),
        WorkspaceSplitAxis::Vertical => line
            .left_0()
            .right_0()
            .top(px(offset))
            .h(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH)),
    };
    line.with_animation(
        animation_id,
        Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION).with_easing(gpui::ease_out_quint()),
        |line, delta| line.opacity(delta),
    )
}

/// Wraps a finished grab strip for its rail. Priority 0 keeps it under every menu and popup, which
/// all draw at priority 1 or higher.
pub(crate) fn resize_rail_deferred_strip(strip: impl IntoElement) -> AnyElement {
    deferred(strip.into_any_element()).into_any_element()
}

/// Where a lone chat pane's outline runs up beside the floating header and closes along the
/// window's top edge (see `resize_rail_outline_segments`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HeaderReach {
    /// How far above the pane the header starts; zero when the chat flows under it.
    pub(crate) above: f32,
    /// How many logical pixels at the window's top edge the window's own frame paints over.
    pub(crate) frame_cover: f32,
}

/// How far down from its top edge the window's own frame covers what the app paints, in logical
/// pixels, measured from the platform so a line drawn just below it lands on a device row of its
/// own at every display scale.
#[cfg(target_os = "windows")]
pub(crate) fn window_frame_top_cover(window: &gpui::Window) -> f32 {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_VISIBLE_FRAME_BORDER_THICKNESS, DwmGetWindowAttribute,
    };
    let scale = window.scale_factor();
    let hwnd = match HasWindowHandle::window_handle(window)
        .ok()
        .map(|handle| handle.as_raw())
    {
        Some(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
        _ => return 1.0 / scale,
    };
    let mut device_pixels: u32 = 0;
    // Windows 10 has no such attribute; its frame covers one device row.
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd as windows_sys::Win32::Foundation::HWND,
            DWMWA_VISIBLE_FRAME_BORDER_THICKNESS as u32,
            (&raw mut device_pixels).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if read != 0 {
        device_pixels = 1;
    }
    device_pixels as f32 / scale
}

/// CDXC:Workarea 2026-10-15 WHY:
/// The user: on macOS "the top line is 1px too low". AppKit paints nothing over a full-size content view's top row, so the outline's top line sits on the window's top edge there; the one-pixel offset was the Windows fix leaking through this fallback.
#[cfg(target_os = "macos")]
pub(crate) fn window_frame_top_cover(_window: &gpui::Window) -> f32 {
    0.0
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn window_frame_top_cover(_window: &gpui::Window) -> f32 {
    WORKSPACE_SPLIT_HANDLE_THICKNESS
}

/// A pane with a focus or attention outline closes it over the rails it touches, because it has no
/// border of its own on those sides. Each segment is one rail thickness wide, sits just outside the
/// pane, and spans the pane's full outer length including the borders it does have.
///
/// CDXC:Workarea 2026-09-21 DECISION:
/// User: the attention outline of a lone chat pane with the tab bar hidden must show "from the top". That pane's top edge is the floating header, not a rail, so its top segment would land off the window or under the header's last row. `header_reach` is how far above the pane the header starts (zero when the chat flows under it): the outline then runs up both sides of the header and closes along the window's top edge, so the header and the chat read as the one outlined pane.
pub(crate) fn resize_rail_outline_segments(
    edges: RailFacingEdges,
    color: Hsla,
    header_reach: Option<HeaderReach>,
) -> Vec<AnyElement> {
    let thickness = WORKSPACE_SPLIT_HANDLE_THICKNESS;
    let inset = |faces_rail: bool| if faces_rail { 0.0 } else { -1.0 };
    let mut segments = Vec::new();
    // CDXC:Workarea 2026-10-15 WHY:
    // The user: the top-right corner "isn't showing the border correct" and the bottom-right "should be very slightly more rounded". A lone chat's outline is one bordered frame from the window's top edge to the pane's bottom that follows the window's rounded corners wherever it meets them (`window_corner_outline`); the separate straight segments it replaces were cut off square by the window's corner mask. Absolute children are placed against the pane's padding box, inside its own border, so every side sits one thickness out: on the pane's own border where it has one and over the rail where it faces one. Placing a bordered side at zero drew a second line beside the pane's own, so that side read one pixel thicker than the top.
    if let Some(reach) = header_reach {
        let outline = div()
            .absolute()
            // CDXC:Workarea 2026-10-11 WHY:
            // Where the outline closes along the window's top edge, the window's own frame covers the client area's top rows (on Windows 11 the DWM border is drawn over them, 2 device pixels at 150%), so the segment sits just below what the frame covers, on the first row the app really paints; the header's layout is unchanged. This supersedes the 2026-10-09 fixed one-pixel offset, which GPUI snaps to device row 1 at 150% (1.5 rounds toward zero), still under the frame.
            .top(px(-reach.above + reach.frame_cover))
            .left(px(-thickness))
            .right(px(-thickness))
            .bottom(px(-thickness))
            .border_1()
            .border_color(color)
            .window_corner_outline();
        segments.push(deferred(outline.into_any_element()).into_any_element());
        return segments;
    }
    let mut vertical = |at_left: bool| {
        let segment = div()
            .absolute()
            .top(px(inset(edges.top)))
            .bottom(px(inset(edges.bottom)))
            .w(px(thickness))
            .bg(color);
        let segment = if at_left {
            segment.left(px(-thickness))
        } else {
            segment.right(px(-thickness))
        };
        segments.push(deferred(segment.into_any_element()).into_any_element());
    };
    if edges.left {
        vertical(true);
    }
    if edges.right {
        vertical(false);
    }
    let mut horizontal = |at_top: bool| {
        let segment = div()
            .absolute()
            .left(px(inset(edges.left)))
            .right(px(inset(edges.right)))
            .h(px(thickness))
            .bg(color);
        let segment = if at_top {
            segment.top(px(-thickness))
        } else {
            segment.bottom(px(-thickness))
        };
        segments.push(deferred(segment.into_any_element()).into_any_element());
    };
    if edges.top {
        horizontal(true);
    }
    if edges.bottom {
        horizontal(false);
    }
    segments
}

/// Gives a pane its one-pixel border on every side that does not touch a resize rail, and closes a
/// non-neutral outline over the rails on the sides that do. Border widths depend only on where the pane
/// sits, never on its state, so focus and attention changes cannot move the pane's content box.
pub(crate) fn rail_aware_pane_border<E: Styled + ParentElement>(
    pane: E,
    edges: RailFacingEdges,
    color: Hsla,
    neutral_color: Hsla,
    header_reach: Option<HeaderReach>,
) -> E {
    let mut pane = pane.border_color(color);
    if !edges.left {
        pane = pane.border_l_1();
    }
    if !edges.right {
        pane = pane.border_r_1();
    }
    if !edges.top {
        pane = pane.border_t_1();
    }
    if !edges.bottom {
        pane = pane.border_b_1();
    }
    if color != neutral_color {
        pane = pane.children(resize_rail_outline_segments(edges, color, header_reach));
    }
    pane
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 WHY:
    /// A rail must keep following the pointer over whatever it crosses. GPUI hands a mouse move to an element's own listener only while that element's hitbox is hovered, and a blocking hitbox above it, such as a terminal or a grab strip, takes the root element out of the hit list, so a drag that entered one stopped moving and its release was never seen. This zero-size canvas listens window-wide instead, the way the terminal element does for its own drags, and does nothing unless a rail is actually being dragged.
    pub(crate) fn render_resize_drag_pointer_tracker(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        gpui::canvas(
            |_bounds, _window, _cx| {},
            move |_bounds, _prepaint, window, _cx| {
                let move_view = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    move_view.update(cx, |app, cx| {
                        app.handle_resize_drag_pointer_move(event, window, cx);
                    });
                });
                let up_view = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if !phase.bubble() || event.button != MouseButton::Left {
                        return;
                    }
                    up_view.update(cx, |app, cx| {
                        app.handle_resize_drag_pointer_up(event, window, cx);
                    });
                });
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    fn handle_resize_drag_pointer_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.resize_rail_drag_active() {
            return;
        }
        self.handle_sidebar_drag_move(event, window, cx);
        self.handle_command_pane_resize_drag_move(event, window, cx);
        self.handle_workspace_split_resize_drag_move(event, window, cx);
        self.handle_command_split_resize_drag_move(event, window, cx);
        self.handle_browser_split_resize_drag_move(event, window, cx);
        self.handle_workarea_split_resize_drag_move(event, window, cx);
    }

    fn handle_resize_drag_pointer_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.resize_rail_drag_active() {
            return;
        }
        self.handle_sidebar_drag_mouse_up(event, window, cx);
        self.handle_command_pane_resize_mouse_up(event, window, cx);
        self.handle_workspace_split_resize_mouse_up(event, window, cx);
        self.handle_command_split_resize_mouse_up(event, window, cx);
        self.handle_browser_split_resize_mouse_up(event, window, cx);
        self.handle_workarea_split_resize_mouse_up(event, window, cx);
    }

    /// Whether any resize rail is being dragged right now.
    pub(crate) fn resize_rail_drag_active(&self) -> bool {
        self.sidebar_drag.is_some()
            || self.workspace_split_drag.is_some()
            || self.command_split_drag.is_some()
            || self.browser_split_drag.is_some()
            || self.workarea_split_drag.is_some()
            || self.command_pane.resize_drag.is_some()
    }
}
