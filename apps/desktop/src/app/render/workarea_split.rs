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
        if let Some(mode) = mode.filter(|_| self.view_panel_maximized()) {
            return self.render_maximized_view_panel(mode, window, cx);
        }
        h_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = metrics_view.update(cx, |this, _cx| {
                    this.record_workarea_split_layout_metrics(&child_bounds);
                });
            })
            .id(format!("ghostex-gpui-workarea-split-{}", mode_slug))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .bg(project_editor_shell_background_color())
            .child(self.render_agents_workspace(
                AgentsWorkspaceLayout::Column { split_ratio },
                window,
                cx,
            ))
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
                    .pt(px(WORKAREA_HEADER_HEIGHT))
                    .child(self.render_workarea_split_divider(cx)),
            )
            .child(
                // CDXC:Workarea 2026-09-14 WHY:
                // Browser owns its borders inside its leaves; other views own a surface border.
                // Keep those borders inside the flex allocation so switching views cannot change the
                // Agents column's width.
                v_flex()
                    .pt(px(WORKAREA_HEADER_HEIGHT))
                    .flex_grow(1.0 - split_ratio)
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
                                let _ = surface_view.update(cx, |this, _cx| {
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
                            .when(strip_mode != TitlebarMode::Browser, |this| {
                                rail_aware_pane_border(
                                    this,
                                    RailFacingEdges {
                                        left: true,
                                        ..outer_rail_edges
                                    },
                                    workspace_pane_border_color_for_state(surface_border_state),
                                    workspace_pane_border_color(),
                                )
                            })
                            .child(match mode {
                                Some(mode) => self.render_project_editor_surface(mode, window, cx),
                                None => self.render_view_picker(cx),
                            })
                            .window_corner_pane(),
                    ),
            )
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
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let mode_slug = mode.element_slug();
        let surface_border_state = self.project_editor_surface_border_state(mode, window);
        let outer_rail_edges = self.main_workspace_outer_rail_edges(window);
        let surface_view = cx.entity().clone();
        v_flex()
            .id(format!("ghostex-gpui-workarea-maximized-{}", mode_slug))
            .pt(px(WORKAREA_HEADER_HEIGHT))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(project_editor_shell_background_color())
            .child(self.render_view_tab_strip(mode, cx))
            .child(
                div()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = surface_view.update(cx, |this, _cx| {
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
                    .when(mode != TitlebarMode::Browser, |this| {
                        rail_aware_pane_border(
                            this,
                            outer_rail_edges,
                            workspace_pane_border_color_for_state(surface_border_state),
                            workspace_pane_border_color(),
                        )
                    })
                    .child(self.render_project_editor_surface(mode, window, cx))
                    .window_corner_pane(),
            )
            .into_any_element()
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The visible two-pixel rail between the Agents column and the view panel is the resize control,
    /// as it was between the companion and the editor. The grab strip stays on the Agents side while
    /// that side is GPUI-painted and straddles the rail when both sides are CEF pages, which is the
    /// same rule every other rail in the workspace follows.
    pub(crate) fn render_workarea_split_divider(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let hover_visible = self.workarea_split_divider_hover_visible;
        // The view panel behind the rail is a CEF page. The Agents column is GPUI-painted unless one
        // of its panes shows React chat, which is a CEF page of its own and takes the mouse itself.
        let grab_side = if self.workspace_node_shows_cef_chat(&self.agents_workspace.root) {
            ResizeRailGrabSide::Straddle
        } else {
            ResizeRailGrabSide::Leading
        };
        div()
            .id("ghostex-gpui-workarea-split-divider")
            .relative()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .bg(project_editor_companion_divider_background_color())
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    "ghostex-gpui-workarea-split-grab-strip",
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
                        "ghostex-gpui-workarea-split-divider-hover-line",
                        WorkspaceSplitAxis::Horizontal,
                        grab_side,
                    ))
                }),
            ))
            .into_any_element()
    }
}
