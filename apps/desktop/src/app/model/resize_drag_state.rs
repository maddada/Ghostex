// C1 wave-3 re-cluster: split/pane resize drag state structs and the shared resize-ratio and drag-bounds math, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


/*
CDXC:GPUICommandPaneSide 2026-08-16:
One drag state serves both docks: the bottom rail tracks pointer Y against the
panel height, the right divider tracks pointer X against the panel width. The
side is captured at mouse-down so a Settings save mid-drag cannot re-interpret
the stored start position on the other axis.
*/
#[derive(Clone, Copy)]
pub(crate) struct CommandPaneResizeDragState {
    pub(crate) side: GpuiCommandPaneSide,
    pub(crate) start_position: f32,
    pub(crate) start_extent: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct SplitResizeMetrics {
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct WorkspaceSplitResizeDragState {
    pub(crate) split_id: WorkspaceSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct CommandPaneSplitResizeDragState {
    pub(crate) split_id: CommandPaneSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct BrowserSplitResizeDragState {
    pub(crate) split_id: BrowserSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) start_position: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorCompanionResizeDragState {
    pub(crate) start_x: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


#[derive(Clone, Copy)]
pub(crate) struct ProjectEditorCompanionSplitResizeDragState {
    pub(crate) start_y: f32,
    pub(crate) start_ratio: f32,
    pub(crate) content_span: f32,
}


pub(crate) fn split_resize_content_span(
    child_bounds: &[Bounds<Pixels>],
    axis: WorkspaceSplitAxis,
) -> Option<f32> {
    let first = child_bounds.first()?;
    let second = child_bounds.get(2)?;
    let span = match axis {
        WorkspaceSplitAxis::Horizontal => first.size.width.as_f32() + second.size.width.as_f32(),
        WorkspaceSplitAxis::Vertical => first.size.height.as_f32() + second.size.height.as_f32(),
    };

    (span > 1.0).then_some(span)
}


pub(crate) fn split_pane_resize_minimum_for_axis(axis: WorkspaceSplitAxis) -> f32 {
    match axis {
        WorkspaceSplitAxis::Horizontal => PANE_RESIZE_MINIMUM_WIDTH,
        WorkspaceSplitAxis::Vertical => PANE_RESIZE_MINIMUM_HEIGHT,
    }
}


pub(crate) fn split_drag_ratio_bounds_from_minimums(
    minimum_before: f32,
    minimum_after: f32,
    content_span: f32,
) -> Option<(f32, f32)> {
    let span = content_span.max(1.0);
    let lower = (minimum_before / span).max(0.1);
    let upper = (1.0 - minimum_after / span).min(0.9);
    (lower <= upper).then_some((lower, upper))
}


pub(crate) fn split_resize_event_position(axis: WorkspaceSplitAxis, position: gpui::Point<Pixels>) -> f32 {
    match axis {
        WorkspaceSplitAxis::Horizontal => position.x.as_f32(),
        WorkspaceSplitAxis::Vertical => position.y.as_f32(),
    }
}


pub(crate) fn workspace_split_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.1, 0.9)
}
