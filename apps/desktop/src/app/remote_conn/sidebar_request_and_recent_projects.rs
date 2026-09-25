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
        // The allowlist, the size bound, the shaping, the tunnel and the refresh are one function
        // that the store's remote actions call too (remote_conn/sidebar_rpc.rs); this arm only
        // turns the renderer's message into its arguments and the answer back into the event the
        // renderer is waiting for.
        let path = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let params = command.get("params").cloned();
        let timeout = gpui_remote_sidebar_request_timeout(command);
        let mode = match response_request_id {
            Some(_) => super::sidebar_rpc::GpuiRemoteSidebarRpcMode::Awaited,
            None => super::sidebar_rpc::GpuiRemoteSidebarRpcMode::FireAndForget,
        };
        let task = self.start_gpui_remote_sidebar_rpc(
            remote_machine_id.as_str(),
            path.as_str(),
            params,
            timeout,
            mode,
            cx,
        );
        let Some(request_id) = response_request_id else {
            task.detach();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => this.dispatch_gpui_sidebar_remote_event(
                    serde_json::json!({
                        "ok": true,
                        "remoteMachineId": remote_machine_id.as_str(),
                        "requestId": request_id.as_str(),
                        "result": gpui_remote_sidebar_response_payload(path.as_str(), result),
                        "type": "remoteGxserverResponse",
                    }),
                    cx,
                ),
                Err(_) => this.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id.as_str(),
                    cx,
                ),
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
                "error": super::sidebar_rpc::GPUI_REMOTE_GXSERVER_REQUEST_FAILED,
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
