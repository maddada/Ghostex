//! The Quick Access Commands rows whose message had no owner: Wake/Sleep Pet, Quick Terminal,
//! All Automations, Open File/Folder Location and every "Open In: X".
//!
//! CDXC:AppModal 2026-09-25 WHY:
//! These rows post `togglePetOverlay`, `createChat`, `openAutomationsPage`,
//! `openCurrentProjectInFinder` and `openCurrentProjectInTarget` as a modal-host `sidebarCommand`.
//! `handle_gpui_app_modal_sidebar_command` had no arm for any of them and the app runtime never saw
//! them, so these rows did nothing. The two the sidebar runtime answered (Quick Terminal and
//! All Automations) now enter the sidebar's own command route, which is where the More menu's All
//! Automations already goes, and the store answers them there (gx_store/create/claim.rs,
//! gx_store/focus_perform.rs). Quick Browser Tab
//! (`openBrowserChat`) is answered by the create family's own arm. The other three are Rust's: the pet toggles the same `petOverlayEnabled` setting its menu writes, Finder
//! is the native project path action, and "Open In" launches the chosen target on the active
//! project without making it the titlebar's default.
//!
//! SEE-ALSO: apps/desktop/src/app/delayed_send.rs (`handle_gpui_app_modal_sidebar_command`).

use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;
use serde_json::{Map, Value, json};

use crate::app::helpers::*;
use crate::*;

/// The `sidebarCommand` types this file answers.
pub(crate) const QUICK_ACCESS_COMMAND_ROW_TYPES: [&str; 5] = [
    "togglePetOverlay",
    "createChat",
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
            "togglePetOverlay" => self.toggle_pet_overlay_from_quick_access(cx),
            "createChat" | "openAutomationsPage" => {
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

    /// The row reads "Sleep Pet" while the pet shows and "Wake Pet" while it does not.
    fn toggle_pet_overlay_from_quick_access(&mut self, cx: &mut gpui::Context<Self>) {
        let enabled = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("petOverlayEnabled")
            .and_then(Value::as_bool)
            == Some(true);
        if enabled {
            self.sleep_gpui_pet_overlay_from_context_menu(cx);
        } else {
            self.handle_gpui_app_modal_update_settings_patch_message(
                &json!({ "patch": { "petOverlayEnabled": true } }),
                cx,
            );
        }
    }

    /// Opens the active project's folder in Finder. A remote project has no local folder, which
    /// the user is told, the way the sidebar's Open Folder tells it.
    fn open_active_project_in_finder_from_quick_access(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(project_id) = self.gpui_app_modal_active_project_id() else {
            return;
        };
        if gpui_remote_project_reference_from_project_id(&project_id).is_some() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote project open unavailable",
                "Remote project locations cannot be opened in the local file manager.",
                cx,
            );
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
