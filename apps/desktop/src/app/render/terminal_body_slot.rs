// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: terminal body slot rendering (workspace pane body dispatch across command placeholder, browser, project-editor, and Agents-terminal states).

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
use gpui::px;
use gpui::rgb;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
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

}
