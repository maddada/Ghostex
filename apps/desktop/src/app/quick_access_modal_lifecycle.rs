//! Open, snapshot routing, command dispatch and close for the native Quick Access window.
//!
//! The four Quick Access modal kinds (`commandPalette`, `recentProjects`, `previousSessions`,
//! `stashedPrompts`) are one window: opening a second kind while it is showing re-targets the open
//! window's tab instead of tearing it down, which is what kept the React child window stable when
//! the tab rail switched pages.
//! SEE-ALSO: apps/desktop/src/app/window/quick_access/ (the window), apps/desktop/src/app/quick_access/host.rs
//! (the gx-core Quick Access model that answers every command and publishes every snapshot).

use crate::app::window::quick_access::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens (or re-targets) Quick Access for one of its four modal kinds.
    pub(crate) fn open_gpui_quick_access_modal(
        &mut self,
        kind: GpuiAppModalKind,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(tab) = QuickAccessTabId::from_modal_kind(kind) else {
            return;
        };
        let mut open = serde_json::json!({ "type": "open", "tab": tab.wire_name() });
        for (source, target) in [
            ("initialQuery", "query"),
            ("initialProjectId", "projectId"),
            ("initialSessionScope", "scope"),
            ("machineId", "machineId"),
            ("projectId", "promptProjectId"),
            ("sessionId", "promptSessionId"),
            ("initialScope", "promptScope"),
        ] {
            if let Some(value) = message.get(source) {
                open[target] = value.clone();
            }
        }
        // A second open request while the window is up only switches its tab.
        if self
            .native_app_modal_kind()
            .is_some_and(|open_kind| QuickAccessTabId::from_modal_kind(open_kind).is_some())
        {
            if let Some(modal) = self.native_app_modal.as_mut() {
                modal.kind = kind;
            }
            self.dispatch_gpui_quick_access_command(open, cx);
            if let Some(modal) = self.native_app_modal.as_ref() {
                let _ = modal.window.update(cx, |_root, window, _cx| {
                    window.activate_window();
                });
            }
            return;
        }
        let host = self.native_app_modal_host(cx, move |app, command: serde_json::Value, cx| {
            app.handle_gpui_quick_access_command(&command, cx);
        });
        self.open_native_app_modal(
            kind,
            APP_MODAL_HOST_COMMAND_PALETTE_WINDOW_WIDTH,
            APP_MODAL_HOST_PREVIOUS_SESSIONS_WINDOW_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiQuickAccessWindow::new(host, window, cx)),
            cx,
        );
        self.dispatch_gpui_quick_access_command(open, cx);
    }

    /// The model's display state for the open window: a snapshot, a row's actions menu, or close.
    pub(crate) fn apply_native_quick_access_update(
        &mut self,
        update: QuickAccessUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(kind) = self.native_app_modal_kind() else {
            return;
        };
        if QuickAccessTabId::from_modal_kind(kind).is_none() {
            return;
        }
        match update {
            QuickAccessUpdate::Snapshot(snapshot) if snapshot.version == 1 => {
                // The window's modal kind follows the tab so the "one app modal at
                // a time" bookkeeping and the hotkey return-focus target stay true.
                if let Some(modal) = self.native_app_modal.as_mut() {
                    modal.kind = snapshot.tab.modal_kind();
                }
                self.update_native_app_modal::<GpuiQuickAccessWindow, ()>(
                    snapshot.tab.modal_kind(),
                    cx,
                    move |view, window, cx| view.apply_snapshot(snapshot, window, cx),
                );
            }
            QuickAccessUpdate::Menu { items } => {
                self.update_native_app_modal::<GpuiQuickAccessWindow, ()>(
                    kind,
                    cx,
                    move |view, _window, cx| view.apply_menu(items, cx),
                );
            }
            QuickAccessUpdate::Close => {
                self.close_gpui_quick_access_window(cx);
            }
            QuickAccessUpdate::Snapshot(_) => {}
        }
    }

    fn handle_gpui_quick_access_command(
        &mut self,
        command: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if command.get("type").and_then(serde_json::Value::as_str) == Some("close") {
            self.close_gpui_quick_access_window(cx);
            return;
        }
        self.dispatch_gpui_quick_access_command(command.clone(), cx);
    }

    pub(crate) fn close_gpui_quick_access_window(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(kind) = self.native_app_modal_kind() else {
            return;
        };
        if QuickAccessTabId::from_modal_kind(kind).is_none() {
            return;
        }
        let return_focus_target = self.app_modal_command_return_focus_target;
        // `remove_native_app_modal_window` tells the controller the window is gone.
        self.remove_native_app_modal_window(cx);
        self.app_modal_command_return_focus_target = return_focus_target;
        self.restore_gpui_app_modal_command_return_focus_if_needed(cx);
    }

    /// Hands one command to the Quick Access model.
    pub(crate) fn dispatch_gpui_quick_access_command(
        &mut self,
        command: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.quick_access_command(command, cx);
    }
}
