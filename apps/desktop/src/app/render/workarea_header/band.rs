//! The band across the top of the workspace column, and how it is divided when a view panel is
//! open: the header row over the Agents column, the view panel's tab strip over the view panel.

use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui_component::h_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Titlebar 2026-09-20 DECISION:
    /// User (screens 02 to 07): the header and the view panel's tab strip are one band of chrome.
    /// The header covers the sessions column and ends at the split divider, and the tab strip takes
    /// the band from there to the edge of the view panel, so Start/Open/Commit and the `⋯` menu sit
    /// over the column they act on and the tabs sit over the panel they open. This supersedes the
    /// phase 4 arrangement that stacked the strip in a second 36px row under a header spanning the
    /// whole workspace column, which cost the view a band of chrome the mockup does not draw.
    ///
    /// CDXC:Titlebar 2026-09-20 WHY:
    /// The two halves are sized by the same flex rules as the columns beneath them (the same grow
    /// ratio, `flex_basis(0)`, the same minimum widths, the same divider width between them), so
    /// they line up during a divider drag and a window resize without anyone measuring a bounds
    /// rectangle from the previous frame. Nothing here overlaps anything: the header row, the
    /// divider gap and the strip are non-overlapping siblings of one row, and only the band as a
    /// whole floats over the content beneath it, which is the overlap the user already approved.
    ///
    /// CDXC:Titlebar 2026-09-23 DECISION:
    /// User: on Windows, close, minimize and maximize must always stay at the very top right of the app, even when the sidebar or another column is shown.
    /// The rightmost band region owns the caption controls as fixed-width siblings, preserving the column divider alignment and the space available to each region's other controls.
    ///
    /// CDXC:Titlebar 2026-09-23 WHY:
    /// Linux client decorations use this same band ownership; putting its window controls among project actions left them above the sessions column when Code or Browser opened to the right.
    pub(crate) fn render_workarea_header(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let hosts_tab_strip = self.workarea_header_hosts_view_tab_strip();
        let split_ratio = workarea_split_ratio(self.project_editor_shell.workarea_split_ratio);
        let trailing_reserve = self.workarea_header_trailing_dock_reserve(window);
        let strip_mode = self.open_view_mode().unwrap_or(TitlebarMode::Agents);
        // An expanded panel runs under the header row too, so the whole band shrinks to the strip.
        let band_height = if hosts_tab_strip && self.view_panel_maximized() {
            WORKAREA_VIEW_TAB_STRIP_HEIGHT
        } else {
            WORKAREA_HEADER_HEIGHT
        };
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let mut window_controls = (cfg!(target_os = "windows")
            || matches!(
                window.window_decorations(),
                gpui::Decorations::Client { .. }
            ))
        .then(|| {
            self.render_titlebar_window_controls(window, cx)
                .into_any_element()
        });
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let mut window_controls: Option<gpui::AnyElement> = None;
        h_flex()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(band_height))
            .items_start()
            .child(
                div()
                    .flex()
                    .h_full()
                    .min_h_0()
                    // Inside the work area frame's card the band starts under the card's 1pt top
                    // border. The row centres its buttons, so a bottom padding of twice the border
                    // lifts them by it and keeps them level with the sidebar's top row and the
                    // traffic lights, while nothing opaque in the band paints over the border.
                    .when(work_area_frame_active(), |region| {
                        region.pb(px(2.0 * WORK_AREA_FRAME_BORDER))
                    })
                    .when(!hosts_tab_strip, |region| region.flex_1().min_w_0())
                    .when(hosts_tab_strip, |region| {
                        // Grows by 1 while the strip slides beside it (render/workarea_split.rs).
                        let grow = if self.panel_motion.view_panel.frame().animating {
                            1.0
                        } else {
                            split_ratio
                        };
                        region
                            .flex_grow(grow)
                            .flex_shrink(1.0)
                            .flex_basis(relative(0.0))
                            .min_w(px(WORKAREA_AGENTS_COLUMN_MIN_WIDTH))
                    })
                    .child(self.render_workarea_header_row(window, cx))
                    .children(if !hosts_tab_strip && trailing_reserve == 0.0 {
                        window_controls.take()
                    } else {
                        None
                    }),
            )
            .when(hosts_tab_strip, |band| {
                band.child(
                    // CDXC:Workarea 2026-09-21 DECISION:
                    // User: the view panel has a border line down its left from the top of the tab strip to the bottom of the panel, and the drag bar covers that whole line. This column sits exactly above the split rail, so it is the rail's top segment: the same colour, grab strip and hover line. This supersedes the same-day rule that kept it visual-only. An expanded panel has no rail to continue, so the column is only painted then.
                    if self.view_panel_maximized() {
                        div()
                            .flex_shrink_0()
                            .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                            .bg(project_editor_companion_divider_background_color())
                            .into_any_element()
                    } else {
                        div()
                            .flex_shrink_0()
                            .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                            .child(self.render_workarea_split_divider("header", cx))
                            .into_any_element()
                    },
                )
                .child({
                    // Alone in its sliding clip it grows by 1, or it would fill only its share
                    // of the clip (render/workarea_split.rs).
                    let strip_grow = if self.panel_motion.view_panel.frame().animating {
                        1.0
                    } else {
                        1.0 - split_ratio
                    };
                    let strip = div()
                        .flex()
                        .flex_grow(strip_grow)
                        .flex_shrink(1.0)
                        .flex_basis(relative(0.0))
                        .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                        .min_h_0()
                        .min_w(px(WORKAREA_VIEW_PANEL_MIN_WIDTH))
                        .overflow_hidden()
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .child(self.render_view_tab_strip(strip_mode, cx)),
                        )
                        .children(if trailing_reserve == 0.0 {
                            window_controls.take()
                        } else {
                            None
                        });
                    // The strip rides the view panel's tween so it stays over the panel.
                    let frame = self.panel_motion.view_panel.frame();
                    if frame.animating {
                        crate::app::panel_motion::clip_panel_horizontally(
                            frame,
                            true,
                            strip.into_any_element(),
                        )
                        .h(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                        .into_any_element()
                    } else {
                        strip.into_any_element()
                    }
                })
            })
            .when(trailing_reserve > 0.0, |band| {
                // The band paints no fill of its own, so the 1px under the shorter tab strip shows
                // the panel beneath; the command pane's reserved width keeps the workspace colour.
                band.child(
                    h_flex()
                        .flex_shrink_0()
                        .h_full()
                        .w(px(trailing_reserve))
                        .bg(workspace_nested_background())
                        .justify_end()
                        .children(window_controls.take()),
                )
            })
    }

    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: the view panel's expand and minimize button stays in the same spot when clicked. So the
    /// band keeps both halves at the same split while the panel is expanded, and the strip, its
    /// pop-out and its expand control never drop to a row of their own. This supersedes the
    /// 2026-09-20 rule that an expanded panel moved the strip under the header.
    pub(crate) fn workarea_header_hosts_view_tab_strip(&self) -> bool {
        self.view_panel_open()
    }

    /// CDXC:CommandPane 2026-09-20 WHY:
    /// A right-docked command pane is a sibling column of the whole workarea, so the band has to
    /// keep its width free or the tab strip would run on past the view panel and over the pane's own
    /// tab bar. The plan is recomputed from the same inputs `render_workspace_with_command_pane`
    /// uses, in the same frame, rather than read back from a recorded bounds rectangle.
    fn workarea_header_trailing_dock_reserve(&self, window: &Window) -> f32 {
        // A right dock opening or closing reserves what is on screen of it this frame.
        let frame = self.panel_motion.command_pane.frame();
        if frame.animating && self.command_pane_side == GpuiCommandPaneSide::Right {
            return frame.extent.max(0.0);
        }
        self.workarea_header_trailing_dock_reserve_at_rest(window)
    }

    /// The reserve with the right dock at its settled size.
    pub(crate) fn workarea_header_trailing_dock_reserve_at_rest(&self, window: &Window) -> f32 {
        match self.command_pane_layout_plan(window) {
            CommandPaneWorkspaceLayoutPlan::PinnedRight { panel_width } => {
                panel_width + COMMAND_PANE_SPLIT_HANDLE_THICKNESS
            }
            _ => 0.0,
        }
    }

    /// How wide the header row itself gets this frame: the whole band, minus anything a right dock
    /// reserves, minus the view panel's share of the split while the strip shares the band. The
    /// header drops its labels from this width rather than from the window's, because with a view
    /// open the labels have only the sessions column to fit in.
    pub(crate) fn workarea_header_row_width(&self, window: &Window) -> f32 {
        (self.workarea_header_row_width_with_reserve(
            window,
            self.workarea_header_trailing_dock_reserve(window),
        ) - self.workarea_header_closing_clearance())
        .max(0.0)
    }

    /// The header row's width with every panel at its settled size.
    pub(crate) fn workarea_header_row_width_at_rest(&self, window: &Window) -> f32 {
        self.workarea_header_row_width_with_reserve(
            window,
            self.workarea_header_trailing_dock_reserve_at_rest(window),
        )
    }

    fn workarea_header_row_width_with_reserve(&self, window: &Window, reserve: f32) -> f32 {
        let band_width =
            (command_pane_workspace_width(window, self.sidebar_width, self.sidebar_collapsed)
                - reserve)
                .max(0.0);
        if !self.workarea_header_hosts_view_tab_strip() {
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            if reserve == 0.0
                && (cfg!(target_os = "windows")
                    || matches!(
                        window.window_decorations(),
                        gpui::Decorations::Client { .. }
                    ))
            {
                return (band_width - 3.0 * TITLEBAR_WINDOW_BUTTON_WIDTH).max(0.0);
            }
            return band_width;
        }
        let split_span = (band_width - WORKSPACE_SPLIT_HANDLE_THICKNESS).max(0.0);
        let ratio = workarea_split_ratio_for_span(
            self.project_editor_shell.workarea_split_ratio,
            split_span,
        );
        split_span * ratio
    }
}
