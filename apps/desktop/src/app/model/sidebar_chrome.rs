// C1 wave-3 re-cluster: sidebar drag/side/collapse/divider chrome state and geometry, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy)]
pub(crate) struct SidebarDragState {
    pub(crate) start_x: f32,
    pub(crate) start_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarSide {
    Left,
    Right,
}

impl GpuiSidebarSide {
    #[allow(dead_code)] // no caller: sidebar side comes from the persisted shell state, not a settings string
    pub(crate) fn from_settings_value(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/*
CDXC:GPUICommandPaneSide 2026-08-16:
Command pane placement is placement-only shell state sourced from shared
Settings (`commandsPanelSide`). Bottom keeps the historical pinned/collapsed
layout; Right renders the pinned pane as a workspace column with a vertical
resize rail. The collapsed footer strip stays at the bottom on both sides so
the pane remains discoverable from the same place.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaneSide {
    Bottom,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarBodyChromePart {
    Sidebar,
    Divider,
    Workspace,
}

pub(crate) fn gpui_next_sidebar_collapsed_state(collapsed: bool) -> bool {
    !collapsed
}

pub(crate) fn gpui_sidebar_chrome_visible(sidebar_collapsed: bool) -> bool {
    !sidebar_collapsed
}

pub(crate) fn gpui_next_sidebar_side(side: GpuiSidebarSide) -> GpuiSidebarSide {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    GPUI sidebar placement is a two-state shell model that mirrors native `sidebarSide`. Moving the sidebar flips only left/right placement; width and collapsed state remain separate user preferences.
    */
    match side {
        GpuiSidebarSide::Left => GpuiSidebarSide::Right,
        GpuiSidebarSide::Right => GpuiSidebarSide::Left,
    }
}

#[allow(dead_code)] // no caller: the body row is laid out inline in the root render() in app/core.rs; kept as the CDXC:GPUISidebarSide ordering contract
pub(crate) fn gpui_sidebar_body_chrome_order(
    side: GpuiSidebarSide,
    sidebar_collapsed: bool,
) -> Vec<GpuiSidebarBodyChromePart> {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    The GPUI body row uses normal non-overlapping siblings for sidebar placement parity. Expanded left renders sidebar/divider/workspace, expanded right renders workspace/divider/sidebar, and collapsed mode removes sidebar chrome without mutating the saved width.
    */
    if !gpui_sidebar_chrome_visible(sidebar_collapsed) {
        return vec![GpuiSidebarBodyChromePart::Workspace];
    }
    match side {
        GpuiSidebarSide::Left => vec![
            GpuiSidebarBodyChromePart::Sidebar,
            GpuiSidebarBodyChromePart::Divider,
            GpuiSidebarBodyChromePart::Workspace,
        ],
        GpuiSidebarSide::Right => vec![
            GpuiSidebarBodyChromePart::Workspace,
            GpuiSidebarBodyChromePart::Divider,
            GpuiSidebarBodyChromePart::Sidebar,
        ],
    }
}

pub(crate) fn gpui_sidebar_resize_delta(
    side: GpuiSidebarSide,
    current_x: f32,
    start_x: f32,
) -> f32 {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    Right-side sidebar resizing reverses the horizontal delta because the visible divider sits on the workspace edge. Dragging that divider left grows the sidebar, matching native AppKit layout math.
    */
    match side {
        GpuiSidebarSide::Left => current_x - start_x,
        GpuiSidebarSide::Right => start_x - current_x,
    }
}

pub(crate) fn gpui_sidebar_divider_x_bounds(
    side: GpuiSidebarSide,
    window_width: f32,
    sidebar_width: f32,
) -> (f32, f32) {
    match side {
        GpuiSidebarSide::Left => (sidebar_width, sidebar_width + SIDEBAR_DIVIDER_WIDTH),
        GpuiSidebarSide::Right => {
            let start = window_width - sidebar_width - SIDEBAR_DIVIDER_WIDTH;
            (start, start + SIDEBAR_DIVIDER_WIDTH)
        }
    }
}
