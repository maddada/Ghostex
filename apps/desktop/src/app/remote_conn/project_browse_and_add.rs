use std::time::Duration;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_remote_gxserver_subscribe_presentation_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        let Some(client_id) = gpui_remote_presentation_client_id_from_command(command) else {
            return;
        };
        let last_revision = command
            .get("lastRevision")
            .and_then(serde_json::Value::as_u64);
        self.restart_gpui_remote_gxserver_presentation_stream(
            remote_machine_id,
            client_id,
            last_revision,
            cx,
        );
    }

    pub(crate) fn open_gpui_remote_gxserver_install_modal(
        &mut self,
        remote_machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machine_name =
            gpui_remote_machine_name_from_settings(remote_machine_id.as_str())
                .unwrap_or_else(|| "Remote".to_string());
        let open_message = serde_json::json!({
            "modal": "remoteGxserverInstall",
            "remoteMachineId": remote_machine_id,
            "remoteMachineName": remote_machine_name,
            "type": "open",
        });
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(
            GpuiAppModalKind::RemoteGxserverInstall,
            cx,
        );
        self.open_gpui_app_modal_window(
            GpuiAppModalKind::RemoteGxserverInstall,
            open_message,
            sidebar_state_message,
            None,
            cx,
        );
    }

    pub(crate) fn handle_gpui_browse_remote_project_directories_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote machine is unavailable."),
                cx,
            );
            return;
        };
        let Some(partial_path) =
            gpui_remote_path_like_string_from_command(command, "partialPath", true)
        else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote path is invalid."),
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote machine is not connected."),
                cx,
            );
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/browseProjectDirectories",
                        &serde_json::json!({ "partialPath": partial_path }),
                        Duration::from_secs(15),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => this.dispatch_gpui_remote_project_directory_browse_result(
                    request_id,
                    true,
                    Some(result),
                    None,
                    cx,
                ),
                Err(_) => this.dispatch_gpui_remote_project_directory_browse_result(
                    request_id,
                    false,
                    None,
                    Some("Remote directory browse failed."),
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_add_remote_project_path_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote machine is unavailable."),
                cx,
            );
            return;
        };
        let Some(path) = gpui_remote_path_like_string_from_command(command, "path", false) else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote path is invalid."),
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote machine is not connected."),
                cx,
            );
            return;
        };
        let fallback_project_path = path.clone();
        let project_name = gpui_remote_project_name_from_path(path.as_str());
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/addProjectPath",
                        &serde_json::json!({
                            "name": project_name,
                            "path": path,
                        }),
                        GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        let project_path = result
                            .get("project")
                            .and_then(|project| project.get("path"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                            .unwrap_or(fallback_project_path);
                        this.dispatch_gpui_remote_project_add_result(
                            request_id,
                            true,
                            Some(project_path),
                            None,
                            cx,
                        );
                    }
                    Err(_) => this.dispatch_gpui_remote_project_add_result(
                        request_id,
                        false,
                        None,
                        Some("Remote project add failed."),
                        cx,
                    ),
                }
                /*
                CDXC:AddProject 2026-07-30:
                Refresh the machine's presentation on BOTH arms. A remote add
                that lands after our request gives up (slow reconnect, dropped
                answer) still registered the project on that machine, and the
                machine's presentation stream is frequently the thing that was
                broken in the first place — so a failure answer is exactly when
                a snapshot pull is needed for the project to become visible.
                */
                this.refresh_gpui_remote_gxserver_presentation_in_background(
                    remote_machine_id,
                    false,
                    cx,
                );
            });
        })
        .detach();
    }

    /*
    CDXC:AddProject 2026-07-30:
    The shared add-project dialog runs in the app-modal child window and reaches
    gxserver only through this request/response pair. `machineId` is the whole
    routing vocabulary: the local machine id goes to the local daemon, a saved
    remote machine id goes through that machine's live tunnel, and an id with no
    live tunnel is answered with an explicit "not connected" error instead of
    silently falling back to the local filesystem.
    */
    pub(crate) fn handle_gpui_add_project_dialog_request_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(operation) = command
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .and_then(GpuiAddProjectDialogOperation::from_wire)
        else {
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                false,
                None,
                Some("The add-project request was invalid."),
                cx,
            );
            return;
        };
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.addProject.request",
            serde_json::json!({ "operation": operation.as_wire() }),
        );
        if operation == GpuiAddProjectDialogOperation::ListMachines {
            let machines = self.gpui_add_project_dialog_machine_options();
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                true,
                Some(serde_json::json!({ "machines": machines })),
                None,
                cx,
            );
            return;
        }
        let requested_machine_id = command
            .get("machineId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|machine_id| !machine_id.is_empty())
            .unwrap_or(GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID);
        let remote_machine_id = if requested_machine_id == GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID
        {
            None
        } else {
            match gpui_normalize_remote_machine_id(requested_machine_id) {
                Some(remote_machine_id) => Some(remote_machine_id),
                None => {
                    self.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some("That machine is unavailable."),
                        cx,
                    );
                    return;
                }
            }
        };
        let empty_params = serde_json::Map::new();
        let raw_params = command
            .get("params")
            .and_then(serde_json::Value::as_object)
            .unwrap_or(&empty_params);
        let Some(params) = gpui_add_project_dialog_params(operation, raw_params) else {
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                false,
                None,
                Some("The add-project request was invalid."),
                cx,
            );
            return;
        };
        #[cfg(target_os = "windows")]
        let params = if remote_machine_id.is_none() {
            match gpui_add_project_dialog_translate_local_windows_paths(operation, params) {
                Ok(params) => params,
                Err(error) => {
                    self.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some(error.as_str()),
                        cx,
                    );
                    return;
                }
            }
        } else {
            params
        };
        let target = match remote_machine_id.as_deref() {
            Some(remote_machine_id) => {
                match self.gpui_remote_gxserver_request_target(remote_machine_id) {
                    Some(target) => Some(target),
                    None => {
                        self.dispatch_gpui_add_project_dialog_result(
                            request_id,
                            false,
                            None,
                            Some("That machine is not connected."),
                            cx,
                        );
                        return;
                    }
                }
            }
            None => None,
        };
        let Some(endpoint) = operation.endpoint() else {
            return;
        };
        let timeout = operation.timeout();
        /*
        CDXC:AddProject 2026-07-30:
        A remote clone job outlives this request: gxserver runs it on the machine
        and registers the project itself when git finishes. Keep the job id so a
        poll whose answer never comes back can be followed natively instead of
        leaving the finished project invisible in the sidebar.
        */
        let clone_watch_job_id = if operation == GpuiAddProjectDialogOperation::ReadCloneJob {
            params
                .get("jobId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    let result = gpui_add_project_dialog_rpc_result(
                        target.as_ref(),
                        endpoint,
                        &params,
                        timeout,
                    )?;
                    if operation == GpuiAddProjectDialogOperation::Add {
                        return gpui_add_project_dialog_restore_recent_project(
                            target.as_ref(),
                            result,
                            timeout,
                        );
                    }
                    Ok(result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let added_project_path = match &result {
                    Ok(value) if operation == GpuiAddProjectDialogOperation::Add => value
                        .get("project")
                        .and_then(|project| project.get("path"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    _ => None,
                };
                let clone_completed = matches!(&result, Ok(value)
                    if operation == GpuiAddProjectDialogOperation::ReadCloneJob
                        && value
                            .get("job")
                            .and_then(|job| job.get("state"))
                            .and_then(serde_json::Value::as_str)
                            == Some("completed"));
                /*
                A failed startClone/readCloneJob answer does NOT mean the clone
                failed: the request can time out on the tunnel while the job
                keeps running and registers the project on the machine. Treat it
                like a possibly-landed mutation.
                */
                let clone_answer_lost = result.is_err()
                    && matches!(
                        operation,
                        GpuiAddProjectDialogOperation::ReadCloneJob
                            | GpuiAddProjectDialogOperation::StartClone
                    );
                match result {
                    Ok(value) => this.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        true,
                        Some(value),
                        None,
                        cx,
                    ),
                    Err(error) => this.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some(error.as_str()),
                        cx,
                    ),
                }
                if operation != GpuiAddProjectDialogOperation::Add
                    && !clone_completed
                    && !clone_answer_lost
                {
                    return;
                }
                match remote_machine_id {
                    /*
                    CDXC:AddProject 2026-07-30:
                    A remote add, a finished remote clone, and a clone request
                    whose answer was lost all refresh that machine's presentation
                    on BOTH arms, because a request that times out can still have
                    registered the project and the machine's presentation stream
                    is often the broken part. A lost readCloneJob answer also
                    hands the job to a native watcher, because that clone can
                    still be running and will register its project minutes after
                    the dialog gave up.
                    */
                    Some(remote_machine_id) => {
                        this.refresh_gpui_remote_gxserver_presentation_in_background(
                            remote_machine_id.clone(),
                            false,
                            cx,
                        );
                        if clone_answer_lost {
                            if let Some(job_id) = clone_watch_job_id {
                                this.watch_gpui_remote_add_project_clone_job(
                                    remote_machine_id,
                                    job_id,
                                    cx,
                                );
                            }
                        }
                    }
                    /*
                    The local sidebar runtime owns local project state and does
                    not receive daemon pushes, so a successful local add is
                    reported through the same workspace-folder bridge the OS
                    folder picker used. The runtime re-registers idempotently,
                    focuses the project, and pulls a fresh local presentation.
                    */
                    None => {
                        if let Some(project_path) = added_project_path {
                            this.dispatch_gpui_workspace_folder_picked_message(
                                serde_json::json!({
                                    "path": project_path,
                                    "type": "workspaceFolderPicked",
                                }),
                                cx,
                            );
                        }
                    }
                }
            });
        })
        .detach();
    }

}

