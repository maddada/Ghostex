// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: Agents workspace top-level render entry and its pane split/leaf recursion.

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
use crate::app::render::resize_rail::*;
use crate::*;

/// Where the Agents workspace sits in the workarea this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AgentsWorkspaceLayout {
    /// No view is open, so the Agents workspace has the whole workarea.
    FullWidth,
    /// A view panel is open beside it; `split_ratio` is the Agents column's share of the row.
    Column { split_ratio: f32 },
    /// The workarea folded the column away and the left-edge reveal is carrying it, so it fills a
    /// slot in the floating panel instead of a column of the main window. There is no header above
    /// it there and no command pane beside it, so it neither insets for one nor fades under one.
    Floating,
}

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-20 WHY:
    /// The Agents workspace is rendered on every frame now, either alone or as the left column of a
    /// workarea whose right half is an open view. Terminals and chats therefore keep recording body
    /// bounds across a view open, change or close, which is what keeps their Ghostty surfaces and
    /// chat pages mounted; nothing outside this function decides whether they exist.
    pub(crate) fn render_agents_workspace(
        &self,
        layout: AgentsWorkspaceLayout,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:Workarea 2026-06-22-05:11:
        Agents mode renders the workspace as GPUI native layout chrome: tab groups and split nodes own normal non-overlapping regions, every leaf keeps a tab bar even when it is the only pane, and Ghostty content is represented by black placeholder surfaces until libghostty integration lands.

        CDXC:Workarea 2026-06-22-14:40:
        The Agents workspace root must be a vertical flex container, not only a flex-sized child of the command-pane wrapper. The rendered split or leaf tree uses flex_1 sizing, so it needs this parent layout context to fill the available height above the command pane instead of leaving a black shell gap below the terminal pane.
        */
        /*
        CDXC:Titlebar 2026-09-20 WHY:
        This column is the one the header floats over, so it starts at the window's top edge while
        it holds the GPUI chat the fade is drawn over, and one header height down in every other
        case, so a tab bar, a pane outline, a terminal grid or a CEF page never ends up behind the
        header. The top edge is the header's own line there, so the pane draws no border against it,
        exactly as it draws none against a resize rail. The rule is in
        render/workarea_header/overlap.rs and the decision behind it on the header itself.
        */
        let floating = layout == AgentsWorkspaceLayout::Floating;
        let flows_under_header = !floating && self.agents_column_flows_under_workarea_header(cx);
        let top_inset = if floating {
            0.0
        } else {
            self.workarea_header_column_top_inset(flows_under_header)
        };
        let rail_edges = if floating {
            // The panel's own rail is the only line on the column's left; nothing else in that
            // window touches it.
            RailFacingEdges {
                left: true,
                ..RailFacingEdges::default()
            }
        } else {
            let outer_rail_edges = RailFacingEdges {
                top: flows_under_header,
                ..self.main_workspace_outer_rail_edges(window)
            };
            match layout {
                // The split divider is the column's right-hand rail, so the panes there own no
                // border.
                AgentsWorkspaceLayout::Column { .. } => RailFacingEdges {
                    right: true,
                    ..outer_rail_edges
                },
                _ => outer_rail_edges,
            }
        };
        let root = v_flex()
            .id("ghostex-gpui-agents-workspace")
            .relative()
            .pt(px(top_inset))
            .min_h_0()
            .overflow_hidden()
            .bg(workspace_background_color());
        let root = match layout {
            AgentsWorkspaceLayout::FullWidth | AgentsWorkspaceLayout::Floating => {
                root.flex_1().min_w_0()
            }
            AgentsWorkspaceLayout::Column { split_ratio } => root
                .flex_grow(split_ratio)
                .flex_shrink_1()
                .flex_basis(relative(0.0))
                .h_full()
                .min_w(px(WORKAREA_AGENTS_COLUMN_MIN_WIDTH)),
        };
        root.child(
            if let Some(pane_id) = self.agents_workspace.focus_mode_pane
                && let Some(leaf) = self.agents_workspace.find_leaf(pane_id)
            {
                self.render_workspace_leaf(leaf, rail_edges, window, cx)
            } else {
                self.render_workspace_node(&self.agents_workspace.root, rail_edges, window, cx)
            },
        )
        .when(flows_under_header, |this| {
            this.child(self.render_workarea_header_content_fade())
        })
        .into_any_element()
    }

    /// The sides of the main workspace area, in any view, that touch a rail owned by something outside
    /// it: the sidebar divider on the left, and the command pane boundary below or to the right when pinned.
    pub(crate) fn main_workspace_outer_rail_edges(&self, window: &Window) -> RailFacingEdges {
        let layout_plan = command_pane_workspace_layout_plan(
            self.command_pane.mode,
            self.command_pane.has_sessions(),
            command_pane_content_height(window),
            self.command_pane.height_ratio,
            self.command_pane_side,
            command_pane_workspace_width(window, self.sidebar_width, self.sidebar_collapsed),
            self.command_pane.width_ratio,
        );
        RailFacingEdges {
            left: gpui_sidebar_chrome_visible(self.sidebar_collapsed),
            right: matches!(
                layout_plan,
                CommandPaneWorkspaceLayoutPlan::PinnedRight { .. }
            ),
            top: false,
            bottom: matches!(layout_plan, CommandPaneWorkspaceLayoutPlan::Pinned { .. }),
        }
    }

    pub(crate) fn render_workspace_node(
        &self,
        node: &WorkspaceNode,
        rail_edges: RailFacingEdges,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match node {
            WorkspaceNode::Split(split) => {
                self.render_workspace_split(split, rail_edges, window, cx)
            }
            WorkspaceNode::Leaf(leaf) => self.render_workspace_leaf(leaf, rail_edges, window, cx),
        }
    }

    pub(crate) fn render_workspace_split(
        &self,
        split: &WorkspaceSplit,
        rail_edges: RailFacingEdges,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        let (first_rail_edges, second_rail_edges) = rail_edges.for_split_children(axis);
        let ratio = workspace_split_ratio(split.ratio);
        let first = div()
            .id(format!("ghostex-gpui-workspace-split-{}-first", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_workspace_node(&split.first, first_rail_edges, window, cx));
        let second = div()
            .id(format!(
                "ghostex-gpui-workspace-split-{}-second",
                split_id.0
            ))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(1.0 - ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_workspace_node(&split.second, second_rail_edges, window, cx));

        /*
        CDXC:Workarea 2026-06-22-05:11:
        Split handles are explicit layout siblings between split children. This keeps future resize hit regions in the normal tree while the current visual separator remains a non-interactive child, avoiding transparent overlays or overlapping terminal/web surfaces.

        CDXC:Workarea 2026-06-22-06:45:
        Workspace split containers report their first/handle/second child bounds from normal GPUI layout so resize drags can update the persisted split ratio for the exact rendered branch. The handle remains the only hit target; there is no invisible resize overlay or root-level hit-test redirection.
        */
        match split.axis {
            WorkspaceSplitAxis::Horizontal => {
                let view = cx.entity().clone();
                h_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_workspace_split_layout_metrics(
                                split_id,
                                axis,
                                &child_bounds,
                            );
                        });
                    })
                    .id(format!("ghostex-gpui-workspace-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .items_start()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_workspace_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
            WorkspaceSplitAxis::Vertical => {
                let view = cx.entity().clone();
                v_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_workspace_split_layout_metrics(
                                split_id,
                                axis,
                                &child_bounds,
                            );
                        });
                    })
                    .id(format!("ghostex-gpui-workspace-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_workspace_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
        }
    }

    pub(crate) fn render_workspace_split_handle(
        &self,
        split: &WorkspaceSplit,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        let hover_visible = self.workspace_split_hover_line_visible(split_id);
        // A pane showing React chat is a CEF page, which takes the mouse itself, so the grab strip moves
        // wholly onto the other side when only one side of the rail has such a pane.
        let grab_side = match (
            self.workspace_node_shows_cef_chat(&split.first),
            self.workspace_node_shows_cef_chat(&split.second),
        ) {
            (true, false) => ResizeRailGrabSide::Trailing,
            (false, true) => ResizeRailGrabSide::Leading,
            _ => ResizeRailGrabSide::Straddle,
        };
        let rail = div()
            .id(format!(
                "ghostex-gpui-workspace-split-handle-{}",
                split_id.0
            ))
            .relative()
            .flex_shrink_0()
            .bg(workspace_split_handle_color());
        let rail = match axis {
            WorkspaceSplitAxis::Horizontal => rail.h_full().w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)),
            WorkspaceSplitAxis::Vertical => rail.w_full().h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS)),
        };
        rail.child(resize_rail_deferred_strip(
            resize_rail_grab_strip(
                format!("ghostex-gpui-workspace-split-grab-strip-{}", split_id.0),
                axis,
                grab_side,
            )
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_workspace_split_hovering(split_id, *hovered, cx);
            }))
            .on_mouse_move(
                cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                    this.set_workspace_split_hovering(split_id, true, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_workspace_split_handle_mouse_down(
                        split_id, axis, event, window, cx,
                    );
                }),
            )
            .when(hover_visible, |this| {
                this.child(resize_rail_hover_line(
                    format!(
                        "ghostex-gpui-workspace-split-resize-hover-line-{}",
                        split_id.0
                    ),
                    axis,
                    grab_side,
                ))
            }),
        ))
        .into_any_element()
    }

    pub(crate) fn workspace_node_shows_cef_chat(&self, node: &WorkspaceNode) -> bool {
        match node {
            WorkspaceNode::Split(split) => {
                self.workspace_node_shows_cef_chat(&split.first)
                    || self.workspace_node_shows_cef_chat(&split.second)
            }
            WorkspaceNode::Leaf(leaf) => leaf.tab_group.active_session_id().is_some_and(|id| {
                self.agents_chat_mode_sessions.contains(&id) && !self.session_chat_use_gpui
            }),
        }
    }

    pub(crate) fn render_workspace_leaf(
        &self,
        leaf: &WorkspaceLeaf,
        rail_edges: RailFacingEdges,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pane_id = leaf.pane_id;
        let border_state = self.workspace_leaf_border_state(leaf, window, cx);
        let view = cx.entity().clone();

        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_workspace_leaf_layout_bounds(pane_id, &child_bounds);
                });
            })
            .id(format!("ghostex-gpui-workspace-pane-{}", pane_id.0))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            // Attention changes only the border color. Keeping its width
            // constant prevents status transitions from resizing the pane's
            // content box and nudging the tab bar, terminal, or action bar.
            // Sides that touch a resize rail have no border at all: the rail is
            // the one line there, and a focus or attention outline closes over
            // it.
            .map(|pane| {
                rail_aware_pane_border(
                    pane,
                    rail_edges,
                    workspace_pane_border_color_for_state(border_state),
                    workspace_pane_border_color(),
                )
            })
            .bg(workspace_terminal_placeholder_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    if this.acknowledge_agents_pane_attention_from_chrome_click(pane_id, cx) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                }),
            )
            .when(self.agents_workspace_tab_bar_visible(), |this| {
                this.child(self.render_workspace_tab_bar(leaf, cx))
            })
            .when_some(
                self.render_agents_terminal_search_bar(leaf, cx),
                |this, surface| this.child(surface),
            )
            .child(self.render_terminal_body_slot(leaf, cx))
            // The agent action bar is a normal sibling *below* the body, the
            // same way the search bar is one above it, so the terminal never
            // renders under it.
            .when_some(
                self.render_agents_pane_terminal_agent_action_bar(leaf, cx),
                |this, bar| this.child(bar),
            )
            .window_corner_pane()
            .into_any_element()
    }
}
