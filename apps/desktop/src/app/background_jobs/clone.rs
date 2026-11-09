//! A repository clone as a background job: gxserver's clone job (`/api/startRepositoryClone`,
//! read every 900ms) followed from the app instead of the Add Project dialog, so the dialog can
//! close as soon as Clone is pressed. gxserver registers the cloned project itself when git
//! finishes (server/src/repository_clone/jobs.rs), so the project shows in the sidebar then.
//! SEE-ALSO: apps/desktop/src/app/remote_conn/project_browse_and_add.rs (the one gxserver route
//! every clone call takes, with its remote refresh and lost-answer watch).

use crate::app::window::add_project_modal::{
    AddProjectCloneJobState, describe_add_project_error, read_add_project_clone_handle,
    read_add_project_clone_job,
};
use crate::app::window::jobs_popover_view::JobStatus;
use crate::*;

use super::{BackgroundJob, JobWork};

/// How often a running clone is read, as the dialog did.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(900);
/// Unanswered reads in a row before the job says it lost the machine.
const MAX_LOST_READS: u32 = 5;

/// What a clone needs: the machine and the dialog's `startClone` parameters.
pub(crate) struct CloneSpec {
    pub(crate) machine_id: String,
    pub(crate) params: serde_json::Value,
    /// `owner/name`, else the URL.
    pub(crate) repository: String,
    pub(crate) destination: String,
}

pub(crate) struct CloneWork {
    pub(crate) machine_id: String,
    pub(crate) params: serde_json::Value,
    pub(crate) server_job_id: Option<String>,
    pub(crate) cancel_requested: bool,
    lost_reads: u32,
    folder_name: String,
}

/// The last folder of a path, for the finished toast.
fn folder_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}

/// A folder under this computer's home, written from `~` so the line stays short.
fn home_as_tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&format!("{home}/")) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_string(),
    }
}

impl GhostexGpuiApp {
    /// The machine's name in a sentence, `None` for this computer.
    fn background_job_machine_name(&self, machine_id: &str) -> Option<String> {
        (machine_id != GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID).then(|| {
            gpui_remote_machine_name_from_settings(machine_id)
                .unwrap_or_else(|| "another computer".to_string())
        })
    }

    pub(crate) fn start_background_clone(&mut self, spec: CloneSpec, cx: &mut gpui::Context<Self>) {
        let detail = match self.background_job_machine_name(&spec.machine_id) {
            Some(machine) => format!("Into {} on {machine}", spec.destination),
            None => format!("Into {}", home_as_tilde(&spec.destination)),
        };
        let id = self.background_jobs.push(BackgroundJob {
            id: 0,
            title: format!("Cloning {}", spec.repository),
            failed_title: format!("Couldn't clone {}", spec.repository),
            detail,
            verb: "Cloning",
            progress: None,
            status: JobStatus::Running,
            work: JobWork::Clone(CloneWork {
                machine_id: spec.machine_id,
                params: spec.params,
                server_job_id: None,
                cancel_requested: false,
                lost_reads: 0,
                folder_name: folder_name(&spec.destination),
            }),
        });
        self.run_background_clone(id, cx);
        self.background_jobs_changed(cx);
    }

    fn background_clone_work(&mut self, id: u64) -> Option<&mut CloneWork> {
        match &mut self.background_jobs.get_mut(id)?.work {
            JobWork::Clone(work) => Some(work),
        }
    }

    pub(super) fn run_background_clone(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let Some(work) = self.background_clone_work(id) else {
            return;
        };
        let machine_id = work.machine_id.clone();
        let params = work.params.as_object().cloned().unwrap_or_default();
        self.run_gpui_add_project_dialog_operation(
            GpuiAddProjectDialogOperation::StartClone,
            Some(machine_id.as_str()),
            &params,
            move |app, result, cx| match result
                .and_then(|value| read_add_project_clone_handle(&value))
            {
                Ok(job_id) => {
                    let Some(work) = app.background_clone_work(id) else {
                        return;
                    };
                    work.server_job_id = Some(job_id);
                    // Cancel pressed before gxserver named the job: cancel it now.
                    if work.cancel_requested {
                        app.cancel_background_clone(id, cx);
                    }
                    app.read_background_clone(id, cx);
                }
                Err(error) => app.background_job_failed(
                    id,
                    describe_add_project_error(&error, "Clone failed."),
                    cx,
                ),
            },
            cx,
        );
    }

    fn read_background_clone(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let Some(work) = self.background_clone_work(id) else {
            return;
        };
        let Some(job_id) = work.server_job_id.clone() else {
            return;
        };
        let machine_id = work.machine_id.clone();
        let mut params = serde_json::Map::new();
        params.insert("jobId".to_string(), serde_json::json!(job_id));
        self.run_gpui_add_project_dialog_operation(
            GpuiAddProjectDialogOperation::ReadCloneJob,
            Some(machine_id.as_str()),
            &params,
            move |app, result, cx| {
                // Retry started another gxserver job meanwhile: this answer is about the old one.
                if app
                    .background_clone_work(id)
                    .is_none_or(|work| work.server_job_id.as_deref() != Some(job_id.as_str()))
                {
                    return;
                }
                match result.and_then(|value| read_add_project_clone_job(&value)) {
                    Ok(job) => app.receive_background_clone(id, job, cx),
                    Err(error) => {
                        let Some(work) = app.background_clone_work(id) else {
                            return;
                        };
                        work.lost_reads += 1;
                        if work.lost_reads >= MAX_LOST_READS {
                            app.background_job_failed(
                                id,
                                format!(
                                    "Ghostex lost touch with the computer while cloning, so the clone may still finish there. {}",
                                    describe_add_project_error(&error, "")
                                )
                                .trim()
                                .to_string(),
                                cx,
                            );
                            return;
                        }
                        app.schedule_background_clone_read(id, cx);
                    }
                }
            },
            cx,
        );
    }

    fn schedule_background_clone_read(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let _ = this.update(cx, |app, cx| app.read_background_clone(id, cx));
        })
        .detach();
    }

    fn receive_background_clone(
        &mut self,
        id: u64,
        job: crate::app::window::add_project_modal::AddProjectCloneJob,
        cx: &mut gpui::Context<Self>,
    ) {
        match job.state {
            AddProjectCloneJobState::Running => {
                if let Some(work) = self.background_clone_work(id) {
                    work.lost_reads = 0;
                }
                if let Some(entry) = self.background_jobs.get_mut(id)
                    && job.progress.is_some()
                    && entry.progress != job.progress
                {
                    entry.progress = job.progress;
                    self.background_jobs_changed(cx);
                }
                self.schedule_background_clone_read(id, cx);
            }
            AddProjectCloneJobState::Canceled => {
                self.background_jobs.remove(id);
                self.background_jobs_changed(cx);
            }
            AddProjectCloneJobState::Failed => {
                let reason = [job.error.as_deref(), job.message.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(str::trim)
                    .find(|text| !text.is_empty())
                    .unwrap_or("Repository clone failed.")
                    .to_string();
                self.background_job_failed(id, reason, cx);
            }
            AddProjectCloneJobState::Completed => {
                let Some(finished) = self.background_jobs.remove(id) else {
                    return;
                };
                let JobWork::Clone(work) = finished.work;
                let name = job
                    .project_path
                    .as_deref()
                    .map(folder_name)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(work.folder_name);
                let description = match self.background_job_machine_name(&work.machine_id) {
                    Some(machine) => format!("{name} is in your projects on {machine}."),
                    None => format!("{name} is in your projects."),
                };
                self.dispatch_gpui_workspace_action_toast(
                    "success",
                    "Repository cloned",
                    &description,
                    cx,
                );
                self.background_jobs_changed(cx);
            }
        }
    }

    /// Cancel: gxserver stops git; the job leaves the list once gxserver says it is canceled.
    pub(super) fn cancel_background_clone(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let Some(work) = self.background_clone_work(id) else {
            return;
        };
        work.cancel_requested = true;
        let machine_id = work.machine_id.clone();
        let server_job_id = work.server_job_id.clone();
        if let Some(job) = self.background_jobs.get_mut(id) {
            job.status = JobStatus::Canceling;
        }
        self.background_jobs_changed(cx);
        let Some(job_id) = server_job_id else {
            return;
        };
        let mut params = serde_json::Map::new();
        params.insert("jobId".to_string(), serde_json::json!(job_id));
        self.run_gpui_add_project_dialog_operation(
            GpuiAddProjectDialogOperation::CancelCloneJob,
            Some(machine_id.as_str()),
            &params,
            move |app, result, cx| {
                if let Err(error) = result {
                    if let Some(job) = app.background_jobs.get_mut(id) {
                        job.status = JobStatus::Running;
                    }
                    app.dispatch_gpui_workspace_action_toast_with_details(
                        "error",
                        "Couldn't cancel the clone",
                        "It is still running. Try Cancel again.",
                        &error,
                        cx,
                    );
                    app.background_jobs_changed(cx);
                }
            },
            cx,
        );
    }
}
