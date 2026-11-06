use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn watch_gpui_remote_add_project_clone_job(
        &mut self,
        remote_machine_id: String,
        job_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AddProject 2026-07-30:
        The dialog stops polling as soon as one readCloneJob answer is lost, but
        the job itself is server-side: git keeps running on the machine and
        gxserver registers the cloned project when it finishes. Follow the job
        natively so the project appears when it lands. Bounded on both axes: a
        maximum number of polls, and a short run of consecutive transport
        failures ends the watch because a dead tunnel cannot deliver a refresh
        either. Only the machine id and the job id are used; nothing about this
        watch is reported to CEF.
        */
        if self
            .gpui_remote_gxserver_request_target(&remote_machine_id)
            .is_none()
        {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let mut consecutive_errors = 0_u32;
            for _ in 0..GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_POLLS {
                cx.background_executor()
                    .timer(GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_INTERVAL)
                    .await;
                let Ok(Some(target)) = this.update(cx, |this, _cx| {
                    this.gpui_remote_gxserver_request_target(remote_machine_id.as_str())
                }) else {
                    return;
                };
                let job_id = job_id.clone();
                let result = background
                    .spawn(async move {
                        gpui_add_project_dialog_rpc_result(
                            Some(&target),
                            "/api/readRepositoryCloneJob",
                            &serde_json::json!({ "jobId": job_id }),
                            GPUI_ADD_PROJECT_DIALOG_JOB_TIMEOUT,
                        )
                    })
                    .await;
                match result {
                    Ok(value) => {
                        let running = value
                            .get("job")
                            .and_then(|job| job.get("state"))
                            .and_then(serde_json::Value::as_str)
                            == Some("running");
                        if running {
                            consecutive_errors = 0;
                            continue;
                        }
                        let _ = this.update(cx, |this, _| {
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                &remote_machine_id,
                            );
                        });
                        return;
                    }
                    Err(_) => {
                        consecutive_errors += 1;
                        if consecutive_errors
                            >= GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_CONSECUTIVE_ERRORS
                        {
                            return;
                        }
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn gpui_add_project_dialog_machine_options(&self) -> serde_json::Value {
        /*
        CDXC:AddProject 2026-07-30:
        The dialog's machine step is built from the same saved machine list the
        sidebar renders, plus this computer. Only display labels and bounded ids
        cross the boundary: SSH hosts, users, identity files, and tunnel ports
        stay native. Machines with no live tunnel are still listed (a remote
        entry point must be able to preselect one) and carry an explicit
        not-connected line instead of quietly disappearing.
        */
        let local_label = if cfg!(target_os = "macos") {
            "This machine"
        } else if cfg!(target_os = "windows") {
            "This PC"
        } else {
            "This Computer"
        };
        let local = serde_json::json!({
            "label": local_label,
            "machineId": GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID,
            "platform": gpui_add_project_dialog_local_platform(),
        });
        // Only the PowerShell backend browses Windows drives; the WSL backend's gxserver sees Linux paths.
        #[cfg(target_os = "windows")]
        let local = {
            let mut local = local;
            if matches!(
                crate::windows_terminal_backend::resolve_current(),
                Ok(crate::windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
            ) {
                local["startsAtDriveList"] = serde_json::json!(true);
            }
            local
        };
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        // Settings > Projects > Default projects folder: where This machine's Local folder and
        // New project folder start (#419). Remote machines keep their home folder.
        let local = match settings
            .object()
            .get("defaultProjectsFolder")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
        {
            Some(folder) => {
                let mut local = local;
                local["addProjectBaseDirectory"] = serde_json::json!(folder);
                local
            }
            None => local,
        };
        let mut machines = vec![local];
        if let Some(saved_machines) = settings
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
        {
            for machine in saved_machines {
                let Some(machine_id) = gpui_remote_machine_id_from_value(machine) else {
                    continue;
                };
                let label = machine
                    .as_object()
                    .and_then(|machine| gpui_remote_machine_string_field(machine, "name"))
                    .unwrap_or_else(|| "Remote".to_string());
                let mut option = serde_json::json!({
                    "label": label,
                    "machineId": machine_id,
                });
                if let Some(target) = self.gpui_remote_gxserver_request_target(&machine_id) {
                    // The picker validates the host filesystem, not the client OS.
                    option["platform"] = serde_json::json!(match target.execution_target {
                        GpuiRemoteExecutionTarget::WindowsPowerShell => "Win32",
                        GpuiRemoteExecutionTarget::WindowsWsl { .. } => "Linux",
                        GpuiRemoteExecutionTarget::PosixHost => "POSIX",
                    });
                    if matches!(
                        target.execution_target,
                        GpuiRemoteExecutionTarget::WindowsPowerShell
                    ) {
                        option["startsAtDriveList"] = serde_json::json!(true);
                    }
                } else {
                    option["platform"] = serde_json::json!("unknown");
                    option["description"] = serde_json::json!("Not connected");
                }
                machines.push(option);
            }
        }
        serde_json::Value::Array(machines)
    }

    pub(crate) fn dispatch_gpui_add_project_dialog_result(
        &mut self,
        request_id: String,
        ok: bool,
        result: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "requestId": request_id,
                "result": result,
                "type": "addProjectDialogResult",
            }),
            cx,
        );
    }
}
