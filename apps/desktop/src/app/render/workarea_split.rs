//! The workarea when a view is open: the Agents workspace in the left column, a real divider, and
//! the view panel beside it. The row, its divider and its width ratio are the project-editor
//! companion's, inherited whole; only the occupant of the left column changed.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::agents_workspace_layout::AgentsWorkspaceLayout;
use crate::app::render::resize_rail::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 DECISION:
    /// User: the Agents workspace and a project view are on screen at the same time, in two columns
    /// with a real divider, instead of replacing each other. `TitlebarMode::Agents` stops meaning "a
    /// mode of its own" and starts meaning "no view is open"; every other mode is the view the right
    /// panel shows. This supersedes the 2026-06-22 rule that project-editor modes replace the main
    /// workspace area while active.
    /// `None` is the panel with no view in it: the tab strip over the picker (screen 02). The
    /// column, the rail and the panel's frame are identical either way, so they are described once.
    pub(crate) fn render_workarea_with_open_view(
        &mut self,
        mode: Option<TitlebarMode>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:CodeEditor 2026-06-22-17:18:
        Source, Browser, Kanban, and Manage share this horizontal shell, and gpui-component h_flex
        centers children by default. Override that alignment and make the view surface slot
        full-height so placeholders and Browser CEF bodies fill the available workspace height
        instead of rendering as a centered band with black space above and below.
        */
        let strip_mode = mode.unwrap_or(TitlebarMode::Agents);
        let mode_slug = strip_mode.element_slug();
        let split_ratio = workarea_split_ratio(self.project_editor_shell.workarea_split_ratio);
        // The picker's own focus target is `ProjectEditorSurface(Agents)`, so the same call gives
        // the panel its focused border while the picker holds the keys.
        let surface_border_state = self.project_editor_surface_border_state(strip_mode, window);
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let metrics_view = cx.entity().clone();
        let surface_view = cx.entity().clone();
        if self.view_panel_maximized() {
            let panel = self.render_maximized_view_panel(mode, window, cx);
            // Folding away, the Agents Panel's frame slides shut on the left (panel_motion.rs).
            let frame = self.panel_motion.agents_column.frame();
            if !frame.animating {
                return panel;
            }
            return h_flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_stretch()
                .overflow_hidden()
                .child(crate::app::panel_motion::closing_panel_ghost(
                    frame,
                    false,
                    true,
                    project_editor_companion_divider_background_color(),
                    WORKSPACE_SPLIT_HANDLE_THICKNESS,
                    workspace_nested_background(),
                ))
                .child(v_flex().flex_1().min_w_0().min_h_0().h_full().child(panel))
                .into_any_element();
        }
        h_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                metrics_view.update(cx, |this, _cx| {
                    this.record_workarea_split_layout_metrics(&child_bounds);
                });
            })
            .id(format!("ghostex-gpui-workarea-split-{}", mode_slug))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .bg(glass_clear(project_editor_shell_background_color()))
            .child({
                /*
                CDXC:Workarea 2026-09-23 WHY:
                Flex grow factors that sum to less than 1 hand out only that fraction of the free space. While the view panel slides it is a fixed-width frame, so the Agents column is the row's only grower, and at its split ratio (0.3 or so) it stopped at its minimum width and left the panel sliding out of it with empty space to the right. It grows by 1 for the slide; the same goes for the panel while the Agents column slides, and for the header band above them. Inside its own sliding clip each one is the clip's only child and grows by 1 too: the panel at its 0.7 or so filled only that share of the clip, so the picker's cards faded in squeezed and jumped wide when the slide settled.
                */
                let split_ratio = if self.panel_motion.view_panel.frame().animating
                    || self.panel_motion.agents_column.frame().animating
                {
                    1.0
                } else {
                    split_ratio
                };
                // The sessions, or Claude's page while Claude is on (app/claude_web/column.rs).
                let agents = self.render_workarea_primary_column(
                    AgentsWorkspaceLayout::Column { split_ratio },
                    window,
                    cx,
                );
                // Coming back, the Agents Panel slides in at its settled width, its left edge in
                // place (panel_motion.rs).
                let frame = self.panel_motion.agents_column.frame();
                if frame.animating {
                    crate::app::panel_motion::clip_panel_horizontally(frame, false, agents)
                        .into_any_element()
                } else {
                    agents
                }
            })
            .child(
                /*
                CDXC:Titlebar 2026-09-20 WHY:
                The rail and the view panel start one header height down because neither may pass
                under the floating band: the rail is a grab target, and the panel's content is a CEF
                page, an AppKit child view that paints over everything GPUI draws and would hide the
                band instead of fading under it. The panel's own tab strip is in the band instead,
                as the mockup draws it (render/workarea_header/band.rs). Only the Agents column, and
                only while it holds the GPUI chat, reaches the window's top edge.
                */
                v_flex()
                    .flex_shrink_0()
                    .h_full()
                    .pt(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                    .child(self.render_workarea_split_divider("body", cx)),
            )
            .child({
                let panel_frame = self.panel_motion.view_panel.frame();
                // CDXC:Workarea 2026-09-14 WHY:
                // Browser owns its borders inside its leaves; other views own a surface border.
                // Keep those borders inside the flex allocation so switching views cannot change the
                // Agents column's width.
                let panel_grow =
                    if panel_frame.animating || self.panel_motion.agents_column.frame().animating {
                        1.0
                    } else {
                        1.0 - split_ratio
                    };
                let panel = v_flex()
                    .pt(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                    .flex_grow(panel_grow)
                    .flex_shrink_1()
                    .flex_basis(relative(0.0))
                    .h_full()
                    .min_w(px(WORKAREA_VIEW_PANEL_MIN_WIDTH))
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .on_children_prepainted(move |child_bounds, _window, cx| {
                                let Some(mode) = mode else {
                                    return;
                                };
                                surface_view.update(cx, |this, _cx| {
                                    this.record_project_editor_surface_layout_bounds(
                                        mode,
                                        &child_bounds,
                                    );
                                });
                            })
                            .id(format!(
                                "ghostex-gpui-project-editor-surface-slot-{}",
                                mode_slug
                            ))
                            .flex()
                            .flex_col()
                            .flex_1()
                            .w_full()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            // Opening, the panel's frame slides in empty and its view fades in near
                            // the end, so it is never seen half revealed (panel_motion.rs).
                            .opacity(panel_frame.opening_content_opacity())
                            .when_some(mode, |this, mode| {
                                this.capture_any_mouse_down(cx.listener(
                                    move |this, _: &MouseDownEvent, window, cx| {
                                        this.focus_view_panel_on_press(mode, window, cx);
                                    },
                                ))
                            })
                            .when(strip_mode != TitlebarMode::Browser, |this| {
                                rail_aware_pane_border(
                                    this,
                                    RailFacingEdges {
                                        left: true,
                                        ..outer_rail_edges
                                    },
                                    workspace_pane_border_color_for_state(surface_border_state),
                                    workspace_pane_border_color(),
                                    None,
                                )
                            })
                            .child(match mode {
                                Some(mode) => self.render_project_editor_surface(mode, window, cx),
                                None => self.render_view_picker(cx),
                            })
                            .window_corner_pane(),
                    );
                // Opening, the panel slides in at its settled width (panel_motion.rs).
                if panel_frame.animating {
                    crate::app::panel_motion::clip_panel_horizontally(
                        panel_frame,
                        true,
                        panel.into_any_element(),
                    )
                    .into_any_element()
                } else {
                    panel.into_any_element()
                }
            })
            .into_any_element()
    }

    /// Closing from the picker, the panel's frame slides shut with the picker still in it, laid out
    /// at its open width and fading out over the first half of the slide (panel_motion.rs).
    pub(crate) fn render_closing_view_picker(
        &mut self,
        frame: crate::app::panel_motion::PanelFrame,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let surface_border_state =
            self.project_editor_surface_border_state(TitlebarMode::Agents, window);
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let slot = div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .opacity(frame.closing_content_opacity());
        let slot = rail_aware_pane_border(
            slot,
            RailFacingEdges {
                left: true,
                ..outer_rail_edges
            },
            workspace_pane_border_color_for_state(surface_border_state),
            workspace_pane_border_color(),
            None,
        )
        .child(self.render_view_picker(cx))
        .window_corner_pane();
        let panel = h_flex()
            .size_full()
            .items_stretch()
            .bg(glass_clear(project_editor_shell_background_color()))
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                    .bg(project_editor_companion_divider_background_color()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .pt(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
                    .child(slot),
            );
        crate::app::panel_motion::clip_panel_horizontally(frame, true, panel.into_any_element())
            .into_any_element()
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// Expanded, the view panel is the workarea: no sessions column, no rail, nothing left behind,
    /// which is the whole point of screen 09. The Agents workspace is not rendered here and
    /// `agents_workspace_visible()` says so in the same frame, so its terminals and chat pages hide
    /// their native child views instead of painting over the maximised page; they are not torn down,
    /// and restoring the column brings every one of them back exactly as a view switch does.
    fn render_maximized_view_panel(
        &mut self,
        mode: Option<TitlebarMode>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let strip_mode = mode.unwrap_or(TitlebarMode::Agents);
        let mode_slug = strip_mode.element_slug();
        let surface_border_state = self.project_editor_surface_border_state(strip_mode, window);
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let surface_view = cx.entity().clone();
        // CDXC:Workarea 2026-09-23 DECISION:
        // User: with the chat hidden, every view except Browser gets a #252525 1px line above the side panel content. It replaces the pane's own top border so the edge is one line, not two.
        let draws_top_line = strip_mode != TitlebarMode::Browser;
        v_flex()
            .id(format!("ghostex-gpui-workarea-maximized-{}", mode_slug))
            .pt(px(WORKAREA_VIEW_TAB_STRIP_HEIGHT))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(glass_clear(project_editor_shell_background_color()))
            .when(draws_top_line, |this| {
                this.child(
                    div()
                        .id(format!(
                            "ghostex-gpui-workarea-maximized-top-line-{}",
                            mode_slug
                        ))
                        .flex_shrink_0()
                        .w_full()
                        .h(px(1.0))
                        .bg(maximized_view_panel_top_line_color()),
                )
            })
            .child(
                div()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let Some(mode) = mode else {
                            return;
                        };
                        surface_view.update(cx, |this, _cx| {
                            this.record_project_editor_surface_layout_bounds(mode, &child_bounds);
                        });
                    })
                    .id(format!(
                        "ghostex-gpui-project-editor-surface-slot-{}",
                        mode_slug
                    ))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .h_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .when_some(mode, |this, mode| {
                        this.capture_any_mouse_down(cx.listener(
                            move |this, _: &MouseDownEvent, window, cx| {
                                this.focus_view_panel_on_press(mode, window, cx);
                            },
                        ))
                    })
                    .when(draws_top_line, |this| {
                        rail_aware_pane_border(
                            this,
                            RailFacingEdges {
                                top: true,
                                ..outer_rail_edges
                            },
                            workspace_pane_border_color_for_state(surface_border_state),
                            workspace_pane_border_color(),
                            None,
                        )
                    })
                    .child(match mode {
                        Some(mode) => self.render_project_editor_surface(mode, window, cx),
                        None => self.render_view_picker(cx),
                    })
                    .window_corner_pane(),
            )
            .into_any_element()
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The visible two-pixel rail between the Agents column and the view panel is the resize control,
    /// as it was between the companion and the editor. The grab strip stays on the Agents side while
    /// that side is GPUI-painted, which it always is: the Agents column holds only GPUI terminals and
    /// GPUI chat.
    ///
    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: the drag bar runs the full height of the view panel's left edge, including beside the header, where its top was missing. The rail is drawn in two segments, one in the header band and one under it, because the band floats over the column; both are the same control and share one hover state, so the accent line shows as one bar.
    pub(crate) fn render_workarea_split_divider(
        &self,
        segment: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover_visible = self.workarea_split_divider_hover_visible;
        // The view panel behind the rail is a CEF page, so the grab strip stays on the Agents side.
        let grab_side = ResizeRailGrabSide::Leading;
        div()
            .id(format!("ghostex-gpui-workarea-split-divider-{segment}"))
            .relative()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .bg(project_editor_companion_divider_background_color())
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    format!("ghostex-gpui-workarea-split-grab-strip-{segment}"),
                    WorkspaceSplitAxis::Horizontal,
                    grab_side,
                )
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    this.set_workarea_split_divider_hovering(*hovered, cx);
                }))
                .on_mouse_move(
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        this.set_workarea_split_divider_hovering(true, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_workarea_split_divider_mouse_down(event, window, cx);
                    }),
                )
                .when(hover_visible, |this| {
                    this.child(resize_rail_hover_line(
                        format!("ghostex-gpui-workarea-split-divider-hover-line-{segment}"),
                        WorkspaceSplitAxis::Horizontal,
                        grab_side,
                    ))
                }),
            ))
            .into_any_element()
    }
}
