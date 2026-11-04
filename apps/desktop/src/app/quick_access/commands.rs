//! The Quick Access Commands rows whose message had no owner: All Automations, Open File/Folder
//! Location, every "Open In: X" and every "Open <extension>".
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! These rows post `openAutomationsPage`, `openCurrentProjectInFinder` and
//! `openCurrentProjectInTarget` as a modal-host `sidebarCommand`.
//! `handle_gpui_app_modal_sidebar_command` had no arm for any of them and the app runtime never saw
//! them, so these rows did nothing. The one the sidebar runtime answered (All Automations) now
//! enters the sidebar's own command route, which is where the More menu's All Automations already
//! goes, and the store answers it there (gx_store/focus_perform.rs). Quick Browser Tab
//! (`openBrowserChat`) is answered by the create family's own arm. The other two are Rust's: Finder
//! is the native project path action, and "Open In" launches the chosen target on the active
//! project without making it the titlebar's default.
//!
//! SEE-ALSO: apps/desktop/src/app/delayed_send/app_modal_sidebar_command/dispatch.rs (`handle_gpui_app_modal_sidebar_command`).

use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;
use serde_json::{Map, Value, json};

use crate::app::helpers::*;
use crate::*;

/// The `sidebarCommand` types this file answers.
pub(crate) const QUICK_ACCESS_COMMAND_ROW_TYPES: [&str; 5] = [
    ghostex_gx_core::APP_LIFECYCLE_MESSAGE_TYPE,
    "launchExtension",
    "openAutomationsPage",
    "openCurrentProjectInFinder",
    "openCurrentProjectInTarget",
];

impl GhostexGpuiApp {
    /// Runs one Quick Access Commands row message. `command_type` is one of
    /// [`QUICK_ACCESS_COMMAND_ROW_TYPES`].
    pub(crate) fn run_quick_access_command_row(
        &mut self,
        command_type: &str,
        command: &Map<String, Value>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            // Never posted on the web build, which is not offered these rows.
            #[cfg(not(target_family = "wasm"))]
            ghostex_gx_core::APP_LIFECYCLE_MESSAGE_TYPE => {
                if let Some(action) = command.get("action").and_then(Value::as_str) {
                    self.run_app_lifecycle_action(action, cx);
                }
            }
            // An "Open <extension>" row (`extensions/launch.rs`); the web build lists none.
            #[cfg(not(target_family = "wasm"))]
            "launchExtension" => {
                if let Some(extension_id) = command.get("extensionId").and_then(Value::as_str) {
                    self.launch_extension_from_quick_access(extension_id, window, cx);
                }
            }
            "openAutomationsPage" => {
                self.dispatch_native_sidebar_command(json!({ "type": command_type }), cx);
            }
            "openCurrentProjectInFinder" => {
                self.open_active_project_in_finder_from_quick_access(cx)
            }
            "openCurrentProjectInTarget" => {
                if let Some(target_id) = command.get("targetId").and_then(Value::as_str) {
                    self.open_active_project_in_target_from_quick_access(target_id, window, cx);
                }
            }
            _ => {}
        }
    }

    /// Opens the active project's folder in Finder. A remote project opens in the Files view, the
    /// way the sidebar's Open Folder opens it (`ghostex_gx_core::plan_remote_project_open`).
    fn open_active_project_in_finder_from_quick_access(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(project_id) = self.gpui_app_modal_active_project_id() else {
            return;
        };
        if self.open_remote_project_from_active_route(
            &project_id,
            ghostex_gx_core::RemoteProjectOpen::Folder,
            cx,
        ) {
            return;
        }
        let payload = json!({
            "action": "openActiveWorkspaceProjectInFinder",
            "projectId": project_id,
            "type": ghostex_gx_core::NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
            "version": ghostex_gx_core::NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
        });
        self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
    }

    /// Opens the active project in one visible Open In target, once.
    fn open_active_project_in_target_from_quick_access(
        &mut self,
        target_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = gpui_visible_open_targets_from_current_settings()
            .into_iter()
            .find(|target| target.id == target_id)
        else {
            return;
        };
        // A remote project's folder is on another computer: gx-core decides what the target can do.
        if let Some(project_id) = self.gpui_app_modal_active_project_id()
            && self.open_remote_project_from_active_route(
                &project_id,
                ghostex_gx_core::RemoteProjectOpen::Target {
                    id: &target.id,
                    label: Some(&target.label),
                },
                cx,
            )
        {
            return;
        }
        let Some(project_path) = self.active_project_open_in_path() else {
            window.push_notification(
                Notification::warning("Open an active project before using Open In."),
                cx,
            );
            cx.notify();
            return;
        };
        if let Err(message) = gpui_launch_open_target(&target, &project_path) {
            window.push_notification(Notification::warning(message), cx);
            cx.notify();
        }
    }
}
