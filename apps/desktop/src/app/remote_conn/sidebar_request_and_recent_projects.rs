use gpui::ClipboardItem;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn gpui_remote_gxserver_request_target(
        &self,
        remote_machine_id: &str,
    ) -> Option<GpuiRemoteGxserverRequestTarget> {
        self.remote_gxserver_connections
            .get(remote_machine_id)
            .map(GpuiRemoteGxserverConnection::request_target)
    }

    pub(crate) fn handle_gpui_app_modal_recent_project_path_action(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project_id) = command
            .get("projectId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|project_id| !project_id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        if gpui_remote_project_reference_from_project_id(project_id.as_str()).is_some() {
            let action = match command_type {
                "copyRecentProjectPath" => {
                    GpuiSidebarNativeProjectPathAction::CopyRemoteProjectPath
                }
                "openRecentProjectInFinder" => {
                    GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal
                }
                "openRecentProjectTerminal" => {
                    GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal
                }
                _ => return,
            };
            self.handle_gpui_remote_project_native_action(
                GpuiSidebarNativeProjectPathActionMessage {
                    action,
                    file_path: None,
                    placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
                    preferred_interface: GpuiPreferredAgentInterface::Terminal,
                    project_id,
                    keep_view: false,
                },
                cx,
            );
            return;
        }
        if !gpui_remote_sidebar_project_id_allowed(project_id.as_str()) {
            return;
        }
        let action = match command_type {
            "copyRecentProjectPath" => GpuiSidebarNativeProjectPathAction::CopyRecentProjectPath,
            "openRecentProjectInFinder" => {
                GpuiSidebarNativeProjectPathAction::OpenRecentProjectInFinder
            }
            _ => return,
        };
        let message = GpuiSidebarNativeProjectPathActionMessage {
            action,
            file_path: None,
            placement: GpuiWorkspaceTerminalFocusPlacement::Tab,
            preferred_interface: GpuiPreferredAgentInterface::Terminal,
            project_id,
            keep_view: false,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { execute_gpui_sidebar_native_project_path_action(message) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(GpuiSidebarNativeProjectPathActionResult::Copied(path)) => {
                    gpui_copy_to_clipboard(ClipboardItem::new_string(path), cx);
                }
                Ok(GpuiSidebarNativeProjectPathActionResult::Opened) => {}
                Err(message) => this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Native action unavailable",
                    &message,
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_app_modal_recent_project_mutation(
        &mut self,
        mutation: GpuiRecentProjectMutation,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scoped_project_id) = command
            .get("projectId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|project_id| !project_id.is_empty())
        else {
            return;
        };
        let (machine_id, machine_name, project_id, remote_target) = if let Some(reference) =
            gpui_remote_project_reference_from_project_id(scoped_project_id)
        {
            let machine_name =
                gpui_remote_machine_name_from_settings(reference.remote_machine_id.as_str());
            let remote_target =
                self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
            (
                Some(reference.remote_machine_id),
                machine_name,
                reference.project_id,
                remote_target,
            )
        } else {
            if !gpui_remote_sidebar_project_id_allowed(scoped_project_id) {
                return;
            }
            (None, None, scoped_project_id.to_string(), None)
        };
        let activation_project_id = scoped_project_id.to_string();
        let request = GpuiRecentProjectsRequest {
            machine_id: machine_id.clone(),
            machine_name,
            remote_target,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (mutated, result_message) = background
                .spawn(async move {
                    gpui_recent_project_mutation_and_result(mutation, project_id, request)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(result_message, cx);
                if mutated {
                    if mutation == GpuiRecentProjectMutation::Restore {
                        this.dispatch_gpui_menu_bar_project_activation(&activation_project_id, cx);
                    }
                    if matches!(
                        mutation,
                        GpuiRecentProjectMutation::Close | GpuiRecentProjectMutation::Restore
                    ) && let Some(machine_id) = machine_id
                    {
                        this.refresh_gpui_remote_gxserver_presentation_in_background(&machine_id);
                    }
                    return;
                }
                this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Recent Projects unavailable",
                    "The project could not be updated through gxserver.",
                    cx,
                );
            });
        })
        .detach();
    }
}
