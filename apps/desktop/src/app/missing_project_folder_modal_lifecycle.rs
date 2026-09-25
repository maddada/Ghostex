//! Open, close, and sidebar bridge plumbing for the native Missing Project Folder dialog.
//! SEE-ALSO: apps/desktop/src/app/window/missing_project_folder_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), the `pickReplacementProjectFolder` and `removeProject` arms in apps/desktop/src/app/delayed_send.rs (the React host's routes, mirrored here).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `missingProjectFolder` modal kind. Refuses payloads the React host would
    /// have refused: `projectId`, `projectName` and `projectPath` must be non-empty.
    pub(crate) fn open_gpui_missing_project_folder_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let (Some(project_id), Some(project_name), Some(project_path)) =
            (text("projectId"), text("projectName"), text("projectPath"))
        else {
            return;
        };
        let config = MissingProjectFolderModalConfig {
            project_name,
            project_path,
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_missing_project_folder_modal_command(&project_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::MissingProjectFolder,
            MISSING_PROJECT_FOLDER_MODAL_WIDTH,
            MISSING_PROJECT_FOLDER_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiMissingProjectFolderModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    fn handle_gpui_missing_project_folder_modal_command(
        &mut self,
        project_id: &str,
        command: MissingProjectFolderModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::MissingProjectFolder;
        let allowed_project_id = Some(project_id.trim())
            .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id));
        match command {
            MissingProjectFolderModalCommand::Locate => {
                // The dialog stays open: the store (gx_store/create/folder_pick.rs) closes it once
                // the relocation succeeds, and a failed pick leaves it as is.
                if let Some(project_id) = allowed_project_id {
                    self.handle_gpui_pick_replacement_project_folder_message(
                        project_id.to_string(),
                        cx,
                    );
                }
            }
            MissingProjectFolderModalCommand::Remove => {
                if let Some(project_id) = allowed_project_id {
                    self.dispatch_gpui_sidebar_host_message(
                        serde_json::json!({
                            "projectId": project_id,
                            "type": "removeProject",
                        }),
                        cx,
                    );
                }
                self.release_native_app_modal_window(kind, cx);
            }
            MissingProjectFolderModalCommand::Cancel => {
                self.release_native_app_modal_window(kind, cx);
            }
        }
    }

    /// Consumes the app-modal host's `close` message (the store's
    /// answer to a successful relocation, gx_store/create/folder_pick.rs) by removing the native window.
    /// Returns false for any other message so the caller can route it elsewhere.
    pub(crate) fn receive_gpui_missing_project_folder_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if message.get("type").and_then(serde_json::Value::as_str) != Some("close") {
            return false;
        }
        let kind = GpuiAppModalKind::MissingProjectFolder;
        let removed = self
            .update_native_app_modal(
                kind,
                cx,
                |_modal: &mut GpuiMissingProjectFolderModalWindow, window, _cx| {
                    window.remove_window();
                },
            )
            .is_some();
        if removed {
            self.release_native_app_modal_window(kind, cx);
        }
        removed
    }
}
