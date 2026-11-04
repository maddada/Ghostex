//! The reveal and hide motion of the window's collapsible panels: the sidebar, the view panel and
//! the command pane, bottom or right. One tween drives all of them so they open and close alike.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{Context, Window};

use crate::GhostexGpuiApp;

mod tween;
pub(crate) use tween::*;

/// Whether the view panel itself is sliding open or shut this frame. Its page is the only one a
/// slide clips; every other slide just pushes pages, which then resize live.
static VIEW_PANEL_SLIDING: AtomicBool = AtomicBool::new(false);

pub(crate) fn view_panel_sliding() -> bool {
    VIEW_PANEL_SLIDING.load(Ordering::Relaxed)
}

/// Whether the view panel is shut and would slide if it opened now: a page shown at this point is
/// the panel opening, and it starts transparent so its last frame never flashes before the slide's
/// first paint takes it over (`CefSurface::set_visible`).
static VIEW_PANEL_WOULD_SLIDE_OPEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn view_panel_would_slide_open() -> bool {
    VIEW_PANEL_WOULD_SLIDE_OPEN.load(Ordering::Relaxed)
}

/// The three panels' motion, sampled once at the top of each root render so the body and the
/// header band read the same frame.
#[derive(Debug, Default)]
pub(crate) struct PanelMotions {
    pub(crate) sidebar: PanelMotion,
    pub(crate) view_panel: PanelMotion,
    /// The Agents Panel (the sessions column beside an open view), folded away by the Agents Panel
    /// toggle and Expand.
    pub(crate) agents_column: PanelMotion,
    pub(crate) command_pane: PanelMotion,
    /// The command pane's dock side the last frame, so a dock move resets instead of animating.
    command_pane_side: Option<crate::app::model::GpuiCommandPaneSide>,
    /// The project on screen the last frame. The view panel and the command pane belong to the
    /// project, so switching projects shows the new one's panels as they are rather than sliding.
    project_id: Option<String>,
    /// Whether the view panel showed the picker the last frame it was open. The picker is GPUI's
    /// own drawing, so a closing slide keeps it in the frame to fade it out; a view's native page
    /// is gone with the view.
    pub(crate) view_panel_showed_picker: bool,
    was_animating: bool,
}

impl PanelMotions {
    /// Whether a panel was still sliding on the last frame drawn.
    pub(crate) fn animating(&self) -> bool {
        self.was_animating
    }
}

impl GhostexGpuiApp {
    /// Samples every panel's tween for this frame and keeps frames coming while one runs. Called
    /// at the top of the root render, before anything reads a panel's size.
    pub(crate) fn sample_panel_motion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = web_time::Instant::now();
        let reduce_motion = cx.reduce_motion();

        if self.panel_motion.project_id != self.agents_workspace_project_id {
            self.panel_motion
                .project_id
                .clone_from(&self.agents_workspace_project_id);
            self.panel_motion.view_panel.reset();
            self.panel_motion.agents_column.reset();
            self.panel_motion.command_pane.reset();
        }

        let sidebar_open = crate::app::model::gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        let sidebar_extent = self.sidebar_width + crate::app::consts::SIDEBAR_DIVIDER_WIDTH;
        self.panel_motion
            .sidebar
            .sample(sidebar_open, sidebar_extent, 0.0, reduce_motion, now);

        if self.view_panel_maximized() {
            self.panel_motion.view_panel.reset();
        } else {
            let open = self.view_panel_open();
            if open {
                self.panel_motion.view_panel_showed_picker = self.view_picker_open();
            }
            let extent = if open {
                self.view_panel_open_width(window)
            } else {
                0.0
            };
            self.panel_motion
                .view_panel
                .sample(open, extent, 0.0, reduce_motion, now);
        }

        // The Agents Panel folds only beside an open panel (a view or the picker); without one it is
        // the whole workarea.
        if self.view_panel_open() {
            let open = !self.view_panel_maximized();
            let extent = self.workarea_header_row_width_at_rest(window);
            self.panel_motion
                .agents_column
                .sample(open, extent, 0.0, reduce_motion, now);
        } else {
            self.panel_motion.agents_column.reset();
        }

        let (side, open, extent, closed) = self.command_pane_motion_state(window);
        if self.panel_motion.command_pane_side != side {
            self.panel_motion.command_pane.reset();
            self.panel_motion.command_pane_side = side;
        }
        if side.is_some() {
            self.panel_motion
                .command_pane
                .sample(open, extent, closed, reduce_motion, now);
        }

        let animating = self.panel_motion.sidebar.frame().animating
            || self.panel_motion.view_panel.frame().animating
            || self.panel_motion.agents_column.frame().animating
            || self.panel_motion.command_pane.frame().animating;
        VIEW_PANEL_SLIDING.store(
            self.panel_motion.view_panel.frame().animating,
            Ordering::Relaxed,
        );
        VIEW_PANEL_WOULD_SLIDE_OPEN.store(
            !self.view_panel_open() && !reduce_motion && !panel_motion_duration().is_zero(),
            Ordering::Relaxed,
        );
        crate::terminal_element::set_grid_resize_held(animating);
        if animating {
            window.request_animation_frame();
        } else if self.panel_motion.was_animating {
            // Views whose bounds did not change on the settling frame are reused from the cache and
            // would keep a held terminal grid or a transparent page; one full refresh lets every
            // one of them take its final size.
            cx.defer_in(window, |_, window, _| window.refresh());
        }
        self.panel_motion.was_animating = animating;
    }
}

impl GhostexGpuiApp {
    /// The view panel's width while it is open: the band's width less the header row's share of the
    /// split and the rail between them, the same numbers the band and the body lay out from.
    fn view_panel_open_width(&self, window: &Window) -> f32 {
        let band_width = (crate::app::model::command_pane_workspace_width(
            window,
            self.sidebar_width,
            self.sidebar_collapsed,
        ) - self.workarea_header_trailing_dock_reserve_at_rest(window))
        .max(0.0);
        (band_width
            - crate::app::consts::WORKSPACE_SPLIT_HANDLE_THICKNESS
            - self.workarea_header_row_width_at_rest(window))
        .max(0.0)
    }

    /// The command pane's dock side (none while it floats, which does not animate), whether it is
    /// open, its size open (its rail included), and the size it collapses to.
    fn command_pane_motion_state(
        &self,
        window: &Window,
    ) -> (
        Option<crate::app::model::GpuiCommandPaneSide>,
        bool,
        f32,
        f32,
    ) {
        use crate::app::consts::{
            COMMAND_PANE_SPLIT_HANDLE_THICKNESS, WORKSPACE_SPLIT_HANDLE_THICKNESS,
        };
        use crate::app::model::{CommandPaneWorkspaceLayoutPlan as Plan, GpuiCommandPaneSide};
        let side = self.command_pane_side;
        let (open, extent, closed) = match self.command_pane_layout_plan(window) {
            Plan::Pinned { panel_height } => {
                (true, panel_height + WORKSPACE_SPLIT_HANDLE_THICKNESS, 0.0)
            }
            Plan::PinnedRight { panel_width } => {
                (true, panel_width + COMMAND_PANE_SPLIT_HANDLE_THICKNESS, 0.0)
            }
            Plan::Collapsed { bottom_reservation } => (
                false,
                0.0,
                if side == GpuiCommandPaneSide::Bottom {
                    bottom_reservation.height
                } else {
                    0.0
                },
            ),
            Plan::Hidden => (false, 0.0, 0.0),
        };
        (Some(side), open, extent, closed)
    }

    /// This frame's command pane layout, from the same inputs its renderer and the header band use.
    pub(crate) fn command_pane_layout_plan(
        &self,
        window: &Window,
    ) -> crate::app::model::CommandPaneWorkspaceLayoutPlan {
        let workspace_width = crate::app::model::command_pane_workspace_width(
            window,
            self.sidebar_width,
            self.sidebar_collapsed,
        );
        crate::app::model::command_pane_workspace_layout_plan(
            self.command_pane.mode,
            self.command_pane.has_panel_sessions(),
            crate::app::model::command_pane_content_height(window),
            self.command_pane.height_ratio,
            self.command_pane_side,
            workspace_width,
            self.command_pane.width_ratio,
        )
    }
}
