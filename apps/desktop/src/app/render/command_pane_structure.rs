// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: command-pane workspace shell: the top-level main/agents-workspace switch, command-pane host layout, side/resize dividers, and the pane split/leaf recursion.

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
    pub(crate) fn render_main_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match self.active_mode {
            TitlebarMode::Agents => self.render_agents_workspace(window, cx),
            mode => self.render_project_editor_shell(mode, window, cx),
        }
    }

    pub(crate) fn render_workspace_with_command_pane(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUICommandPane 2026-06-22-05:42:
        Command terminals render as a bottom workspace surface beside Agents and project/editor modes without entering the normal workspace tab tree. Pinned mode reserves height and pushes the active workspace area up, floating mode keeps a collapsed bottom strip while drawing the expanded command surface above the workspace, collapsed mode keeps the compact command strip visible while sessions exist, and the command surface disappears when its final session closes.
        */
        let workspace_width =
            command_pane_workspace_width(window, self.sidebar_width, self.sidebar_collapsed);
        let layout_plan = command_pane_workspace_layout_plan(
            self.command_pane.mode,
            self.command_pane.has_sessions(),
            command_pane_content_height(window),
            self.command_pane.height_ratio,
            self.command_pane_side,
            workspace_width,
            self.command_pane.width_ratio,
        );

        match layout_plan {
            CommandPaneWorkspaceLayoutPlan::Hidden => v_flex()
                .id("ghostex-gpui-workspace-without-command-pane")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(self.render_main_workspace(window, cx))
                .into_any_element(),
            CommandPaneWorkspaceLayoutPlan::Pinned { panel_height } => v_flex()
                .id("ghostex-gpui-workspace-with-command-pinned")
                .relative()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(self.render_main_workspace(window, cx))
                .child(self.render_command_pane_resize_divider(None, cx))
                .child(self.render_command_pane_panel(
                    GpuiCommandPaneSide::Bottom,
                    panel_height,
                    false,
                    command_pane_panel_chrome_width(workspace_width, false),
                    cx,
                ))
                .into_any_element(),
            CommandPaneWorkspaceLayoutPlan::PinnedRight { panel_width } => h_flex()
                /*
                CDXC:GPUICommandPaneSide 2026-08-16:
                Right dock is strict normal layout: workspace, a real 5px
                divider sibling, and the pane column. gpui-component `h_flex`
                centers its children by default, so stretch them or the pane
                and workspace collapse to their content height.
                */
                .id("ghostex-gpui-workspace-with-command-pinned-right")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_stretch()
                .overflow_hidden()
                .child(self.render_main_workspace(window, cx))
                .child(self.render_command_pane_side_divider(cx))
                .child(self.render_command_pane_panel(
                    GpuiCommandPaneSide::Right,
                    panel_width,
                    false,
                    command_pane_panel_chrome_width(panel_width, false),
                    cx,
                ))
                .into_any_element(),
            CommandPaneWorkspaceLayoutPlan::Floating {
                panel_height,
                bottom_reservation,
            } => v_flex()
                .id("ghostex-gpui-workspace-with-command-floating")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden()
                        .child(self.render_main_workspace(window, cx))
                        .child(self.render_command_pane_panel(
                            GpuiCommandPaneSide::Bottom,
                            panel_height,
                            true,
                            command_pane_panel_chrome_width(workspace_width, true),
                            cx,
                        ))
                        .child(self.render_command_pane_resize_divider(
                            Some((
                                COMMAND_PANE_FLOATING_MARGIN + panel_height,
                                COMMAND_PANE_FLOATING_MARGIN,
                            )),
                            cx,
                        )),
                )
                .child(self.render_command_pane_bottom_reservation(
                    bottom_reservation,
                    workspace_width,
                    cx,
                ))
                .into_any_element(),
            CommandPaneWorkspaceLayoutPlan::Collapsed { bottom_reservation } => v_flex()
                .id("ghostex-gpui-workspace-with-command-collapsed")
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(self.render_main_workspace(window, cx))
                .child(self.render_command_pane_bottom_reservation(
                    bottom_reservation,
                    workspace_width,
                    cx,
                ))
                .into_any_element(),
        }
    }

    pub(crate) fn render_command_pane_panel(
        &self,
        side: GpuiCommandPaneSide,
        extent: f32,
        floating: bool,
        panel_chrome_width: f32,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUICommandPaneSide 2026-08-16:
        `extent` is the panel height for the bottom dock and the panel width
        for the right dock. Floating is always bottom-anchored. The right dock
        skips the top border because its divider sibling paints the separator.
        */
        let view = cx.entity().clone();
        let owner_content_width = command_pane_owner_content_width(panel_chrome_width);
        let docked_right = side == GpuiCommandPaneSide::Right && !floating;

        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_command_pane_layout_bounds(&child_bounds);
                });
            })
            .id(if floating {
                "ghostex-gpui-command-pane-floating"
            } else if docked_right {
                "ghostex-gpui-command-pane-pinned-right"
            } else {
                "ghostex-gpui-command-pane-pinned"
            })
            .when(!docked_right, |this| this.h(px(extent)))
            .when(docked_right, |this| {
                this.w(px(extent)).h_full().flex_shrink_0()
            })
            .when(!floating && !docked_right, |this| this.w_full())
            .when(!floating && !docked_right, |this| {
                this.min_h(px(COMMAND_PANE_TAB_BAR_HEIGHT))
            })
            .overflow_hidden()
            .when(!docked_right, |this| this.border_t_1())
            .border_color(command_pane_panel_separator_color())
            .bg(command_pane_chrome_color())
            .when(floating, |this| {
                this.absolute()
                    .left(px(COMMAND_PANE_FLOATING_MARGIN))
                    .right(px(COMMAND_PANE_FLOATING_MARGIN))
                    .bottom(px(COMMAND_PANE_FLOATING_MARGIN))
                    .shadow_md()
            })
            .child(
                // gpui `div()` lays out as display:block, which gives the
                // command group/split tree its content height (tab bar only)
                // and collapses the terminal body to 0px. The content host
                // must be a flex container so the single child node stretches
                // to the pane's remaining height like the Agents workspace
                // column does.
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .mr(px(COMMAND_PANE_OUTER_CONTENT_RIGHT_INSET))
                    .when_some(
                        self.command_pane
                            .focus_mode_group
                            .and_then(|group_id| self.command_pane.find_leaf(group_id))
                            .filter(|leaf| {
                                self.command_pane
                                    .focus_mode_eligible_group_count_without_focus()
                                    > 1
                                    && self
                                        .command_pane
                                        .group_is_focus_mode_eligible_without_focus(leaf.group_id)
                            }),
                        |this, focus_leaf| {
                            this.child(self.render_command_pane_leaf(
                                focus_leaf,
                                owner_content_width,
                                false,
                                cx,
                            ))
                        },
                    )
                    .when(
                        self.command_pane
                            .focus_mode_group
                            .and_then(|group_id| self.command_pane.find_leaf(group_id))
                            .filter(|leaf| {
                                self.command_pane
                                    .focus_mode_eligible_group_count_without_focus()
                                    > 1
                                    && self
                                        .command_pane
                                        .group_is_focus_mode_eligible_without_focus(leaf.group_id)
                            })
                            .is_none(),
                        |this| {
                            this.child(self.render_command_pane_node(
                                &self.command_pane.root,
                                owner_content_width,
                                false,
                                cx,
                            ))
                        },
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_command_pane_side_divider(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUICommandPaneSide 2026-08-16:
        The right-docked pane's grab target is a real 5px divider sibling, the
        same strict-layout pattern as the sidebar divider and command split
        handles, rather than porting the bottom rail's approved 12px overlap to
        a second edge. Its right edge paints the panel separator so the pane
        keeps a visible boundary; hover feedback reuses the shared PanelRail
        hover state and white 3px line.
        */
        div()
            .id("ghostex-gpui-command-pane-side-divider")
            .relative()
            .flex_shrink_0()
            .h_full()
            .w(px(COMMAND_PANE_SPLIT_HANDLE_THICKNESS))
            .cursor_ew_resize()
            .bg(command_pane_split_handle_color())
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.set_command_resize_hovering(
                    CommandPaneResizeHoverTarget::PanelRail,
                    *hovered,
                    cx,
                );
            }))
            .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
                this.set_command_resize_hovering(CommandPaneResizeHoverTarget::PanelRail, true, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.handle_command_pane_side_divider_mouse_down(event, window, cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .cursor_ew_resize()
                    .bg(command_pane_panel_separator_color()),
            )
            .when(
                self.command_resize_hover_visible == Some(CommandPaneResizeHoverTarget::PanelRail),
                |this| {
                    this.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px((COMMAND_PANE_SPLIT_HANDLE_THICKNESS
                                - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH)
                                / 2.0))
                            .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                            .cursor_ew_resize()
                            .bg(sidebar_divider_hover_line_color())
                            .with_animation(
                                "ghostex-gpui-command-pane-side-divider-hover-line",
                                Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                    .with_easing(gpui::ease_out_quint()),
                                |line, delta| line.opacity(delta),
                            ),
                    )
                },
            )
            .into_any_element()
    }

    pub(crate) fn render_command_pane_resize_divider(
        &self,
        floating_offsets: Option<(f32, f32)>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUICommandPaneResize 2026-08-18:
        The command-pane boundary uses the same real five-pixel divider as the
        project-editor companion boundary. In pinned mode it is a reserved
        normal-layout sibling between the workspace and command pane; floating
        mode positions the same divider at the floating panel edge. The whole
        visible gap owns resize/reset input, with the same centered three-pixel
        delayed hover line and no invisible overlap over adjacent content.
        */
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        div()
            .id("ghostex-gpui-command-pane-resize-divider")
            .relative()
            .flex()
            .flex_shrink_0()
            .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .items_center()
            .justify_center()
            .cursor_ns_resize()
            .bg(project_editor_companion_divider_background_color())
            .when(floating_offsets.is_none(), |this| this.w_full())
            .when_some(
                floating_offsets,
                |this, (bottom_offset, horizontal_inset)| {
                    this.absolute()
                        .left(px(horizontal_inset))
                        .right(px(horizontal_inset))
                        .bottom(px(bottom_offset))
                },
            )
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.set_command_resize_hovering(
                    CommandPaneResizeHoverTarget::PanelRail,
                    *hovered,
                    cx,
                );
            }))
            .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
                cpraildbg(&format!("rail_hover_move y={:?}", _event.position.y));
                this.set_command_resize_hovering(CommandPaneResizeHoverTarget::PanelRail, true, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.handle_command_pane_resize_divider_mouse_down(event, window, cx);
                }),
            )
            .child(
                div()
                    .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .w_full()
                    .cursor_ns_resize()
                    .bg(project_editor_companion_divider_line_color()),
            )
            .when(
                self.command_resize_hover_visible == Some(CommandPaneResizeHoverTarget::PanelRail),
                |this| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(hover_line_offset))
                            .h(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                            .w_full()
                            .cursor_ns_resize()
                            .bg(sidebar_divider_hover_line_color())
                            .with_animation(
                                "ghostex-gpui-command-pane-resize-hover-line",
                                Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                    .with_easing(gpui::ease_out_quint()),
                                |line, delta| line.opacity(delta),
                            ),
                    )
                },
            )
            .into_any_element()
    }

    pub(crate) fn render_command_pane_node(
        &self,
        node: &CommandPaneNode,
        estimated_chrome_width: f32,
        has_pane_to_right: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match node {
            CommandPaneNode::Split(split) => {
                self.render_command_pane_split(split, estimated_chrome_width, has_pane_to_right, cx)
            }
            CommandPaneNode::Leaf(leaf) => {
                self.render_command_pane_leaf(leaf, estimated_chrome_width, has_pane_to_right, cx)
            }
        }
    }

    pub(crate) fn render_command_pane_split(
        &self,
        split: &CommandPaneSplit,
        estimated_chrome_width: f32,
        has_pane_to_right: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        let ratio = workspace_split_ratio(split.ratio);
        let split_child_width = match axis {
            WorkspaceSplitAxis::Horizontal => {
                (estimated_chrome_width - COMMAND_PANE_SPLIT_HANDLE_THICKNESS).max(0.0)
            }
            WorkspaceSplitAxis::Vertical => estimated_chrome_width,
        };
        let first_estimated_width = match axis {
            WorkspaceSplitAxis::Horizontal => split_child_width * ratio,
            WorkspaceSplitAxis::Vertical => split_child_width,
        };
        let second_estimated_width = match axis {
            WorkspaceSplitAxis::Horizontal => split_child_width * (1.0 - ratio),
            WorkspaceSplitAxis::Vertical => split_child_width,
        };
        let first = div()
            .id(format!("ghostex-gpui-command-split-{}-first", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_command_pane_node(
                &split.first,
                first_estimated_width,
                has_pane_to_right || axis == WorkspaceSplitAxis::Horizontal,
                cx,
            ));
        let second = div()
            .id(format!("ghostex-gpui-command-split-{}-second", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(1.0 - ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_command_pane_node(
                &split.second,
                second_estimated_width,
                has_pane_to_right,
                cx,
            ));

        /*
        CDXC:GPUIFocusedSplits 2026-06-25-16:05:
        Command-pane split layout is axis-aware so persisted command split geometry renders from its stored axis. Focused command hotkeys currently create horizontal splits only, matching native; the split handle remains a real non-overlapping layout sibling when restored layouts carry either orientation.
        */
        match split.axis {
            WorkspaceSplitAxis::Horizontal => {
                let view = cx.entity().clone();
                h_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_command_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-command-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_command_pane_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
            WorkspaceSplitAxis::Vertical => {
                let view = cx.entity().clone();
                v_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_command_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-command-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_command_pane_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
        }
    }

    pub(crate) fn render_command_pane_split_handle(
        &self,
        split: &CommandPaneSplit,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        match split.axis {
            WorkspaceSplitAxis::Horizontal => div()
                .id(format!("ghostex-gpui-command-split-handle-{}", split_id.0))
                .relative()
                .flex()
                .flex_shrink_0()
                .h_full()
                .w(px(COMMAND_PANE_SPLIT_HANDLE_THICKNESS))
                .items_center()
                .justify_center()
                .cursor_ew_resize()
                .bg(command_pane_split_handle_color())
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    this.set_command_resize_hovering(
                        CommandPaneResizeHoverTarget::Split(split_id),
                        *hovered,
                        cx,
                    );
                }))
                .on_mouse_move(
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        this.set_command_resize_hovering(
                            CommandPaneResizeHoverTarget::Split(split_id),
                            true,
                            cx,
                        );
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_command_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h_full()
                        .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .cursor_ew_resize()
                        .bg(command_pane_split_separator_color()),
                )
                .when(
                    self.command_resize_hover_visible
                        == Some(CommandPaneResizeHoverTarget::Split(split_id)),
                    |this| {
                        this.child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px((COMMAND_PANE_SPLIT_HANDLE_THICKNESS
                                    - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH)
                                    / 2.0))
                                .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                                .cursor_ew_resize()
                                .bg(sidebar_divider_hover_line_color())
                                .with_animation(
                                    format!(
                                        "ghostex-gpui-command-split-resize-hover-line-{}",
                                        split_id.0
                                    ),
                                    Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                        .with_easing(gpui::ease_out_quint()),
                                    |line, delta| line.opacity(delta),
                                ),
                        )
                    },
                )
                .into_any_element(),
            WorkspaceSplitAxis::Vertical => div()
                .id(format!("ghostex-gpui-command-split-handle-{}", split_id.0))
                .relative()
                .flex()
                .flex_shrink_0()
                .h(px(COMMAND_PANE_SPLIT_HANDLE_THICKNESS))
                .w_full()
                .items_center()
                .justify_center()
                .cursor_ns_resize()
                .bg(command_pane_split_handle_color())
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    this.set_command_resize_hovering(
                        CommandPaneResizeHoverTarget::Split(split_id),
                        *hovered,
                        cx,
                    );
                }))
                .on_mouse_move(
                    cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                        this.set_command_resize_hovering(
                            CommandPaneResizeHoverTarget::Split(split_id),
                            true,
                            cx,
                        );
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_command_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .w_full()
                        .cursor_ns_resize()
                        .bg(command_pane_split_separator_color()),
                )
                .when(
                    self.command_resize_hover_visible
                        == Some(CommandPaneResizeHoverTarget::Split(split_id)),
                    |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .top(px((COMMAND_PANE_SPLIT_HANDLE_THICKNESS
                                    - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH)
                                    / 2.0))
                                .h(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                                .cursor_ns_resize()
                                .bg(sidebar_divider_hover_line_color())
                                .with_animation(
                                    format!(
                                        "ghostex-gpui-command-split-resize-hover-line-{}",
                                        split_id.0
                                    ),
                                    Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                        .with_easing(gpui::ease_out_quint()),
                                    |line, delta| line.opacity(delta),
                                ),
                        )
                    },
                )
                .into_any_element(),
        }
    }
}
