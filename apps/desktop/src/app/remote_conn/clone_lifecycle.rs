use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    #[allow(dead_code)] // no caller: Clone Repository was folded into the Add Project dialog; this is the standalone clone flow
    pub(crate) fn handle_gpui_cancel_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Repository clone already finished",
                "There is no active remote clone for that request.",
                cx,
            );
            return;
        };
        let target = match active_clone.remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let job_id = active_clone.job_id.clone();
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/cancelRepositoryCloneJob",
                        &serde_json::json!({ "jobId": job_id }),
                        GPUI_REMOTE_REPOSITORY_CLONE_JOB_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_active = this
                    .remote_repository_clone_requests
                    .get(&request_id)
                    .is_some_and(|current| {
                        current.job_id == active_clone.job_id
                            && current.remote_machine_id == active_clone.remote_machine_id
                    });
                if !still_active {
                    return;
                }
                match result {
                    Ok(result) => match result.get("job").cloned() {
                        Some(job) => {
                            this.handle_gpui_remote_repository_clone_job_status(
                                request_id, job, cx,
                            );
                        }
                        None => {
                            this.fail_gpui_remote_repository_clone_request(
                                request_id,
                                Some(active_clone.toast_id),
                                "Repository clone failed.",
                                cx,
                            );
                        }
                    },
                    Err(_) => this.dispatch_gpui_repository_clone_toast(
                        "error",
                        "Repository clone cancel failed",
                        "gxserver did not cancel the clone.",
                        Some(active_clone.toast_id),
                        false,
                        None,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_remote_repository_clone_job_status(
        &mut self,
        request_id: String,
        job: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            return;
        };
        match job
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("failed")
        {
            "running" => {
                self.schedule_gpui_remote_repository_clone_poll(request_id, cx);
            }
            "completed" => {
                self.remote_repository_clone_requests.remove(&request_id);
                let completed_description = if active_clone.remote_machine_id.is_some() {
                    "The remote project was added."
                } else {
                    "The project was added."
                };
                if let Some(machine_id) = active_clone.remote_machine_id {
                    self.refresh_gpui_remote_gxserver_presentation_in_background(
                        machine_id, false, cx,
                    );
                }
                self.dispatch_gpui_repository_clone_toast(
                    "success",
                    "Repository cloned",
                    completed_description,
                    Some(active_clone.toast_id),
                    false,
                    None,
                    cx,
                );
                self.dispatch_gpui_repository_clone_result(request_id, true, None, None, cx);
            }
            "canceled" => {
                self.remote_repository_clone_requests.remove(&request_id);
                let canceled_description = if active_clone.remote_machine_id.is_some() {
                    "The partial remote folder may still exist."
                } else {
                    "The partial folder may still exist."
                };
                self.dispatch_gpui_repository_clone_toast(
                    "warning",
                    "Repository clone canceled",
                    canceled_description,
                    Some(active_clone.toast_id),
                    false,
                    None,
                    cx,
                );
                self.dispatch_gpui_repository_clone_result(
                    request_id,
                    false,
                    None,
                    Some("Repository clone canceled."),
                    cx,
                );
            }
            _ => {
                self.fail_gpui_remote_repository_clone_request(
                    request_id,
                    Some(active_clone.toast_id),
                    "Repository clone failed.",
                    cx,
                );
            }
        }
    }

    pub(crate) fn schedule_gpui_remote_repository_clone_poll(
        &mut self,
        request_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            return;
        };
        let target = match active_clone.remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(GPUI_REMOTE_REPOSITORY_CLONE_POLL_INTERVAL)
                .await;
            let job_id = active_clone.job_id.clone();
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/readRepositoryCloneJob",
                        &serde_json::json!({ "jobId": job_id }),
                        GPUI_REMOTE_REPOSITORY_CLONE_JOB_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_active = this
                    .remote_repository_clone_requests
                    .get(&request_id)
                    .is_some_and(|current| {
                        current.job_id == active_clone.job_id
                            && current.remote_machine_id == active_clone.remote_machine_id
                    });
                if !still_active {
                    return;
                }
                match result {
                    Ok(result) => {
                        if let Some(job) = result.get("job").cloned() {
                            this.handle_gpui_remote_repository_clone_job_status(
                                request_id, job, cx,
                            );
                        } else {
                            this.fail_gpui_remote_repository_clone_request(
                                request_id,
                                Some(active_clone.toast_id),
                                "Repository clone failed.",
                                cx,
                            );
                        }
                    }
                    Err(_) => this.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn fail_gpui_remote_repository_clone_request(
        &mut self,
        request_id: String,
        toast_id: Option<String>,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let toast_id = toast_id.or_else(|| {
            self.remote_repository_clone_requests
                .remove(&request_id)
                .map(|active| active.toast_id)
        });
        self.remote_repository_clone_requests.remove(&request_id);
        self.dispatch_gpui_repository_clone_toast(
            "error",
            "Repository clone failed",
            "The repository clone did not complete.",
            toast_id,
            false,
            None,
            cx,
        );
        self.dispatch_gpui_repository_clone_result(request_id, false, None, Some(message), cx);
    }
}
