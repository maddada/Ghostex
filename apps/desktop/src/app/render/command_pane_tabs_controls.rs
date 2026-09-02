// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: command-pane tab strip controls (separator, add button, sticky active tab, drop target, insertion marker, close button), hover-state setters, and the pane control buttons.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::ScrollHandle;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

const COMMAND_PANE_MINIMIZE_TOOLTIP: &str = "Right click an empty spot in the tabs bar to toggle minimizing.\nAlso double click to create a new tab.";

impl GhostexGpuiApp {
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    /*
                    Right-click toggling belongs only to empty tab-strip chrome.
                    Keep the inline New Terminal control from bubbling into the
                    strip handler when the pointer is over this real control.
                    */
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
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

    pub(crate) fn handle_command_pane_empty_titlebar_right_mouse_down(
        &mut self,
        group_id: Option<CommandPaneGroupId>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        Empty command tab-strip chrome toggles between the current expanded
        mode and the collapsed strip. Tabs consume right-click for their
        context menu, and controls consume their own pointer events, so this
        handler runs only for the unoccupied titlebar region.
        */
        window.prevent_default();
        cx.stop_propagation();
        self.handle_command_pane_control_action(
            CommandPaneControlAction::ToggleExpanded,
            group_id,
            window,
            cx,
        );
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
        let pin_icon_path = command_pane_panel_pin_icon_path(self.command_pane.mode);
        let expand_icon_path =
            command_pane_panel_visibility_icon_path(self.command_pane.is_expanded());
        let visibility_tooltip = expanded_chrome.then_some(COMMAND_PANE_MINIMIZE_TOOLTIP);
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
                        None,
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
                        Some(pin_tooltip),
                        CommandPaneControlAction::TogglePinned,
                        group_id,
                        cx,
                    ))
                },
            )
            .child(self.render_command_pane_control_button(
                "expand",
                expand_icon_path,
                visibility_tooltip,
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
        tooltip: Option<&'static str>,
        action: CommandPaneControlAction,
        group_id: Option<CommandPaneGroupId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let element_id = match group_id {
            Some(group_id) => format!("ghostex-gpui-command-pane-control-{id}-{}", group_id.0),
            None => format!("ghostex-gpui-command-pane-control-{id}"),
        };
        let is_visibility_control = matches!(action, CommandPaneControlAction::ToggleExpanded);

        div()
            .id(element_id)
            .flex()
            .size(px(COMMAND_PANE_CONTROL_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .when(is_visibility_control, |this| {
                this.pl(px(COMMAND_PANE_VISIBILITY_ICON_LEADING_PADDING))
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
            .when_some(tooltip, |this, tooltip| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    move |window, cx| Tooltip::new(tooltip).build(window, cx),
                )
            })
            .child(titlebar_svg_icon(
                icon_path,
                COMMAND_PANE_CONTROL_ICON_SIZE,
                command_pane_control_text_color(),
            ))
            .into_any_element()
    }
}
