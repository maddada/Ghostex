use crate::*;
use gpui::{MouseMoveEvent, WindowHandle};
use std::ffi::c_void;

unsafe extern "C" {
    fn GhostexGpuiCompanionRevealUpdate(
        root: *mut c_void,
        popup: *mut c_void,
        enabled: bool,
        on_right: bool,
        width: f64,
        titlebar_height: f64,
    ) -> bool;
    fn GhostexGpuiReparentPaneNativeView(
        view: *mut c_void,
        parent: *mut c_void,
        from: *mut c_void,
    ) -> bool;
}

pub(crate) struct CompanionReveal {
    pub(crate) window: WindowHandle<FloatingCompanionWindow>,
    pub(crate) native_view: *mut c_void,
    pub(crate) mode: TitlebarMode,
    pub(crate) project_id: Option<String>,
}

impl CompanionReveal {
    pub(crate) fn dispose_native_host(&self) {
        unsafe {
            GhostexGpuiCompanionRevealUpdate(
                std::ptr::null_mut(),
                self.native_view,
                false,
                false,
                0.0,
                0.0,
            );
        }
        unregister_gpui_first_responder_callback_target(self.native_view);
        unregister_gpui_keyboard_router_target(self.native_view);
        unregister_gpui_terminal_key_event_callback_target(self.native_view);
    }
}

pub(crate) struct FloatingCompanionWindow {
    app: gpui::WeakEntity<GhostexGpuiApp>,
    _subscription: gpui::Subscription,
}

impl Render for FloatingCompanionWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        app.update(cx, |app, cx| {
            if !app.companion_reveal.as_ref().is_some_and(|reveal| {
                reveal.mode == app.active_mode
                    && reveal.project_id == app.agents_workspace_project_id
                    && !app.project_editor_shell.left_companion_visible
            }) {
                return div().into_any_element();
            }
            app.project_editor_companion_terminal_mount_slot_bounds
                .clear();
            app.project_editor_companion_layout_bounds = None;
            if matches!(app.shell_focus, ShellFocusTarget::ProjectEditorCompanion(_)) {
                app.drain_pending_keyboard_handoff(window, cx);
            }
            app.sync_session_chat_pane_focus(window, cx, false);
            let width = app.floating_companion_width();
            let offset = if app.sidebar_side == GpuiSidebarSide::Right {
                0.0
            } else {
                window.bounds().size.width.as_f32() - width
            };
            div()
                .size_full()
                .overflow_hidden()
                .bg(workspace_background_color())
                .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, window, cx| {
                    app.handle_project_editor_companion_split_resize_drag_move(event, window, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|app, event: &MouseUpEvent, window, cx| {
                        app.handle_project_editor_companion_split_resize_mouse_up(
                            event, window, cx,
                        );
                    }),
                )
                .child(div().flex().w(px(width)).h_full().ml(px(offset)).child(
                    app.render_project_editor_companion_pane(app.active_mode, 1.0, window, cx),
                ))
                .into_any_element()
        })
    }
}

impl GhostexGpuiApp {
    fn floating_companion_width(&self) -> f32 {
        let width = self.main_window_bounds.size.width.as_f32();
        (width
            * project_editor_companion_width_ratio(
                self.project_editor_shell.left_companion_width_ratio,
            ))
        .max(PROJECT_EDITOR_COMPANION_MIN_WIDTH)
        .min(width)
    }

    /// CDXC:Workarea 2026-09-09 DECISION:
    /// User: the bottom-half hover must reveal the companion floating like Sessions, not expand its docked layout.
    /// CDXC:Workarea 2026-09-09 WHY:
    /// GPUI open_window synchronously draws its root, whose render updates this app entity.
    /// Opening it inside the hover handler's app update caused the repeated double-lease panic in the September 9 crash reports.
    /// Defer creation onto App itself, outside any app-entity update, then attach the completed window in a separate update.
    pub(crate) fn open_floating_companion(&mut self, cx: &mut gpui::Context<Self>) {
        let app = cx.weak_entity();
        let requested_mode = self.active_mode;
        let requested_project = self.agents_workspace_project_id.clone();
        gpui::App::defer(cx, move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let options = {
                let this = app.read(cx);
                if this.companion_reveal.is_some()
                    || !this.sidebar_collapsed
                    || !this.active_mode.is_project_editor_mode()
                    || this.project_editor_shell.left_companion_visible
                    || this.active_mode != requested_mode
                    || this.agents_workspace_project_id != requested_project
                {
                    return;
                }
                let bounds = Bounds::new(
                    this.main_window_bounds.origin,
                    size(
                        px(this.floating_companion_width()),
                        px(
                            (this.main_window_bounds.size.height.as_f32() - TITLEBAR_HEIGHT)
                                .max(1.0),
                        ),
                    ),
                );
                gpui::WindowOptions {
                    window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                    display_id: this.main_window_display_id,
                    focus: false,
                    show: false,
                    kind: gpui::WindowKind::PopUp,
                    is_movable: false,
                    is_resizable: false,
                    is_minimizable: false,
                    titlebar: None,
                    ..Default::default()
                }
            };
            let observed_app = app.clone();
            let result = cx.open_window(options, move |_, cx| {
                cx.new(|cx| FloatingCompanionWindow {
                    app: observed_app.downgrade(),
                    _subscription: cx.observe(&observed_app, |_, _, cx| cx.notify()),
                })
            });
            let handle = match result {
                Ok(handle) => handle,
                Err(error) => {
                    app.update(cx, |this, cx| {
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Companion unavailable",
                            &error.to_string(),
                            cx,
                        );
                    });
                    return;
                }
            };
            let native_view = handle
                .update(cx, |_, window, _| cef_parent_native_view(window))
                .ok()
                .and_then(Result::ok);
            let Some(native_view) = native_view else {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                return;
            };
            app.update(cx, |this, cx| {
                register_gpui_first_responder_callback_target(
                    native_view,
                    cx.weak_entity(),
                    cx.to_async(),
                );
                register_gpui_keyboard_router_target(native_view, cx.weak_entity(), cx.to_async());
                register_gpui_terminal_key_event_callback_target(
                    native_view,
                    cx.weak_entity(),
                    cx.to_async(),
                );
                cef::install_first_responder_observer(native_view);
                this.companion_reveal = Some(CompanionReveal {
                    window: handle,
                    native_view,
                    mode: requested_mode,
                    project_id: requested_project,
                });
                this.agents_terminal_runtime_sessions
                    .reconcile_with_workspace(&this.agents_workspace);
                this.sync_project_editor_companion_terminal_selection();
                this.update_active_mode_cef_child_visibility(cx);
                this.sync_floating_companion(cx);
                cx.notify();
            });
        });
    }

    pub(crate) fn sync_floating_companion(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(reveal) = self.companion_reveal.as_ref() else {
            return;
        };
        let enabled = self.sidebar_collapsed
            && !self.project_editor_shell.left_companion_visible
            && reveal.mode == self.active_mode
            && reveal.project_id == self.agents_workspace_project_id;
        let visible = unsafe {
            GhostexGpuiCompanionRevealUpdate(
                self.parent_ns_view,
                reveal.native_view,
                enabled,
                self.sidebar_side == GpuiSidebarSide::Right,
                self.floating_companion_width() as f64,
                TITLEBAR_HEIGHT as f64,
            )
        };
        if !visible {
            self.close_floating_companion(cx);
        }
    }

    pub(crate) fn companion_native_parent(&self) -> *mut c_void {
        self.companion_reveal
            .as_ref()
            .map_or(self.parent_ns_view, |reveal| reveal.native_view)
    }

    pub(crate) fn reparent_floating_companion_terminal_hosts(&self) {
        for host in self
            .project_editor_companion_terminal_host_native_views
            .values()
        {
            unsafe {
                GhostexGpuiReparentPaneNativeView(
                    host.native_view_handle().as_ptr(),
                    self.companion_native_parent(),
                    std::ptr::null_mut(),
                );
            }
        }
    }

    pub(crate) fn close_floating_companion(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(reveal) = self.companion_reveal.take() else {
            return;
        };
        unsafe {
            GhostexGpuiCompanionRevealUpdate(
                self.parent_ns_view,
                reveal.native_view,
                false,
                false,
                0.0,
                0.0,
            );
        }
        for surface in self.agents_chat_surfaces.values() {
            surface.update(cx, |surface, _| {
                surface.reparent_native_view(self.parent_ns_view, reveal.native_view)
            });
        }
        for host in self
            .project_editor_companion_terminal_host_native_views
            .values()
        {
            unsafe {
                GhostexGpuiReparentPaneNativeView(
                    host.native_view_handle().as_ptr(),
                    self.parent_ns_view,
                    reveal.native_view,
                );
            }
        }
        if matches!(
            self.shell_focus,
            ShellFocusTarget::ProjectEditorCompanion(_)
        ) {
            self.focus_default_surface_for_active_mode(cx);
            cef::focus_gpui_root_view(self.parent_ns_view);
        }
        self.project_editor_companion_terminal_mount_slot_bounds
            .clear();
        self.project_editor_companion_layout_bounds = None;
        self.finish_project_editor_companion_split_resize_drag(cx);
        self.update_active_mode_cef_child_visibility(cx);
        reveal.dispose_native_host();
        let _ = reveal
            .window
            .update(cx, |_, window, _| window.remove_window());
        cx.notify();
    }
}
