//! The More menu rows whose whole answer already lives in Rust: Join Discord, Keep Awake, the
//! app's panels (Ask Ghostex, Tips & Tricks, Resources, Dev servers, Extensions) and the app's own
//! New Window, Check for Updates, Restart and Quit rows.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! Join Discord (`openExternalUrl`) used to leave Rust for the app runtime, which posted the same
//! message straight back over the modal host to `receive_gpui_titlebar_resources_open_external_url_message`.
//! Keep Awake (`runTitlebarKeepAwakeCommand`, beta) reached the runtime too, which had no arm for
//! it, so the row did nothing. Both are answered here and go no further, so the runtime never sees
//! either: the first would otherwise open the page twice once a runtime arm existed, and the
//! second is the titlebar's own Keep Awake period through the same start and stop functions.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/navigation.rs (the rows),
//! apps/desktop/src/app/os_integration/keep_awake_core.rs (the Keep Awake runtime).

use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::shared_settings::SharedKeepAwakeDurationMinutes;

impl GhostexGpuiApp {
    /// Answers the More menu's Join Discord, Keep Awake, panel and app rows. Returns whether it did, in which
    /// case the command must NOT also reach the old runtime.
    pub(crate) fn gx_store_run_sidebar_more_menu(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        match message.get("type").and_then(Value::as_str) {
            Some("openExternalUrl") => {
                self.receive_gpui_titlebar_resources_open_external_url_message(message);
                true
            }
            Some("runTitlebarKeepAwakeCommand") => {
                self.run_sidebar_keep_awake_command(message, cx);
                true
            }
            // A row of Tools or Help that opens one of the app's panels (titlebar/menu_panels.rs).
            // The panel is a window of its own, opened once this click has left the sidebar's menu.
            Some(ghostex_gx_core::MENU_PANEL_MESSAGE_TYPE) => {
                if let Some(panel) = message.get("panel").and_then(Value::as_str) {
                    let panel = panel.to_owned();
                    self.defer_in_main_window(cx, move |app, window, cx| {
                        app.open_sidebar_menu_panel(&panel, window, cx);
                    });
                }
                true
            }
            Some(ghostex_gx_core::APP_LIFECYCLE_MESSAGE_TYPE) => {
                if let Some(action) = message.get("action").and_then(Value::as_str) {
                    self.run_app_lifecycle_action(action, cx);
                }
                true
            }
            _ => false,
        }
    }

    /// New Window, Check for Updates, Restart and the two Quits, from the sidebar menu or a Quick
    /// Access Commands row: the same actions the macOS menu bar dispatches, so each has one handler
    /// (helpers/os_cli/main_menus.rs, and main.rs for New Window). Deferred because the click
    /// arrives inside a window update, where dispatching into the active window is refused.
    pub(crate) fn run_app_lifecycle_action(&mut self, action: &str, cx: &mut gpui::Context<Self>) {
        use crate::app::actions::{
            CheckForGhostexGpuiUpdates, QuitGhostexGpui, QuitGhostexGpuiAndBackgroundServices,
            RestartGhostexGpui,
        };
        if action == "newWindow" {
            // From the window the row was in, not whichever window is key: Quick Access is
            // (app/workspace_windows/).
            let source = self
                .main_window_handle
                .map(|handle| (handle, cx.weak_entity()));
            cx.defer(move |cx| {
                crate::app::workspace_windows::open_new_workspace_window_from(source, cx)
            });
            return;
        }
        let action: Box<dyn gpui::Action> = match action {
            "checkForUpdates" => Box::new(CheckForGhostexGpuiUpdates),
            "restart" => Box::new(RestartGhostexGpui),
            "quit" => Box::new(QuitGhostexGpui),
            "quitWithBackgroundServices" => Box::new(QuitGhostexGpuiAndBackgroundServices),
            _ => return,
        };
        cx.defer(move |cx| cx.dispatch_action(action.as_ref()));
    }

    /// `{action: 'start', durationMinutes}` starts a Keep Awake period with one of the three
    /// durations the titlebar menu offers; `{action: 'stop'}` stops it the way "Don't keep awake"
    /// does. Any other shape is dropped, as the runtime dropped every shape.
    fn run_sidebar_keep_awake_command(&mut self, message: &Value, cx: &mut gpui::Context<Self>) {
        match message.get("action").and_then(Value::as_str) {
            Some("start") => {
                let Some(duration) = message
                    .get("durationMinutes")
                    .and_then(Value::as_u64)
                    .and_then(SharedKeepAwakeDurationMinutes::from_minutes)
                else {
                    return;
                };
                // The start function warns through the main window's notifications.
                self.defer_in_main_window(cx, move |app, window, cx| {
                    app.start_gpui_keep_awake_period(duration, window, cx);
                    app.gx_store_install_sidebar_list(cx);
                });
            }
            Some("stop") => {
                self.stop_gpui_keep_awake_from_titlebar(cx);
                self.gx_store_install_sidebar_list(cx);
            }
            _ => {}
        }
    }
}
