// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: workspace / command-pane / browser render_* element builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Animation;
use gpui::AnimationExt as _;
use gpui::AnyElement;
use gpui::AppContext as _;
use gpui::Entity;
use gpui::FontWeight;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MousePressureEvent;
use gpui::MouseUpEvent;
use gpui::ObjectFit;
use gpui::ParentElement as _;
use gpui::ScrollHandle;
use gpui::ScrollWheelEvent;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::StyledImage as _;
use gpui::Window;
use gpui::WindowControlArea;
use gpui::canvas;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui::rgb;
use gpui::svg;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_titlebar(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:GPUITitlebar 2026-06-14-16:47:
        The GPUI titlebar mirrors the macOS app: native traffic lights, passive project identity, full-width mode tabs for Agents/Source/Browser/Kanban/Automate/Docs, a compact mode dropdown below 1050px, and right-side icon buttons.

        CDXC:GPUTitlebarAvailability 2026-07-04-01:00:
        Quick/projectless GPUI contexts keep Agents and Source selectable, keep Browser, Kanban, Automate, and Docs visible but disabled, and use the same availability helper for tabs, the compact dropdown, hotkeys, restore, and persistence.

        CDXC:GPUITitlebarParity 2026-06-22-19:39:
        The GPUI titlebar must match the current macOS titlebar chrome: the sidebar toggle is a flat Tabler layout-sidebar glyph instead of the older blue circular chevron, and the right controls are the same project/window actions as macOS: Tips, Resources, Git, Actions, and Open In. Settings and Keep Awake live in sidebar shortcut chrome, not this titlebar strip.
        */
        let show_mode_switcher = !self.titlebar_mode_switcher_items().is_empty();
        let use_compact_mode_dropdown = show_mode_switcher
            && window.bounds().size.width.as_f32() < TITLEBAR_COMPACT_MODE_WIDTH_THRESHOLD;
        div()
            .id("ghostex-gpui-titlebar")
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .bg(titlebar_gradient_fill())
            .text_color(titlebar_text_color())
            .font_family("Inter Variable")
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .child(
                h_flex()
                    .h_full()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .when(show_mode_switcher && !use_compact_mode_dropdown, |this| {
                        this.child(self.render_mode_switcher(cx))
                    }),
            )
            .child(self.render_project_slot(use_compact_mode_dropdown, cx))
            .child(self.render_right_titlebar_controls(window, cx))
    }

    pub(crate) fn render_project_slot(
        &self,
        show_compact_mode_dropdown: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let project_icon = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.project_icon_data_url.as_deref())
            .and_then(gpui_project_icon_image_from_data_url);
        h_flex()
            .absolute()
            .left(px(TITLEBAR_PROJECT_LEFT))
            .top(px(1.0))
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .max_w(px(620.0))
            .min_w_0()
            .items_center()
            .window_control_area(WindowControlArea::Drag)
            .child(self.render_sidebar_collapse_button(cx))
            .when(self.update_available || self.update_downloading, |this| {
                this.child(self.render_titlebar_update_button(cx))
            })
            /*
            CDXC:NavigationHistory 2026-08-19:
            Back/Forward sit LEFT of the project name, next to the sidebar
            toggle. Placing them after the name made them slide horizontally
            every time the active project's title changed length, which is
            exactly the kind of moving target a frequently clicked control must
            not be.
            */
            .child(self.render_titlebar_navigation_history_buttons(cx))
            .child(
                h_flex()
                    .h(px(TITLEBAR_CONTROL_HEIGHT))
                    .max_w(px(210.0))
                    .min_w_0()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .px(px(3.0))
                    .mt(px(2.0))
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(TITLEBAR_CONTROL_HEIGHT))
                    .text_color(titlebar_project_text_color())
                    .when_some(project_icon, |this, image| {
                        this.child(
                            img(image)
                                .size(px(16.0))
                                .mr(px(6.0))
                                .flex_shrink_0()
                                .rounded(px(4.0))
                                .object_fit(ObjectFit::Fill),
                        )
                    })
                    .child(self.project_name.clone()),
            )
            .when(show_compact_mode_dropdown, |this| {
                this.child(self.render_compact_mode_dropdown(cx))
            })
    }

    pub(crate) fn render_sidebar_collapse_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        /*
        CDXC:GPUITitlebarParity 2026-06-22-19:39:
        The visible sidebar toggle should match the macOS React titlebar's current flat layout-sidebar icon. Keep its 29px GPUI hit target 3px away from the native traffic lights. Do not render the old blue circular chevron visual.

        CDXC:GPUISidebarCollapse 2026-06-26-10:04:
        The GPUI titlebar sidebar button toggles the same in-shell collapsed chrome state as Cmd+B and the shared command-palette action. Collapse hides the sidebar and divider siblings without writing sidebarWidth, so the user's expanded width is restored on the next toggle.

        Windows and Linux do not have traffic lights to clear. Their collapse
        control uses the same full-height 42px segmented frame as Open In,
        mirrored with a trailing divider, and remains inside the 9px titlebar
        inset instead of extending past the window edge.
        */
        let icon = match self.sidebar_side {
            GpuiSidebarSide::Left => TITLEBAR_ICON_LAYOUT_SIDEBAR,
            GpuiSidebarSide::Right => TITLEBAR_ICON_LAYOUT_SIDEBAR_RIGHT,
        };
        let button = div()
            .id("ghostex-gpui-sidebar-collapse")
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_gpui_sidebar_collapsed(cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                titlebar_tooltip("Collapse Sidebar", window, cx)
            });
        #[cfg(target_os = "macos")]
        let button = button
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(29.0))
            .ml(px(-13.0))
            .flex_shrink_0()
            .rounded(px(5.0))
            .child(
                div()
                    .flex()
                    .ml(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET))
                    .mt(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_TOP_OFFSET))
                    .items_center()
                    .justify_center()
                    .child(titlebar_svg_icon(
                        icon,
                        TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
                        titlebar_active_text_color(),
                    )),
            );
        #[cfg(not(target_os = "macos"))]
        let button = button
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .border_r_1()
            .border_color(titlebar_button_border_color())
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
                titlebar_icon_color(),
            ));
        button
    }

    pub(crate) fn render_mode_switcher(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let items = self.titlebar_mode_switcher_items();
        let mode_count = items.len();

        let mut switcher = h_flex()
            .id("ghostex-gpui-titlebar-mode-switcher")
            .relative()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center();
        for (index, item) in items.into_iter().enumerate() {
            switcher = switcher.child(self.render_mode_tab(
                item.mode,
                item.mode.display_label(),
                index + 1 == mode_count,
                item.is_available,
                item.disabled_reason,
                cx,
            ));
        }
        switcher
    }

    pub(crate) fn render_compact_mode_dropdown(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let label = self.active_mode.display_label();
        h_flex()
            .id("ghostex-gpui-titlebar-compact-mode-dropdown")
            .h(px(25.0))
            .min_w(px(108.0))
            .ml(px(7.0))
            .mt(px(2.0))
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(titlebar_button_border_color())
            .px(px(9.0))
            .text_size(px(12.5))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(25.0))
            .text_color(titlebar_active_text_color())
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_mode_menu(event.position, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .child(label)
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_CHEVRON_DOWN,
                12.0,
                titlebar_icon_color(),
            ))
    }

    pub(crate) fn render_mode_tab(
        &self,
        mode: TitlebarMode,
        label: &'static str,
        is_last: bool,
        is_available: bool,
        disabled_reason: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let is_active = is_available && self.active_mode == mode;
        /*
        CDXC:GPUTitlebarAvailability 2026-07-04-01:00:
        Disabled Quick/projectless tabs remain normal titlebar segments with a hover reason and no separate hit target. Browser, Kanban, Automate, and Docs share the native disabled reason while click handling still calls the central availability guard before changing active workspace mode.
        */
        div()
            .id(format!("ghostex-gpui-titlebar-mode-{label}"))
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .min_w(px(70.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .when(is_last, |this| this.border_r_1())
            .border_color(titlebar_button_border_color())
            .px(px(14.0))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .text_color(if !is_available {
                titlebar_disabled_text_color()
            } else if is_active {
                titlebar_active_text_color()
            } else {
                titlebar_inactive_text_color()
            })
            .cursor_default()
            .when(is_active, |this| this.bg(titlebar_active_segment_color()))
            .when(!is_available, |this| {
                this.bg(titlebar_disabled_segment_color())
            })
            .hover(move |this| {
                if !is_available {
                    return this;
                }
                let this = this.text_color(titlebar_active_text_color());
                if is_active {
                    this.bg(titlebar_active_segment_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &gpui::MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if is_available && this.set_active_mode(mode, window, cx) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .when_some(disabled_reason, |this, reason| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    move |window, cx| titlebar_tooltip(reason, window, cx),
                )
            })
            .child(label)
    }

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

    pub(crate) fn render_command_pane_side_divider(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
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

    pub(crate) fn render_command_pane_leaf(
        &self,
        leaf: &CommandPaneLeaf,
        estimated_chrome_width: f32,
        has_pane_to_right: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let group_id = leaf.group_id;
        let border_color = command_pane_group_border_color(
            self.command_pane.mode,
            self.shell_focus,
            self.command_pane.focused_group,
            leaf.group_id,
        );
        let border_width = command_pane_group_border_width(
            self.shell_focus,
            self.command_pane.focused_group,
            leaf.group_id,
        );
        let view = cx.entity().clone();

        let group = v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_command_group_layout_bounds(group_id, &child_bounds);
                });
            })
            .id(format!("ghostex-gpui-command-pane-group-{}", group_id.0))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden();

        /*
        Keep the total edge inset constant at 2px across focus changes: the 1px
        focused border gains 1px padding so showing first-responder chrome never
        shifts or resizes the command group content.
        */
        let group = match border_width {
            CommandPaneGroupBorderWidth::Focused => group.border_1().p(px(1.0)),
            CommandPaneGroupBorderWidth::Inactive => group.border_2(),
        };

        let group = group
            .border_color(border_color)
            .bg(command_terminal_placeholder_color())
            .child(self.render_command_pane_titlebar(leaf, estimated_chrome_width, cx))
            .when_some(
                self.render_command_terminal_search_bar(leaf, cx),
                |this, surface| this.child(surface),
            )
            .child(self.render_command_terminal_placeholder(leaf, cx));

        /*
        Each command leaf owns its persistent left edge. Leaves with another
        pane geometrically to their right also own a right edge, without
        changing or overlapping the real split handles.
        */
        div()
            .flex()
            .flex_row()
            .items_stretch()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .border_l_1()
            .when(has_pane_to_right, |this| this.border_r_1())
            .border_color(command_pane_side_edge_color())
            .child(group)
            .into_any_element()
    }

    pub(crate) fn render_command_pane_titlebar(
        &self,
        leaf: &CommandPaneLeaf,
        estimated_chrome_width: f32,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let group_id = leaf.group_id;
        let chrome_width = self
            .command_group_layout_bounds
            .get(&group_id)
            .map(|bounds| bounds.size.width.as_f32())
            .unwrap_or(estimated_chrome_width);
        let show_tab_add_button =
            command_pane_inline_tab_add_visible_for_chrome_width(chrome_width, true);
        let scroll_handle = self.command_tab_scroll_handle(group_id);
        let wheel_scroll_handle = scroll_handle.clone();
        let tab_count = leaf.tab_group.tabs.len();
        let sticky_active_tab = leaf
            .tab_group
            .active_session_index()
            .and_then(|active_index| {
                command_pane_sticky_active_tab_edge_for_scroll_handle(&scroll_handle, active_index)
                    .map(|edge| (edge, active_index))
            });
        let sticky_trailing_inset =
            command_pane_sticky_active_tab_trailing_inset(true, show_tab_add_button);

        h_flex()
            .id(format!("ghostex-gpui-command-pane-titlebar-{}", group_id.0))
            .relative()
            .flex_shrink_0()
            .h(px(COMMAND_PANE_TAB_BAR_HEIGHT))
            .w_full()
            .items_center()
            .overflow_hidden()
            .border_b_1()
            .border_color(command_pane_titlebar_separator_color())
            .bg(command_pane_chrome_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                    /*
                    CDXC:GPUICommandPaneFocus 2026-06-26-00:00:
                    Expanded command titlebar chrome clicks should match native `focusTerminal` by focusing the clicked command group and revealing that same group's active command tab in both expanded and collapsed strips. Resolve the activated session from the clicked group instead of using first-group fallback so Attention acknowledgement stays scoped to the clicked command session.
                    */
                    if !this.command_pane.focus_group(group_id) {
                        return;
                    }
                    let active_session_id = this
                        .command_pane
                        .find_leaf(group_id)
                        .and_then(|leaf| leaf.tab_group.active_session_id());
                    let attention_acknowledged = active_session_id.is_some_and(|session_id| {
                        this.command_pane
                            .acknowledge_attention_for_session_activation(session_id)
                    });
                    this.focus_command_pane();
                    if let Some(session_id) = active_session_id {
                        this.request_command_terminal_text_focus_handoff(
                            CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            },
                        );
                    }
                    this.scroll_command_group_active_tab(group_id);
                    if attention_acknowledged {
                        this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                    }
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .id(format!("ghostex-gpui-command-pane-tabs-{}", group_id.0))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .overflow_hidden()
                    .track_scroll(&scroll_handle)
                    .on_scroll_wheel(cx.listener(
                        move |_this, event: &ScrollWheelEvent, window, cx| {
                            if command_pane_handle_tab_strip_scroll_wheel(
                                &wheel_scroll_handle,
                                event.delta,
                                window.line_height(),
                            ) {
                                window.prevent_default();
                                cx.stop_propagation();
                                cx.notify();
                            }
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.handle_command_pane_empty_titlebar_mouse_down(
                                Some(group_id),
                                event,
                                window,
                                cx,
                            );
                        }),
                    )
                    .children(
                        leaf.tab_group
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(tab_index, tab)| {
                                self.render_command_pane_tab(
                                    group_id,
                                    tab.session_id,
                                    Some(tab_index),
                                    tab_index + 1 < tab_count,
                                    false,
                                    cx,
                                )
                            }),
                    )
                    .when(
                        command_pane_new_command_control_placement()
                            == CommandPaneNewCommandControlPlacement::InlineTabRun,
                        |this| {
                            this.when(show_tab_add_button, |this| {
                                this.child(self.render_command_pane_tab_add_button(
                                    Some(group_id),
                                    false,
                                    cx,
                                ))
                            })
                        },
                    )
                    .child(self.render_command_tab_strip_end_drop_target(
                        group_id,
                        leaf.tab_group.tabs.len(),
                        cx,
                    )),
            )
            .child(self.render_command_pane_controls(Some(group_id), true, cx))
            .when_some(sticky_active_tab, |this, (edge, active_index)| {
                this.child(self.render_command_pane_sticky_active_tab_button(
                    format!("group-{}-{}", group_id.0, edge.element_slug()),
                    edge,
                    sticky_trailing_inset,
                    group_id,
                    scroll_handle.clone(),
                    active_index,
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(crate) fn render_command_pane_bottom_reservation(
        &self,
        bottom_reservation: CommandPaneWorkspaceBottomReservation,
        workspace_width: f32,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match bottom_reservation.chrome {
            CommandPaneBottomReservationChrome::PlainChrome => {
                self.render_command_pane_floating_reserved_bottom_bar(bottom_reservation.height)
            }
            CommandPaneBottomReservationChrome::CollapsedStrip => {
                self.render_command_pane_strip(workspace_width, bottom_reservation.height, cx)
            }
        }
    }

    pub(crate) fn render_command_pane_floating_reserved_bottom_bar(&self, height: f32) -> AnyElement {
        /*
        CDXC:GPUICommandPaneFloating 2026-06-25-18:19:
        Expanded floating command panels need the native reserved bottom footprint as plain command-panel chrome. Do not render tabs, plus, Expand, Pin/Unpin, or Minimize controls in this bottom reservation; those controls live in the floating panel itself.
        */
        div()
            .id("ghostex-gpui-command-pane-floating-reserved-bottom-bar")
            .flex_shrink_0()
            .h(px(height))
            .w_full()
            .bg(command_pane_strip_color())
            .into_any_element()
    }

    pub(crate) fn render_command_pane_strip(
        &self,
        workspace_width: f32,
        height: f32,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let flat_tabs = self.command_pane.flat_tab_ids();
        let flat_tab_count = flat_tabs.len();
        let strip_chrome_width = (workspace_width
            - COMMAND_PANE_COLLAPSED_STRIP_LEFT_MARGIN
            - COMMAND_PANE_COLLAPSED_STRIP_RIGHT_MARGIN)
            .max(0.0);
        let show_tab_add_button =
            command_pane_inline_tab_add_visible_for_chrome_width(strip_chrome_width, false);
        let scroll_handle = self.command_collapsed_tab_scroll_handle.clone();
        let wheel_scroll_handle = scroll_handle.clone();
        let active_flat_tab = self.command_pane.active_group_and_session_id().and_then(
            |(active_group_id, active_session_id)| {
                flat_tabs
                    .iter()
                    .position(|(group_id, session_id)| {
                        *group_id == active_group_id && *session_id == active_session_id
                    })
                    .map(|active_index| (active_group_id, active_index))
            },
        );
        let sticky_active_tab = active_flat_tab.and_then(|(active_group_id, active_index)| {
            command_pane_sticky_active_tab_edge_for_scroll_handle(&scroll_handle, active_index)
                .map(|edge| (edge, active_group_id, active_index))
        });
        let sticky_trailing_inset =
            command_pane_sticky_active_tab_trailing_inset(false, show_tab_add_button);

        /*
        CDXC:GPUICommandPaneControls 2026-06-25-12:32:
        Native minimized command panels are command tab chrome only: the panel frame starts after a 4px left margin, leaves an 8px black right margin, and does not prepend a separate "Command" label block before the tabs.
        */
        h_flex()
            .id("ghostex-gpui-command-pane-collapsed-strip-row")
            .flex_shrink_0()
            .h(px(height))
            .w_full()
            .items_center()
            .overflow_hidden()
            .bg(command_pane_strip_color())
            .child(
                h_flex()
                    .id("ghostex-gpui-command-pane-collapsed-strip")
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .overflow_hidden()
                    .border_t_1()
                    .border_color(command_pane_panel_separator_color())
                    .bg(command_pane_strip_color())
                    .ml(px(COMMAND_PANE_COLLAPSED_STRIP_LEFT_MARGIN))
                    .mr(px(COMMAND_PANE_COLLAPSED_STRIP_RIGHT_MARGIN))
                    .child(
                        h_flex()
                            .id("ghostex-gpui-command-pane-strip-tabs")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .overflow_hidden()
                            .track_scroll(&scroll_handle)
                            .on_scroll_wheel(cx.listener(
                                move |_this, event: &ScrollWheelEvent, window, cx| {
                                    if command_pane_handle_tab_strip_scroll_wheel(
                                        &wheel_scroll_handle,
                                        event.delta,
                                        window.line_height(),
                                    ) {
                                        window.prevent_default();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    this.handle_command_pane_empty_titlebar_mouse_down(
                                        None, event, window, cx,
                                    );
                                }),
                            )
                            .children(flat_tabs.into_iter().enumerate().map(
                                |(tab_index, (group_id, session_id))| {
                                    self.render_command_pane_tab(
                                        group_id,
                                        session_id,
                                        None,
                                        tab_index + 1 < flat_tab_count,
                                        true,
                                        cx,
                                    )
                                },
                            ))
                            .when(
                                command_pane_new_command_control_placement()
                                    == CommandPaneNewCommandControlPlacement::InlineTabRun,
                                |this| {
                                    this.when(show_tab_add_button, |this| {
                                        this.child(
                                            self.render_command_pane_tab_add_button(None, true, cx),
                                        )
                                    })
                                },
                            ),
                    )
                    .child(self.render_command_pane_controls(None, false, cx))
                    .when_some(
                        sticky_active_tab,
                        |this, (edge, active_group_id, active_index)| {
                            this.child(self.render_command_pane_sticky_active_tab_button(
                                format!("collapsed-{}", edge.element_slug()),
                                edge,
                                sticky_trailing_inset,
                                active_group_id,
                                scroll_handle.clone(),
                                active_index,
                                cx,
                            ))
                        },
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_command_pane_tab(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        tab_index: Option<usize>,
        has_following_command_tab: bool,
        expand_on_click: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (title, tab_status, is_sleeping) = self
            .command_pane
            .session(session_id)
            .map(|session| {
                (
                    session.title.clone(),
                    session.tab_status(),
                    session.is_sleeping,
                )
            })
            .unwrap_or_else(|| {
                (
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                    CommandTerminalTabStatus::Idle,
                    false,
                )
            });
        /*
        CDXC:GPUIWindowsCommandTitles 2026-08-04:
        Command-pane titles are owned by the command model: plain terminals use
        Command Terminal, Actions use their configured title, and Rename edits
        that same field. A live shell OSC title describes the process/window;
        on Windows PowerShell commonly publishes C:\WINDOWS\system32, which
        must not replace the command session's actual tab title.
        */
        let chrome_signature = self
            .command_pane
            .find_leaf(group_id)
            .map(|leaf| command_tab_chrome_signature(&leaf.tab_group, session_id, tab_status))
            .unwrap_or(CommandTabChromeSignature {
                tab_status,
                active_in_tab_group: false,
            });
        let is_active = chrome_signature.active_in_tab_group;
        let tab_status = chrome_signature.tab_status;
        let dragged_tab = DraggedCommandTab {
            source_group_id: group_id,
            session_id,
            title: title.clone(),
            tab_status,
        };
        let view = cx.entity().clone();
        let show_insertion_marker = tab_index.is_some_and(|tab_index| {
            self.command_drop_feedback
                == Some(CommandPaneDropFeedback {
                    group_id,
                    target: CommandPaneDropTarget::TabStrip(tab_index),
                })
        });
        let tab_hover_key = CommandPaneHoverTab {
            group_id,
            session_id,
        };
        let is_tab_hovered = self.hovered_command_tab == Some(tab_hover_key);
        let show_status_indicator =
            command_terminal_tab_status_indicator_visible(tab_status, is_tab_hovered);
        let title_trailing_reserved_width =
            command_terminal_tab_status_title_trailing_reserved_width(tab_status);
        let delayed_send_remaining_label =
            self.gpui_command_delayed_send_remaining_label_for_session(session_id);
        let tab_tooltip = command_pane_tab_tooltip(&title, delayed_send_remaining_label.as_deref());

        div()
            .id(format!(
                "ghostex-gpui-command-pane-tab-{}-{}",
                group_id.0, session_id.0
            ))
            .relative()
            .flex()
            .flex_grow(1.0)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .h(px(COMMAND_PANE_TAB_BAR_HEIGHT))
            .min_w(px(COMMAND_PANE_TAB_MIN_WIDTH))
            .max_w(px(COMMAND_PANE_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .pl(px(8.0))
            .pr(px(0.0))
            .text_size(px(COMMAND_PANE_TAB_TITLE_FONT_SIZE))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(command_pane_tab_title_text_color(is_active, is_sleeping))
            .cursor_default()
            .bg(command_pane_tab_background_color(is_active, is_sleeping))
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                Tooltip::new(tab_tooltip.clone()).build(window, cx)
            })
            .when(show_insertion_marker, |this| {
                this.child(self.render_command_tab_insertion_marker(
                    group_id,
                    tab_index.unwrap_or(0),
                    "before",
                ))
            })
            .hover(move |this| {
                this.bg(command_pane_tab_hover_background_color(
                    is_active,
                    is_sleeping,
                ))
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_command_pane_tab_hovered(tab_hover_key, *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.begin_pending_command_tab_click(group_id, session_id, expand_on_click);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_command_pane_tab_left_mouse_up(
                        group_id,
                        session_id,
                        expand_on_click,
                        event.click_count,
                        window,
                        cx,
                    );
                    if command_pane_tab_left_mouse_up_finishes_drag(this.command_tab_drag_active) {
                        this.finish_command_tab_drag(cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, _window, _cx| {
                    this.cancel_pending_command_tab_click_for_tab(
                        group_id,
                        session_id,
                        expand_on_click,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.cancel_pending_command_tab_click();
                    this.show_command_tab_context_menu(
                        group_id,
                        session_id,
                        expand_on_click,
                        event.position,
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.cancel_pending_command_tab_click();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    /*
                    CDXC:GPUICommandTabClose 2026-06-25-14:01:
                    Command tabs mirror native AppKit tab buttons: button-2 is owned by the clicked tab and closes it on mouse-up through the normal command close path, without selecting the tab or creating separate session teardown behavior.
                    */
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_command_pane_tab(group_id, session_id, cx);
                }),
            )
            .when_some(tab_index, |this, tab_index| {
                this.on_drag(dragged_tab, move |dragged, _offset, _window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_command_tab_drag(cx);
                    });
                    cx.new(|_| CommandTabDragPreview {
                        title: dragged.title.clone(),
                        tab_status: dragged.tab_status,
                    })
                })
                .on_drag_move::<DraggedCommandTab>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                        this.update_command_tab_drag_feedback(event, group_id, tab_index, cx);
                    },
                ))
                .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                    move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                        this.update_workspace_tab_over_command_tab_drag_feedback(
                            event, group_id, tab_index, cx,
                        );
                    },
                ))
                .can_drop(move |value, _window, _cx| {
                    value
                        .downcast_ref::<DraggedCommandTab>()
                        .is_some_and(|dragged| dragged.source_group_id == group_id)
                        || value.is::<DraggedWorkspaceTab>()
                })
                .on_drop(
                    cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                        this.handle_command_tab_strip_drop(
                            group_id, tab_index, dragged, window, cx,
                        );
                    }),
                )
                .on_drop(cx.listener(
                    move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                        this.handle_workspace_tab_command_tab_strip_drop(
                            group_id, tab_index, dragged, window, cx,
                        );
                    },
                ))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .pr(px(title_trailing_reserved_width))
                    .child(title),
            )
            .when(show_status_indicator, |this| {
                this.child(command_pane_tab_status_indicator_element(
                    format!(
                        "ghostex-gpui-command-tab-status-indicator-{}-{}",
                        session_id.0,
                        tab_status.element_slug()
                    ),
                    tab_status,
                ))
            })
            .when(
                command_pane_tab_separator_visible(has_following_command_tab),
                |this| this.child(self.render_command_pane_tab_separator()),
            )
            .when(is_tab_hovered, |this| {
                this.child(self.render_command_pane_tab_close_button(group_id, session_id, cx))
            })
            .into_any_element()
    }

    pub(crate) fn render_command_pane_tab_separator(&self) -> AnyElement {
        div()
            .absolute()
            .right_0()
            .top_0()
            .h_full()
            .w(px(COMMAND_PANE_TAB_SEPARATOR_WIDTH))
            .bg(command_pane_tab_separator_color())
            .into_any_element()
    }

    pub(crate) fn render_command_pane_tab_add_button(
        &self,
        group_id: Option<CommandPaneGroupId>,
        collapsed_strip: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let element_id = match group_id {
            Some(group_id) => format!("ghostex-gpui-command-pane-tab-add-{}", group_id.0),
            None => "ghostex-gpui-command-pane-strip-tab-add".to_string(),
        };
        let size = if collapsed_strip {
            COMMAND_PANE_STRIP_HEIGHT
        } else {
            COMMAND_PANE_TAB_BAR_HEIGHT
        };

        /*
        CDXC:GPUICommandPaneControls 2026-06-25-12:13:
        New Terminal is command-tab chrome, not a fixed panel action. Render it inline after the command tab run so expanded titlebars and the collapsed strip mirror macOS command chrome while reusing the existing command-placeholder creation path.

        CDXC:GPUICommandPaneControls 2026-06-25-14:44:
        Native command tab-add chrome uses `setTabBarIconChrome`, so its #0e0e0e background and #cfcfcf plus tint are stable in normal, hover, and active states. Do not leave the inline New Terminal button transparent until hover.
        */
        div()
            .id(element_id)
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(size))
            .items_center()
            .justify_center()
            .border_r_1()
            .border_color(command_pane_titlebar_separator_color())
            .bg(command_pane_control_button_color())
            .text_color(command_pane_control_text_color())
            .cursor_default()
            .hover(|this| this.bg(command_pane_control_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_command_pane_control_action(
                        CommandPaneControlAction::NewCommandPlaceholder,
                        group_id,
                        window,
                        cx,
                    );
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                Tooltip::new(command_pane_tab_add_tooltip()).build(window, cx)
            })
            .child(titlebar_svg_icon(
                command_pane_tab_add_icon_path(),
                COMMAND_PANE_CONTROL_ICON_SIZE,
                command_pane_control_text_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_command_pane_sticky_active_tab_button(
        &self,
        element_id: impl Into<String>,
        edge: CommandPaneStickyActiveTabEdge,
        trailing_inset: f32,
        group_id: CommandPaneGroupId,
        scroll_handle: ScrollHandle,
        active_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tooltip_placement = match edge {
            CommandPaneStickyActiveTabEdge::Leading => ManagedTooltipPlacement::Right,
            CommandPaneStickyActiveTabEdge::Trailing => ManagedTooltipPlacement::Left,
        };
        /*
        CDXC:GPUICommandTabOverflow 2026-06-25-13:34:
        Render native Show Active Tab as a real 30px command-role button at the clipped tab-strip edge. It owns only one inner border, uses stable command icon-button chrome, and scrolls the existing active tab instead of creating a decorative reveal slot.

        CDXC:GPUICommandTabOverflow 2026-06-25-18:51:
        Native overlays the Show Active Tab button on the tab viewport edge instead of reserving flex width. Position GPUI's proxy absolutely before tab-add/fixed panel controls so clipped tabs keep the same available viewport width as macOS.
        */
        div()
            .id(format!(
                "ghostex-gpui-command-sticky-active-tab-{}",
                element_id.into()
            ))
            .absolute()
            .top_0()
            .bottom_0()
            .flex()
            .w(px(COMMAND_PANE_STICKY_ACTIVE_TAB_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .bg(command_pane_sticky_active_tab_button_color())
            .text_color(command_pane_sticky_active_tab_icon_color())
            .cursor_default()
            .when(edge == CommandPaneStickyActiveTabEdge::Leading, |this| {
                this.left_0()
                    .border_r_1()
                    .border_color(command_pane_sticky_active_tab_border_color())
            })
            .when(edge == CommandPaneStickyActiveTabEdge::Trailing, |this| {
                this.right(px(trailing_inset))
                    .border_l_1()
                    .border_color(command_pane_sticky_active_tab_border_color())
            })
            .hover(|this| this.bg(command_pane_sticky_active_tab_button_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_command_pane_active_tab_from_sticky_proxy(
                        group_id,
                        scroll_handle.clone(),
                        active_index,
                        cx,
                    );
                }),
            )
            .managed_tooltip_with_placement(tooltip_placement, move |window, cx| {
                Tooltip::new(command_pane_sticky_active_tab_tooltip()).build(window, cx)
            })
            .child(titlebar_svg_icon(
                command_pane_sticky_active_tab_icon_path(edge),
                COMMAND_PANE_STICKY_ACTIVE_TAB_ICON_SIZE,
                command_pane_sticky_active_tab_icon_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_command_tab_strip_end_drop_target(
        &self,
        group_id: CommandPaneGroupId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let show_insertion_marker = self.command_drop_feedback
            == Some(CommandPaneDropFeedback {
                group_id,
                target: CommandPaneDropTarget::TabStrip(insertion_index),
            });

        div()
            .id(format!(
                "ghostex-gpui-command-tabstrip-end-drop-{}",
                group_id.0
            ))
            .relative()
            .h_full()
            .flex_shrink_0()
            .w(px(COMMAND_PANE_TAB_END_DROP_TARGET_WIDTH))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_command_pane_empty_titlebar_mouse_down(
                        Some(group_id),
                        event,
                        window,
                        cx,
                    );
                }),
            )
            .when(show_insertion_marker, |this| {
                this.child(self.render_command_tab_insertion_marker(
                    group_id,
                    insertion_index,
                    "end",
                ))
            })
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_end_drag_feedback(event, group_id, insertion_index, cx);
                },
            ))
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_tab_over_command_tab_end_drag_feedback(
                        event,
                        group_id,
                        insertion_index,
                        cx,
                    );
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedCommandTab>()
                    .is_some_and(|dragged| dragged.source_group_id == group_id)
                    || value.is::<DraggedWorkspaceTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_strip_drop(
                        group_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_tab_command_tab_strip_drop(
                        group_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(crate) fn render_command_tab_insertion_marker(
        &self,
        group_id: CommandPaneGroupId,
        insertion_index: usize,
        marker_kind: &'static str,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-command-tab-drop-marker-{}-{}-{marker_kind}",
                group_id.0, insertion_index
            ))
            .absolute()
            .left_0()
            .top(px(4.0))
            .h(px(COMMAND_PANE_TAB_BAR_HEIGHT - 8.0))
            .w(px(2.0))
            .rounded_full()
            .bg(workspace_drop_feedback_border_color())
            .into_any_element()
    }

    pub(crate) fn render_command_pane_tab_close_button(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-command-pane-tab-close-{}-{}",
                group_id.0, session_id.0
            ))
            .absolute()
            .right(px(COMMAND_PANE_TAB_CLOSE_TRAILING_PADDING))
            .top(px(COMMAND_PANE_TAB_CLOSE_TOP_OFFSET))
            .flex()
            .size(px(COMMAND_PANE_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(COMMAND_PANE_TAB_CLOSE_CORNER_RADIUS))
            .bg(command_pane_control_button_color())
            .text_color(command_pane_control_text_color())
            .cursor_default()
            .hover(|this| this.bg(command_pane_control_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    /*
                    CDXC:GPUICommandTabClose 2026-06-25-14:04:
                    Native inline tab Close records the clicked action on mouse-down but invokes the close only on mouse-up if the close control still owns the pointer. GPUI command close chrome should consume down without tearing down the tab early.
                    */
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_command_pane_tab(group_id, session_id, cx);
                }),
            )
            .child(titlebar_svg_icon(
                COMMAND_ICON_XMARK,
                COMMAND_PANE_TAB_CLOSE_ICON_SIZE,
                command_pane_control_text_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn set_workspace_tab_hovered(
        &mut self,
        tab: WorkspaceHoverTab,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if hovered {
            if self.hovered_workspace_tab != Some(tab) {
                self.hovered_workspace_tab = Some(tab);
                cx.notify();
            }
            return;
        }

        if self.hovered_workspace_tab == Some(tab) {
            self.hovered_workspace_tab = None;
            cx.notify();
        }
    }

    pub(crate) fn set_command_pane_tab_hovered(
        &mut self,
        tab: CommandPaneHoverTab,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandTabChrome 2026-06-25-13:11:
        macOS command-tab close controls are draw-time hover chrome. Track the hovered command tab separately from command model state so hover can reveal one absolute close affordance without changing selection, persistence, tab order, or title layout.
        */
        if hovered {
            if self.hovered_command_tab != Some(tab) {
                self.hovered_command_tab = Some(tab);
                cx.notify();
            }
            return;
        }

        if self.hovered_command_tab == Some(tab) {
            self.hovered_command_tab = None;
            cx.notify();
        }
    }

    pub(crate) fn set_command_resize_hovering(
        &mut self,
        target: CommandPaneResizeHoverTarget,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPaneResize 2026-06-25-13:19:
        Native resize rails reveal a white hover line only after a 50ms delay and fade it over 180ms, scoped to the exact rail under the pointer. Keep GPUI command resize hover as runtime chrome state separate from command-pane layout, drag, and persistence.
        */
        if hovered {
            if self.command_resize_hovering == Some(target) {
                return;
            }
            self.command_resize_hover_epoch = self.command_resize_hover_epoch.wrapping_add(1);
            self.command_resize_hovering = Some(target);
            self.command_resize_hover_visible = None;
            let epoch = self.command_resize_hover_epoch;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                    .await;

                let _ = this.update(cx, |this, cx| {
                    if this.command_resize_hover_epoch == epoch
                        && this.command_resize_hovering == Some(target)
                    {
                        this.command_resize_hover_visible = Some(target);
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }

        if self.command_resize_hovering == Some(target)
            || self.command_resize_hover_visible == Some(target)
        {
            self.command_resize_hover_epoch = self.command_resize_hover_epoch.wrapping_add(1);
            self.command_resize_hovering = None;
            self.command_resize_hover_visible = None;
            cx.notify();
        }
    }

    pub(crate) fn clear_command_resize_hover_state(&mut self) -> bool {
        clear_command_resize_hover_state_fields(
            &mut self.command_resize_hovering,
            &mut self.command_resize_hover_visible,
            &mut self.command_resize_hover_epoch,
        )
    }

    pub(crate) fn clear_command_resize_hover_state_if_command_pane_hidden(&mut self) -> bool {
        clear_command_resize_hover_state_fields_if_command_pane_hidden(
            self.command_pane.has_sessions(),
            &mut self.command_resize_hovering,
            &mut self.command_resize_hover_visible,
            &mut self.command_resize_hover_epoch,
        )
    }

    pub(crate) fn render_command_pane_controls(
        &self,
        group_id: Option<CommandPaneGroupId>,
        expanded_chrome: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pin_tooltip = command_pane_panel_pin_label(self.command_pane.mode);
        let expand_tooltip = if self.command_pane.is_expanded() {
            command_pane_panel_minimize_label()
        } else {
            command_pane_panel_expand_menu_label()
        };
        let pin_icon_path = command_pane_panel_pin_icon_path(self.command_pane.mode);
        let expand_icon_path =
            command_pane_panel_visibility_icon_path(self.command_pane.is_expanded());
        let controls_id = if expanded_chrome {
            match group_id {
                Some(group_id) => {
                    format!("ghostex-gpui-command-pane-titlebar-controls-{}", group_id.0)
                }
                None => "ghostex-gpui-command-pane-titlebar-controls".to_string(),
            }
        } else {
            "ghostex-gpui-command-pane-strip-controls".to_string()
        };

        /*
        CDXC:GPUICommandPaneControls 2026-06-24-07:33:
        Visible command-pane chrome copy should describe command actions instead of placeholder internals. This source-only control stays non-launching and private-detail-free: it creates only the existing command shell entry without terminal/CEF runtime work, command text, or output.

        CDXC:GPUICommandPaneControls 2026-06-25-12:05:
        Collapsed command-strip chrome keeps New Terminal inline with the tab run and keeps Expand in the fixed panel cluster, but omits Pin/Unpin because macOS hidden command tabs expose expand-only panel actions. Panel mode mutation stays scoped to expanded titlebars so a hidden strip cannot flip pinned/floating state before opening.

        CDXC:GPUICommandPaneControls 2026-06-25-12:13:
        The fixed command-pane action cluster excludes New Terminal because macOS renders creation as an inline tab-run plus button. Keep this cluster panel-scoped so collapsed chrome has only Expand while expanded titlebars have Pin/Unpin plus the single Minimize affordance.

        CDXC:GPUICommandPaneControls 2026-06-25-12:26:
        Native visible command panels publish exactly Pin/Unpin Commands Panel plus closeCommandsPanel, rendered as the Minimize chevron. Do not add a second `x` minimize button to expanded GPUI command titlebars.

        CDXC:GPUICommandPaneControls 2026-06-25-13:47:
        Native command-panel action buttons are normal titlebar button frames, not a padded cluster: keep buttons contiguous, flat, stable-colored, and apply the 8px trailing inset only in expanded command titlebars.
        */
        h_flex()
            .id(controls_id)
            .flex_shrink_0()
            .h_full()
            .items_center()
            .gap(px(COMMAND_PANE_CONTROL_BUTTON_GAP))
            .pl(px(0.0))
            .pr(px(command_pane_control_trailing_padding(expanded_chrome)))
            .bg(command_pane_control_cluster_color())
            .when(
                command_pane_new_command_control_placement()
                    == CommandPaneNewCommandControlPlacement::FixedActionCluster,
                |this| {
                    this.child(self.render_command_pane_control_button(
                        "new-command",
                        command_pane_tab_add_icon_path(),
                        command_pane_tab_add_tooltip(),
                        CommandPaneControlAction::NewCommandPlaceholder,
                        group_id,
                        cx,
                    ))
                },
            )
            .when(
                command_pane_panel_mode_controls_visible(expanded_chrome),
                |this| {
                    this.child(self.render_command_pane_control_button(
                        "pin",
                        pin_icon_path,
                        pin_tooltip,
                        CommandPaneControlAction::TogglePinned,
                        group_id,
                        cx,
                    ))
                },
            )
            .child(self.render_command_pane_control_button(
                "expand",
                expand_icon_path,
                expand_tooltip,
                CommandPaneControlAction::ToggleExpanded,
                group_id,
                cx,
            ))
            .into_any_element()
    }

    pub(crate) fn render_command_pane_control_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        tooltip: &'static str,
        action: CommandPaneControlAction,
        group_id: Option<CommandPaneGroupId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let element_id = match group_id {
            Some(group_id) => format!("ghostex-gpui-command-pane-control-{id}-{}", group_id.0),
            None => format!("ghostex-gpui-command-pane-control-{id}"),
        };
        let is_expanded_minimize = matches!(action, CommandPaneControlAction::ToggleExpanded)
            && self.command_pane.is_expanded();

        div()
            .id(element_id)
            .flex()
            .size(px(COMMAND_PANE_CONTROL_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .when(is_expanded_minimize, |this| {
                this.pl(px(COMMAND_PANE_EXPANDED_MINIMIZE_ICON_LEADING_PADDING))
            })
            .rounded(px(COMMAND_PANE_CONTROL_CORNER_RADIUS))
            .bg(command_pane_control_button_color())
            .text_color(command_pane_control_text_color())
            .cursor_default()
            .hover(|this| this.bg(command_pane_control_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_command_pane_control_action(action, group_id, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
                Tooltip::new(tooltip).build(window, cx)
            })
            .child(titlebar_svg_icon(
                icon_path,
                COMMAND_PANE_CONTROL_ICON_SIZE,
                command_pane_control_text_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_command_terminal_placeholder(
        &self,
        leaf: &CommandPaneLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let group_id = leaf.group_id;
        let body_owner = self.command_pane.visible_command_body_owner_for_leaf(leaf);
        let body_has_session = body_owner.is_some();
        let active_session_id = body_owner
            .map(|owner| owner.session_id)
            .unwrap_or(CommandSessionId(0));
        let active_session_is_sleeping = body_owner.is_some_and(|owner| owner.is_sleeping);
        let mount_slot_id = body_owner.and_then(CommandPaneVisibleBodyOwner::mount_slot_id);
        // See CDXC:GPUITerminalGpuiEngine in the Agents body renderer: an
        // engine-claimed command slot renders the composited element and
        // skips native probes/forwarding gates.
        let gpui_engine_view = mount_slot_id
            .and_then(|slot_id| self.command_gpui_engine_terminals.get(&slot_id.session_id))
            .map(|record| record.view.clone());
        let gpui_engine_owns_pointer_input = gpui_engine_view.is_some();
        let gpui_engine_slot_id = mount_slot_id.filter(|_| gpui_engine_owns_pointer_input);
        let native_mount_slot_id = mount_slot_id.filter(|_| gpui_engine_view.is_none());
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let (terminal_horizontal_padding, terminal_vertical_padding) =
            settings_snapshot.terminal_pane_padding_px();
        let sleeping_wake_label = command_pane_sleeping_placeholder_wake_label(
            active_session_is_sleeping,
            command_pane_click_to_wake_sleeping_sessions_from_shared_settings(&settings_snapshot),
        );
        let delayed_send_remaining_label = mount_slot_id.and_then(|_| {
            self.gpui_command_delayed_send_remaining_label_for_session(active_session_id)
        });

        /*
        CDXC:GPUICommandTerminalInputForwarding 2026-06-23-09:41:
        Mounted command terminal input uses the same command body div that already owns focus and drop behavior. Button press focuses the command group/pane before trying Ghostty forwarding, movement and button release pass GPUI modifiers to Ghostty input.Mods, and wheel events update pointer position with those mapped modifiers while keeping scroll mods precision-only.

        CDXC:GPUITerminalPressureForwarding 2026-06-23-09:51:
        Command terminal force-click pressure events are wired only for mounted command body slots. Forward pressure through the existing body element without default-stop so command focus and drop behavior keep their current ownership.

        CDXC:GPUICommandTerminalInputForwarding 2026-06-23-10:05:
        Mounted command bodies use GPUI element-level mouse-up-out only for capture-gated release parity. The handler does not route through the window, record outside positions, or synthesize coordinates.

        CDXC:GPUITerminalMouseButtons 2026-06-23-10:23:
        Mounted command bodies must forward right and middle down/up/up-out through the same body element as left. Right and middle down focus the mounted command terminal surface before forwarding the press, while placeholders keep their existing left-click-only focus behavior and no tab/chrome context menus are changed.

        CDXC:GPUITerminalSelectionDrag 2026-06-23-12:43:
        Mounted command selection drag does not need a command-pane-owned selection or drag-capture field. The command body already owns left press, body-relative move, in-body release, capture-gated up-out, typed command/workspace tab drag-over/drop handlers, and command focus handoff as normal sibling layout behavior; keep selection forwarding inside that body instead of adding overlays, hidden hit regions, stored coordinates, global capture, or root/window mouse routing.

        CDXC:GPUITerminalNativeKeyBridge 2026-07-10:
        Mounted libghostty terminals keep keyboard and IME ownership on their exact AppKit host NSView. The body still owns normal mouse/layout behavior, but it must not track GPUI's legacy terminal text focus handle because that changes the window first responder and drops native keycodes/modifiers before libghostty sees them.

        CDXC:GPUICommandTabSleep 2026-06-25-14:27:
        A sleeping active command tab renders the same body placeholder but no Ghostty mount slot. Left-clicking that body wakes the command session; tab selection, right-click menus, and placeholder paint do not wake it.

        CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
        The sleeping command body paints the native centered wake label only while click-to-wake placeholders are enabled. Mounting command placeholders stay blank, while the normal body element continues to own click wake, drag/drop, and any future terminal mount slot.

        CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
        Render the sleeping wake label as paint-only canvas chrome using this exact body element's prepaint bounds. Do not add flex overlays, input-owning label elements, root/window routing, persistent geometry, or fallback dimensions; the body remains the sole layout and wake interaction owner.

        CDXC:GPUICommandDelayedSend 2026-06-25-15:42:
        Active command Delayed Send timers also paint a centered countdown badge inside the same body element. This is visual-only child chrome so command body focus, mouse forwarding, wake, drag/drop, and native host bounds stay owned by the normal command body layout.

        CDXC:GPUICommandAttention 2026-06-25-19:58:
        Command body clicks are direct command-session activation. Match native by acknowledging only the clicked command session's Attention state when the body takes focus, without clearing Working, Delayed Send, sleeping placeholders, or Agents activity.

        CDXC:GPUICommandPaneFocus 2026-06-26-00:00:
        Non-mounted command body focus must reveal the focused group's active command tab in the expanded strip and collapsed strip, sharing the same body-click parity as mounted command terminals without changing wake, attention, or placeholder activation semantics.

        CDXC:GPUICommandTerminalSurface 2026-06-27-04:36:
        The command body renderer must consume the same visible body owner as mount-slot reconciliation. This keeps sleeping selected tabs visible as wake placeholders, lets selected non-sleeping tabs mount Ghostty, and prevents stale active ids from borrowing an inactive sibling's session for badges, wake, or input forwarding.
        */
        div()
            .id(format!(
                "ghostex-gpui-command-terminal-placeholder-{}-{}",
                group_id.0, active_session_id.0
            ))
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(command_terminal_placeholder_color())
            .when(
                native_mount_slot_id
                    .is_some_and(|slot_id| self.command_terminal_slot_hovers_link(slot_id)),
                |this| this.cursor_pointer(),
            )
            .when_some(gpui_engine_view, |this, view| {
                this.child(
                    div()
                        .absolute()
                        .left(px(terminal_horizontal_padding))
                        .right(px(terminal_horizontal_padding))
                        .top(px(terminal_vertical_padding))
                        .bottom(px(terminal_vertical_padding))
                        .child(view),
                )
            })
            .when_some(gpui_engine_slot_id, |this, slot_id| {
                this.capture_any_mouse_down(cx.listener(
                    move |this, _event: &MouseDownEvent, window, cx| {
                        /*
                        The composited terminal child owns pointer input, so the
                        command body must claim shell focus during capture without
                        stopping propagation to the terminal. This matches the
                        Agents and project-editor companion terminal paths.
                        */
                        this.focus_command_terminal_mount_slot(slot_id, window, cx);
                        this.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
                    },
                ))
            })
            .when(!gpui_engine_owns_pointer_input, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if let Some(slot_id) = mount_slot_id {
                            this.focus_command_terminal_mount_slot(slot_id, window, cx);
                            this.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx);
                            let _ = this.forward_command_terminal_mount_slot_mouse_button(
                                slot_id,
                                event.position,
                                ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                                event.button,
                                event.modifiers,
                            );
                        } else if active_session_is_sleeping {
                            this.wake_command_pane_session(group_id, active_session_id, cx);
                        } else {
                            this.command_pane.focus_group(group_id);
                            let attention_acknowledged = this
                                .command_pane
                                .acknowledge_attention_for_session_activation(active_session_id);
                            this.focus_command_pane();
                            if body_has_session {
                                this.request_command_terminal_text_focus_handoff(
                                    CommandTerminalBodyMountSlotId {
                                        group_id,
                                        session_id: active_session_id,
                                    },
                                );
                            }
                            this.scroll_command_group_active_tab(group_id);
                            if attention_acknowledged {
                                this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                            }
                            cx.notify();
                        }
                    }),
                )
            })
            .when_some(sleeping_wake_label, |this, label| {
                this.child(
                    canvas(
                        move |bounds, window, _| {
                            command_pane_sleeping_placeholder_wake_label_prepaint(
                                bounds, label, window,
                            )
                        },
                        move |bounds, paint_state, window, cx| {
                            if let Some(paint_state) = paint_state {
                                command_pane_sleeping_placeholder_wake_label_paint(
                                    bounds,
                                    paint_state,
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .when_some(delayed_send_remaining_label, |this, label| {
                this.child(
                    canvas(
                        move |bounds, window, _| {
                            command_pane_delayed_send_badge_prepaint(bounds, label, window)
                        },
                        move |bounds, paint_state, window, cx| {
                            if let Some(paint_state) = paint_state {
                                command_pane_delayed_send_badge_paint(
                                    bounds,
                                    paint_state,
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_pane_drag_feedback(event, group_id, cx);
                },
            ))
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_tab_over_command_pane_drag_feedback(event, group_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.is::<DraggedCommandTab>() || value.is::<DraggedWorkspaceTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_pane_body_drop(group_id, dragged, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_tab_command_pane_body_drop(group_id, dragged, window, cx);
                }),
            )
            .when_some(native_mount_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.focus_command_terminal_mount_slot(slot_id, window, cx);
                        let _ = this.forward_command_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.focus_command_terminal_mount_slot(slot_id, window, cx);
                        let _ = this.forward_command_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_position(
                            slot_id,
                            event.position,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_pressure(cx.listener(
                    move |this, event: &MousePressureEvent, _window, _cx| {
                        let _ = this.forward_command_terminal_mount_slot_mouse_pressure(
                            slot_id,
                            event.position,
                            event.stage,
                            event.pressure,
                            event.modifiers,
                        );
                    },
                ))
                .on_scroll_wheel(
                    cx.listener(move |this, event: &ScrollWheelEvent, window, cx| {
                        if this.forward_command_terminal_mount_slot_mouse_scroll(
                            slot_id,
                            event.position,
                            event.delta,
                            event.modifiers,
                        ) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    }),
                )
                .child({
                    let bounds_view = view.clone();
                    let input_handler_view = view.clone();
                    canvas(
                        move |bounds, window, cx| {
                            let scale_factor = window.scale_factor();
                            let _ = bounds_view.update(cx, |this, cx| {
                                this.record_command_terminal_mount_slot_bounds(
                                    slot_id,
                                    bounds,
                                    scale_factor,
                                    cx,
                                );
                            });
                        },
                        move |bounds, _, window, cx| {
                            let input_view = input_handler_view.clone();
                            let _ = input_handler_view.update(cx, |this, cx| {
                                this.register_command_terminal_text_input_handler(
                                    slot_id, bounds, input_view, window, cx,
                                );
                            });
                        },
                    )
                    .absolute()
                    .left(px(terminal_horizontal_padding))
                    .right(px(terminal_horizontal_padding))
                    .top(px(terminal_vertical_padding))
                    .bottom(px(terminal_vertical_padding))
                })
            })
            .when_some(self.command_pane_drop_zone(group_id), |this, zone| {
                this.child(self.render_command_pane_drop_feedback(group_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn command_pane_drop_zone(&self, group_id: CommandPaneGroupId) -> Option<WorkspaceDropZone> {
        match self.command_drop_feedback {
            Some(CommandPaneDropFeedback {
                group_id: feedback_group_id,
                target: CommandPaneDropTarget::PaneBody(zone),
            }) if feedback_group_id == group_id => Some(zone),
            _ => None,
        }
    }

    pub(crate) fn render_command_pane_drop_feedback(
        &self,
        group_id: CommandPaneGroupId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        let feedback = div()
            .id(format!(
                "ghostex-gpui-command-pane-drop-feedback-{}",
                group_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full();

        match zone {
            WorkspaceDropZone::Left => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .left_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Right => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .right_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Center | WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => {
                feedback
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_2()
                    .border_color(agents_drop_feedback_border_color())
                    .bg(agents_drop_group_feedback_color())
                    .into_any_element()
            }
        }
    }

    pub(crate) fn render_agents_workspace(&self, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
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
            .when(
                border_state == WorkspacePaneBorderState::Attention,
                |this| this.border_2(),
            )
            .when(
                border_state != WorkspacePaneBorderState::Attention,
                |this| this.border_1(),
            )
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
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIWorkspaceTabs 2026-06-22-05:18:
        Agents terminal panes need native-style tab chrome before libghostty mounts: every pane keeps an always-visible tab bar, tab selection colors stay tied to active tab state instead of pane focus, horizontal overflow remains scrollable, and right-side pane actions stay in the tab chrome instead of overlapping terminal bodies.

        CDXC:GPUIAgentsPaneActions 2026-06-22-06:39:
        Agents pane action chrome mirrors the native compact tab-bar cluster: fixed New Terminal, New Browser Tab, and pane overflow controls live at the far right. Split, rotate, and merge actions stay in the overflow menu rather than occupying fixed tab-bar slots.

        CDXC:GPUIAgentsMergeAllTabs 2026-06-22-13:17:
        Merge All Tabs lives in the Agents pane overflow after Split Sideways, Split Downwards, and Rotate Panes Clockwise, matching the native pane titlebar ordering while keeping command terminals in the separate command-pane action set.
        */
        let scroll_handle = self.workspace_tab_scroll_handle(leaf.pane_id);

        h_flex()
            .id(format!("ghostex-gpui-workspace-tabbar-{}", leaf.pane_id.0))
            .flex_shrink_0()
            .h(px(WORKSPACE_TAB_BAR_HEIGHT))
            .w_full()
            .items_center()
            .overflow_hidden()
            .bg(workspace_tab_bar_color())
            .border_b_1()
            .border_color(workspace_tab_border_color())
            .when_some(
                self.render_agents_terminal_queued_prompts_chip(leaf, cx),
                |this, chip| this.child(chip),
            )
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-workspace-tabstrip-{}",
                        leaf.pane_id.0
                    ))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .overflow_x_scroll()
                    .track_scroll(&scroll_handle)
                    .children(
                        leaf.tab_group
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(tab_index, tab)| {
                                self.render_workspace_tab(
                                    leaf.pane_id,
                                    &leaf.tab_group,
                                    tab_index,
                                    tab,
                                    cx,
                                )
                            }),
                    )
                    .child(self.render_workspace_tab_strip_end_drop_target(
                        leaf.pane_id,
                        leaf.tab_group.tabs.len(),
                        cx,
                    )),
            )
            .child(self.render_workspace_tab_action_cluster(leaf.pane_id, cx))
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab(
        &self,
        pane_id: WorkspacePaneId,
        tab_group: &WorkspaceTabGroup,
        tab_index: usize,
        tab: &WorkspaceTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let session = self.agents_workspace.session(tab.session_id);
        let chrome_signature = workspace_tab_chrome_signature(tab_group, tab.session_id, session);
        let presentation_state = chrome_signature.presentation_state;
        let tab_status = chrome_signature.tab_status;
        let visual_tone = chrome_signature.lifecycle_visual_tone;
        let title = self.agents_workspace_tab_display_title(tab.session_id);
        let agent_icon = session.and_then(|session| session.agent_icon);
        let session_id = tab.session_id;
        let can_close = self.agents_workspace.can_close_tab(pane_id, session_id);
        let dragged_tab = DraggedWorkspaceTab {
            source_pane_id: pane_id,
            session_id,
            title: title.clone(),
            presentation_state,
            tab_status,
            agent_icon,
        };
        let show_insertion_marker = self.workspace_drop_feedback
            == Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::TabStrip(tab_index),
            });
        let tab_hover_key = WorkspaceHoverTab {
            pane_id,
            session_id,
        };
        let is_tab_hovered = self.hovered_workspace_tab == Some(tab_hover_key);
        let show_status_indicator =
            workspace_tab_status_indicator_visible(visual_tone, tab_status, is_tab_hovered);
        let title_trailing_reserved_width =
            workspace_tab_status_title_trailing_reserved_width(visual_tone, tab_status);
        let view = cx.entity().clone();

        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-{}-{}",
                pane_id.0, session_id.0
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_TAB_WIDTH))
            .items_center()
            .overflow_hidden()
            .border_r_1()
            .border_color(workspace_tab_border_color())
            .pl(px(11.0))
            .pr(px(6.0))
            .text_size(px(12.5))
            .font_weight(FontWeight::NORMAL)
            .text_color(workspace_tab_text_color(visual_tone))
            .cursor_default()
            .bg(workspace_tab_background_color(visual_tone))
            .when(show_insertion_marker, |this| {
                this.child(self.render_workspace_tab_insertion_marker(pane_id, tab_index, "before"))
            })
            .hover(|this| this.bg(workspace_tab_background_color(visual_tone)))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_workspace_tab_hovered(tab_hover_key, *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.begin_pending_workspace_tab_click(pane_id, session_id);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_workspace_tab_left_mouse_up(
                        pane_id,
                        session_id,
                        event.click_count,
                        cx,
                    );
                    if this.workspace_tab_drag_active {
                        this.finish_workspace_tab_drag(cx);
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, _window, _cx| {
                    this.cancel_pending_workspace_tab_click_for_tab(pane_id, session_id);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    /*
                    CDXC:GPUIAgentsTabClose 2026-07-10:
                    Agents tabs must own button-2 like command and Browser
                    tabs. Consume the press without selecting or starting a
                    drag, then close the exact clicked tab on mouse-up through
                    the same local-first close helper as the visible X.
                    */
                    window.prevent_default();
                    cx.stop_propagation();
                    this.cancel_pending_workspace_tab_click();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if can_close {
                        this.close_agents_tab(pane_id, session_id, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.cancel_pending_workspace_tab_click();
                    this.show_agents_tab_context_menu(
                        pane_id,
                        session_id,
                        event.position,
                        window,
                        cx,
                    );
                }),
            )
            .on_drag(dragged_tab, move |dragged, _offset, _window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.begin_workspace_tab_drag(cx);
                });
                cx.new(|_| WorkspaceTabDragPreview {
                    title: dragged.title.clone(),
                    presentation_state: dragged.presentation_state,
                    tab_status: dragged.tab_status,
                    agent_icon: dragged.agent_icon,
                })
            })
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_tab_drag_feedback(event, pane_id, tab_index, cx);
                },
            ))
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_over_workspace_tab_drag_feedback(
                        event, pane_id, tab_index, cx,
                    );
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedWorkspaceTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
                    || value.is::<DraggedCommandTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_tab_strip_drop(pane_id, tab_index, dragged, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_tab_strip_drop(
                        pane_id, tab_index, dragged, window, cx,
                    );
                }),
            )
            .child(workspace_tab_icon_element(
                format!("ghostex-gpui-workspace-tab-icon-{}", session_id.0),
                agent_icon,
                visual_tone,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(8.0))
                    .pr(px(title_trailing_reserved_width))
                    .child(title),
            )
            .when(show_status_indicator, |this| {
                this.child(workspace_tab_status_indicator_element(
                    format!(
                        "ghostex-gpui-workspace-tab-status-indicator-{}-{}",
                        session_id.0,
                        tab_status.element_slug()
                    ),
                    visual_tone,
                    tab_status,
                ))
            })
            .when(
                workspace_tab_sleep_icon_visible(visual_tone, is_tab_hovered),
                |this| this.child(self.render_workspace_tab_sleep_icon(session_id)),
            )
            .when_some(presentation_state.tab_badge_label(), |this, label| {
                this.child(self.render_workspace_tab_state_badge(session_id, visual_tone, label))
            })
            .when(is_tab_hovered && can_close, |this| {
                this.child(self.render_workspace_tab_close_button(pane_id, session_id, cx))
            })
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_strip_end_drop_target(
        &self,
        pane_id: WorkspacePaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIWorkspaceTabEndGap 2026-08-03:
        The end-of-strip drop target is interaction chrome, not permanent tab
        spacing. Keep its 24px normal-layout target while either supported tab
        drag is active, but collapse it at rest so the final workspace tab is
        flush with the fixed action cluster.
        */
        let drag_active = self.workspace_tab_drag_active || self.command_tab_drag_active;
        let show_insertion_marker = self.workspace_drop_feedback
            == Some(WorkspaceDropFeedback {
                pane_id,
                target: WorkspaceDropTarget::TabStrip(insertion_index),
            });

        div()
            .id(format!(
                "ghostex-gpui-workspace-tabstrip-end-drop-{}",
                pane_id.0
            ))
            .relative()
            .h_full()
            .when(drag_active, |this| this.flex_grow_1().min_w(px(24.0)))
            .when(!drag_active, |this| this.w(px(0.0)).flex_shrink_0())
            .when(show_insertion_marker, |this| {
                this.child(self.render_workspace_tab_insertion_marker(
                    pane_id,
                    insertion_index,
                    "end",
                ))
            })
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_tab_end_drag_feedback(
                        event,
                        pane_id,
                        insertion_index,
                        cx,
                    );
                },
            ))
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_over_workspace_tab_end_drag_feedback(
                        event,
                        pane_id,
                        insertion_index,
                        cx,
                    );
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedWorkspaceTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
                    || value.is::<DraggedCommandTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_tab_strip_drop(
                        pane_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_tab_strip_drop(
                        pane_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_insertion_marker(
        &self,
        pane_id: WorkspacePaneId,
        insertion_index: usize,
        marker_kind: &'static str,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-drop-marker-{}-{}-{marker_kind}",
                pane_id.0, insertion_index
            ))
            .absolute()
            .left_0()
            .top(px(4.0))
            .h(px(WORKSPACE_TAB_BAR_HEIGHT - 8.0))
            .w(px(2.0))
            .rounded_full()
            .bg(workspace_tab_reorder_insertion_marker_color())
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_sleep_icon(&self, session_id: TerminalSessionId) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-sleep-icon-{}",
                session_id.0
            ))
            .absolute()
            .right(px(WORKSPACE_TAB_SLEEP_ICON_TRAILING_PADDING))
            .top(px((WORKSPACE_TAB_BAR_HEIGHT
                - WORKSPACE_TAB_SLEEP_ICON_SIZE)
                / 2.0))
            .flex()
            .size(px(WORKSPACE_TAB_SLEEP_ICON_SIZE))
            .items_center()
            .justify_center()
            .text_color(workspace_tab_sleep_icon_color())
            .child(titlebar_svg_icon(
                COMMAND_ICON_MOON,
                WORKSPACE_TAB_SLEEP_SVG_SIZE,
                workspace_tab_sleep_icon_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_state_badge(
        &self,
        session_id: TerminalSessionId,
        visual_tone: WorkspaceTabLifecycleVisualTone,
        label: &'static str,
    ) -> AnyElement {
        let presentation_state = visual_tone.presentation_state;
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-state-{}-{}",
                session_id.0,
                presentation_state.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .h(px(15.0))
            .min_w(px(26.0))
            .ml(px(6.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .bg(workspace_tab_state_badge_background(visual_tone))
            .px(px(4.0))
            .text_size(px(9.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(workspace_tab_state_badge_text_color(visual_tone))
            .child(label)
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_close_button(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-close-{}-{}",
                pane_id.0, session_id.0
            ))
            .absolute()
            .right(px(WORKSPACE_TAB_CLOSE_TRAILING_PADDING))
            .top(px(WORKSPACE_TAB_CLOSE_TOP_OFFSET))
            .flex()
            .size(px(WORKSPACE_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(0.0))
            .bg(workspace_tab_action_button_color())
            .text_color(workspace_tab_action_icon_color())
            .cursor_default()
            .hover(|this| this.bg(tab_bar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    /*
                    CDXC:GPUIAgentsWorkspaceTabs 2026-06-26-06:57:
                    Native inline Agents tab Close records pointer ownership on mouse-down and performs close on mouse-up. Consume the down event here so tab selection/drag does not start under the close affordance, but keep teardown out of mouse-down.
                    */
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_agents_tab(pane_id, session_id, cx);
                }),
            )
            .child(titlebar_svg_icon(
                COMMAND_ICON_XMARK,
                WORKSPACE_TAB_CLOSE_ICON_SIZE,
                workspace_tab_action_icon_color(),
            ))
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_action_cluster(
        &self,
        pane_id: WorkspacePaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUISessionChatSurface 2026-08-02:
        The Terminal/Chat toggle lives in the floating top-right cluster over
        the surface itself: the terminal's agent-action overlay in terminal
        view, and the chat page's own in-DOM cluster in chat view (a
        gpui-drawn overlay would sit UNDER the native CEF chat view). The tab
        chrome hosts no toggle.
        */
        /*
        CDXC:GlobalActions 2026-08-01-16:00:
        The cluster no longer has a fixed button count: users hide built-in
        buttons they do not use, and Global Actions render as extra icons here.
        Width is therefore derived from what is actually drawn instead of the
        old four-button constant, so the tabs keep exactly the space the
        cluster does not take.

        Global Actions render before the built-ins so the user's own actions sit
        closest to the tabs and the built-in controls stay where muscle memory
        expects them, against the right edge.
        */
        let built_in_buttons = self.tab_strip_built_in_buttons;
        let global_actions = self.sidebar_global_actions.clone();
        let button_count = 1 // pane overflow, never hideable
            + usize::from(built_in_buttons.show_new_terminal)
            + usize::from(built_in_buttons.show_new_browser)
            + global_actions.len();
        let cluster_width = WORKSPACE_TAB_ACTION_BUTTON_WIDTH * button_count as f32 - 1.0;
        h_flex()
            .id(format!("ghostex-gpui-workspace-tab-actions-{}", pane_id.0))
            .flex_shrink_0()
            .h_full()
            .w(px(cluster_width))
            .items_center()
            .justify_center()
            .bg(workspace_tab_action_cluster_color())
            .children(
                global_actions.iter().map(|action| {
                    self.render_workspace_tab_global_action_button(pane_id, action, cx)
                }),
            )
            .when(built_in_buttons.show_new_terminal, |this| {
                this.child(self.render_workspace_tab_action_button(
                    pane_id,
                    "new-terminal",
                    WorkspaceTabActionIcon::NewTerminal,
                    "New Terminal",
                    cx,
                ))
            })
            .when(built_in_buttons.show_new_browser, |this| {
                this.child(self.render_workspace_tab_action_button(
                    pane_id,
                    "new-browser",
                    WorkspaceTabActionIcon::NewBrowser,
                    "New Browser Tab",
                    cx,
                ))
            })
            .child(self.render_workspace_tab_action_button(
                pane_id,
                "overflow",
                WorkspaceTabActionIcon::Overflow,
                "Pane actions menu",
                cx,
            ))
            .into_any_element()
    }

    /*
    CDXC:GlobalActions 2026-08-01-16:00:
    A Global Action button carries only the id back to the sidebar runtime,
    which resolves the trusted saved definition and runs it through the existing
    Action bridge — the same selector-shaped path the Command Palette uses. No
    command text or URL is held by, or dispatched from, the tab strip.
    */
    pub(crate) fn render_workspace_tab_global_action_button(
        &self,
        pane_id: WorkspacePaneId,
        action: &GpuiSidebarGlobalActionState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let command_id = action.command_id.clone();
        let icon_path = gpui_sidebar_command_icon_asset_path(action.icon.as_deref());
        let tooltip = if action.name.is_empty() {
            "Global action".to_string()
        } else {
            action.name.clone()
        };
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-global-action-{}-{}",
                pane_id.0, action.command_id
            ))
            .flex()
            .w(px(WORKSPACE_TAB_ACTION_BUTTON_WIDTH))
            .h(px(WORKSPACE_TAB_ACTION_BUTTON_HEIGHT))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(workspace_tab_action_left_border_color())
            .bg(workspace_tab_action_button_color())
            .cursor_default()
            .hover(|this| this.bg(tab_bar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.dispatch_gpui_run_sidebar_command_with_scope(
                        &command_id,
                        None,
                        Some("global"),
                        cx,
                    );
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
                Tooltip::new(tooltip.clone()).my_0().build(window, cx)
            })
            .child(
                svg()
                    .size(px(WORKSPACE_TAB_ACTION_ICON_SIZE))
                    .path(icon_path)
                    .text_color(workspace_tab_action_icon_color())
                    .into_any_element(),
            )
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_action_button(
        &self,
        pane_id: WorkspacePaneId,
        action: &'static str,
        icon: WorkspaceTabActionIcon,
        tooltip: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-workspace-tab-action-{}-{action}",
                pane_id.0
            ))
            .flex()
            .w(px(if action == "overflow" {
                WORKSPACE_TAB_ACTION_BUTTON_WIDTH - 1.0
            } else {
                WORKSPACE_TAB_ACTION_BUTTON_WIDTH
            }))
            .h(px(WORKSPACE_TAB_ACTION_BUTTON_HEIGHT))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(workspace_tab_action_left_border_color())
            .bg(workspace_tab_action_button_color())
            .cursor_default()
            .hover(|this| this.bg(tab_bar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    match icon {
                        WorkspaceTabActionIcon::NewTerminal => {
                            this.add_agents_registered_terminal_tab(pane_id, cx);
                        }
                        WorkspaceTabActionIcon::NewBrowser => {
                            this.add_browser_tab_from_hotkey(window, cx);
                        }
                        // Win32's native popup tracking loop must start after
                        // GPUI releases the mouse capture from this press. The
                        // matching mouse-up handler opens the menu, like the
                        // Browser pane overflow control below.
                        WorkspaceTabActionIcon::Overflow => {}
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    if !matches!(icon, WorkspaceTabActionIcon::Overflow) {
                        return;
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_agents_pane_actions_menu(pane_id, event.position, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
                Tooltip::new(tooltip).my_0().build(window, cx)
            })
            .child(self.render_workspace_tab_action_icon(icon))
            .into_any_element()
    }

    pub(crate) fn render_workspace_tab_action_icon(&self, icon: WorkspaceTabActionIcon) -> AnyElement {
        let path = match icon {
            WorkspaceTabActionIcon::NewTerminal => COMMAND_ICON_PLUS,
            WorkspaceTabActionIcon::NewBrowser => BROWSER_ICON_WORLD,
            WorkspaceTabActionIcon::Overflow => TITLEBAR_ICON_LAYOUT_BOARD_SPLIT,
        };
        titlebar_svg_icon(
            path,
            WORKSPACE_TAB_ACTION_ICON_SIZE,
            workspace_tab_action_icon_color(),
        )
        .into_any_element()
    }

    pub(crate) fn render_terminal_body_slot(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if let Some(session_id) = leaf.tab_group.active_session_id() {
            /*
            CDXC:GPUISessionChatSurface 2026-07-31:
            Chat mode swaps this tab's body for the per-session chat CEF
            surface in the same slot. The terminal mount is not
            destroyed: skipping the mount-slot render parks the Ghostty
            surface exactly like selecting a different tab, and toggling back
            reattaches through the normal parked-owner path.
            */
            if self.agents_chat_mode_sessions.contains(&session_id) {
                return self.render_agents_session_chat_body(leaf.pane_id, session_id, cx);
            }
            /*
            CDXC:AgentHistorySearch 2026-08-20:
            Find mode swaps this tab's body for the Find CEF surface on the same
            terms as chat: the terminal mount is parked, not destroyed, and
            toggling back reattaches through the normal parked-owner path.
            */
            if self.agents_find_mode_sessions.contains(&session_id) {
                let content = self.render_session_find_surface_content(session_id);
                return self.render_agents_session_chat_body_frame(
                    leaf.pane_id,
                    session_id,
                    content,
                    cx,
                );
            }
        }
        let mount_candidate = self.agents_workspace.terminal_body_mount_candidate(leaf);
        let pane_id = mount_candidate.pane_id;
        let active_session_id = mount_candidate.active_session_id;
        let active_session = active_session_id.and_then(|id| self.agents_workspace.session(id));
        let presentation_state = active_session.map(|session| session.presentation_state);
        let title = active_session
            .map(|session| session.title.clone())
            .unwrap_or_else(|| "Terminal".to_string());
        let session_id = active_session_id.unwrap_or(TerminalSessionId(0));
        let body_state_slug = if mount_candidate.eligible_for_terminal_surface() {
            "libghostty-mount-slot"
        } else {
            mount_candidate
                .presentation
                .element_slug(presentation_state)
        };
        let mount_slot_id = mount_candidate.mount_slot_id();
        /*
        CDXC:GPUITerminalGpuiEngine 2026-07-04:
        A mount slot claimed by the GPUI terminal engine renders the
        composited TerminalView as a normal child of the same body div
        instead of recording bounds for a native host view. The body keeps
        drag/drop ownership, while the child element exclusively owns
        terminal mouse/key/IME/cursor behavior through its own focus handle.
        Do not install the native/placeholder body click listener for these
        slots: it stops propagation before terminal mouse reporting can see
        the click. The shared terminal text service and native canvas probes
        are also skipped for these slots.
        */
        let gpui_engine_view = mount_slot_id
            .and_then(|slot_id| self.agents_gpui_engine_terminals.get(&slot_id.session_id))
            .map(|record| record.view.clone());
        let gpui_engine_owns_pointer_input = gpui_engine_view.is_some();
        let gpui_engine_slot_id = mount_slot_id.filter(|_| gpui_engine_owns_pointer_input);
        let native_mount_slot_id = mount_slot_id.filter(|_| gpui_engine_view.is_none());
        let startup_body_slot_id = active_session_id
            .filter(|_| presentation_state == Some(TerminalSessionPresentationState::Mounting))
            .map(|session_id| AgentsTerminalStartupBodySlotId {
                pane_id,
                session_id,
            })
            .filter(|slot_id| {
                self.agents_workspace
                    .is_current_terminal_startup_body_slot(*slot_id)
            });
        let parked_owner_body_slot_id = active_session_id
            .filter(|_| presentation_state == Some(TerminalSessionPresentationState::Mounting))
            .map(|session_id| AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            })
            .filter(|slot_id| {
                self.agents_workspace
                    .is_current_terminal_parked_owner_body_slot(*slot_id)
            });
        let body_click_action = agents_terminal_body_click_action(mount_candidate, session_id);
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let (terminal_horizontal_padding, terminal_vertical_padding) =
            settings_snapshot.terminal_pane_padding_px();
        let persistence_label = settings_snapshot
            .show_session_id_in_terminal_panes()
            .then(|| {
                active_session
                    .and_then(|session| session.zmx_session_name.as_deref())
                    .map(|name| format!("zmx - {name}"))
            })
            .flatten();
        let sleeping_wake_label = command_pane_sleeping_placeholder_wake_label(
            presentation_state == Some(TerminalSessionPresentationState::Sleeping),
            gpui_click_to_wake_sleeping_sessions_from_shared_settings(&settings_snapshot),
        );
        let is_generating_first_prompt_title =
            active_session.is_some_and(|session| session.is_generating_first_prompt_title);
        let remote_connect_status = self.agents_remote_connect_status_for_session(session_id);

        /*
        CDXC:GPUIWorkspaceLifecycle 2026-06-22-05:23:
        Selecting a sleeping, restored/unmounted, mounting, failed-startup, or popped-out Agents tab is a presentation action only in this slice. The selected placeholder focuses the pane and stays active, but it must not mutate the session to running or imply that wake, mount, materialize, retry, or reattach behavior has been implemented.

        CDXC:GPUIAgentsTerminalActivation 2026-06-22-23:33:
        Placeholder body activation is the explicit GPUI wake/materialize/reattach/retry path for sleeping, restored/unmounted, popped-out, and failed-startup Agents sessions. The activation stays shell-only by changing presentation state to Mounting so pending startup/materialization remains selectable and focusable without fabricating a Running terminal or process.

        CDXC:GPUILibghosttyMountBoundary 2026-06-22-20:14:
        Render the active running Agents body as the future libghostty mount slot only after the pure mount candidate says the selected session is eligible. Ineligible selected states keep the placeholder card and drop behavior, while missing sessions render an honest missing-session placeholder instead of a black fake terminal surface.

        CDXC:GPUILibghosttyMountBoundary 2026-06-22-22:45:
        Every rendered visible selected running Agents leaf is a real libghostty mount slot. Non-running and missing selected states keep placeholder cards, inactive tabs stay hidden, and hidden-by-Focus leaves never record body bounds or own surfaces.

        CDXC:GPUILibghosttyMountBounds 2026-06-22-20:29:
        The future libghostty native view must attach to the exact terminal body slot below the Agents tab bar, not to the pane border or tab chrome. A non-interactive canvas probe records that body rectangle for the current mount identity while the body div keeps click/drop ownership and no hidden hit region is introduced.

        CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
        Visible selected Mounting bodies render a blank terminal body and use the same body click/drop owner, plus a paint-only canvas probe for exact startup body bounds. The probe records only runtime geometry through `AgentsTerminalStartupBodySlotId`; it does not create a Running mount slot, host view, Ghostty surface, overlay, or hit-test route.

        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Visible selected non-startup Mounting placeholders also get a paint-only geometry probe for the parked-owner reattach path. The placeholder body remains the only click/drop owner, and geometry alone never creates startup launch state, new Ghostty surfaces, fallback Running state, overlays, hidden hit regions, or persisted data.

        CDXC:GPUITerminalMouseForwarding 2026-06-23-08:32:
        Mounted Running body events forward left/right/middle down/up plus body-level pointer movement to the same current Ghostty mount slot after the existing focus path. Mouse down/up/move pass GPUI modifiers through the Ghostty input.Mods mapper while missing or stale bounds/surfaces no-op, placeholders keep their activation semantics, and mouse forwarding remains separate from capture policy, text/key/IME delivery, the Cmd+V paste action, logging, persistence, overlays, hidden hit regions, and root/window routing.

        CDXC:GPUITerminalScrollForwarding 2026-06-23-09:32:
        Mounted Running body wheel events forward only from the existing body div for exact current mount slots. Successful forwarding consumes the GPUI scroll event after updating Ghostty's body-relative pointer position with mapped keyboard modifiers; placeholders and stale surfaces still fall through without scroll routing, overlays, hidden hit regions, logging, persistence, or momentum mapping.

        CDXC:GPUITerminalPressureForwarding 2026-06-23-09:51:
        Mounted Running body pressure events forward only inside the existing mounted-slot body branch. Forward pressure without default-stop so placeholder activation and body drag/drop ownership stay unchanged while exact current surfaces receive force-click data.

        CDXC:GPUITerminalMouseCapture 2026-06-23-10:05:
        Mounted Running Agents bodies use GPUI element-level mouse-up-out only to release an already captured Ghostty mouse. The outside event passes modifiers but never updates terminal mouse_pos, clamps coordinates, adds overlays, or routes input through the window.

        CDXC:GPUITerminalMouseButtons 2026-06-23-10:23:
        Mounted Running Agents bodies must forward right and middle down/up/up-out through the same body element as left. Right and middle down focus the mounted Agents terminal surface before forwarding the press, while placeholders keep their existing activation behavior and no tab/chrome right-click menus are changed.

        CDXC:GPUITerminalSelectionDrag 2026-06-23-12:43:
        Mounted Agents selection drag is not separate GPUI behavior beyond the body-scoped Ghostty event stream. The existing body div owns placeholder activation, mounted terminal focus handoff, body-relative move forwarding during a press, in-body release, capture-gated up-out, and typed tab drag-over/drop handlers without overlap; keep any future expansion inside this normal body boundary instead of storing selection text, storing raw coordinates, adding global capture, or routing mouse input through root/window pre-dispatch.

        CDXC:GPUITerminalNativeKeyBridge 2026-07-10:
        Mounted libghostty terminals keep keyboard and IME ownership on their exact AppKit host NSView. The existing body remains the normal mouse/layout owner, but it must not focus GPUI's legacy text service after the native handoff because doing so strips Tab, Option/Alt, arrows, and terminal bindings down to committed-text-only input.

        CDXC:GPUIAgentsSleepingPlaceholder 2026-07-05:
        Sleeping Agents bodies mirror native AppKit sleeping pane placeholders: black body, no diagnostic card, and the same centered paint-only "Press Any Key to Wake" affordance controlled by click-to-wake settings. The existing body click and focused key handlers remain the only wake behavior.
        */
        div()
            .id(format!(
                "ghostex-gpui-terminal-body-slot-{}-{}-{}",
                pane_id.0, session_id.0, body_state_slug
            ))
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .bg(workspace_terminal_body_color(presentation_state))
            .when(
                native_mount_slot_id
                    .is_some_and(|slot_id| self.agents_terminal_slot_hovers_link(slot_id)),
                |this| this.cursor_pointer(),
            )
            .when_some(gpui_engine_slot_id, |this, slot_id| {
                this.capture_any_mouse_down(cx.listener(
                    move |this, _event: &MouseDownEvent, window, cx| {
                        support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.terminalEngine.pointerFocusCapture",
                            serde_json::json!({
                                "surface": "agents",
                                "pane": slot_id.pane_id.0,
                                "session": slot_id.session_id.0,
                                "activeMode": format!("{:?}", this.active_mode),
                                "shellFocusBefore": format!("{:?}", this.shell_focus),
                                "firstResponderBefore": format!("{:?}", this.first_responder_target),
                            }),
                        );
                        this.focus_agents_terminal_mount_slot(slot_id, window, cx);
                        this.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
                    },
                ))
            })
            .when(!gpui_engine_owns_pointer_input, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        match body_click_action {
                            AgentsTerminalBodyClickAction::FocusRunningMountSlot(slot_id) => {
                                this.focus_agents_terminal_mount_slot(slot_id, window, cx);
                                this.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx);
                                let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                                    slot_id,
                                    event.position,
                                    ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                                    event.button,
                                    event.modifiers,
                                );
                            }
                            AgentsTerminalBodyClickAction::ActivatePlaceholder {
                                pane_id,
                                session_id,
                            } => {
                                this.activate_agents_terminal_placeholder(pane_id, session_id, cx);
                            }
                            AgentsTerminalBodyClickAction::None => {}
                        }
                    }),
                )
            })
            .when_some(native_mount_slot_id, |this, slot_id| {
                this.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.focus_agents_terminal_mount_slot(slot_id, window, cx);
                        let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.focus_agents_terminal_mount_slot(slot_id, window, cx);
                        let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_PRESS,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |this, event: &MouseMoveEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_position(
                            slot_id,
                            event.position,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_button(
                            slot_id,
                            event.position,
                            ghostty_kit::ffi::GHOSTTY_MOUSE_RELEASE,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Middle,
                    cx.listener(move |this, event: &MouseUpEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_release_outside(
                            slot_id,
                            event.button,
                            event.modifiers,
                        );
                    }),
                )
                .on_mouse_pressure(cx.listener(
                    move |this, event: &MousePressureEvent, _window, _cx| {
                        let _ = this.forward_agents_terminal_mount_slot_mouse_pressure(
                            slot_id,
                            event.position,
                            event.stage,
                            event.pressure,
                            event.modifiers,
                        );
                    },
                ))
                .on_scroll_wheel(cx.listener(
                    move |this, event: &ScrollWheelEvent, window, cx| {
                        if this.forward_agents_terminal_mount_slot_mouse_scroll(
                            slot_id,
                            event.position,
                            event.delta,
                            event.modifiers,
                        ) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    },
                ))
            })
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_over_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.is::<DraggedWorkspaceTab>() || value.is::<DraggedCommandTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .when(
                mount_candidate.renders_placeholder_child(),
                |this| match mount_candidate.presentation {
                    AgentsTerminalBodyPresentation::LifecyclePlaceholder => {
                        if matches!(
                            presentation_state,
                            Some(
                                TerminalSessionPresentationState::Sleeping
                                    | TerminalSessionPresentationState::Mounting
                            )
                        ) {
                            this
                        } else if let Some(presentation_state) = presentation_state {
                            this.child(self.render_terminal_state_placeholder(
                                pane_id,
                                session_id,
                                title,
                                presentation_state,
                                cx,
                            ))
                        } else {
                            this
                        }
                    }
                    AgentsTerminalBodyPresentation::MissingSessionPlaceholder => {
                        this.child(self.render_terminal_missing_session_placeholder(session_id))
                    }
                    AgentsTerminalBodyPresentation::EmptyWorkspacePlaceholder => this,
                    AgentsTerminalBodyPresentation::MountSlot
                    | AgentsTerminalBodyPresentation::RunningPlaceholder => this,
                },
            )
            .when_some(sleeping_wake_label, |this, label| {
                this.child(
                    canvas(
                        move |bounds, window, _| {
                            command_pane_sleeping_placeholder_wake_label_prepaint(
                                bounds, label, window,
                            )
                        },
                        move |bounds, paint_state, window, cx| {
                            if let Some(paint_state) = paint_state {
                                command_pane_sleeping_placeholder_wake_label_paint(
                                    bounds,
                                    paint_state,
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .when_some(gpui_engine_view, |this, view| {
                this.child(
                    div()
                        .absolute()
                        .left(px(terminal_horizontal_padding))
                        .right(px(terminal_horizontal_padding))
                        .top(px(terminal_vertical_padding))
                        .bottom(px(terminal_vertical_padding))
                        .child(view),
                )
            })
            .when_some(native_mount_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.child({
                    let bounds_view = view.clone();
                    let input_handler_view = view.clone();
                    canvas(
                        move |bounds, window, cx| {
                            let scale_factor = window.scale_factor();
                            let _ = bounds_view.update(cx, |this, cx| {
                                this.record_agents_terminal_mount_slot_bounds(
                                    slot_id,
                                    bounds,
                                    scale_factor,
                                    cx,
                                );
                            });
                        },
                        move |bounds, _, window, cx| {
                            let input_view = input_handler_view.clone();
                            let _ = input_handler_view.update(cx, |this, cx| {
                                this.register_agents_terminal_text_input_handler(
                                    slot_id, bounds, input_view, window, cx,
                                );
                            });
                        },
                    )
                    .absolute()
                    .left(px(terminal_horizontal_padding))
                    .right(px(terminal_horizontal_padding))
                    .top(px(terminal_vertical_padding))
                    .bottom(px(terminal_vertical_padding))
                })
            })
            .when_some(startup_body_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.child(
                    canvas(
                        move |bounds, window, cx| {
                            let scale_factor = window.scale_factor();
                            let _ = view.update(cx, |this, cx| {
                                this.record_agents_terminal_startup_body_slot_bounds(
                                    slot_id,
                                    bounds,
                                    scale_factor,
                                    cx,
                                );
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left(px(terminal_horizontal_padding))
                    .right(px(terminal_horizontal_padding))
                    .top(px(terminal_vertical_padding))
                    .bottom(px(terminal_vertical_padding)),
                )
            })
            .when_some(parked_owner_body_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.child(
                    canvas(
                        move |bounds, window, cx| {
                            let scale_factor = window.scale_factor();
                            let _ = view.update(cx, |this, cx| {
                                this.record_agents_terminal_parked_owner_body_slot_bounds(
                                    slot_id,
                                    bounds,
                                    scale_factor,
                                    cx,
                                );
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left(px(terminal_horizontal_padding))
                    .right(px(terminal_horizontal_padding))
                    .top(px(terminal_vertical_padding))
                    .bottom(px(terminal_vertical_padding)),
                )
            })
            .when_some(persistence_label, |this, label| {
                this.child(
                    div()
                        .absolute()
                        .top(px(6.0))
                        .right(px(3.0))
                        .text_size(px(10.0))
                        .text_color(rgb(0xffffff).opacity(0.24))
                        .child(label),
                )
            })
            .when(is_generating_first_prompt_title, |this| {
                this.child(render_agents_first_prompt_title_overlay(pane_id, session_id))
            })
            .when_some(remote_connect_status, |this, (title, detail)| {
                this.child(render_agents_remote_connect_status_overlay(
                    pane_id, session_id, title, detail,
                ))
            })
            .when_some(self.workspace_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_workspace_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn render_agents_session_chat_body(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Chat owns the same normal-layout workspace body rectangle as a
        terminal: a per-session CefSurface child
        plus ordinary placeholder layout children. No terminal mount canvas,
        native geometry probe, overlay, or hidden hit region participates.
        */
        let content = self.render_session_chat_surface_content(session_id);
        self.render_agents_session_chat_body_frame(pane_id, session_id, content, cx)
    }

    /// The chat surface (or its loading/unavailable placeholder) for one
    /// session — shared by the Agents workspace body and the project-editor
    /// companion slot body.
    pub(crate) fn render_session_chat_surface_content(&self, session_id: TerminalSessionId) -> AnyElement {
        let surface = self.agents_chat_surfaces.get(&session_id).cloned();
        if let Some(surface) = surface {
            div()
                .id(format!("ghostex-gpui-session-chat-cef-{}", session_id.0))
                .relative()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(surface)
                .into_any_element()
        } else {
            let bootstrap_missing = self.sidebar_gxserver_bootstrap.is_none();
            let (title, message) = if bootstrap_missing {
                (
                    "Chat unavailable",
                    "Session Chat needs the local Ghostex server. Start it from the sidebar, then toggle Chat View again.",
                )
            } else {
                ("Loading Chat...", "")
            };
            v_flex()
                .id(format!(
                    "ghostex-gpui-session-chat-placeholder-{}",
                    session_id.0
                ))
                .size_full()
                .min_w_0()
                .min_h_0()
                .items_center()
                .justify_center()
                .bg(gpui_session_chat_background_color())
                .child(
                    v_flex()
                        .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                        .items_center()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(workspace_terminal_placeholder_title_color())
                                .child(title),
                        )
                        .when(!bootstrap_missing, |this| {
                            this.child(
                                canvas(
                                    move |_bounds, _window, _cx| {},
                                    move |bounds, _state: (), window, _cx| {
                                        window.request_animation_frame();
                                        paint_agent_gui_loading_spinner(bounds, window);
                                    },
                                )
                                .size(px(18.0))
                                .mt(px(10.0)),
                            )
                        })
                        .when(bootstrap_missing, |this| {
                            this.child(
                                div()
                                    .mt(px(5.0))
                                    .max_w(px(390.0))
                                    .text_size(px(12.5))
                                    .line_height(px(18.0))
                                    .text_color(workspace_terminal_placeholder_message_color())
                                    .child(message),
                            )
                        }),
                )
                .into_any_element()
        }
    }

    /// The Find surface (or its loading/unavailable placeholder) for one pane.
    pub(crate) fn render_session_find_surface_content(&self, session_id: TerminalSessionId) -> AnyElement {
        let surface = self.agents_find_surfaces.get(&session_id).cloned();
        if let Some(surface) = surface {
            return div()
                .id(format!("ghostex-gpui-find-prompts-cef-{}", session_id.0))
                .relative()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(surface)
                .into_any_element();
        }
        let bootstrap_missing = self.sidebar_gxserver_bootstrap.is_none();
        let (title, message) = if bootstrap_missing {
            (
                "Find unavailable",
                "Searching your prompt history needs the local Ghostex server. Start it from the sidebar, then open Find again.",
            )
        } else {
            ("Loading Find...", "")
        };
        v_flex()
            .id(format!("ghostex-gpui-find-prompts-placeholder-{}", session_id.0))
            .size_full()
            .min_w_0()
            .min_h_0()
            .items_center()
            .justify_center()
            .bg(gpui_session_chat_background_color())
            .child(
                v_flex()
                    .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(workspace_terminal_placeholder_title_color())
                            .child(title),
                    )
                    .when(!bootstrap_missing, |this| {
                        this.child(
                            canvas(
                                move |_bounds, _window, _cx| {},
                                move |bounds, _state: (), window, _cx| {
                                    window.request_animation_frame();
                                    paint_agent_gui_loading_spinner(bounds, window);
                                },
                            )
                            .size(px(18.0))
                            .mt(px(10.0)),
                        )
                    })
                    .when(bootstrap_missing, |this| {
                        this.child(
                            div()
                                .mt(px(5.0))
                                .max_w(px(390.0))
                                .text_size(px(12.5))
                                .line_height(px(18.0))
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(message),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn render_agents_session_chat_body_frame(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-session-chat-body-{}-{}",
                pane_id.0, session_id.0
            ))
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(gpui_session_chat_background_color())
            .on_drag_move::<DraggedWorkspaceTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedWorkspaceTab>, _window, cx| {
                    this.update_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .on_drag_move::<DraggedCommandTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedCommandTab>, _window, cx| {
                    this.update_command_tab_over_workspace_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| {
                value.is::<DraggedWorkspaceTab>() || value.is::<DraggedCommandTab>()
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedWorkspaceTab, window, cx| {
                    this.handle_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .on_drop(
                cx.listener(move |this, dragged: &DraggedCommandTab, window, cx| {
                    this.handle_command_tab_workspace_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .child(content)
            .when_some(self.workspace_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_workspace_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn workspace_pane_drop_zone(&self, pane_id: WorkspacePaneId) -> Option<WorkspaceDropZone> {
        match self.workspace_drop_feedback {
            Some(WorkspaceDropFeedback {
                pane_id: feedback_pane_id,
                target: WorkspaceDropTarget::PaneBody(zone),
            }) if feedback_pane_id == pane_id => Some(zone),
            _ => None,
        }
    }

    pub(crate) fn render_workspace_pane_drop_feedback(
        &self,
        pane_id: WorkspacePaneId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        /*
        CDXC:GPUIWorkspaceDragDrop 2026-06-22-05:31:
        Drag feedback for Agents pane-body drops must be visible but non-interactive. Render the center group or edge split indication as a normal child inside the pane body instead of adding transparent overlap, root hit-test shields, or window-level mouse routing.
        */
        let feedback = div()
            .id(format!(
                "ghostex-gpui-workspace-pane-drop-feedback-{}",
                pane_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full();

        match zone {
            WorkspaceDropZone::Center => feedback
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(agents_drop_feedback_border_color())
                .bg(agents_drop_group_feedback_color())
                .into_any_element(),
            WorkspaceDropZone::Left => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .left_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Right => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .right_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Top => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .top_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Bottom => feedback
                .child(
                    self.render_agents_workspace_drop_edge_band(zone)
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_agents_workspace_drop_edge_band(&self, zone: WorkspaceDropZone) -> gpui::Div {
        let band = div()
            .absolute()
            .border_2()
            .border_color(agents_drop_feedback_border_color())
            .bg(agents_drop_split_feedback_color());

        match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => band
                .w(relative(AGENTS_SPLIT_DROP_PREVIEW_FRACTION))
                .h_full(),
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => band
                .h(relative(AGENTS_SPLIT_DROP_PREVIEW_FRACTION))
                .w_full(),
            WorkspaceDropZone::Center => band.size_full(),
        }
    }

    pub(crate) fn render_workspace_drop_edge_band(
        &self,
        label: &'static str,
        zone: WorkspaceDropZone,
    ) -> gpui::Div {
        let band = div()
            .absolute()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_drop_split_feedback_color())
            .child(self.render_workspace_drop_feedback_label(label, zone));

        match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => {
                band.w(relative(WORKSPACE_DROP_EDGE_BAND_FRACTION)).h_full()
            }
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => {
                band.h(relative(WORKSPACE_DROP_EDGE_BAND_FRACTION)).w_full()
            }
            WorkspaceDropZone::Center => band.size_full(),
        }
    }

    pub(crate) fn render_workspace_drop_feedback_label(
        &self,
        label: &'static str,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        div()
            .flex()
            .h(px(24.0))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(workspace_drop_feedback_border_color())
            .bg(workspace_drop_feedback_label_color(zone))
            .px(px(9.0))
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(workspace_drop_feedback_text_color())
            .child(label)
            .into_any_element()
    }

    pub(crate) fn render_terminal_missing_session_placeholder(
        &self,
        session_id: TerminalSessionId,
    ) -> AnyElement {
        /*
        CDXC:GPUITerminalMissingSessionCopy 2026-06-24-07:38:
        Missing-session visible copy must describe the terminal surface state without exposing source records or private details. Keep this source-only for the parity pass because runtime checks are user-side and validation commands are deferred.
        */
        v_flex()
            .id(format!(
                "ghostex-gpui-terminal-missing-session-placeholder-{}",
                session_id.0
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(0x7f8a99).opacity(0.22))
                    .bg(rgb(0x11151b))
                    .px(px(28.0))
                    .py(px(24.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(rgb(0x7f8a99).opacity(0.16))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xd8dee8).opacity(0.92))
                            .child("Missing"),
                    )
                    .child(
                        div()
                            .mt(px(14.0))
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_title_color())
                            .child("Terminal session unavailable"),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .max_w(px(390.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(
                                "This selected tab has no available terminal session, so no terminal surface can be shown.",
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_terminal_state_placeholder(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        title: String,
        presentation_state: TerminalSessionPresentationState,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(format!(
                "ghostex-gpui-terminal-state-placeholder-{}-{}",
                session_id.0,
                presentation_state.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .max_w(px(WORKSPACE_STATE_PLACEHOLDER_MAX_WIDTH))
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(workspace_terminal_placeholder_border_color(
                        presentation_state,
                    ))
                    .bg(workspace_terminal_placeholder_card_color(
                        presentation_state,
                    ))
                    .px(px(28.0))
                    .py(px(24.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .bg(workspace_terminal_placeholder_badge_background(
                                presentation_state,
                            ))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_badge_text_color(
                                presentation_state,
                            ))
                            .child(presentation_state.placeholder_label()),
                    )
                    .child(
                        div()
                            .mt(px(14.0))
                            .text_size(px(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_title_color())
                            .child(presentation_state.placeholder_title()),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .max_w(px(390.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(presentation_state.placeholder_message()),
                    )
                    .child(
                        div()
                            .mt(px(9.0))
                            .max_w(px(390.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.0))
                            .text_color(workspace_terminal_placeholder_session_color())
                            .child(title),
                    )
                    .child(
                        div()
                            .id(format!(
                                "ghostex-gpui-terminal-state-action-{}-{}",
                                session_id.0,
                                presentation_state.element_slug()
                            ))
                            .flex()
                            .h(px(29.0))
                            .mt(px(18.0))
                            .items_center()
                            .justify_center()
                            .rounded(px(5.0))
                            .border_1()
                            .border_color(workspace_terminal_placeholder_action_border_color(
                                presentation_state,
                            ))
                            .bg(workspace_terminal_placeholder_action_color(
                                presentation_state,
                            ))
                            .px(px(12.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_action_text_color(
                                presentation_state,
                            ))
                            .hover(|this| {
                                this.bg(workspace_terminal_placeholder_action_hover_color(
                                    presentation_state,
                                ))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    this.activate_agents_terminal_placeholder(
                                        pane_id, session_id, cx,
                                    );
                                }),
                            )
                            .child(presentation_state.placeholder_action_label()),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_shell(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Project-editor modes replace the main workspace area while active, but they still flow through the same command-pane wrapper as Agents mode. Browser keeps the existing CEF toolbar/body inside this shell, while Source, Kanban, Automate, and Docs render distinct GPUI-colored placeholders until their direct runtime CEF gates can replace them.

        CDXC:GPUIProjectEditor 2026-06-22-08:15:
        When the companion is hidden, the shell still owns a visible restore rail as a normal left layout sibling before the editor surface. The rail never overlaps the editor surface or Browser CEF child view, and restoring the companion reuses the stored width ratio instead of resetting layout.

        CDXC:GPUIProjectEditorLayout 2026-06-22-17:18:
        Source, Browser, Kanban, and Manage share this horizontal shell, and gpui-component h_flex centers children by default. Override that alignment and make the editor surface slot full-height so placeholders and Browser CEF bodies fill the available workspace height instead of rendering as a centered band with black space above and below.
        */
        let mode_slug = mode.element_slug();
        let surface_border_state = self.project_editor_surface_border_state(mode, window);
        if self.project_editor_shell.left_companion_visible {
            let companion_ratio = project_editor_companion_width_ratio(
                self.project_editor_shell.left_companion_width_ratio,
            );
            let view = cx.entity().clone();
            let surface_view = cx.entity().clone();
            h_flex()
                .on_children_prepainted(move |child_bounds, _window, cx| {
                    let _ = view.update(cx, |this, _cx| {
                        this.record_project_editor_companion_layout_metrics(&child_bounds);
                    });
                })
                .id(format!("ghostex-gpui-project-editor-shell-{}", mode_slug))
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_start()
                .overflow_hidden()
                .bg(project_editor_shell_background_color())
                .child(self.render_project_editor_companion_pane(mode, window, cx))
                .child(self.render_project_editor_companion_divider(mode, cx))
                .child(
                    div()
                        .on_children_prepainted(move |child_bounds, _window, cx| {
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
                        .flex_grow(1.0 - companion_ratio)
                        .flex_shrink_1()
                        .flex_basis(relative(0.0))
                        .h_full()
                        .min_w(px(WORKSPACE_MIN_WIDTH))
                        .min_h_0()
                        .overflow_hidden()
                        .when(mode != TitlebarMode::Browser, |this| {
                            this.border_1()
                                .border_color(workspace_pane_border_color_for_state(
                                    surface_border_state,
                                ))
                        })
                        .child(self.render_project_editor_surface(mode, window, cx)),
                )
                .into_any_element()
        } else {
            let surface_view = cx.entity().clone();
            h_flex()
                .id(format!("ghostex-gpui-project-editor-shell-{}", mode_slug))
                .flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .items_start()
                .overflow_hidden()
                .bg(project_editor_shell_background_color())
                .child(self.render_project_editor_companion_restore_rail(mode, cx))
                .child(
                    div()
                        .on_children_prepainted(move |child_bounds, _window, cx| {
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
                        .h_full()
                        .min_w(px(WORKSPACE_MIN_WIDTH))
                        .min_h_0()
                        .overflow_hidden()
                        .when(mode != TitlebarMode::Browser, |this| {
                            this.border_1()
                                .border_color(workspace_pane_border_color_for_state(
                                    surface_border_state,
                                ))
                        })
                        .child(self.render_project_editor_surface(mode, window, cx)),
                )
                .into_any_element()
        }
    }

    pub(crate) fn render_project_editor_companion_pane(
        &self,
        mode: TitlebarMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_focused = self.shell_focus == ShellFocusTarget::ProjectEditorCompanion(mode);
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
        let border_state = if has_terminal_split {
            WorkspacePaneBorderState::Neutral
        } else {
            self.project_editor_companion_border_state(mode, window)
        };
        let companion_title = self.project_editor_companion_active_title(mode);
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_project_editor_companion_layout_bounds(mode, &child_bounds);
                });
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-pane-{}",
                mode.element_slug()
            ))
            .flex_grow(project_editor_companion_width_ratio(
                self.project_editor_shell.left_companion_width_ratio,
            ))
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w(px(PROJECT_EDITOR_COMPANION_MIN_WIDTH))
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .border_1()
            .border_color(project_editor_companion_border_color_for_state(
                border_state,
            ))
            .bg(workspace_terminal_placeholder_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_companion(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-project-editor-companion-tabbar-{}",
                        mode.element_slug()
                    ))
                    .flex_shrink_0()
                    .h(px(WORKSPACE_TAB_BAR_HEIGHT))
                    .w_full()
                    .items_center()
                    .border_b_1()
                    .border_color(workspace_tab_border_color())
                    .bg(workspace_tab_bar_color())
                    .child(
                        h_flex()
                            .h_full()
                            .w_full()
                            .items_center()
                            .overflow_hidden()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_tab_active_text_color())
                            .child(self.render_project_editor_companion_collapse_button(
                                mode, is_focused, cx,
                            ))
                            .child(
                                div()
                                    .mx(px(8.0))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(companion_title),
                            )
                            .child(self.render_project_editor_companion_split_button(
                                mode, is_focused, cx,
                            )),
                    ),
            )
            .child(self.render_project_editor_companion_terminal_body(mode, window, cx))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_terminal_body(
        &self,
        mode: TitlebarMode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let top_session_id = self.project_editor_companion_terminal_session_id;
        let bottom_session_id = self.project_editor_companion_secondary_terminal_session_id;
        let focused_slot = (bottom_session_id.is_some()
            && self.project_editor_companion_border_state(mode, window)
                == WorkspacePaneBorderState::Focused)
            .then_some(self.project_editor_companion_focused_terminal_slot);
        let split_ratio = self
            .project_editor_shell
            .left_companion_split_ratio
            .clamp(0.1, 0.9);
        let view = cx.entity().clone();
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                if bottom_session_id.is_some() {
                    let _ = view.update(cx, |this, _cx| {
                        this.record_project_editor_companion_split_layout_metrics(&child_bounds);
                    });
                }
            })
            .id(format!(
                "ghostex-gpui-project-editor-companion-terminal-stack-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(self.render_project_editor_companion_terminal_slot_body(
                mode,
                ProjectEditorCompanionTerminalSlot::Top,
                top_session_id,
                if bottom_session_id.is_some() {
                    split_ratio
                } else {
                    1.0
                },
                focused_slot == Some(ProjectEditorCompanionTerminalSlot::Top),
                cx,
            ))
            .when_some(bottom_session_id, |this, session_id| {
                this.child(self.render_project_editor_companion_split_divider(
                    mode,
                    focused_slot.is_none(),
                    cx,
                ))
                .child(self.render_project_editor_companion_terminal_slot_body(
                    mode,
                    ProjectEditorCompanionTerminalSlot::Bottom,
                    Some(session_id),
                    1.0 - split_ratio,
                    focused_slot == Some(ProjectEditorCompanionTerminalSlot::Bottom),
                    cx,
                ))
            })
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_terminal_slot_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: Option<TerminalSessionId>,
        flex_grow: f32,
        show_focus_outline: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUISessionChatSurface 2026-08-02:
        Chat mode swaps this companion slot's terminal body for the same
        per-session chat surface the Agents workspace shows in the same slot;
        the terminal mount parks exactly like an Agents tab in
        chat mode. The way back is the chat page's in-DOM cluster.
        */
        if let Some(session_id) = session_id {
            // CDXC:AgentHistorySearch 2026-08-20: Find swaps the companion slot
            // on the same terms as chat, so both go through one body.
            let pane_surface_content = if self.agents_chat_mode_sessions.contains(&session_id) {
                Some(self.render_session_chat_surface_content(session_id))
            } else if self.agents_find_mode_sessions.contains(&session_id) {
                Some(self.render_session_find_surface_content(session_id))
            } else {
                None
            };
            if let Some(content) = pane_surface_content {
                return self.render_project_editor_companion_pane_surface_body(
                    mode,
                    slot,
                    session_id,
                    flex_grow,
                    show_focus_outline,
                    content,
                    cx,
                );
            }
        }
        let slot_id = session_id
            .map(|session_id| ProjectEditorCompanionTerminalBodyMountSlotId { mode, session_id })
            .filter(|slot_id| {
                self.is_current_project_editor_companion_terminal_body_mount_slot(*slot_id)
            });
        let gpui_engine_view = slot_id
            .and_then(|slot_id| self.agents_gpui_engine_terminals.get(&slot_id.session_id))
            .map(|record| record.view.clone());
        let gpui_engine_owns_pointer_input = gpui_engine_view.is_some();
        let gpui_engine_slot_id = slot_id.filter(|_| gpui_engine_owns_pointer_input);
        let remote_attach_unavailable_message = slot_id.and_then(|slot_id| {
            self.project_editor_companion_remote_attach_unavailable_message(slot_id)
        });
        let native_slot_id = slot_id
            .filter(|_| gpui_engine_view.is_none() && remote_attach_unavailable_message.is_none());
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
        let slot_slug = match slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        };
        let body_id = match slot_id {
            Some(slot_id) => format!(
                "ghostex-gpui-project-editor-companion-terminal-body-{}-{}-{}",
                mode.element_slug(),
                slot_slug,
                slot_id.session_id.0
            ),
            None => format!(
                "ghostex-gpui-project-editor-companion-empty-body-{}-{}",
                mode.element_slug(),
                slot_slug
            ),
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let (terminal_horizontal_padding, terminal_vertical_padding) =
            settings_snapshot.terminal_pane_padding_px();
        let persistence_label = settings_snapshot
            .show_session_id_in_terminal_panes()
            .then(|| {
                session_id.and_then(|session_id| {
                    self.agents_workspace
                        .session(session_id)
                        .and_then(|session| session.zmx_session_name.as_deref())
                        .map(|name| format!("zmx - {name}"))
                })
            })
            .flatten();
        let search_bar = slot_id.and_then(|slot_id| {
            self.render_project_editor_companion_terminal_search_bar(slot_id, cx)
        });
        let terminal_body = div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .bg(workspace_terminal_placeholder_color())
            .when_some(gpui_engine_slot_id, |this, slot_id| {
                this.capture_any_mouse_down(cx.listener(
                    move |this, _event: &MouseDownEvent, window, cx| {
                        support_logs::append(
                            support_logs::GpuiSupportLog::TerminalFocus,
                            "gpui.terminalEngine.pointerFocusCapture",
                            serde_json::json!({
                                "surface": "projectEditorCompanion",
                                "mode": format!("{:?}", mode),
                                "session": slot_id.session_id.0,
                                "activeMode": format!("{:?}", this.active_mode),
                                "shellFocusBefore": format!("{:?}", this.shell_focus),
                                "firstResponderBefore": format!("{:?}", this.first_responder_target),
                            }),
                        );
                        this.focus_project_editor_companion_terminal_session(
                            mode,
                            slot_id.session_id,
                            window,
                            cx,
                        );
                        this.refresh_zmx_persistence_companion_terminal_if_stale(mode, cx);
                        cx.notify();
                    },
                ))
            })
            .when(!gpui_engine_owns_pointer_input, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        if let Some(session_id) = session_id {
                            this.focus_project_editor_companion_terminal_session(
                                mode, session_id, window, cx,
                            );
                        } else {
                            this.focus_project_editor_companion(mode, window, cx);
                        }
                        this.refresh_zmx_persistence_companion_terminal_if_stale(mode, cx);
                        cx.notify();
                    }),
                )
            })
            .when(slot_id.is_none(), |this| {
                this.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(workspace_tab_inactive_text_color())
                        .child("No running terminal"),
                )
            })
            .when_some(remote_attach_unavailable_message, |this, message| {
                this.child(
                    v_flex()
                        .absolute()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .px(px(24.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(workspace_terminal_placeholder_title_color())
                                .child("Remote terminal unavailable"),
                        )
                        .child(
                            div()
                                .mt(px(5.0))
                                .max_w(px(390.0))
                                .text_size(px(12.5))
                                .line_height(px(18.0))
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(message),
                        ),
                )
            })
            .when_some(gpui_engine_view, |this, view| {
                this.child(
                    div()
                        .absolute()
                        .left(px(terminal_horizontal_padding))
                        .right(px(terminal_horizontal_padding))
                        .top(px(terminal_vertical_padding))
                        .bottom(px(terminal_vertical_padding))
                        .child(view),
                )
            })
            .when_some(native_slot_id, |this, slot_id| {
                let view = cx.entity().clone();
                this.on_scroll_wheel(cx.listener(
                    move |this, event: &ScrollWheelEvent, window, cx| {
                        if this.forward_project_editor_companion_terminal_mount_slot_mouse_scroll(
                            slot_id,
                            event.position,
                            event.delta,
                            event.modifiers,
                        ) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    },
                ))
                .child({
                    let bounds_view = view.clone();
                    let input_handler_view = view.clone();
                    canvas(
                        move |bounds, window, cx| {
                            let scale_factor = window.scale_factor();
                            let _ = bounds_view.update(cx, |this, cx| {
                                this.record_project_editor_companion_terminal_mount_slot_bounds(
                                    slot_id,
                                    bounds,
                                    scale_factor,
                                    cx,
                                );
                            });
                        },
                        move |bounds, _, window, cx| {
                            let input_view = input_handler_view.clone();
                            let _ = input_handler_view.update(cx, |this, cx| {
                                this.register_project_editor_companion_terminal_text_input_handler(
                                    slot_id, bounds, input_view, window, cx,
                                );
                            });
                        },
                    )
                    .absolute()
                    .left(px(terminal_horizontal_padding))
                    .right(px(terminal_horizontal_padding))
                    .top(px(terminal_vertical_padding))
                    .bottom(px(terminal_vertical_padding))
                })
            })
            .when_some(persistence_label, |this, label| {
                this.child(
                    div()
                        .absolute()
                        .top(px(6.0))
                        .right(px(3.0))
                        .text_size(px(10.0))
                        .text_color(rgb(0xffffff).opacity(0.24))
                        .child(label),
                )
            });
        v_flex()
            .id(body_id)
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .when(has_terminal_split, |this| {
                this.border_1().border_color(if show_focus_outline {
                    workspace_pane_focused_border_color()
                } else {
                    rgb(0x000000).opacity(0.0).into()
                })
            })
            .bg(workspace_terminal_placeholder_color())
            .when_some(search_bar, |this, search_bar| this.child(search_bar))
            .child(terminal_body)
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_pane_surface_body(
        &self,
        mode: TitlebarMode,
        slot: ProjectEditorCompanionTerminalSlot,
        session_id: TerminalSessionId,
        flex_grow: f32,
        show_focus_outline: bool,
        content: AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let has_terminal_split = self
            .project_editor_companion_secondary_terminal_session_id
            .is_some();
        let slot_slug = match slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-chat-body-{}-{}-{}",
                mode.element_slug(),
                slot_slug,
                session_id.0
            ))
            .relative()
            .flex_grow(flex_grow)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .when(has_terminal_split, |this| {
                this.border_1().border_color(if show_focus_outline {
                    workspace_pane_focused_border_color()
                } else {
                    rgb(0x000000).opacity(0.0).into()
                })
            })
            .bg(gpui_session_chat_background_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.focus_project_editor_companion_terminal_session(
                        mode, session_id, window, cx,
                    );
                    cx.notify();
                }),
            )
            .child(content)
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_split_divider(
        &self,
        mode: TitlebarMode,
        show_separator_line: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        let hover_visible = self.project_editor_companion_split_divider_hover_visible == Some(mode);
        let separator_line_color: Hsla = if show_separator_line {
            rgb(0x6d6d6d).into()
        } else {
            rgb(0x000000).opacity(0.0).into()
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-divider-{}",
                mode.element_slug()
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .w_full()
            .items_center()
            .justify_center()
            .cursor_ns_resize()
            .bg(project_editor_companion_divider_background_color())
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_project_editor_companion_split_divider_hovering(mode, *hovered, cx);
            }))
            .on_mouse_move(
                cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                    this.set_project_editor_companion_split_divider_hovering(mode, true, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_project_editor_companion_split_divider_mouse_down(
                        mode, event, window, cx,
                    );
                }),
            )
            .child(
                div()
                    .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .w_full()
                    .cursor_ns_resize()
                    .bg(separator_line_color),
            )
            .when(hover_visible, |this| {
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
                            format!(
                                "ghostex-gpui-project-editor-companion-split-divider-hover-line-{}",
                                mode.element_slug()
                            ),
                            Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                .with_easing(gpui::ease_out_quint()),
                            |line, delta| line.opacity(delta),
                        ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_split_button(
        &self,
        mode: TitlebarMode,
        is_focused: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let is_split = self.project_editor_shell.left_companion_split_enabled
            && self
                .project_editor_companion_secondary_terminal_session_id
                .is_some();
        let tooltip = if is_split {
            "Show one companion session"
        } else {
            "Split companion vertically"
        };
        let icon = if is_split {
            TITLEBAR_ICON_LAYOUT_SINGLE_PANE
        } else {
            TITLEBAR_ICON_LAYOUT_SPLIT_VERTICAL
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-split-{}",
                mode.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(41.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(rgb(0x252525))
            .text_color(if is_focused {
                workspace_tab_close_active_color()
            } else {
                workspace_tab_close_inactive_color()
            })
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_project_editor_companion_split(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::BelowLeft,
                move |window, cx| Tooltip::new(tooltip).build(window, cx),
            )
            .child(titlebar_svg_icon(
                icon,
                13.0,
                if is_focused {
                    workspace_tab_close_active_color()
                } else {
                    workspace_tab_close_inactive_color()
                },
            ))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_collapse_button(
        &self,
        mode: TitlebarMode,
        is_focused: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let icon_color = if is_focused {
            workspace_tab_close_active_color()
        } else {
            workspace_tab_close_inactive_color()
        };
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-collapse-{}",
                mode.element_slug()
            ))
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(31.0))
            .items_center()
            .justify_center()
            .border_r_1()
            .border_color(rgb(0x252525))
            .text_color(icon_color)
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.hide_project_editor_companion(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                Tooltip::new("Hide companion").build(window, cx)
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_LAYOUT_SIDEBAR_LEFT_COLLAPSE,
                13.0,
                icon_color,
            ))
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_restore_rail(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-companion-restore-rail-{}",
                mode.element_slug()
            ))
            .flex_shrink_0()
            .h_full()
            .w(px(PROJECT_EDITOR_COMPANION_RESTORE_RAIL_WIDTH))
            .items_center()
            .border_r_1()
            .border_t_1()
            .border_color(rgb(0x252525))
            .bg(workspace_tab_bar_color())
            .cursor_default()
            .hover(|this| this.bg(workspace_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.restore_project_editor_companion(mode, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                Tooltip::new("Show companion").build(window, cx)
            })
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .h(px(WORKSPACE_TAB_BAR_HEIGHT))
                    .w_full()
                    .items_center()
                    .justify_center()
                    .child(titlebar_svg_icon(
                        PROJECT_EDITOR_COMPANION_RESTORE_ICON,
                        12.0,
                        rgb(0x737373).into(),
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_companion_divider(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        The project-editor companion boundary is a real reserved layout region between sibling panes. The visible divider is the resize/reset hit target; it persists shell-only companion sizing and does not use invisible overlays or root-level hit-test routing.
        */
        let hover_line_offset =
            (WORKSPACE_SPLIT_HANDLE_THICKNESS - SIDEBAR_DIVIDER_HOVER_LINE_WIDTH) / 2.0;
        let hover_visible = self.project_editor_companion_divider_hover_visible == Some(mode);
        div()
            .id(format!(
                "ghostex-gpui-project-editor-companion-divider-{}",
                mode.element_slug()
            ))
            .relative()
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
            .items_center()
            .justify_center()
            .cursor_ew_resize()
            .bg(project_editor_companion_divider_background_color())
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_project_editor_companion_divider_hovering(mode, *hovered, cx);
            }))
            .on_mouse_move(
                cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                    this.set_project_editor_companion_divider_hovering(mode, true, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.handle_project_editor_companion_divider_mouse_down(
                        mode, event, window, cx,
                    );
                }),
            )
            .child(
                div()
                    .h_full()
                    .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                    .cursor_ew_resize()
                    .bg(project_editor_companion_divider_line_color()),
            )
            .when(hover_visible, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(hover_line_offset))
                        .h_full()
                        .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                        .cursor_ew_resize()
                        .bg(sidebar_divider_hover_line_color())
                        .with_animation(
                            format!(
                                "ghostex-gpui-project-editor-companion-divider-hover-line-{}",
                                mode.element_slug()
                            ),
                            Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                .with_easing(gpui::ease_out_quint()),
                            |line, delta| line.opacity(delta),
                        ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_project_editor_surface(
        &mut self,
        mode: TitlebarMode,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if mode.is_project_editor_mode() && !self.project_editor_shell.is_mode_awake(mode) {
            return self.render_project_editor_sleeping_placeholder(mode, cx);
        }

        match mode {
            TitlebarMode::Agents => self.render_agents_workspace(window, cx),
            TitlebarMode::Browser => self.render_browser_workspace(window, cx),
            TitlebarMode::Source => self.render_source_workarea_surface(cx),
            TitlebarMode::Kanban => self.render_kanban_workarea_surface(cx),
            TitlebarMode::Automate => self.render_automate_workarea_surface(cx),
            TitlebarMode::Manage => self.render_manage_workarea_surface(cx),
        }
    }

    pub(crate) fn render_project_workarea_runtime_cef_surface(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        surface: Entity<CefSurface>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Real Source, Kanban, Automate, and Manage CEF panes render as normal-layout GPUI children only after the app-owned slot already has a CefSurface and the corresponding gate permits placeholder replacement. Focus uses the existing project-editor surface path; no overlay, hidden child view, hit-test routing, WKWebView/WebKit path, temporary page, or fallback URL is involved.
        */
        let mode = slot_key.titlebar_mode();
        div()
            .id(format!(
                "ghostex-gpui-project-workarea-runtime-cef-surface-{}",
                slot_key.privacy_label()
            ))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(if slot_key == ProjectWorkareaCefSurfaceSlotKey::Source {
                source_view_background_color()
            } else {
                workspace_terminal_placeholder_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                }),
            )
            .child(surface)
            .into_any_element()
    }

    pub(crate) fn source_workarea_placeholder_signature(&self) -> ProjectEditorPlaceholderSignature {
        let fallback = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Source)
            .expect("Source placeholder signature must exist");
        let Some(snapshot) = self.latest_sidebar_project_snapshot.as_ref() else {
            return fallback;
        };
        let Some(target) = self.source_code_server_runtime_target(snapshot) else {
            return fallback;
        };
        let settings = SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(
            &self.sidebar_runtime_settings_snapshot,
        );
        if self.source_code_server_runtime.target.as_ref() != Some(&target)
            || self.source_code_server_runtime.settings.as_ref() != Some(&settings)
        {
            return fallback;
        }
        match self.source_code_server_runtime.state {
            SourceCodeServerRuntimeLaunchState::InstallRequired => {
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: None,
                    message: SOURCE_CODE_SERVER_INSTALL_PROMPT.to_string(),
                    actions: vec![
                        ProjectEditorPlaceholderAction::HideCodeViewTab,
                        ProjectEditorPlaceholderAction::InstallSourceComponent,
                    ],
                };
            }
            SourceCodeServerRuntimeLaunchState::Installing => {
                let message = match self.source_code_server_runtime.install_progress {
                    Some(component_store::ComponentStoreProgressPhase::Checking) => {
                        "Checking the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Downloading) => {
                        "Downloading the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Verifying) => {
                        "Verifying the download…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Installing) => {
                        "Installing the component…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Pruning) => {
                        "Finishing installation…"
                    }
                    Some(component_store::ComponentStoreProgressPhase::Ready) | None => {
                        "Preparing Source…"
                    }
                };
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: Some("Installing VS Code IDE component".to_string()),
                    message: message.to_string(),
                    actions: Vec::new(),
                };
            }
            SourceCodeServerRuntimeLaunchState::Failed => {
                return ProjectEditorPlaceholderSignature {
                    mode: TitlebarMode::Source,
                    title: Some("Source needs another try".to_string()),
                    message: self
                        .source_code_server_runtime
                        .failure
                        .unwrap_or(SourceCodeServerRuntimeFailure::Launch)
                        .placeholder_message()
                        .to_string(),
                    actions: vec![ProjectEditorPlaceholderAction::RetrySourceLoad],
                };
            }
            _ => {}
        }
        ProjectEditorPlaceholderSignature::for_source_code_server_launch_state(
            self.source_code_server_runtime.state,
            self.source_code_server_runtime
                .started_at
                .map(|started_at| started_at.elapsed()),
        )
        .unwrap_or(fallback)
    }

    pub(crate) fn render_source_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUISourceWorkarea 2026-06-23-12:16:
        Source has its own render dispatch instead of sharing the generic Kanban/Automate/Manage placeholder branch. Source must not synthesize readiness from project paths, URLs, localhost values, or native constants.

        CDXC:GPUISourceWorkarea 2026-06-23-12:25:
        The normal sidebar project payload can provide sourceWorkareaId, but missing or malformed Source identity remains a placeholder-only block. Do not recover by inventing Source ids or readiness from paths, titles, fixture names, group ids, filesystem probes, URLs, or localhost constants.

        CDXC:GPUISourceWorkarea 2026-06-23-14:41:
        Loading and load-failed Source code-server states may alter only the static placeholder title/message. Runtime states still cannot create fallback URLs, logs, overlays, or private shell-state fields from the render path.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Source readiness messages no longer make the runtime available. Only the app-owned code-server state for the current explicit project target can drive loading/error placeholder copy, and only a direct runtime URL plus owned CEF surface can replace the placeholder.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Source now checks the permanent app-owned CEF surface map first. When a real Source runtime URL has already produced an owned CefSurface and the gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real URL/process/surface authority is still absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Source render no longer constructs source-proof CEF/code-server objects. The placeholder changes only when the direct runtime URL gate plus an owned normal-layout CefSurface already exist for the current explicit project.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Source placeholder loading/error copy now comes only from the app-owned code-server runtime target for the current sidebar snapshot. Legacy Source readiness messages are compatibility no-ops and cannot make Source ready or failed.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Source;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = self.source_workarea_placeholder_signature();
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_kanban_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Kanban now checks the permanent app-owned CEF surface map first. When a real Kanban runtime URL has already produced an owned CefSurface and the gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real navigable URL authority is absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Kanban render no longer builds source-proof CEF mount objects. The placeholder changes only when the direct bundled runtime URL gate plus an owned normal-layout CefSurface already exist for the current explicit project.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Kanban has no readiness store in the render path. If the direct URL/owned-CEF gate cannot produce a surface, render the static Kanban placeholder and let the active awake runtime edge try creation.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Kanban;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Kanban)
            .expect("Kanban placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_automate_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIAutomateWorkarea 2026-07-04-23:18:
        Automate uses the bundled Kanban/tasks page as a first-party CEF workarea with `surface=automations`, matching macOS. It may replace the placeholder only through the same direct runtime URL plus owned CEF surface gate as Kanban; Quick/projectless contexts and missing Automate identity stay on the static placeholder.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Automate;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Automate)
            .expect("Automate placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_manage_workarea_surface(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        /*
        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        Manage now checks the permanent app-owned CEF surface map first. When a real Manage runtime URL has already produced an owned CefSurface and the CEF/file-bridge gate permits replacement, render returns that normal-layout CEF child; otherwise the placeholder remains because real navigable URL and file-bridge authority are absent.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-28-17:09:
        Manage render no longer builds source-proof CEF/file-bridge mount objects. The placeholder changes only when the direct bundled runtime URL gate plus an owned normal-layout CefSurface already exist; file operations remain owned by the separate sanitized Manage bridge path.

        CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
        Manage has no readiness/file-proof store in the render path. If the direct URL/owned-CEF gate cannot produce a surface, render the static Manage placeholder while first-party file requests remain handled by the project-workarea CEF bridge.
        */
        let slot_key = ProjectWorkareaCefSurfaceSlotKey::Manage;
        if let Some(surface) = self.project_workarea_runtime_cef_surface_for_render(slot_key) {
            return self.render_project_workarea_runtime_cef_surface(slot_key, surface, cx);
        }
        let signature = ProjectEditorPlaceholderSignature::for_mode(TitlebarMode::Manage)
            .expect("Docs placeholder signature must exist");
        self.render_project_editor_placeholder(signature, cx)
    }

    pub(crate) fn render_project_editor_sleeping_placeholder(
        &self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let signature = ProjectEditorSleepingPlaceholderSignature::for_mode(mode)
            .expect("selected sleeping project-editor placeholders exclude Agents");
        let mode = signature.mode;

        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-sleeping-placeholder-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .bg(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else {
                workspace_background_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                v_flex()
                    .max_w(px(430.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .text_center()
                    .child(
                        div()
                            .text_center()
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(signature.title),
                    )
                    .child(
                        div()
                            .mt(px(5.0))
                            .max_w(px(430.0))
                            .text_center()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(workspace_terminal_placeholder_message_color())
                            .child(signature.message),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_project_editor_placeholder(
        &self,
        signature: ProjectEditorPlaceholderSignature,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let ProjectEditorPlaceholderSignature {
            mode,
            title,
            message,
            actions,
        } = signature;
        let has_title = title.is_some();
        let has_actions = !actions.is_empty();
        let mut action_row = h_flex()
            .mt(px(16.0))
            .items_center()
            .justify_center()
            .gap(px(8.0));
        for action in actions {
            let (id, label) = match action {
                ProjectEditorPlaceholderAction::HideCodeViewTab => {
                    ("ghostex-gpui-source-hide-code-tab", "Hide “Code” tab")
                }
                ProjectEditorPlaceholderAction::InstallSourceComponent => {
                    ("ghostex-gpui-source-install-component", "Install")
                }
                ProjectEditorPlaceholderAction::RetrySourceLoad => {
                    ("ghostex-gpui-source-load-retry", "Retry")
                }
            };
            action_row = action_row.child(
                div()
                    .id(id)
                    .flex()
                    .h(px(29.0))
                    .items_center()
                    .justify_center()
                    .rounded(px(5.0))
                    .border_1()
                    .border_color(rgb(0xffffff).opacity(0.18))
                    .bg(
                        if action == ProjectEditorPlaceholderAction::InstallSourceComponent {
                            rgb(0xffffff).opacity(0.14)
                        } else {
                            rgb(0xffffff).opacity(0.08)
                        },
                    )
                    .px(px(12.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff).opacity(0.9))
                    .cursor_pointer()
                    .hover(|this| this.bg(rgb(0xffffff).opacity(0.18)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            match action {
                                ProjectEditorPlaceholderAction::HideCodeViewTab => {
                                    this.hide_code_view_tab(cx);
                                }
                                ProjectEditorPlaceholderAction::InstallSourceComponent => {
                                    this.install_source_code_server_component(cx);
                                }
                                ProjectEditorPlaceholderAction::RetrySourceLoad => {
                                    this.retry_source_code_server_load(cx);
                                }
                            }
                        }),
                    )
                    .child(label),
            );
        }
        v_flex()
            .id(format!(
                "ghostex-gpui-project-editor-placeholder-{}",
                mode.element_slug()
            ))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .bg(if mode == TitlebarMode::Source {
                source_view_background_color()
            } else {
                workspace_background_color()
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_project_editor_surface(mode, window, cx);
                    cx.notify();
                }),
            )
            .child(
                v_flex()
                    .max_w(px(430.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .px(px(24.0))
                    .text_center()
                    .when_some(title, |this, title| {
                        this.child(
                            div()
                                .text_center()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(title),
                        )
                    })
                    .when(!message.is_empty(), |this| {
                        this.child(
                            div()
                                .when(has_title, |this| this.mt(px(5.0)))
                                .max_w(px(430.0))
                                .text_center()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(workspace_terminal_placeholder_message_color())
                                .child(message),
                        )
                    })
                    .when(has_actions, |this| this.child(action_row)),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.sync_browser_address_inputs(window, cx);
        self.sync_browser_find_inputs(window, cx);
        v_flex()
            .flex_1()
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .bg(browser_toolbar_background())
            .child(self.render_browser_node(&self.browser_tabs.root, window, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_node(
        &self,
        node: &BrowserNode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match node {
            BrowserNode::Split(split) => self.render_browser_split(split, window, cx),
            BrowserNode::Leaf(leaf) => self.render_browser_leaf(leaf, window, cx),
        }
    }

    pub(crate) fn render_browser_split(
        &self,
        split: &BrowserSplit,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        let ratio = workspace_split_ratio(split.ratio);
        let first = div()
            .id(format!("ghostex-gpui-browser-split-{}-first", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_browser_node(&split.first, window, cx));
        let second = div()
            .id(format!("ghostex-gpui-browser-split-{}-second", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(1.0 - ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_browser_node(&split.second, window, cx));

        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser split panes are normal non-overlapping layout siblings. Split creation and persistence stay shell-owned while rendered leaves may attach existing tab-owned CEF bodies without adding overlays or hidden hit regions.

        CDXC:GPUIBrowserSplitResize 2026-06-22-09:05:
        Browser split containers report first/handle/second child bounds from normal GPUI layout before resize starts. The visible handle is the actual drag target, matching Agents workspace and command-pane split behavior without transparent overlays, root hit-test routing, or hidden drag regions.
        */
        match split.axis {
            WorkspaceSplitAxis::Horizontal => {
                let view = cx.entity().clone();
                h_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_browser_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-browser-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .items_start()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_browser_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
            WorkspaceSplitAxis::Vertical => {
                let view = cx.entity().clone();
                v_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_browser_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-browser-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_browser_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
        }
    }

    pub(crate) fn render_browser_split_handle(
        &self,
        split: &BrowserSplit,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        match split.axis {
            WorkspaceSplitAxis::Horizontal => div()
                .id(format!("ghostex-gpui-browser-split-handle-{}", split_id.0))
                .flex()
                .flex_shrink_0()
                .h_full()
                .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .items_center()
                .justify_center()
                .cursor_ew_resize()
                .bg(workspace_split_handle_color())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_browser_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h_full()
                        .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .cursor_ew_resize()
                        .bg(browser_split_separator_color()),
                )
                .into_any_element(),
            WorkspaceSplitAxis::Vertical => div()
                .id(format!("ghostex-gpui-browser-split-handle-{}", split_id.0))
                .flex()
                .flex_shrink_0()
                .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .w_full()
                .items_center()
                .justify_center()
                .cursor_ns_resize()
                .bg(workspace_split_handle_color())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_browser_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .w_full()
                        .cursor_ns_resize()
                        .bg(browser_split_separator_color()),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_browser_leaf(
        &self,
        leaf: &BrowserLeaf,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pane_id = leaf.pane_id;
        let border_state = self.browser_leaf_border_state(leaf, window);
        let view = cx.entity().clone();

        /*
        CDXC:GPUIBrowserKeyboardFocus 2026-06-22-09:24:
        Browser split leaf panes report focus geometry from their actual rendered tab-strip/body children. Directional keyboard focus must treat Browser split placeholders and existing visible Browser CEF bodies as real panes, so geometry stays runtime-only and comes from normal layout rather than overlays or hit-test routing.
        */
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_browser_leaf_layout_bounds(pane_id, &child_bounds);
                });
            })
            .id(format!("ghostex-gpui-browser-pane-{}", pane_id.0))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .border_1()
            .border_color(workspace_pane_border_color_for_state(border_state))
            .bg(workspace_terminal_placeholder_color())
            .child(self.render_browser_toolbar(pane_id, cx))
            .when_some(
                self.render_browser_media_permission_bar(leaf, cx),
                |this, permission_bar| this.child(permission_bar),
            )
            .when_some(self.render_browser_find_bar(leaf, cx), |this, find_bar| {
                this.child(find_bar)
            })
            .child(self.render_browser_body(leaf, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_find_bar(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let tab_id = leaf.tab_group.active_tab_id()?;
        let find = self.browser_find_states.get(&tab_id)?;
        let input = self.browser_find_inputs.get(&tab_id).cloned();
        let count_label = browser_find_count_label(find);
        let element_id_suffix = format!("{}-{}", leaf.pane_id.0, tab_id.0);

        Some(
            h_flex()
                .id(format!("ghostex-gpui-browser-find-row-{element_id_suffix}"))
                .flex_shrink_0()
                .w_full()
                .h(px(FIND_BAR_HEIGHT))
                .items_center()
                .justify_end()
                .pl(px(8.0))
                .bg(terminal_search_bar_row_color())
                .border_b_1()
                .border_color(terminal_search_bar_divider_color())
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            cx.stop_propagation();
                            this.close_browser_find(tab_id, window, cx);
                        }
                        "up" => {
                            cx.stop_propagation();
                            let _ = this.perform_browser_find_navigation(tab_id, false, cx);
                        }
                        "down" => {
                            cx.stop_propagation();
                            let _ = this.perform_browser_find_navigation(tab_id, true, cx);
                        }
                        _ => {}
                    }
                }))
                .child(
                    h_flex()
                        .id(format!("ghostex-gpui-browser-find-bar-{element_id_suffix}"))
                        .w(px(300.0))
                        .max_w_full()
                        .h_full()
                        .items_center()
                        .gap(px(4.0))
                        .border_l_1()
                        .border_color(terminal_search_bar_border_color())
                        .bg(terminal_search_bar_background_color())
                        .pl(px(9.0))
                        .when_some(input, |this, input| {
                            this.child(
                                div().flex_1().min_w_0().overflow_hidden().child(
                                    Input::new(&input)
                                        .with_size(ComponentSize::XSmall)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .px(px(0.0))
                                        .py(px(0.0))
                                        .text_size(px(13.0))
                                        .text_color(terminal_search_bar_text_color()),
                                ),
                            )
                        })
                        .when(!count_label.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(terminal_search_bar_count_color())
                                    .child(count_label),
                            )
                        })
                        .child(
                            h_flex()
                                .h_full()
                                .flex_shrink_0()
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-prev-{element_id_suffix}"),
                                    "↑",
                                    FIND_BAR_NAV_BUTTON_WIDTH,
                                    move |this, _window, cx| {
                                        let _ =
                                            this.perform_browser_find_navigation(tab_id, false, cx);
                                    },
                                    cx,
                                ))
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-next-{element_id_suffix}"),
                                    "↓",
                                    FIND_BAR_NAV_BUTTON_WIDTH,
                                    move |this, _window, cx| {
                                        let _ =
                                            this.perform_browser_find_navigation(tab_id, true, cx);
                                    },
                                    cx,
                                ))
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-close-{element_id_suffix}"),
                                    "✕",
                                    FIND_BAR_CLOSE_BUTTON_WIDTH,
                                    move |this, window, cx| {
                                        this.close_browser_find(tab_id, window, cx);
                                    },
                                    cx,
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_browser_media_permission_bar(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        /*
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
        The prompt is a real chrome row owned by the pane's normal layout, like
        the find bar: it shrinks the CEF child view instead of floating over it,
        so nothing overlaps the page and no hit-test routing is involved.
        */
        let tab_id = leaf.tab_group.active_tab_id()?;
        let prompt = self.browser_media_permission_prompt_for_tab(tab_id)?;
        let element_id_suffix = format!("{}-{}", leaf.pane_id.0, tab_id.0);
        let icon = if prompt.pending.microphone {
            BROWSER_ICON_MICROPHONE
        } else {
            BROWSER_ICON_CAMERA
        };
        let message = format!(
            "{} wants to use {}",
            gpui_browser_media_permission_display_origin(&prompt.origin),
            gpui_browser_media_permission_kinds_label(prompt.pending),
        );

        Some(
            h_flex()
                .id(format!(
                    "ghostex-gpui-browser-media-permission-bar-{element_id_suffix}"
                ))
                .flex_shrink_0()
                .w_full()
                .h(px(BROWSER_MEDIA_PERMISSION_BAR_HEIGHT))
                .items_center()
                .gap(px(8.0))
                .px(px(BROWSER_TOOLBAR_HORIZONTAL_PADDING))
                .bg(browser_toolbar_background())
                .border_b_1()
                .border_color(rgb(0x252525))
                .child(titlebar_svg_icon(
                    icon,
                    15.0,
                    browser_toolbar_button_icon_color(),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.5))
                        .text_color(rgb(0xffffff).opacity(0.88))
                        .child(message),
                )
                .child(self.render_browser_media_permission_button(
                    format!("ghostex-gpui-browser-media-permission-block-{element_id_suffix}"),
                    "Block",
                    false,
                    move |this, _window, cx| {
                        this.resolve_browser_media_permission_prompt(tab_id, false, cx);
                    },
                    cx,
                ))
                .child(self.render_browser_media_permission_button(
                    format!("ghostex-gpui-browser-media-permission-allow-{element_id_suffix}"),
                    "Allow",
                    true,
                    move |this, _window, cx| {
                        this.resolve_browser_media_permission_prompt(tab_id, true, cx);
                    },
                    cx,
                ))
                .into_any_element(),
        )
    }

    pub(crate) fn render_browser_media_permission_button(
        &self,
        id: String,
        label: &'static str,
        primary: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (background, hover_background, border, text) = if primary {
            (0.16, 0.22, 0.28, 0.95)
        } else {
            (0.06, 0.11, 0.16, 0.8)
        };

        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .h(px(24.0))
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .border_1()
            .border_color(rgb(0xffffff).opacity(border))
            .bg(rgb(0xffffff).opacity(background))
            .px(px(12.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffffff).opacity(text))
            .cursor_pointer()
            .hover(|this| this.bg(rgb(0xffffff).opacity(hover_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_click(this, window, cx);
                }),
            )
            .child(label)
            .into_any_element()
    }

    pub(crate) fn render_browser_body(&self, leaf: &BrowserLeaf, cx: &mut gpui::Context<Self>) -> AnyElement {
        let pane_id = leaf.pane_id;
        let placeholder = self.browser_body_placeholder_for_leaf(leaf);
        let active_browser_surface = self.browser_surface_for_rendered_leaf(leaf);
        let render_empty_body = active_browser_surface.is_none();
        div()
            .id(format!("ghostex-gpui-browser-body-{}", pane_id.0))
            .relative()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(workspace_terminal_placeholder_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_browser_pane(pane_id, window, cx);
                }),
            )
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| value.is::<DraggedBrowserTab>())
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .when_some(active_browser_surface, |this, browser| this.child(browser))
            .when(render_empty_body, |this| {
                this.child(self.render_browser_placeholder_body(pane_id, placeholder))
            })
            .when_some(self.browser_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_browser_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn browser_body_placeholder_for_leaf(&self, leaf: &BrowserLeaf) -> BrowserBodyPlaceholder {
        leaf.tab_group
            .active_tab_id()
            .and_then(|tab_id| self.browser_tabs.tab(tab_id))
            .map(|tab| {
                BrowserBodyPlaceholder::from_tab(tab, self.browser_surfaces.contains_key(&tab.id))
            })
            .unwrap_or_else(BrowserBodyPlaceholder::blank)
    }

    pub(crate) fn render_browser_placeholder_body(
        &self,
        pane_id: BrowserPaneId,
        placeholder: BrowserBodyPlaceholder,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Address-only Browser tabs are real shell tabs but not real page surfaces yet. Render an empty black GPUI body for those tabs so creating or selecting a new tab never exposes the previous tab's CEF page.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser split panes remain visible shell panes even when their active loaded tab has no existing CEF entity. Render those loaded bodies as restored/sleeping placeholders while preserving their tab groups and selected tab ids for later focused activation or wake materialization.

        CDXC:GPUIBrowserRestoredPlaceholder 2026-06-22-13:38:
        Restored loaded Browser tabs are shell metadata until focus materializes their tab-owned CEF surface. Render a visible sleeping placeholder from sanitized shell URL state only: no CEF creation from render, runtime page titles, query strings, fragments, credentials, local paths, tokens, cookies, or user content.
        */
        if placeholder.state == BrowserTabState::Loaded && !placeholder.has_cef_surface {
            return self.render_browser_restored_placeholder_body(pane_id, placeholder);
        }

        div()
            .id(format!(
                "ghostex-gpui-browser-placeholder-body-{}",
                pane_id.0
            ))
            .size_full()
            .bg(workspace_terminal_placeholder_color())
            .into_any_element()
    }

    pub(crate) fn render_browser_restored_placeholder_body(
        &self,
        pane_id: BrowserPaneId,
        placeholder: BrowserBodyPlaceholder,
    ) -> AnyElement {
        let title = placeholder
            .safe_title
            .unwrap_or_else(|| "Restored Browser tab".to_string());

        v_flex()
            .id(format!(
                "ghostex-gpui-browser-restored-placeholder-body-{}",
                pane_id.0
            ))
            .size_full()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(0x1f1f1f))
            .bg(rgb(0x000000))
            .px(px(32.0))
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(640.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px(540.0))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf2f2f2))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.5))
                            .text_color(rgb(0x8f8f8f))
                            .child("Click to load tab"),
                    ),
            )
            .into_any_element()
    }

    #[allow(dead_code)] // no caller: the browser tab strip is drawn by the CEF browser chrome; this native gpui strip (and everything it calls) is the superseded implementation
    pub(crate) fn render_browser_tab_strip(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-05:56:
        Browser mode needs native-style tabs above the address toolbar while GPUI owns Browser tab identity. Render the strip as a real top chrome row in normal layout, keep tab overflow horizontally scrollable, and reserve only an in-memory new-tab control in this slice.

        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Each rendered Browser pane's active loaded tab selects which tab-owned CEF entity may occupy that pane's body. Inactive tabs retain their shell identity and any runtime CEF entity, but their native views are hidden instead of being stacked under another tab.

        CDXC:GPUIBrowserDragDrop 2026-06-22-07:41:
        Browser tabs need Agents/command-style typed GPUI drag within this single tab strip only. Render a visible insertion marker at the computed tab boundary plus a real end-of-strip drop target, and leave Browser body edge drops and cross-pane Browser splitting out of this slice.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser tab strips now belong to Browser panes, not one flat workspace strip. Each pane owns its tab order and active selection, while the shared toolbar follows the focused pane's active tab and CEF ownership stays keyed by BrowserTabId.
        */
        let pane_id = leaf.pane_id;
        let scroll_handle = self.browser_tab_scroll_handle(pane_id);

        h_flex()
            .id(format!("ghostex-gpui-browser-tabbar-{}", pane_id.0))
            .flex_shrink_0()
            .h(px(BROWSER_TAB_BAR_HEIGHT))
            .w_full()
            .items_center()
            .overflow_hidden()
            .border_b_1()
            .border_color(browser_tab_separator_color())
            .bg(browser_tab_bar_color())
            .child(
                h_flex()
                    .id(format!("ghostex-gpui-browser-tabstrip-{}", pane_id.0))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(BROWSER_TAB_GAP))
                    .overflow_x_scroll()
                    .track_scroll(&scroll_handle)
                    .children(
                        leaf.tab_group
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(tab_index, tab)| {
                                self.render_browser_tab(
                                    pane_id,
                                    &leaf.tab_group,
                                    tab_index,
                                    tab,
                                    cx,
                                )
                            }),
                    )
                    .child(self.render_browser_tab_strip_end_drop_target(
                        pane_id,
                        leaf.tab_group.tabs.len(),
                        cx,
                    )),
            )
            .child(self.render_browser_tab_action_cluster(pane_id, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_tab(
        &self,
        pane_id: BrowserPaneId,
        tab_group: &BrowserTabGroup,
        tab_index: usize,
        pane_tab: &BrowserPaneTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tab_id = pane_tab.tab_id;
        let tab = self.browser_tabs.tab(tab_id);
        let state = tab
            .map(|tab| tab.state)
            .unwrap_or(BrowserTabState::AddressOnly);
        let has_cef_surface = self.browser_surfaces.contains_key(&tab_id);
        let chrome_signature =
            browser_tab_chrome_signature(tab_group, tab_id, state, has_cef_surface);
        let is_active = chrome_signature.active_in_tab_group;
        let state = chrome_signature.state;
        let chrome_status = chrome_signature.chrome_status;
        let display_title = tab
            .map(BrowserTab::display_title)
            .unwrap_or_else(|| "New Tab".to_string());
        let runtime_favicon_url = tab.and_then(|tab| tab.runtime_favicon_url.as_deref());
        let runtime_favicon_image = tab.and_then(|tab| tab.runtime_favicon_image.clone());
        let runtime_favicon_fetch = tab.and_then(|tab| tab.runtime_favicon_fetch.clone());
        let profile_id = tab
            .map(|tab| tab.profile_id)
            .unwrap_or_else(BrowserProfileId::default_profile);
        let dragged_tab = DraggedBrowserTab {
            source_pane_id: pane_id,
            tab_id,
            profile_id,
            title: display_title.clone(),
            runtime_favicon_url: runtime_favicon_url.map(str::to_string),
            runtime_favicon_image: runtime_favicon_image.clone(),
            runtime_favicon_fetch: runtime_favicon_fetch.clone(),
            state,
            chrome_status,
        };
        let view = cx.entity().clone();
        let show_insertion_marker = self.browser_tab_drop_feedback
            == Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::TabStrip(tab_index),
            });
        let tab_hover_key = BrowserHoverTab { pane_id, tab_id };
        let is_tab_hovered = self.hovered_browser_tab == Some(tab_hover_key);
        let can_close = state != BrowserTabState::AddressOnly || tab_group.tabs.len() > 1;
        let show_close_button = can_close && is_tab_hovered;
        let tab_tooltip = display_title.clone();

        div()
            .id(format!(
                "ghostex-gpui-browser-tab-{}-{}",
                pane_id.0, tab_id.0
            ))
            .relative()
            .flex()
            .flex_shrink_1()
            .h_full()
            .w(px(BROWSER_TAB_MAX_WIDTH))
            .min_w(px(BROWSER_TAB_MIN_WIDTH))
            .max_w(px(BROWSER_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .pl(px(8.0))
            .pr(px(4.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(browser_tab_text_color(state, is_active))
            .cursor_default()
            .bg(if is_active {
                browser_tab_active_color()
            } else {
                browser_tab_inactive_color()
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                Tooltip::new(tab_tooltip.clone()).build(window, cx)
            })
            .when(show_insertion_marker, |this| {
                this.child(self.render_browser_tab_insertion_marker(pane_id, tab_index, "before"))
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_browser_tab_hovered(tab_hover_key, *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.select_browser_tab_in_pane(pane_id, tab_id, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_browser_tab_context_menu(pane_id, tab_id, event.position, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if can_close {
                        this.close_browser_tab(tab_id, window, cx);
                    }
                }),
            )
            .on_drag(dragged_tab, move |dragged, _offset, _window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.begin_browser_tab_drag(cx);
                });
                cx.new(|_| BrowserTabDragPreview {
                    profile_id: dragged.profile_id,
                    title: dragged.title.clone(),
                    runtime_favicon_url: dragged.runtime_favicon_url.clone(),
                    runtime_favicon_image: dragged.runtime_favicon_image.clone(),
                    runtime_favicon_fetch: dragged.runtime_favicon_fetch.clone(),
                    state: dragged.state,
                    chrome_status: dragged.chrome_status,
                })
            })
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_tab_drag_feedback(event, pane_id, tab_index, cx);
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedBrowserTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_tab_strip_drop(pane_id, tab_index, dragged, window, cx);
                }),
            )
            .child(self.render_browser_tab_icon(
                profile_id,
                chrome_status,
                runtime_favicon_url,
                runtime_favicon_image.as_ref(),
                runtime_favicon_fetch.as_ref(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(5.0))
                    .child(display_title),
            )
            .when(show_close_button, |this| {
                this.child(self.render_browser_tab_close_button(tab_id, cx))
            })
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_strip_end_drop_target(
        &self,
        pane_id: BrowserPaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let show_insertion_marker = self.browser_tab_drop_feedback
            == Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::TabStrip(insertion_index),
            });

        div()
            .id(format!(
                "ghostex-gpui-browser-tabstrip-end-drop-{}",
                pane_id.0
            ))
            .relative()
            .h_full()
            .flex_grow_1()
            .min_w(px(20.0))
            .when(show_insertion_marker, |this| {
                this.child(self.render_browser_tab_insertion_marker(
                    pane_id,
                    insertion_index,
                    "end",
                ))
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    if event.click_count < 2 {
                        return;
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    this.browser_tabs.focus_pane(pane_id);
                    this.add_browser_tab(window, cx);
                }),
            )
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_tab_end_drag_feedback(event, pane_id, insertion_index, cx);
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedBrowserTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_tab_strip_drop(
                        pane_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_insertion_marker(
        &self,
        pane_id: BrowserPaneId,
        insertion_index: usize,
        marker_kind: &'static str,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-tab-drop-marker-{}-{}-{marker_kind}",
                pane_id.0, insertion_index
            ))
            .absolute()
            .left_0()
            .top(px(4.0))
            .h(px(BROWSER_TAB_BAR_HEIGHT - 8.0))
            .w(px(2.0))
            .rounded_full()
            .bg(workspace_drop_feedback_border_color())
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_icon(
        &self,
        profile_id: BrowserProfileId,
        chrome_status: BrowserTabChromeStatus,
        runtime_favicon_url: Option<&str>,
        runtime_favicon_image: Option<&BrowserFaviconImage>,
        runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
    ) -> AnyElement {
        browser_tab_icon_element(
            profile_id,
            chrome_status,
            runtime_favicon_url,
            runtime_favicon_image,
            runtime_favicon_fetch,
        )
    }

    pub(crate) fn browser_pane_drop_zone(&self, pane_id: BrowserPaneId) -> Option<WorkspaceDropZone> {
        match self.browser_tab_drop_feedback {
            Some(BrowserDropFeedback {
                pane_id: feedback_pane_id,
                target: BrowserTabDropTarget::PaneBody(zone),
            }) if feedback_pane_id == pane_id => Some(zone),
            _ => None,
        }
    }

    pub(crate) fn render_browser_pane_drop_feedback(
        &self,
        pane_id: BrowserPaneId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser body drop feedback must distinguish center grouping from edge split intent while staying non-interactive. Render the indication as a normal child inside the Browser pane body so native CEF views stay hidden by the drag visibility gate instead of relying on transparent overlays or hit-test rerouting.
        */
        let label = match zone {
            WorkspaceDropZone::Center => "Group",
            WorkspaceDropZone::Left => "Split left",
            WorkspaceDropZone::Right => "Split right",
            WorkspaceDropZone::Top => "Split top",
            WorkspaceDropZone::Bottom => "Split bottom",
        };

        let feedback = div()
            .id(format!(
                "ghostex-gpui-browser-pane-drop-feedback-{}",
                pane_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full();

        match zone {
            WorkspaceDropZone::Center => feedback
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(workspace_drop_feedback_border_color())
                .bg(workspace_drop_group_feedback_color())
                .child(self.render_workspace_drop_feedback_label(label, zone))
                .into_any_element(),
            WorkspaceDropZone::Left => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .left_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Right => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .right_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Top => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .top_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Bottom => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_browser_tab_action_cluster(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(format!("ghostex-gpui-browser-tab-actions-{}", pane_id.0))
            .flex_shrink_0()
            .h_full()
            .w(px(BROWSER_TAB_ACTION_CLUSTER_WIDTH))
            .items_center()
            .bg(browser_tab_action_cluster_color())
            .child(
                div()
                    .id(format!("ghostex-gpui-browser-tab-action-new-{}", pane_id.0))
                    .flex()
                    .h_full()
                    .w(px(BROWSER_TAB_ACTION_BUTTON_SIZE))
                    .items_center()
                    .justify_center()
                    .rounded(px(0.0))
                    .border_l_1()
                    .border_color(browser_tab_separator_color())
                    .bg(browser_tab_action_cluster_color())
                    .cursor_default()
                    .hover(|this| this.bg(browser_tab_action_hover_color()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.browser_tabs.focus_pane(pane_id);
                            this.add_browser_tab(window, cx);
                        }),
                    )
                    .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                        Tooltip::new("New browser tab").build(window, cx)
                    })
                    .child(self.render_browser_tab_new_icon(17.0)),
            )
            .child(
                div()
                    .id(format!(
                        "ghostex-gpui-browser-tab-action-overflow-{}",
                        pane_id.0
                    ))
                    .flex()
                    .h_full()
                    .w(px(BROWSER_TAB_ACTION_BUTTON_SIZE))
                    .items_center()
                    .justify_center()
                    .rounded(px(0.0))
                    .border_l_1()
                    .border_color(browser_tab_separator_color())
                    .bg(browser_tab_action_cluster_color())
                    .cursor_default()
                    .hover(|this| this.bg(browser_tab_action_hover_color()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.show_browser_pane_actions_menu(
                                pane_id,
                                event.position,
                                window,
                                cx,
                            );
                        }),
                    )
                    .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                        Tooltip::new("Browser pane actions menu").build(window, cx)
                    })
                    .child(self.render_browser_tab_overflow_icon()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_new_icon(&self, size: f32) -> AnyElement {
        let arm_length = size - 2.0;
        let arm_offset = (size - 1.0) / 2.0;
        div()
            .relative()
            .size(px(size))
            .child(
                div()
                    .absolute()
                    .left(px(arm_offset))
                    .top(px(1.0))
                    .w(px(1.0))
                    .h(px(arm_length))
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(arm_offset))
                    .w(px(arm_length))
                    .h(px(1.0))
                    .bg(browser_tab_action_icon_color()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_overflow_icon(&self) -> AnyElement {
        h_flex()
            .size(px(14.0))
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_close_button(
        &self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!("ghostex-gpui-browser-tab-close-{}", tab_id.0))
            .flex()
            .flex_shrink_0()
            .size(px(BROWSER_TAB_CLOSE_SIZE))
            .ml(px(5.0))
            .items_center()
            .justify_center()
            .rounded(px(0.0))
            .bg(browser_tab_close_background_color())
            .cursor_default()
            .hover(|this| this.bg(browser_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_browser_tab(tab_id, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                BROWSER_ICON_STOP,
                8.5,
                browser_tab_close_color(),
            ))
            .into_any_element()
    }
}
