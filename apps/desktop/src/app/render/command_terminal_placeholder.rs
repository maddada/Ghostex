// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: command-pane terminal placeholder rendering and command-pane drop-zone/drop-feedback.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MousePressureEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::ScrollWheelEvent;
use gpui::Styled as _;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder as _;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

use super::terminal_content_layout::terminal_content_frame;

impl GhostexGpuiApp {
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
        let (terminal_horizontal_padding, terminal_vertical_padding, terminal_width_percent) =
            settings_snapshot.terminal_pane_layout(
                settings_snapshot.terminal_width_applies_to_command_pane_terminals(),
            );
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
                this.child(terminal_content_frame(
                    div().size_full().child(view),
                    terminal_horizontal_padding,
                    terminal_vertical_padding,
                    terminal_width_percent,
                ))
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
                    terminal_content_frame(
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
                        .size_full(),
                        terminal_horizontal_padding,
                        terminal_vertical_padding,
                        terminal_width_percent,
                    )
                })
            })
            .when_some(self.command_pane_drop_zone(group_id), |this, zone| {
                this.child(self.render_command_pane_drop_feedback(group_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn command_pane_drop_zone(
        &self,
        group_id: CommandPaneGroupId,
    ) -> Option<WorkspaceDropZone> {
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
}
