//! A Git review: what the Git entry points open (the Changes tab, see changes.rs) and what a
//! confirmed review does (commit, push, pull request, Multiple Commits). Ported from the old
//! runtime's `promptSidebarGitActionReview`, `confirmSidebarGitCommit` and
//! `runSidebarGitMultipleCommits`, with their remote twins.
//!
//! CDXC:Git 2026-06-24-15:55:
//! Worktree completion may run direct merge-to-main and delete-after cleanup only from a confirmed
//! Git review request. The confirm names the review's request id, and every path it commits or
//! inspects must be one that review listed; renderer paths, branch text and modal labels are never
//! authority for side effects.

use ghostex_gx_core::git_menu::{
    GIT_MULTIPLE_COMMITS_PROMPT, GitReviewAction, GitState, GitToastLevel, PendingGitReview,
    finished_title, resolve_trusted_file_selection, started_title,
};
use serde_json::json;

use super::actions::PullRequestWorkflow;
use super::hud::GitReadOptions;
use super::scope::{GitScope, ScopeMiss};
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;

/// A confirmed review: the Changes view's commit bar (`confirmSidebarGitCommit`, `confirmSidebarGitDirectMerge`).
#[derive(Clone, Debug, Default)]
pub(crate) struct GitReviewConfirm {
    pub(crate) request_id: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) file_paths: Option<Vec<String>>,
    pub(crate) commit_on_new_ref: bool,
    pub(crate) delete_worktree_after: bool,
}

impl GhostexGpuiApp {
    /// What used to open the commit review window over the state just read
    /// (`promptSidebarGitActionReview`) now opens the project's Changes tab with its commit bar
    /// ready (CDXC:Changes 2026-10-25 in changes.rs).
    pub(crate) fn git_open_review(
        &mut self,
        scope: &GitScope,
        state: &GitState,
        action: GitReviewAction,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.git_scope_is_active(scope) {
            self.git_focus_project(&scope.key, cx);
        }
        let (changes_scope, commit_action) = Self::changes_preset_for_review(action);
        self.open_changes_view(
            Some(crate::app::native_changes::ChangesPreset {
                scope: changes_scope,
                action: Some(commit_action),
                reveal: None,
            }),
            cx,
        );
        if !scope.is_remote() && self.git_scope_is_active(scope) {
            self.gx_store.git.state = GitState {
                is_busy: false,
                ..state.clone()
            };
            self.git_publish_menu_state(cx);
        }
    }

    /// The review's scope, or the toast that says why there is none.
    fn git_review_scope(
        &mut self,
        review: &PendingGitReview,
        unavailable: (&'static str, &'static str),
        cx: &mut gpui::Context<Self>,
    ) -> Option<GitScope> {
        match self.git_scope_for_project(&review.project_id) {
            Ok(scope) => Some(scope),
            Err(ScopeMiss::RemoteUnavailable) | Err(ScopeMiss::Unknown)
                if review.remote.is_some() =>
            {
                self.git_toast(
                    GitToastLevel::Warning,
                    "Remote Git unavailable",
                    ToastOptions::described(unavailable.1),
                    cx,
                );
                None
            }
            Err(_) => {
                self.git_toast(
                    GitToastLevel::Error,
                    unavailable.0,
                    ToastOptions::described(
                        "The selected gxserver project is no longer available.",
                    ),
                    cx,
                );
                self.git_publish_menu_state(cx);
                None
            }
        }
    }

    /// Reads the review's project before a confirm acts on it.
    pub(crate) fn git_read_for_confirm(
        &mut self,
        scope: &GitScope,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<GitState> {
        let options = match scope.is_remote() {
            true => GitReadOptions::default(),
            false => GitReadOptions {
                force: true,
                publish_busy: true,
                toast_on_failure: true,
                ..GitReadOptions::default()
            },
        };
        self.git_refresh_state(scope.clone(), options, cx)
    }

    /// `confirmSidebarGitCommit` and its remote twin.
    pub(crate) fn git_confirm_review(
        &mut self,
        confirm: GitReviewConfirm,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(review) = self.gx_store.git.reviews.remove(&confirm.request_id) else {
            self.git_publish_menu_state(cx);
            return;
        };
        let remote = review.remote.is_some();
        let Some(scope) = self.git_review_scope(
            &review,
            (
                "Git action unavailable",
                "Reconnect the remote machine before confirming this Git action.",
            ),
            cx,
        ) else {
            return;
        };
        let read = self.git_read_for_confirm(&scope, cx);
        cx.spawn(async move |this, cx| {
            let state = read.await;
            let _ = this.update(cx, |this, cx| {
                this.git_confirm_review_over_state(scope, review, confirm, state, remote, cx);
            });
        })
        .detach();
    }

    fn git_confirm_review_over_state(
        &mut self,
        scope: GitScope,
        review: PendingGitReview,
        confirm: GitReviewConfirm,
        state: GitState,
        remote: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if !state.is_repo {
            let (title, description) = match remote {
                true => (
                    "Remote Git unavailable",
                    "Open a Git repository on the remote machine to use Git actions.",
                ),
                false => (
                    "Git unavailable",
                    "Open a Git repository to use Git actions.",
                ),
            };
            return self.git_toast(
                GitToastLevel::Warning,
                title,
                ToastOptions::described(description),
                cx,
            );
        }
        let trusted = match review.has_commit {
            false => None,
            true => match resolve_trusted_file_selection(&review, confirm.file_paths.as_deref()) {
                Ok(selection) => Some(selection),
                Err(_) => {
                    let description = match (review.action, remote) {
                        (GitReviewAction::Pr, false) => {
                            "Choose files from the current Git review before creating a pull request."
                        }
                        (GitReviewAction::Pr, true) => {
                            "Choose files from the current remote Git review before creating a pull request."
                        }
                        (_, false) => "Choose files from the current Git review before committing.",
                        (_, true) => {
                            "Choose files from the current remote Git review before committing."
                        }
                    };
                    self.git_toast(
                        GitToastLevel::Warning,
                        "Invalid file selection",
                        ToastOptions::described(description),
                        cx,
                    );
                    if !remote {
                        self.git_set_busy(false, cx);
                    }
                    return;
                }
            },
        };
        if review.action == GitReviewAction::Pr && !confirm.delete_worktree_after {
            return self.git_run_pull_request_agent_workflow(
                scope,
                state,
                PullRequestWorkflow {
                    agent_id: confirm.agent_id.clone(),
                    file_paths: trusted
                        .as_ref()
                        .map(|selection| selection.file_paths.clone()),
                    has_explicit_file_selection: trusted
                        .as_ref()
                        .is_some_and(|selection| selection.explicit),
                    has_commit: review.has_commit,
                    message: confirm.message.clone(),
                    create_branch: confirm.commit_on_new_ref,
                },
                cx,
            );
        }
        let mut params = json!({ "projectId": scope.project_id() });
        if review.has_commit {
            params["commit"] = json!({
                "agentId": self.git_default_prompt_agent_id(confirm.agent_id.as_deref()),
                "commitOnNewRef": confirm.commit_on_new_ref,
                "filePaths": trusted.as_ref().map(|selection| selection.file_paths.clone()),
                "message": confirm.message,
            });
        }
        match review.action {
            GitReviewAction::Pr => params["pullRequest"] = json!(true),
            GitReviewAction::Push => params["push"] = json!(true),
            GitReviewAction::Commit => {}
        }
        let started = started_title(review.action, review.has_commit);
        let finished = finished_title(review.action);
        let mutation = self.git_run_mutation(&scope, started, finished, params, cx);
        let action = review.action;
        let delete_after = confirm.delete_worktree_after;
        cx.spawn(async move |this, cx| {
            let result = mutation.await;
            let _ = this.update(cx, |this, cx| {
                if action == GitReviewAction::Pr {
                    let Some(result) = result else {
                        let title = match remote {
                            true => "Remote worktree cleanup skipped",
                            false => "Worktree cleanup skipped",
                        };
                        return this.git_toast(
                            GitToastLevel::Warning,
                            title,
                            ToastOptions::described("Pull request creation did not complete."),
                            cx,
                        );
                    };
                    // gxserver answers only a confirmed open pull request (CDXC:Git
                    // 2026-06-24-16:28), so an answer is the confirmation.
                    if !result["pullRequest"].is_null() {
                        this.git_open_existing_pull_request(&scope, cx);
                        this.git_delete_worktree_after_completed(&scope, cx)
                            .detach();
                    }
                    return;
                }
                if result.is_some() && delete_after {
                    this.git_delete_worktree_after_completed(&scope, cx)
                        .detach();
                }
            });
        })
        .detach();
    }

    /// `runSidebarGitMultipleCommits`.
    pub(crate) fn git_run_multiple_commits(
        &mut self,
        request_id: &str,
        agent_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let review = self.gx_store.git.reviews.remove(request_id);
        let scoped = match &review {
            Some(review) => Some(review.project_id.clone()),
            None => self.git_active_scoped_project_id(),
        };
        let scope = scoped.as_deref().map(|id| self.git_scope_for_project(id));
        let scope = match (
            scope,
            review
                .as_ref()
                .is_some_and(|review| review.remote.is_some()),
        ) {
            (Some(Ok(scope)), _) => scope,
            (_, true) => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Remote Git unavailable",
                    ToastOptions::described(
                        "Reconnect the remote machine before starting this Git workflow.",
                    ),
                    cx,
                );
            }
            (_, false) => {
                self.git_toast(
                    GitToastLevel::Warning,
                    "Git unavailable",
                    ToastOptions::described("No active gxserver project is available."),
                    cx,
                );
                return self.git_publish_menu_state(cx);
            }
        };
        self.git_run_prompt_workflow(
            scope,
            "Multiple Commits",
            GIT_MULTIPLE_COMMITS_PROMPT.to_string(),
            agent_id,
            None,
            cx,
        );
    }
}
