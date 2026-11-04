use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
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
        let machine_id = command
            .get("machineId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let empty_params = serde_json::Map::new();
        let raw_params = command
            .get("params")
            .and_then(serde_json::Value::as_object)
            .unwrap_or(&empty_params);
        self.run_gpui_add_project_dialog_operation(
            operation,
            machine_id.as_deref(),
            raw_params,
            move |this, result, cx| match result {
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
            },
            cx,
        );
    }

    /// One Add Project round trip, answered through `on_result`: the React dialog's bridge
    /// request above and the native dialog (add_project_modal_lifecycle.rs) both run it, so the
    /// routing, the parameter allowlist, the Windows path translation and the follow-ups after an
    /// add or a clone (activation, remote refresh, clone watch) are one code path.
    pub(crate) fn run_gpui_add_project_dialog_operation(
        &mut self,
        operation: GpuiAddProjectDialogOperation,
        machine_id: Option<&str>,
        raw_params: &serde_json::Map<String, serde_json::Value>,
        on_result: impl FnOnce(&mut Self, Result<serde_json::Value, String>, &mut gpui::Context<Self>)
        + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.addProject.request",
            serde_json::json!({ "operation": operation.as_wire() }),
        );
        if operation == GpuiAddProjectDialogOperation::ListMachines {
            let machines = self.gpui_add_project_dialog_machine_options();
            on_result(self, Ok(serde_json::json!({ "machines": machines })), cx);
            return;
        }
        let requested_machine_id = machine_id
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
                    on_result(self, Err("That machine is unavailable.".to_string()), cx);
                    return;
                }
            }
        };
        let Some(params) = gpui_add_project_dialog_params(operation, raw_params) else {
            on_result(
                self,
                Err("The add-project request was invalid.".to_string()),
                cx,
            );
            return;
        };
        #[cfg(target_os = "windows")]
        let params = if remote_machine_id.is_none() {
            match gpui_add_project_dialog_translate_local_windows_paths(operation, params) {
                Ok(params) => params,
                Err(error) => {
                    on_result(self, Err(error), cx);
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
                        on_result(self, Err("That machine is not connected.".to_string()), cx);
                        return;
                    }
                }
            }
            None => None,
        };
        // CDXC:Workspaces 2026-10-09 WHY: a project added in a window, or cloned from Add
        // Project, joins the workspace that window shows on this computer (gxserver's
        // `place_added_project`; a clone job carries the id until the project is registered).
        let mut params = params;
        if remote_machine_id.is_none()
            && matches!(
                operation,
                GpuiAddProjectDialogOperation::Add | GpuiAddProjectDialogOperation::StartClone
            )
            && let (Some(workspace_id), Some(params)) = (
                self.gx_store_window_non_default_workspace_id(),
                params.as_object_mut(),
            )
        {
            params.insert("workspaceId".to_string(), serde_json::json!(workspace_id));
        }
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
                let added_project_id = match &result {
                    Ok(value) if operation == GpuiAddProjectDialogOperation::Add => value
                        .get("project")
                        .and_then(|project| project.get("projectId"))
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
                on_result(this, result, cx);
                // CDXC:AddProject 2026-09-06 DECISION: User: newly added projects become active, switch to their Space, expand their sidebar groups, and scroll into view, just like Quick Access project activation.
                // Completed clones register through Add too; the shared activation route refreshes the project and focuses or creates its default session.
                if let Some(project_id) = added_project_id {
                    this.forward_gpui_added_project_to_sidebar(
                        &project_id,
                        remote_machine_id.as_deref(),
                        cx,
                    );
                    let scoped_project_id = match remote_machine_id.as_deref() {
                        Some(machine_id) => gpui_remote_scoped_project_id(machine_id, &project_id),
                        None => project_id,
                    };
                    // CDXC:TranscriptExport 2026-10-25 WHY: a project added for a handoff
                    // gets the handed-off session; activating it would also start its default one.
                    if !this.git_export_add_project_waiting() {
                        this.dispatch_gpui_menu_bar_project_activation(&scoped_project_id, cx);
                    }
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
                            &remote_machine_id,
                        );
                        if clone_answer_lost && let Some(job_id) = clone_watch_job_id {
                            this.watch_gpui_remote_add_project_clone_job(
                                remote_machine_id,
                                job_id,
                                cx,
                            );
                        }
                    }
                    None => {}
                }
            });
        })
        .detach();
    }
}
