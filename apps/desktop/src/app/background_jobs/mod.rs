//! Long jobs that outlive the dialog that started them, behind the work area header's jobs button
//! (render/workarea_header/jobs_button.rs) and its popover (window/jobs_popover.rs). The look is
//! window/jobs_popover_view.rs, which also holds the decision record (CDXC:BackgroundJobs).
//!
//! Only repository clones run here so far (clone.rs, and clone_on.rs for the sidebar's Clone on…);
//! a new kind of job adds a [`JobWork`] arm and its own runner file.

pub(crate) mod clone;

pub(crate) use clone::CloneSpec;

use crate::app::window::jobs_popover_view::{JobRow, JobStatus};
use crate::*;

/// One job and what it is doing.
pub(crate) struct BackgroundJob {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) failed_title: String,
    pub(crate) detail: String,
    pub(crate) verb: &'static str,
    pub(crate) progress: Option<String>,
    pub(crate) status: JobStatus,
    pub(crate) work: JobWork,
}

pub(crate) enum JobWork {
    Clone(clone::CloneWork),
}

#[derive(Default)]
pub(crate) struct BackgroundJobs {
    pub(crate) jobs: Vec<BackgroundJob>,
    next_id: u64,
    /// The open popover, redrawn whenever a job moves.
    pub(crate) panel: Option<gpui::WeakEntity<crate::app::window::jobs_popover::JobsPopoverPanel>>,
}

impl BackgroundJobs {
    pub(crate) fn push(&mut self, mut job: BackgroundJob) -> u64 {
        self.next_id += 1;
        job.id = self.next_id;
        self.jobs.push(job);
        self.next_id
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut BackgroundJob> {
        self.jobs.iter_mut().find(|job| job.id == id)
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<BackgroundJob> {
        let index = self.jobs.iter().position(|job| job.id == id)?;
        Some(self.jobs.remove(index))
    }

    /// The jobs as the button and the popover draw them.
    pub(crate) fn rows(&self) -> Vec<JobRow> {
        self.jobs
            .iter()
            .map(|job| JobRow {
                id: job.id,
                title: job.title.clone(),
                failed_title: job.failed_title.clone(),
                detail: job.detail.clone(),
                percent: job
                    .progress
                    .as_deref()
                    .and_then(crate::app::window::jobs_popover_view::progress_percent),
                progress: job.progress.clone(),
                status: job.status.clone(),
                verb: job.verb.to_string(),
            })
            .collect()
    }
}

impl GhostexGpuiApp {
    /// A job moved: the header redraws, the open popover too, and an empty list closes it.
    pub(crate) fn background_jobs_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(panel) = self
            .background_jobs
            .panel
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
        {
            panel.update(cx, |_, cx| cx.notify());
        }
        if self.background_jobs.jobs.is_empty()
            && self
                .titlebar_popup_menu
                .as_ref()
                .is_some_and(|state| state.kind == GpuiTitlebarPopupKind::Jobs)
        {
            self.titlebar_popup_menu = None;
            if let Some(popup_window) = self.titlebar_popup_window.take() {
                let _ = popup_window.update(cx, |_, popup_window, _| popup_window.remove_window());
            }
        }
        cx.notify();
    }

    pub(crate) fn background_job_cancel(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let Some(job) = self.background_jobs.get_mut(id) else {
            return;
        };
        match &job.work {
            JobWork::Clone(_) => self.cancel_background_clone(id, cx),
        }
    }

    pub(crate) fn background_job_retry(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        let Some(job) = self.background_jobs.get_mut(id) else {
            return;
        };
        if !matches!(job.status, JobStatus::Failed(_)) {
            return;
        }
        job.status = JobStatus::Running;
        job.progress = None;
        match &mut job.work {
            JobWork::Clone(work) => {
                work.server_job_id = None;
                work.cancel_requested = false;
                self.run_background_clone(id, cx);
            }
        }
        self.background_jobs_changed(cx);
    }

    pub(crate) fn background_job_dismiss(&mut self, id: u64, cx: &mut gpui::Context<Self>) {
        if self
            .background_jobs
            .jobs
            .iter()
            .any(|job| job.id == id && matches!(job.status, JobStatus::Failed(_)))
        {
            self.background_jobs.remove(id);
            self.background_jobs_changed(cx);
        }
    }

    /// A job stopped with `reason`: it stays in the popover until Retry or Dismiss.
    pub(crate) fn background_job_failed(
        &mut self,
        id: u64,
        reason: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(job) = self.background_jobs.get_mut(id) {
            job.status = JobStatus::Failed(reason);
            job.progress = None;
        }
        self.background_jobs_changed(cx);
    }
}
