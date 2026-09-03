use gpui::ClipboardItem;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_remote_gxserver_sidebar_request_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-06-24-16:48:
        Sidebar-origin remote gxserver actions are allowlisted Rust-owned RPCs through the selected live SSH tunnel. Renderer commands may identify only a saved remote machine id, an allowed endpoint, and endpoint params; Rust must not accept tokens, hosts, SSH users, key paths, command text, URLs, or raw response handling authority from CEF.

        CDXC:Git 2026-06-24-17:47:
        Remote Git/GitHub/worktree parity expands this bridge to gxserver-owned project actions. Responses must be shaped at this boundary before CEF sees them: no command summaries, no PR URL launch authority, no raw delete-project bodies, and no remote tokens, hostnames, SSH details, stdout/stderr logging, or daemon body persistence.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        let response_request_id = gpui_remote_request_id_from_command(command);
        let Some(path) = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| gpui_remote_sidebar_request_path_allowed(path))
            .map(str::to_string)
        else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            }
            return;
        };
        let Some(params) = command.get("params").cloned().filter(|params| {
            params.is_object() && params.to_string().len() <= GPUI_REMOTE_GXSERVER_PARAMS_MAX_BYTES
        }) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "GPUI rejected the remote gxserver request.",
                    cx,
                );
            }
            return;
        };
        let Some(params) = gpui_remote_sidebar_request_params(path.as_str(), params) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "GPUI rejected the remote gxserver request.",
                    cx,
                );
            }
            return;
        };
        let timeout = gpui_remote_sidebar_request_timeout(command);
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "Reconnect the remote machine before using its sessions.",
                    cx,
                );
            }
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let response_path = path.clone();
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(&target, path.as_str(), &params, timeout)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        if let Some(request_id) = response_request_id.as_deref() {
                            this.dispatch_gpui_sidebar_remote_event(
                                serde_json::json!({
                                    "ok": true,
                                    "remoteMachineId": remote_machine_id.as_str(),
                                    "requestId": request_id,
                                    "result": gpui_remote_sidebar_response_payload(
                                        response_path.as_str(),
                                        result,
                                    ),
                                    "type": "remoteGxserverResponse",
                                }),
                                cx,
                            );
                        }
                    }
                    Err(_) => {
                        if let Some(request_id) = response_request_id.as_deref() {
                            this.dispatch_gpui_sidebar_remote_event(
                                serde_json::json!({
                                    "error": "Remote gxserver request failed.",
                                    "ok": false,
                                    "remoteMachineId": remote_machine_id.as_str(),
                                    "requestId": request_id,
                                    "type": "remoteGxserverResponse",
                                }),
                                cx,
                            );
                        } else {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Remote action failed",
                                "The remote gxserver action did not complete.",
                                cx,
                            );
                        }
                    }
                }
                if gpui_remote_sidebar_request_refreshes_presentation(response_path.as_str()) {
                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                        remote_machine_id,
                        false,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_remote_gxserver_request_error(
        &mut self,
        remote_machine_id: &str,
        request_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_sidebar_remote_event(
            serde_json::json!({
                "error": "Remote gxserver request failed.",
                "ok": false,
                "remoteMachineId": remote_machine_id,
                "requestId": request_id,
                "type": "remoteGxserverResponse",
            }),
            cx,
        );
    }

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
                    preferred_interface: GpuiPreferredAgentInterface::Terminal,
                    project_id,
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
            preferred_interface: GpuiPreferredAgentInterface::Terminal,
            project_id,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { execute_gpui_sidebar_native_project_path_action(message) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(GpuiSidebarNativeProjectPathActionResult::Copied(path)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(path));
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
                    if matches!(
                        mutation,
                        GpuiRecentProjectMutation::Close | GpuiRecentProjectMutation::Restore
                    ) && let Some(machine_id) = machine_id
                    {
                        this.refresh_gpui_remote_gxserver_presentation_in_background(
                            machine_id, false, cx,
                        );
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
