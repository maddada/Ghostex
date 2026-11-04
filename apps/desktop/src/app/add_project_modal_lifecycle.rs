//! Open and host plumbing for the native Add Project dialog.
//! SEE-ALSO: apps/desktop/src/app/window/add_project_modal/ (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/remote_conn/project_browse_and_add.rs (`run_gpui_add_project_dialog_operation`, which answers every round trip).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the native dialog for the `addProject` open message. Every opener (the sidebar's
    /// empty state and menu, a remote machine's header, Quick Access, the menu bar) sends the
    /// same message; a `machineId` preselects that machine and skips the machine step.
    pub(crate) fn open_gpui_add_project_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let initial_machine_id = message
            .get("machineId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|machine_id| !machine_id.is_empty())
            .map(str::to_string);
        let config = AddProjectModalConfig {
            machines: parse_add_project_machine_options(
                &self.gpui_add_project_dialog_machine_options(),
            ),
            initial_machine_id,
            active_project_cwd: None,
            client_platform: gpui_add_project_dialog_local_platform().to_string(),
            palette: self.gpui_native_dialog_palette(),
            clone_job_poll_interval: AddProjectModalConfig::CLONE_JOB_POLL_INTERVAL,
            slow_operation_notice: AddProjectModalConfig::SLOW_OPERATION_NOTICE,
            // A handoff's clone of the session's repository (gx_store/git/export_handoff_remote.rs).
            initial_clone_url: message
                .get("cloneUrl")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .map(str::to_string),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_add_project_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::AddProject,
            ADD_PROJECT_MODAL_WIDTH,
            ADD_PROJECT_MODAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiAddProjectModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    fn handle_gpui_add_project_modal_command(
        &mut self,
        command: AddProjectModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::AddProject;
        match command {
            AddProjectModalCommand::Request {
                request_id,
                operation,
                machine_id,
                params,
            } => {
                let Some(operation) = GpuiAddProjectDialogOperation::from_wire(operation) else {
                    return;
                };
                let empty = serde_json::Map::new();
                let params = params.as_object().unwrap_or(&empty);
                let added_on =
                    (operation == GpuiAddProjectDialogOperation::Add).then(|| machine_id.clone());
                self.run_gpui_add_project_dialog_operation(
                    operation,
                    Some(machine_id.as_str()),
                    params,
                    move |app, result, cx| {
                        if let (Some(machine_id), Ok(answer)) = (&added_on, &result) {
                            app.git_export_note_added_project(machine_id, answer);
                        }
                        app.update_native_app_modal(
                            kind,
                            cx,
                            |modal: &mut GpuiAddProjectModalWindow, window, cx| {
                                modal.receive_response(request_id, result, window, cx);
                            },
                        );
                    },
                    cx,
                );
            }
            AddProjectModalCommand::Close => {
                self.release_native_app_modal_window(kind, cx);
                // Opened from a handoff's destination: Handoff / Export comes back now.
                self.git_export_add_project_closed(cx);
            }
        }
    }
}
