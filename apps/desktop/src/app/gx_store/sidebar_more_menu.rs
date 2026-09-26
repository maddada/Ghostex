//! Two More menu rows whose whole answer already lives in Rust: Join Discord and Keep Awake.
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
    /// Answers the More menu's Join Discord and Keep Awake rows. Returns whether it did, in which
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
            _ => false,
        }
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
