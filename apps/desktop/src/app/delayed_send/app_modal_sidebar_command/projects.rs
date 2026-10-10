//! Projects modal and Quick Access rows: recent projects, focusing a session, and running a sidebar Action.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_projects_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "requestRecentProjects" => {
                let machine_id = command
                    .get("machineId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let normalized_machine_id = machine_id
                    .as_deref()
                    .and_then(gpui_normalize_remote_machine_id);
                let remote_target = normalized_machine_id
                    .as_deref()
                    .and_then(|machine_id| self.gpui_remote_gxserver_request_target(machine_id));
                let machine_name = normalized_machine_id
                    .as_deref()
                    .and_then(gpui_remote_machine_name_from_settings);
                let request = GpuiRecentProjectsRequest {
                    machine_id,
                    machine_name,
                    remote_target,
                };
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_recent_projects_result_message(&request),
                    cx,
                );
            }
            "restoreRecentProject" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Restore,
                    command,
                    cx,
                );
            }
            "closeProjectFromProjects" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Close,
                    command,
                    cx,
                );
            }
            "focusRecentProject" => {
                if let Some(project_id) =
                    command.get("projectId").and_then(serde_json::Value::as_str)
                {
                    let _ = self.dispatch_gpui_menu_bar_project_activation(project_id, cx);
                }
            }
            "removeRecentProject" => {
                self.handle_gpui_app_modal_recent_project_mutation(
                    GpuiRecentProjectMutation::Remove,
                    command,
                    cx,
                );
            }
            "copyRecentProjectPath" | "openRecentProjectInFinder" | "openRecentProjectTerminal" => {
                self.handle_gpui_app_modal_recent_project_path_action(command_type, command, cx);
            }
            // CDXC:Navigation 2026-09-23 DECISION:
            // User: selecting a Quick Access session while chat is collapsed shows it floating at the side and keeps the chat collapsed.
            "focusSession" => {
                if let Some(session_id) = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    if self.dispatch_gpui_command_palette_session_focus(&session_id, cx) {
                        // Dismiss Quick Access before giving the floating sessions pane focus, in
                        // the window the focus went to (workspace_windows/session_routing.rs).
                        self.close_gpui_quick_access_window(cx);
                        self.run_in_session_window(&session_id, cx, |app, cx| {
                            app.reveal_floating_sessions(cx)
                        });
                    }
                }
            }
            "runSidebarCommand" => {
                if let Some(command_id) = command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let run_mode = command
                        .get("runMode")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    let _ = self.dispatch_gpui_command_palette_run_sidebar_command(
                        &command_id,
                        run_mode.as_deref(),
                        cx,
                    );
                }
            }
            _ => {}
        }
    }
}
