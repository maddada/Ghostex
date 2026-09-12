use gpui::ClipboardItem;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
#[cfg(target_os = "linux")]
use gpui::Pixels;
use gpui::Render;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
#[cfg(target_os = "linux")]
use gpui::{CursorStyle, Decorations, ResizeEdge};
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::hotkeys::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;

#[cfg(target_os = "linux")]
const GPUI_LINUX_CLIENT_FRAME_WIDTH: Pixels = px(4.0);

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum GpuiLinuxResizeRegionShape {
    Horizontal,
    Vertical,
    Corner,
}

#[cfg(target_os = "linux")]
fn gpui_linux_resize_region(
    edge: ResizeEdge,
    shape: GpuiLinuxResizeRegionShape,
) -> gpui::AnyElement {
    let cursor = match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    };
    div()
        .flex()
        .flex_shrink_0()
        .bg(titlebar_button_border_color())
        .cursor(cursor)
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Horizontal),
            |this| this.h(GPUI_LINUX_CLIENT_FRAME_WIDTH).flex_1(),
        )
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Vertical),
            |this| this.w(GPUI_LINUX_CLIENT_FRAME_WIDTH).h_full(),
        )
        .when(
            matches!(shape, GpuiLinuxResizeRegionShape::Corner),
            |this| this.size(GPUI_LINUX_CLIENT_FRAME_WIDTH),
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
            window.start_window_resize(edge);
        })
        .into_any_element()
}

#[cfg(target_os = "linux")]
fn gpui_linux_client_window_frame(
    content: gpui::AnyElement,
    window: &mut Window,
) -> gpui::AnyElement {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return content;
    };

    /*
    Keep resize ownership in eight exact, non-overlapping layout regions. The
    visible four-pixel frame is itself the grab target; no transparent view or
    full-window hit-test layer extends across app or CEF content. Tiled edges
    leave the layout entirely, matching the compositor's edge constraints.
    */
    window.set_client_inset(GPUI_LINUX_CLIENT_FRAME_WIDTH);
    v_flex()
        .size_full()
        .min_w_0()
        .min_h_0()
        .when(!tiling.top, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .h(GPUI_LINUX_CLIENT_FRAME_WIDTH)
                    .when(!tiling.left, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::TopLeft,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    })
                    .child(gpui_linux_resize_region(
                        ResizeEdge::Top,
                        GpuiLinuxResizeRegionShape::Horizontal,
                    ))
                    .when(!tiling.right, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::TopRight,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    }),
            )
        })
        .child(
            h_flex()
                .w_full()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .when(!tiling.left, |this| {
                    this.child(gpui_linux_resize_region(
                        ResizeEdge::Left,
                        GpuiLinuxResizeRegionShape::Vertical,
                    ))
                })
                .child(
                    div()
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_hidden()
                        .child(content),
                )
                .when(!tiling.right, |this| {
                    this.child(gpui_linux_resize_region(
                        ResizeEdge::Right,
                        GpuiLinuxResizeRegionShape::Vertical,
                    ))
                }),
        )
        .when(!tiling.bottom, |this| {
            this.child(
                h_flex()
                    .w_full()
                    .h(GPUI_LINUX_CLIENT_FRAME_WIDTH)
                    .when(!tiling.left, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::BottomLeft,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    })
                    .child(gpui_linux_resize_region(
                        ResizeEdge::Bottom,
                        GpuiLinuxResizeRegionShape::Horizontal,
                    ))
                    .when(!tiling.right, |this| {
                        this.child(gpui_linux_resize_region(
                            ResizeEdge::BottomRight,
                            GpuiLinuxResizeRegionShape::Corner,
                        ))
                    }),
            )
        })
        .into_any_element()
}

impl Render for GhostexGpuiApp {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _profile = crate::profiling::span(crate::profiling::Metric::AppRender);
        // Only the main workspace window renders this entity, so this keeps
        // the authoritative frame and display every child popup anchors against.
        #[cfg(target_os = "macos")]
        self.sync_terminal_model_picker_keyboard_scope();
        self.main_window_bounds = window.bounds();
        self.main_window_display_id = window.display(cx).map(|display| display.id());
        #[cfg(target_os = "windows")]
        if self.windows_first_run_setup_state != GpuiWindowsFirstRunSetupState::Ready {
            return self.render_windows_first_run_setup(cx);
        }
        /*
        CDXC:CefRuntime 2026-06-14-12:06:
        GPUI must prove the macOS sidebar React UI and normal browser surfaces can run as CEF children inside the shell. Keep the CEF child views as exact GPUI layout siblings, with the address-bar chrome owned by GPUI above only the main browser area, so future Linux and Windows backends can replace the macOS FFI without changing the app layout contract.

        CDXC:Sidebar 2026-06-21-18:34:
        The GPUI sidebar must match native macOS sidebar resizing: start from the persisted native sidebarWidth, reserve a real five-pixel divider rail between the sidebar and browser siblings, clamp drag/reset width to 150px..520px while preserving a 240px workspace minimum, and use the Settings-owned sidebarDefaultWidthPx only for double-click reset.

        CDXC:Sidebar 2026-06-21-22:17:
        The GPUI divider rail must behave like the native sidebar divider without AppKit dependencies: the rail stays a real GPUI sibling, keeps the sidebar background color, uses the ew-resize cursor over the rail, and reveals the white hover line after the same short delay/fade from pointer hover instead of requiring a click.

        CDXC:Workarea 2026-06-22-13:36:
        The main workspace column must own the full height available below the titlebar. The body row top-aligns its full-height workspace column and uses a black shell background so GPUI's h_flex center alignment or any late child surface cannot expose a white window fill above or below the workspace.

        CDXC:Sidebar 2026-06-26-23:35:
        Sidebar side parity is implemented as normal sibling order, never overlays or hit-test rerouting: expanded left is sidebar/divider/workspace, expanded right is workspace/divider/sidebar, and collapsed mode removes sidebar/divider while preserving the saved expanded width.
        */
        self.sidebar_width =
            clamp_sidebar_width(self.sidebar_width, current_sidebar_max_width(window));
        self.refresh_gpui_sidebar_browser_tabs_if_changed(cx);
        self.refresh_gpui_sidebar_displayed_sessions_if_changed(cx);
        self.prepare_focus_bounds_for_render(window.scale_factor(), cx);
        #[cfg(target_os = "macos")]
        self.sync_terminal_close_confirm_dialog(window, cx);
        self.sync_terminal_paste_confirmation_dialog(window, cx);
        self.sync_terminal_search_inputs(window, cx);
        self.sync_composited_terminal_keyboard_owner(window, cx);
        self.drain_pending_keyboard_handoff(window, cx);
        self.sync_session_chat_pane_focus(window, cx, false);
        self.refresh_zmx_persistence_focused_terminal_if_changed(cx);
        let sidebar_chrome_visible = gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        let sidebar_on_left = self.sidebar_side == GpuiSidebarSide::Left;
        let titlebar_popup_dismissal_active =
            self.titlebar_popup_menu.is_some() || self.titlebar_extension_popup.is_some();

        let content = v_flex()
            .relative()
            .size_full()
            .bg(workspace_background_color())
            .when(
                self.titlebar_popup_menu.as_ref().is_some_and(|state| {
                    matches!(state.kind, GpuiTitlebarPopupKind::AccountUsage(_))
                }),
                |this| {
                    this.track_focus(&self.titlebar_dropdown_focus_handle)
                        .capture_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                            app.forward_account_usage_key(event, cx);
                        }))
                },
            )
            .when(self.active_mode == TitlebarMode::Browser, |this| {
                this.key_context(BROWSER_KEY_CONTEXT)
            })
            .when(titlebar_popup_dismissal_active, |this| {
                /*
                Native titlebar dropdowns live in non-activating panels, while
                extension popups are anchored in this main window. Close either
                popup on a main-window mouse-down outside both its content and
                trigger button (a mouse-down on the trigger itself is left to
                the button's own toggle handler), and put the dropdown key
                context on the root so the existing
                Escape -> TitlebarDropdownCancel binding dispatches from
                wherever focus currently is.
                */
                this.key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
                    .capture_any_mouse_down(cx.listener(
                        |app, event: &MouseDownEvent, window, cx| {
                            let outside_popup_menu_trigger =
                                app.titlebar_popup_menu.as_ref().is_some_and(|state| {
                                    !state.trigger_bounds.contains(&event.position)
                                });
                            let outside_extension_trigger =
                                app.titlebar_extension_popup.as_ref().is_some_and(|state| {
                                    !state.trigger_bounds.contains(&event.position)
                                });
                            let outside_extension_popup = app
                                .titlebar_extension_popup_bounds(window)
                                .is_some_and(|bounds| !bounds.contains(&event.position));
                            let popup_kind = app
                                .titlebar_popup_menu
                                .as_ref()
                                .map(|state| state.kind.diagnostic_label())
                                .or_else(|| {
                                    app.titlebar_extension_popup
                                        .as_ref()
                                        .map(|_| "extension")
                                });
                            let trigger_bounds = app
                                .titlebar_popup_menu
                                .as_ref()
                                .map(|state| {
                                    gpui_titlebar_popup_bounds_diagnostic(Some(
                                        state.trigger_bounds,
                                    ))
                                })
                                .or_else(|| {
                                    app.titlebar_extension_popup.as_ref().map(|state| {
                                        gpui_titlebar_popup_bounds_diagnostic(Some(
                                            state.trigger_bounds,
                                        ))
                                    })
                                });
                            log_gpui_titlebar_popup_repro(
                                "gpui.titlebarPopup.mainWindowMouseCapture",
                                serde_json::json!({
                                    "kind": popup_kind,
                                    "mainWindowActive": window.is_window_active(),
                                    "outsideTrigger": outside_popup_menu_trigger || outside_extension_trigger,
                                    "pointerX": event.position.x.as_f32(),
                                    "pointerY": event.position.y.as_f32(),
                                    "triggerBounds": trigger_bounds,
                                }),
                            );
                            if outside_popup_menu_trigger {
                                app.close_gpui_titlebar_popup(None, window, cx);
                            } else if outside_extension_trigger && outside_extension_popup {
                                app.close_titlebar_extension_popup(window, cx);
                            }
                        },
                    ))
            })
            .when(
                self.sidebar_drag.is_some()
                    || self.command_split_drag.is_some()
                    || self
                        .browser_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Horizontal)
                    || self.project_editor_companion_drag.is_some()
                    || self
                        .workspace_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Horizontal),
                |this| this.cursor_ew_resize(),
            )
            .when(
                self.command_pane.resize_drag.is_some()
                    || self
                        .browser_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Vertical)
                    || self
                        .workspace_split_drag
                        .is_some_and(|drag| drag.axis == WorkspaceSplitAxis::Vertical),
                |this| this.cursor_ns_resize(),
            )
            /*
            CDXC:Terminal 2026-07-10:
            Root committed-text forwarding is only for GPUI-composited engine terminals. Native libghostty surfaces own keyDown and NSTextInputClient directly on their exact AppKit host NSView; routing their ordinary text through this GPUI listener would recreate the competing committed-text-only path that loses physical keys and modifiers.
            CDXC:SessionSleep 2026-06-25-14:49:
            Focused sleeping command placeholders consume plain alphanumeric key-downs to wake before terminal text delivery. This matches native "Press Any Key to Wake" behavior without forwarding the wake key to Ghostty or creating a broad keyboard fallback for non-terminal surfaces.
            CDXC:Terminal 2026-07-04-08:20:
            Root key-down forwarding derives its terminal target from app-level shell focus, which intentionally stays on the terminal pane while the Cmd+F search bar is open. If a terminal search input holds GPUI keyboard focus, forwarding here would write every typed character into the focused terminal PTY and consume the event, so macOS never runs the insertText path that feeds the focused input. Keyboard focus on a search input therefore ends root terminal key forwarding (and placeholder wake) for the keystroke; the search input's own dispatch path owns it.
            */
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.terminal_search_input_owns_keyboard_focus(window, cx)
                    || this.browser_find_input_owns_keyboard_focus(window, cx)
                {
                    return;
                }
                if this
                    .wake_focused_sleeping_command_placeholder_from_keystroke(&event.keystroke, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this
                    .wake_focused_sleeping_agents_placeholder_from_keystroke(&event.keystroke, cx)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this.focused_gpui_engine_terminal_view().is_none() {
                    return;
                }
                let Some(text) = committed_terminal_text_from_key_down_event(event) else {
                    return;
                };
                if this.send_text_to_focused_terminal_surface(text, cx) {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenBrowserHistory, window, cx| {
                this.show_browser_history_popup(this.browser_tabs.focused_pane, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenCommandPane, window, cx| {
                this.open_command_pane_from_keyboard(window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &RunConfiguredGhostexHotkey, window, cx| {
                    if this.propagate_source_workarea_cef_configured_hotkey_passthrough(
                        action.action_id.as_str(),
                        cx,
                    ) {
                        return;
                    }
                    if action.action_id == "openModelPicker" {
                        if !this.request_focused_session_model_picker(cx) {
                            cx.propagate();
                        }
                        return;
                    }
                    this.handle_gpui_app_modal_sidebar_command(
                        serde_json::json!({
                            "message": {
                                "actionId": action.action_id,
                                "type": "runGhostexHotkeyAction",
                            },
                        }),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &PasteIntoFocusedTerminal, _window, cx| {
                    if this.propagate_renderer_edit_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    if !this.paste_into_focused_terminal_from_clipboard(cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &FindInFocusedTerminal, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                if this.start_find_in_focused_browser(window, cx) {
                    return;
                }
                let _ = this.start_search_in_focused_terminal_surface(cx);
            }))
            .on_action(
                cx.listener(|this, _: &FindNextInFocusedBrowser, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    let ShellFocusTarget::BrowserPane(pane_id) = this.shell_focus else {
                        cx.propagate();
                        return;
                    };
                    let Some(tab_id) = this.browser_tabs.active_tab_id_for_pane(pane_id) else {
                        cx.propagate();
                        return;
                    };
                    if !this.perform_browser_find_navigation(tab_id, true, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &FindPreviousInFocusedBrowser, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    let ShellFocusTarget::BrowserPane(pane_id) = this.shell_focus else {
                        cx.propagate();
                        return;
                    };
                    let Some(tab_id) = this.browser_tabs.active_tab_id_for_pane(pane_id) else {
                        cx.propagate();
                        return;
                    };
                    if !this.perform_browser_find_navigation(tab_id, false, cx) {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &ZoomInFocusedSurface, _window, cx| {
                if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::In, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomOutFocusedSurface, _window, cx| {
                if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::Out, cx) {
                    cx.propagate();
                }
            }))
            .on_action(
                cx.listener(|this, _: &ResetFocusedSurfaceZoom, _window, cx| {
                    if !this.perform_focused_surface_zoom(GpuiFocusedSurfaceZoomCommand::Reset, cx)
                    {
                        cx.propagate();
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &TitlebarDropdownCancel, window, cx| {
                if this.titlebar_popup_menu.is_some() {
                    this.cancel_gpui_titlebar_popup(window, cx);
                } else if this.titlebar_extension_popup.is_some() {
                    this.close_titlebar_extension_popup(window, cx);
                } else if this.titlebar_resources_panel_open {
                    this.set_gpui_titlebar_resources_panel_open(false, window, cx);
                } else if this.titlebar_tips_panel_open {
                    this.set_gpui_titlebar_tips_panel_open(false, window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(
                cx.listener(|this, _: &SleepInactiveSessionsFromTitlebar, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_sleep_inactive_sessions(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &StartGpuiGxserverFromTitlebar, _window, cx| {
                    this.start_gpui_local_gxserver_bootstrap(true, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &StopGpuiGxserverFromTitlebar, _window, cx| {
                    this.stop_gpui_local_gxserver_from_titlebar(false, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &RestartGpuiGxserverFromTitlebar, _window, cx| {
                    this.stop_gpui_local_gxserver_from_titlebar(true, cx);
                }),
            )
            .on_action(cx.listener(
                |this, _: &OpenGpuiPortlessSetupModalFromTitlebar, window, cx| {
                    if GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
                        this.open_gpui_app_modal_from_titlebar(
                            GpuiAppModalKind::PortlessSetup,
                            window,
                            cx,
                        );
                    }
                },
            ))
            .on_action(cx.listener(|this, _: &CycleFocusedTabForward, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.cycle_focused_tab(false, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CycleFocusedTabBackward, window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.cycle_focused_tab(true, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CloseFocusedSurface, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.close_focused_surface(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CloseFocusedSurfaceMenuOnly, window, cx| {
                    this.close_focused_surface(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleGpuiSidebarCollapsed, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.toggle_gpui_sidebar_collapsed(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &ToggleProjectEditorCompanion, window, cx| {
                    this.toggle_project_editor_companion_from_hotkey(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SleepFocusedSession, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                /*
                CDXC:FocusMode 2026-06-25-14:56:
                Option+Shift+S sleeps whichever terminal session owns shell focus. Command tabs use their native sleep mutation; Agents and companion sessions use the same scoped lifecycle request as their tab action.
                */
                if !this.sleep_focused_command_pane_session(cx)
                    && let Some(session_id) = this.focused_agents_or_companion_shell_session_id()
                    && let Some(pane_id) = this.agents_workspace.pane_id_for_session(session_id)
                {
                    let _ = this.sleep_agents_tabs_for_scope(
                        pane_id,
                        session_id,
                        AgentsWorkspaceTabSleepScope::Sleep,
                        cx,
                    );
                }
            }))
            .on_action(cx.listener(|this, _: &WakeFocusedSession, _window, cx| {
                /*
                CDXC:FocusMode 2026-06-25-15:01:
                Wake Focused Session has no default key, but the shared command palette and Hotkeys UI expose it. Keep the GPUI action unbound by default and route it only through the command-pane focused sleeping tab wake path.
                */
                this.wake_focused_command_pane_session(cx);
            }))
            .on_action(cx.listener(|this, _: &NewTerminalTab, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.add_terminal_placeholder_tab_from_hotkey(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SplitFocusedTerminalRight, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.split_focused_terminal_from_hotkey(
                        FocusedTerminalSplitDirection::Right,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &SplitFocusedTerminalDown, _window, cx| {
                    if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                        return;
                    }
                    this.split_focused_terminal_from_hotkey(
                        FocusedTerminalSplitDirection::Down,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &NewBrowserTab, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.add_browser_tab_from_hotkey(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleAgentsFocusMode, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.toggle_agents_focus_mode(cx);
            }))
            .on_action(cx.listener(|this, _: &MergeAllTabs, _window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.merge_all_agents_tabs_from_hotkey(cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchAgentsWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Agents, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchSourceWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchBrowserWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Browser, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchKanbanWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Kanban, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SwitchManageWorkarea, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenGpuiHotkeysModal, window, cx| {
                this.open_gpui_app_modal_from_titlebar(GpuiAppModalKind::Hotkeys, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiConfigureAgentsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::ConfigureAgents,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiConfigureActionsModal, window, cx| {
                    this.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiOpenTargetsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::OpenTargets,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &OpenGpuiWorkspaceInTarget, window, cx| {
                    this.open_active_project_with_open_target_index(
                        action.target_index as usize,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &SelectGpuiTitlebarMode, window, cx| {
                    this.select_titlebar_mode_from_menu(action.mode_index, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarAction, window, cx| {
                    this.run_gpui_titlebar_action_index(action.action_index as usize, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &RunGpuiTitlebarGitMenuAction, _window, cx| {
                    this.run_gpui_titlebar_git_menu_row(action.row_index as usize, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &CopyGpuiTitlebarGitBranch, _window, cx| {
                    let Some(branch) = this
                        .titlebar_git_menu_state
                        .as_ref()
                        .and_then(|state| state.branch.clone())
                    else {
                        return;
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(branch));
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenGpuiTitlebarGitCommitScreen, _window, cx| {
                    this.dispatch_gpui_titlebar_git_action_selector(
                        GpuiTitlebarGitMenuActionId::Commit.selector(),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &RunGpuiTitlebarGitRemoteSync, _window, cx| {
                    this.dispatch_gpui_titlebar_git_action_selector(
                        GpuiTitlebarGitMenuActionId::SyncRemote.selector(),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &ConfigureGpuiTitlebarActions, window, cx| {
                    this.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &StartGpuiKeepAwakePeriod, window, cx| {
                    let Some(duration_minutes) =
                        shared_settings::SharedKeepAwakeDurationMinutes::from_minutes(
                            action.duration_minutes,
                        )
                    else {
                        return;
                    };
                    this.start_gpui_keep_awake_period(duration_minutes, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &StopGpuiKeepAwake, _window, cx| {
                this.stop_gpui_keep_awake_from_titlebar(cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiPowerSettingsModal, window, cx| {
                    this.open_gpui_power_settings_modal_from_titlebar(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &SleepGpuiPetOverlay, _window, cx| {
                this.sleep_gpui_pet_overlay_from_context_menu(cx);
            }))
            .on_action(
                cx.listener(|this, _: &GoToGhostexFromGpuiPetOverlay, window, cx| {
                    this.go_to_ghostex_from_gpui_pet_overlay(window, cx);
                }),
            )
            .on_action(cx.listener(|_this, _: &GpuiKeepAwakeMenuLabel, _window, _cx| {}))
            .on_action(
                cx.listener(|this, _: &OpenGpuiCommandPaletteModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::CommandPalette,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, action: &crate::app::project_views::ProjectViewCommand, window, cx| { this.project_view_command(action, window, cx); }))
            .on_action(cx.listener(|this, _: &OpenGpuiExtensionsModal, window, cx| {
                /*
                CDXC:Titlebar 2026-08-13:
                GPUI popup menu dispatches through the main window's rendered
                action tree. Handle Extensions on that tree, alongside the
                other titlebar menu actions, so right-click selection opens
                the explicit Settings > Extensions route instead of relying
                on an app-global fallback that this window dispatch may
                never reach.
                */
                this.open_gpui_settings_extensions_page(Some(window), cx);
            }))
            .on_action(
                cx.listener(|this, _: &OpenGpuiPreviousSessionsModal, window, cx| {
                    this.open_gpui_app_modal_from_titlebar(
                        GpuiAppModalKind::PreviousSessions,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &OpenGpuiAgentsHubModal, window, cx| {
                this.open_gpui_app_modal_from_titlebar(GpuiAppModalKind::AgentsHub, window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &NewTerminalTabInPane, _window, cx| {
                    this.add_agents_registered_terminal_tab(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &SplitPaneRightWithNewTerminal, _window, cx| {
                    this.split_agents_registered_terminal_right(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SplitPaneBelowWithNewTerminal, _window, cx| {
                    this.split_agents_registered_terminal_below(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &RotateAgentsPanesForPane, _window, cx| {
                    this.rotate_agents_panes_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(cx.listener(
                |this, action: &AppendFullWidthTerminalRowForPane, _window, cx| {
                    this.append_agents_registered_terminal_bottom_row(
                        WorkspacePaneId(action.pane_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &MergeAllTabsForPane, _window, cx| {
                    this.merge_all_agents_tabs_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &ToggleFocusModeForPane, _window, cx| {
                    this.toggle_agents_focus_mode_for_pane(WorkspacePaneId(action.pane_id), cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &SelectAgentsWorkspaceTab, _window, cx| {
                    this.select_agents_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseAgentsWorkspaceTab, _window, cx| {
                    this.close_agents_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &CloseAgentsWorkspaceTabsByScope, _window, cx| {
                    this.close_agents_tabs_for_scope_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SleepAgentsWorkspaceTabsByScope, _window, cx| {
                    this.sleep_agents_tabs_for_scope_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &RenameAgentsWorkspaceTab, _window, cx| {
                    this.open_gpui_rename_session_modal_for_agents_tab(
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &FocusAgentsWorkspaceTab, _window, cx| {
                    this.toggle_agents_focus_mode_for_tab_from_action(
                        WorkspacePaneId(action.pane_id),
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &ForkAgentsWorkspaceTab, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_terminal_runtime_action(
                        "forkSession",
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &ReloadAgentsWorkspaceTab, _window, cx| {
                    let _ = this.dispatch_gpui_workspace_terminal_runtime_action(
                        "fullReloadSession",
                        TerminalSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &OpenBrowserPaneInExternalBrowser, _window, _cx| {
                    this.open_browser_pane_in_external_browser(BrowserPaneId(action.pane_id));
                },
            ))
            .on_action(cx.listener(
                |_this, action: &SetBrowserPageAppearance, window, cx| {
                    if let Err(error) = cef::set_browser_page_appearance(action.appearance) {
                        window.push_notification(
                            Notification::error(format!("Could not save browser appearance: {error}")),
                            cx,
                        );
                    }
                    cx.notify();
                },
            ))
            .on_action(
                cx.listener(|this, action: &NewBrowserTabInPane, window, cx| {
                    this.add_browser_tab_in_pane_from_action(
                        BrowserPaneId(action.pane_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &SplitBrowserPaneRightWithBrowserTab, window, cx| {
                    this.split_browser_pane_with_new_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        WorkspaceDropZone::Right,
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, action: &SplitBrowserPaneBelowWithBrowserTab, window, cx| {
                    this.split_browser_pane_with_new_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        WorkspaceDropZone::Bottom,
                        window,
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &SelectBrowserTabInPane, window, cx| {
                    this.select_browser_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        BrowserTabId(action.tab_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseBrowserTabInPane, window, cx| {
                    this.close_browser_tab_from_action(
                        BrowserPaneId(action.pane_id),
                        BrowserTabId(action.tab_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &RunBrowserFeedbackTool, window, cx| {
                this.run_browser_feedback_tool_from_toolbar(
                    this.browser_tabs.focused_pane,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &ResetBrowserZoom, window, cx| {
                this.reset_browser_zoom_from_toolbar(this.browser_tabs.focused_pane, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleBrowserDevTools, window, cx| {
                this.toggle_browser_devtools_from_toolbar(
                    this.browser_tabs.focused_pane,
                    window,
                    cx,
                );
            }))
            .on_action(
                cx.listener(|this, action: &SelectBrowserProfile, window, cx| {
                    this.select_browser_profile_from_menu(
                        BrowserPaneId(action.pane_id),
                        BrowserProfileId(action.profile_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CreateBrowserProfile, window, cx| {
                    this.create_browser_profile_from_menu(
                        BrowserPaneId(action.pane_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &CloseCommandPaneTabsByScope, _window, cx| {
                    this.close_command_pane_tabs_for_scope_from_action(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &SleepCommandPaneTabsByScope, _window, cx| {
                    this.sleep_command_pane_tabs_for_scope_from_action(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        action.scope,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &RenameCommandPaneTab, _window, cx| {
                    this.open_gpui_rename_session_modal_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, action: &DelayedSendCommandPaneTab, window, cx| {
                    this.open_gpui_delayed_send_modal_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(
                |this, action: &ToggleCloseAfterDoneCommandPaneTab, _window, cx| {
                    this.toggle_gpui_command_close_after_done_for_command_pane_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                },
            ))
            .on_action(
                cx.listener(|this, action: &FocusCommandPaneTab, _window, cx| {
                    this.toggle_command_pane_focus_mode_for_tab(
                        CommandPaneGroupId(action.group_id),
                        CommandSessionId(action.session_id),
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &FocusWorkspaceLeft, window, cx| {
                this.focus_workspace_direction(WorkspaceFocusDirection::Left, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceUp, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.focus_workspace_direction(WorkspaceFocusDirection::Up, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceRight, window, cx| {
                this.focus_workspace_direction(WorkspaceFocusDirection::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FocusWorkspaceDown, window, cx| {
                if this.propagate_source_workarea_cef_hotkey_passthrough(cx) {
                    return;
                }
                this.focus_workspace_direction(WorkspaceFocusDirection::Down, window, cx);
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.handle_sidebar_root_mouse_move(event, window, cx);
                this.handle_command_pane_resize_drag_move(event, window, cx);
                this.handle_workspace_split_resize_drag_move(event, window, cx);
                this.handle_command_split_resize_drag_move(event, window, cx);
                this.handle_browser_split_resize_drag_move(event, window, cx);
                this.handle_project_editor_companion_resize_drag_move(event, window, cx);
                this.handle_project_editor_companion_split_resize_drag_move(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    this.handle_sidebar_drag_mouse_up(event, window, cx);
                    this.handle_command_pane_resize_mouse_up(event, window, cx);
                    this.handle_workspace_split_resize_mouse_up(event, window, cx);
                    this.handle_command_split_resize_mouse_up(event, window, cx);
                    this.handle_browser_split_resize_mouse_up(event, window, cx);
                    this.handle_project_editor_companion_resize_mouse_up(event, window, cx);
                    this.handle_project_editor_companion_split_resize_mouse_up(event, window, cx);
                    this.finish_workspace_tab_drag(cx);
                    this.finish_command_tab_drag(cx);
                    this.finish_browser_tab_drag(cx);
                }),
            )
            .child(self.render_titlebar(window, cx))
            .child(
                /*
                Every top-row pane draws its own 1px frame and the titlebar
                draws a 1px bottom border, so stacked they showed a 2px line
                above the workspace. Pull the body row up by that 1px so a
                pane's top edge paints over the titlebar hairline: neutral
                panes leave one line, and a focused or attention pane shows
                its outline color on the top edge too. flex_1 absorbs the
                negative margin, so the row is 1px taller rather than leaving
                a gap at the bottom. The sidebar column and its divider draw
                the hairline themselves so it stays continuous across the
                window.
                */
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .mt(px(-1.0))
                    .items_start()
                    .overflow_hidden()
                    .bg(workspace_background_color())
                    .when(sidebar_chrome_visible && sidebar_on_left, |this| {
                        this.child(
                            /*
                            CDXC:Sidebar 2026-06-26-10:04:
                            Sidebar collapse is real layout ownership in GPUI: the sidebar CEF child and divider are removed as body-row siblings instead of being covered, overlapped, or resized to zero. Keep `sidebar_width` untouched so expand restores the previous user width.
                            */
                            div()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .border_t_1()
                                .border_color(titlebar_button_border_color())
                                .when_some(self.sidebar.clone(), |this, sidebar| {
                                    this.child(sidebar)
                                }),
                        )
                    })
                    .when(sidebar_chrome_visible && sidebar_on_left, |this| {
                        this.child(self.render_sidebar_resize_divider(cx))
                    })
                    .child(
                        v_flex()
                            .id("ghostex-gpui-workspace-column")
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .bg(workspace_background_color())
                            .child(self.render_workspace_with_command_pane(window, cx)),
                    )
                    .when(sidebar_chrome_visible && !sidebar_on_left, |this| {
                        this.child(self.render_sidebar_resize_divider(cx))
                    })
                    .when(sidebar_chrome_visible && !sidebar_on_left, |this| {
                        this.child(
                            div()
                                .w(px(self.sidebar_width))
                                .h_full()
                                .border_t_1()
                                .border_color(titlebar_button_border_color())
                                .when_some(self.sidebar.clone(), |this, sidebar| {
                                    this.child(sidebar)
                                }),
                        )
                    }),
            )
            .child(self.render_gpui_status_pet_presentation(cx))
            /*
            gpui-component's `Root` does not draw its dialog layer; the app view
            must render it. Without this every `open_alert_dialog` (the paste
            protection confirmation, the terminal close confirmation) still
            pushed an active dialog and moved keyboard focus onto it, but drew
            nothing: the pending paste was silently held, Cmd+V and typing
            went dead until focus moved elsewhere. Last child so it paints
            above the workspace.
            */
            .children(gpui_component::Root::render_dialog_layer(window, cx))
            .into_any_element();

        #[cfg(target_os = "linux")]
        let content = gpui_linux_client_window_frame(content, window);

        content
    }
}
