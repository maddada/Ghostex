use crate::app::helpers::*;
use crate::app::model::*;
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
                        let _ = this.update(cx, |this, cx| {
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                remote_machine_id.clone(),
                                false,
                                cx,
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
            "This Mac"
        } else if cfg!(target_os = "windows") {
            "This PC"
        } else {
            "This Computer"
        };
        let mut machines = vec![serde_json::json!({
            "label": local_label,
            "machineId": GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID,
            "platform": gpui_add_project_dialog_local_platform(),
        })];
        if let Some(saved_machines) = shared_settings::shared_sidebar_settings_snapshot()
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
                if self
                    .gpui_remote_gxserver_request_target(&machine_id)
                    .is_none()
                {
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

    #[allow(dead_code)] // no caller: Clone Repository was folded into the Add Project dialog; this is the standalone clone flow
    pub(crate) fn handle_gpui_preview_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteClone 2026-06-24-19:35:
        GPUI remote Clone Repository preview accepts only the selected saved machine id plus explicit modal input. Rust forwards those bounded fields through the live tunnel and returns only the preview payload needed by the shared modal; raw failures remain generic and are not logged.
        */
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let remote_machine_id = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id);
        let Some(params) = gpui_remote_repository_clone_preview_params_from_command(command) else {
            self.dispatch_gpui_repository_clone_preview_result(
                request_id,
                false,
                None,
                Some("Repository clone preview failed."),
                cx,
            );
            return;
        };
        let target = match remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_repository_clone_preview_result(
                        request_id,
                        false,
                        None,
                        Some("Repository clone preview failed."),
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/previewRepositoryClone",
                        &params,
                        GPUI_REMOTE_REPOSITORY_CLONE_PREVIEW_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => {
                    if let Some(preview) = result.get("preview").cloned() {
                        this.dispatch_gpui_repository_clone_preview_result(
                            request_id,
                            true,
                            Some(preview),
                            None,
                            cx,
                        );
                    } else {
                        this.dispatch_gpui_repository_clone_preview_result(
                            request_id,
                            false,
                            None,
                            Some("Repository clone preview failed."),
                            cx,
                        );
                    }
                }
                Err(_) => this.dispatch_gpui_repository_clone_preview_result(
                    request_id,
                    false,
                    None,
                    Some("Repository clone preview failed."),
                    cx,
                ),
            });
        })
        .detach();
    }

    #[allow(dead_code)] // no caller: Clone Repository was folded into the Add Project dialog; this is the standalone clone flow
    pub(crate) fn handle_gpui_start_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteClone 2026-06-24-19:35:
        Starting a remote clone is a Rust-owned gxserver job request, not renderer shell execution. CEF may submit repository URL text, target parent text, optional folder/branch text, clone flags, and the selected machine id; the remote daemon validates, derives the target path, runs Git, registers the project, and publishes presentation.
        */
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let remote_machine_id = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id);
        let Some(params) = gpui_remote_repository_clone_start_params_from_command(command) else {
            self.dispatch_gpui_repository_clone_result(
                request_id,
                false,
                None,
                Some("Repository clone failed."),
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "error",
                "Repository clone failed",
                "The clone request was invalid.",
                cx,
            );
            return;
        };
        let target = match remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_repository_clone_result(
                        request_id,
                        false,
                        None,
                        Some("Repository clone failed."),
                        cx,
                    );
                    self.dispatch_gpui_app_modal_toast(
                        "error",
                        "Repository clone failed",
                        "Reconnect the remote machine before cloning a repository.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let running_description = if remote_machine_id.is_some() {
            "The remote clone is running."
        } else {
            "The clone is running."
        };
        let toast_id = gpui_remote_repository_clone_toast_id(&request_id);
        self.remote_repository_clone_requests.insert(
            request_id.clone(),
            GpuiRemoteRepositoryCloneRequest {
                job_id: String::new(),
                remote_machine_id: remote_machine_id.clone(),
                toast_id: toast_id.clone(),
            },
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/startRepositoryClone",
                        &params,
                        GPUI_REMOTE_REPOSITORY_CLONE_START_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => {
                    let Some(job) = result.get("job").cloned() else {
                        this.fail_gpui_remote_repository_clone_request(
                            request_id,
                            Some(toast_id),
                            "Repository clone failed.",
                            cx,
                        );
                        return;
                    };
                    let Some(job_id) = job
                        .get("jobId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| gpui_remote_repository_clone_job_id_allowed(value))
                        .map(str::to_string)
                    else {
                        this.fail_gpui_remote_repository_clone_request(
                            request_id,
                            Some(toast_id),
                            "Repository clone failed.",
                            cx,
                        );
                        return;
                    };
                    this.remote_repository_clone_requests.insert(
                        request_id.clone(),
                        GpuiRemoteRepositoryCloneRequest {
                            job_id,
                            remote_machine_id,
                            toast_id: toast_id.clone(),
                        },
                    );
                    this.dispatch_gpui_repository_clone_toast(
                        "info",
                        "Cloning repository",
                        running_description,
                        Some(toast_id),
                        true,
                        Some(request_id.clone()),
                        cx,
                    );
                    this.handle_gpui_remote_repository_clone_job_status(request_id, job, cx);
                }
                Err(_) => this.fail_gpui_remote_repository_clone_request(
                    request_id,
                    Some(toast_id),
                    "Repository clone failed.",
                    cx,
                ),
            });
        })
        .detach();
    }
}
