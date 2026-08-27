// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: command-pane leaf chrome (pane titlebar, bottom reservation, floating reserved bar) and the command-pane tab strip/tab element.

use gpui::AnyElement;
use gpui::AppContext as _;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::ScrollWheelEvent;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
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
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            this.handle_command_pane_empty_titlebar_right_mouse_down(
                                Some(group_id),
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

    pub(crate) fn render_command_pane_floating_reserved_bottom_bar(
        &self,
        height: f32,
    ) -> AnyElement {
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
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                                    this.handle_command_pane_empty_titlebar_right_mouse_down(
                                        None, window, cx,
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
}
