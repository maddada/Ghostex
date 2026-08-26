// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: Agents workspace top-level render entry and its pane split/leaf recursion.

use gpui::Animation;
use gpui::AnimationExt as _;
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
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_agents_workspace(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIWorkspaceLayout 2026-06-22-05:11:
        Agents mode renders the workspace as GPUI native layout chrome: tab groups and split nodes own normal non-overlapping regions, every leaf keeps a tab bar even when it is the only pane, and Ghostty content is represented by black placeholder surfaces until libghostty integration lands.

        CDXC:GPUIWorkspaceLayout 2026-06-22-14:40:
        The Agents workspace root must be a vertical flex container, not only a flex-sized child of the command-pane wrapper. The rendered split or leaf tree uses flex_1 sizing, so it needs this parent layout context to fill the available height above the command pane instead of leaving a black shell gap below the terminal pane.
        */
        v_flex()
            .id("ghostex-gpui-agents-workspace")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(workspace_background_color())
            .child(
                if let Some(pane_id) = self.agents_workspace.focus_mode_pane
                    && let Some(leaf) = self.agents_workspace.find_leaf(pane_id)
                {
                    self.render_workspace_leaf(leaf, window, cx)
                } else {
                    self.render_workspace_node(&self.agents_workspace.root, window, cx)
                },
            )
            .into_any_element()
    }

    pub(crate) fn render_workspace_node(
        &self,
        node: &WorkspaceNode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match node {
            WorkspaceNode::Split(split) => self.render_workspace_split(split, window, cx),
            WorkspaceNode::Leaf(leaf) => self.render_workspace_leaf(leaf, window, cx),
        }
    }

    pub(crate) fn render_workspace_split(
        &self,
        split: &WorkspaceSplit,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
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
            .child(self.render_workspace_node(&split.first, window, cx));
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
            .child(self.render_workspace_node(&split.second, window, cx));

        /*
        CDXC:GPUIWorkspaceLayout 2026-06-22-05:11:
        Split handles are explicit layout siblings between split children. This keeps future resize hit regions in the normal tree while the current visual separator remains a non-interactive child, avoiding transparent overlays or overlapping terminal/web surfaces.

        CDXC:GPUIWorkspaceResize 2026-06-22-06:45:
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
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        match split.axis {
            WorkspaceSplitAxis::Horizontal => div()
                .id(format!(
                    "ghostex-gpui-workspace-split-handle-{}",
                    split_id.0
                ))
                .relative()
                .flex()
                .flex_shrink_0()
                .h_full()
                .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .items_center()
                .justify_center()
                .cursor_ew_resize()
                .bg(workspace_split_handle_color())
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
                .child(
                    div()
                        .h_full()
                        .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .cursor_ew_resize()
                        .bg(workspace_split_separator_color()),
                )
                .when(hover_visible, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top_0()
                            .h_full()
                            .left(px(hover_line_offset))
                            .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                            .cursor_ew_resize()
                            .bg(sidebar_divider_hover_line_color())
                            .with_animation(
                                format!(
                                    "ghostex-gpui-workspace-split-resize-hover-line-{}",
                                    split_id.0
                                ),
                                Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                    .with_easing(gpui::ease_out_quint()),
                                |line, delta| line.opacity(delta),
                            ),
                    )
                })
                .into_any_element(),
            WorkspaceSplitAxis::Vertical => div()
                .id(format!(
                    "ghostex-gpui-workspace-split-handle-{}",
                    split_id.0
                ))
                .relative()
                .flex()
                .flex_shrink_0()
                .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .w_full()
                .items_center()
                .justify_center()
                .cursor_ns_resize()
                .bg(workspace_split_handle_color())
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
                .child(
                    div()
                        .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .w_full()
                        .cursor_ns_resize()
                        .bg(workspace_split_separator_color()),
                )
                .when(hover_visible, |this| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .w_full()
                            .top(px(hover_line_offset))
                            .h(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                            .cursor_ns_resize()
                            .bg(sidebar_divider_hover_line_color())
                            .with_animation(
                                format!(
                                    "ghostex-gpui-workspace-split-resize-hover-line-{}",
                                    split_id.0
                                ),
                                Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                    .with_easing(gpui::ease_out_quint()),
                                |line, delta| line.opacity(delta),
                            ),
                    )
                })
                .into_any_element(),
        }
    }

    pub(crate) fn render_workspace_leaf(
        &self,
        leaf: &WorkspaceLeaf,
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
            .border_1()
            .border_color(workspace_pane_border_color_for_state(border_state))
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
            .child(self.render_workspace_tab_bar(leaf, cx))
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
            .into_any_element()
    }
}
