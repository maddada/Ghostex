// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: Agents workspace tab bar, tab element, drop target/insertion marker, sleep icon, state badge, close button, and tab action cluster.

use gpui::AnyElement;
use gpui::AppContext as _;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::svg;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Workarea 2026-09-04 DECISION:
    /// User: hide the tabs bar above the agents pane when the screen is not split, off by default.
    /// The gate counts real leaves, not the focus-mode projection, so a zoomed split still shows its bar and the double-click exit stays reachable.
    /// Splitting an unsplit workspace goes through Advanced > Split Right in the sidebar session menu (`splitSessionRight`).
    pub(crate) fn agents_workspace_tab_bar_visible(&self) -> bool {
        self.agents_workspace.leaf_order().len() > 1
            || shared_settings::shared_sidebar_settings_snapshot()
                .show_agents_pane_tab_bar_when_unsplit()
    }

    pub(crate) fn render_workspace_tab_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:Workarea 2026-06-22-05:18:
        Agents terminal panes need native-style tab chrome before libghostty mounts: every pane keeps a tab bar (visible on split workspaces, see `agents_workspace_tab_bar_visible`), tab selection colors stay tied to active tab state instead of pane focus, horizontal overflow remains scrollable, and right-side pane actions stay in the tab chrome instead of overlapping terminal bodies.

        CDXC:CommandPane 2026-06-22-06:39:
        Agents pane action chrome mirrors the native compact tab-bar cluster: fixed New Terminal, New Browser Tab, and pane overflow controls live at the far right. Split, rotate, and merge actions stay in the overflow menu rather than occupying fixed tab-bar slots.

        CDXC:CommandPane 2026-06-22-13:17:
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
                    CDXC:CommandPane 2026-07-10:
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
        CDXC:Workarea 2026-08-03:
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

    pub(crate) fn render_workspace_tab_sleep_icon(
        &self,
        session_id: TerminalSessionId,
    ) -> AnyElement {
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
                    CDXC:Workarea 2026-06-26-06:57:
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
        CDXC:SessionChat 2026-08-02:
        The Terminal/Chat toggle lives in the floating top-right cluster over
        the surface itself: the terminal's agent-action overlay in terminal
        view, and the chat page's own in-DOM cluster in chat view (a
        gpui-drawn overlay would sit UNDER the native CEF chat view). The tab
        chrome hosts no toggle.
        */
        /*
        CDXC:AgentLauncher 2026-08-01-16:00:
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
    CDXC:AgentLauncher 2026-08-01-16:00:
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

    pub(crate) fn render_workspace_tab_action_icon(
        &self,
        icon: WorkspaceTabActionIcon,
    ) -> AnyElement {
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
}
