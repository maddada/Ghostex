use crate::*;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct CommandPaneAutoMinimize {
    keep_open_projects: HashSet<Option<String>>,
    pub(crate) idle_since: Option<Instant>,
    context: Option<(u64, TitlebarMode, bool)>,
    delay: Option<Duration>,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiCommandPanePointerActive(
        root: *mut std::ffi::c_void,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> bool;
}

impl GhostexGpuiApp {
    /// CDXC:CommandPane 2026-09-12 DECISION:
    /// User: only show the Keep open icon when Auto-minimize Commands pane is enabled.
    pub(crate) fn command_pane_keep_open_control_visible(&self, expanded_chrome: bool) -> bool {
        expanded_chrome
            && shared_settings::shared_sidebar_settings_snapshot()
                .command_pane_auto_minimize_delay()
                .is_some()
    }

    pub(crate) fn command_pane_keep_open(&self) -> bool {
        self.command_pane_auto_minimize
            .keep_open_projects
            .contains(&self.command_pane_project_id)
    }

    /// CDXC:CommandPane 2026-09-12 DECISION:
    /// User: enable auto-minimize by default after one minute away from the pane, with 15s, 30s, 1m, 2m and 5m choices, and a highlighted Keep open toggle beside Minimize.
    /// Keep open applies to the current project until toggled off or manually minimized, and is not saved across restarts.
    /// Focus, hover and interaction restart the countdown; background commands and output do not, and minimizing leaves commands running.
    pub(crate) fn toggle_command_pane_keep_open(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.command_pane.is_expanded() {
            return;
        }
        if !self
            .command_pane_auto_minimize
            .keep_open_projects
            .remove(&self.command_pane_project_id)
        {
            self.command_pane_auto_minimize
                .keep_open_projects
                .insert(self.command_pane_project_id.clone());
        }
        self.command_pane_auto_minimize.idle_since = None;
        cx.notify();
    }

    pub(crate) fn clear_command_pane_keep_open(&mut self) {
        self.command_pane_auto_minimize
            .keep_open_projects
            .remove(&self.command_pane_project_id);
        self.command_pane_auto_minimize.idle_since = None;
    }

    pub(crate) fn start_command_pane_auto_minimize_polling(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let result = window.update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| {
                        this.poll_command_pane_auto_minimize(window, cx)
                    })
                });
                if !matches!(result, Ok(Ok(()))) {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_command_pane_auto_minimize(&mut self, window: &Window, cx: &mut gpui::Context<Self>) {
        let context = (
            self.command_pane_project_epoch,
            self.active_mode,
            self.command_pane.is_expanded(),
        );
        if self.command_pane_auto_minimize.context != Some(context) {
            self.command_pane_auto_minimize.context = Some(context);
            self.command_pane_auto_minimize.idle_since = None;
        }
        if !self.command_pane.is_expanded() {
            self.command_pane_auto_minimize.idle_since = None;
            return;
        }
        let delay =
            shared_settings::shared_sidebar_settings_snapshot().command_pane_auto_minimize_delay();
        if self.command_pane_auto_minimize.delay != delay {
            self.command_pane_auto_minimize.delay = delay;
            self.command_pane_auto_minimize.idle_since = None;
            cx.notify();
        }
        let Some(delay) = delay else {
            self.command_pane_auto_minimize.idle_since = None;
            return;
        };
        if self.command_pane_keep_open()
            || !window.is_window_active()
            || self.command_pane_project_id != self.agents_workspace_project_id
            || self.command_pane_has_input_focus()
            || self.command_pane_pointer_active(window)
            || self.command_pane.resize_drag.is_some()
            || self.command_split_drag.is_some()
            || self.command_tab_drag_active
            || self.workspace_tab_drag_active
        {
            self.command_pane_auto_minimize.idle_since = None;
            return;
        }
        let since = self
            .command_pane_auto_minimize
            .idle_since
            .get_or_insert_with(Instant::now);
        if since.elapsed() < delay {
            return;
        }

        // Focus is already elsewhere. Collapsing must not steal it back from
        // the sidebar, editor or chat through the manual minimize focus path.
        self.command_pane.collapse();
        self.restore_non_command_focus_after_surface_removed(None, cx);
        self.command_pane_auto_minimize.idle_since = None;
        self.clear_command_resize_hover_state();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }

    fn command_pane_has_input_focus(&self) -> bool {
        // The sidebar can own the keyboard while the last workspace focus
        // still names Commands. Use the physical owner for native children.
        #[cfg(target_os = "macos")]
        if matches!(
            self.first_responder_target,
            FirstResponderTarget::CefSurface(_)
        ) {
            return false;
        }
        matches!(
            self.keyboard_owner_session(),
            Some(KeyboardOwnerSession::Command(_))
        ) || self.shell_focus == ShellFocusTarget::CommandPane
    }

    fn command_pane_pointer_active(&self, _window: &Window) -> bool {
        let Some(bounds) = self.command_pane_layout_bounds else {
            return true;
        };
        #[cfg(target_os = "macos")]
        {
            // Native CEF/terminal children can consume mouse moves before
            // GPUI sees them. Read the actual pointer without routing events.
            unsafe {
                GhostexGpuiCommandPanePointerActive(
                    self.parent_ns_view,
                    bounds.origin.x.as_f32() as f64,
                    bounds.origin.y.as_f32() as f64,
                    bounds.size.width.as_f32() as f64,
                    bounds.size.height.as_f32() as f64,
                )
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            bounds.contains(&_window.mouse_position())
        }
    }
}
