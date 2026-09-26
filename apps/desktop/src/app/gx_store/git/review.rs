//! The commit review dialog: opening it, and what its buttons do (confirm, cancel, Multiple
//! Commits, a file's diff, opening a file). Ported from the old runtime's
//! `promptSidebarGitActionReview`, `confirmSidebarGitCommit`, `runSidebarGitMultipleCommits`,
//! `openSidebarGitChangedFileDiff` and `openSidebarGitChangedFileInIde`, with their remote twins.
//!
//! CDXC:Git 2026-06-24-15:55:
//! Worktree completion may run direct merge-to-main and delete-after cleanup only from a confirmed
//! Git review request. The confirm names the review's request id, and every path it commits or
//! inspects must be one that review listed; renderer paths, branch text and modal labels are never
//! authority for side effects.

use ghostex_gx_core::git_menu::{
    GIT_MULTIPLE_COMMITS_PROMPT, GitRemoteProject, GitReviewAction, GitState, GitToastLevel,
    PendingGitReview, ReviewDraftInput, finished_title, normalize_relative_git_file_path,
    resolve_trusted_file_selection, review_modal_draft, started_title,
};
use serde_json::{Value, json};

use super::actions::PullRequestWorkflow;
use super::calls;
use super::hud::GitReadOptions;
use super::scope::{GitScope, ScopeMiss};
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;

/// A confirm from the dialog (`confirmSidebarGitCommit`, `confirmSidebarGitDirectMerge`).
#[derive(Clone, Debug, Default)]
pub(crate) struct GitReviewConfirm {
    pub(crate) request_id: String,
    pub(crate) message: String,
    pub(crate) agent_id: Option<String>,
    pub(crate) file_paths: Option<Vec<String>>,
    pub(crate) commit_on_new_ref: bool,
    pub(crate) delete_worktree_after: bool,
}

impl GitReviewConfirm {
    fn read(command: &Value) -> Option<Self> {
        let text = |key: &str| command.get(key).and_then(Value::as_str);
        let request_id =
            text("requestId").filter(|id| !id.is_empty() && id.chars().count() <= 120)?;
        let message = text("message").filter(|message| message.chars().count() <= 20_000)?;
        Some(Self {
            request_id: request_id.to_string(),
            message: message.to_string(),
            agent_id: text("agentId")
                .filter(|id| !id.is_empty() && id.chars().count() <= 300)
                .map(str::to_string),
            file_paths: command
                .get("filePaths")
                .and_then(Value::as_array)
                .map(|paths| {
                    paths
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|path| !path.is_empty() && path.chars().count() <= 1024)
                        .map(str::to_string)
                        .collect()
                }),
            commit_on_new_ref: command.get("commitOnNewRef").and_then(Value::as_bool) == Some(true),
            delete_worktree_after: command.get("deleteWorktreeAfter").and_then(Value::as_bool)
                == Some(true),
        })
    }
}

fn review_request_id(remote: bool) -> String {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    match remote {
        true => format!("gpui-remote-git-action-{millis:x}"),
        false => format!("gpui-git-action-{millis:x}"),
    }
}

impl GhostexGpuiApp {
    /// A command from the commit review dialog (`handleGpuiGitCommitModalCommand`), after the
    /// dialog's field allowlist (`forward_gpui_git_commit_modal_command`).
    pub(crate) fn git_commit_modal_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str, max: usize| {
            command
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.chars().count() <= max)
                .map(str::to_string)
        };
        let Some(request_id) = text("requestId", 120) else {
            return;
        };
        match command.get("type").and_then(Value::as_str) {
            Some("confirmSidebarGitCommit") => {
                if let Some(confirm) = GitReviewConfirm::read(command) {
                    self.git_confirm_review(confirm, cx);
                }
            }
            Some("confirmSidebarGitDirectMerge") => {
                if let Some(confirm) = GitReviewConfirm::read(command) {
                    self.git_confirm_direct_merge(confirm, cx);
                }
            }
            Some("runSidebarGitMultipleCommits") => {
                self.git_run_multiple_commits(&request_id, text("agentId", 300), cx);
            }
            Some("openSidebarGitChangedFileDiff") => {
                if let Some(file_path) = text("filePath", 1024) {
                    self.git_open_changed_file_diff(&request_id, &file_path, cx);
                }
            }
            Some("openSidebarGitChangedFile") => {
                if let Some(file_path) = text("filePath", 1024) {
                    let open_location =
                        command.get("openLocation").and_then(Value::as_bool) == Some(true);
                    self.git_open_changed_file(Some(&request_id), &file_path, open_location, cx);
                }
            }
            Some("cancelSidebarGitCommit") => {
                self.gx_store.git.reviews.remove(&request_id);
                self.git_publish_menu_state(cx);
            }
            _ => {}
        }
    }

    /// Opens the review dialog over the state just read (`promptSidebarGitActionReview`).
    pub(crate) fn git_open_review(
        &mut self,
        scope: &GitScope,
        state: &GitState,
        action: GitReviewAction,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = review_request_id(scope.is_remote());
        self.gx_store.git.reviews.insert(
            request_id.clone(),
            PendingGitReview {
                action,
                files: state.files.clone(),
                has_commit: state.has_working_tree_changes,
                project_id: scope.scoped_id(),
                remote: scope.machine_id().map(|machine_id| GitRemoteProject {
                    machine_id: machine_id.to_string(),
                    project_id: scope.project_id(),
                }),
            },
        );
        // The remote dialog always names an agent id; the local one only an agent this app knows.
        let agent_id = match scope.is_remote() {
            true => Some(self.git_default_prompt_agent_id(None)),
            false => {
                let agent_id = self.git_default_prompt_agent_id(None);
                self.git_hud_agent(&agent_id).map(|agent| agent.agent_id)
            }
        };
        let worktree_name = scope.worktree_name();
        let draft = review_modal_draft(ReviewDraftInput {
            action,
            agent_id: agent_id.as_deref(),
            state,
            is_worktree: scope.is_worktree(),
            remote: scope.is_remote(),
            request_id: &request_id,
            worktree_name: worktree_name.as_deref(),
        });
        self.open_app_modal_from_bridge(
            json!({ "gitCommitDraft": draft, "modal": "gitCommit", "type": "open" }),
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
    fn git_run_multiple_commits(
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

    /// A changed file's diff in the review dialog (`openSidebarGitChangedFileDiff`). Only paths the
    /// review listed are read.
    fn git_open_changed_file_diff(
        &mut self,
        request_id: &str,
        file_path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(review) = self.gx_store.git.reviews.get(request_id).cloned() else {
            return;
        };
        let Ok(scope) = self.git_scope_for_project(&review.project_id) else {
            return;
        };
        let Some(path) = normalize_relative_git_file_path(file_path) else {
            return;
        };
        let Some(file) = review.file(&path).cloned() else {
            return;
        };
        let remote = scope.remote.clone();
        let project_id = scope.project_id();
        let request_id = request_id.to_string();
        cx.spawn(async move |this, cx| {
            let patch = read_file_patch(remote, project_id, &path).await;
            let patch = patch
                .filter(|patch| !patch.trim().is_empty())
                .map(|patch| patch.trim().to_string())
                .unwrap_or_else(|| format!("No diff is available for {path}."));
            let _ = this.update(cx, |this, cx| {
                this.open_app_modal_from_bridge(
                    json!({
                        "gitFileDiff": {
                            "additions": file.additions,
                            "deletions": file.deletions,
                            "filePath": path,
                            "patch": patch,
                        },
                        "modal": "gitFileDiff",
                        "requestId": request_id,
                        "type": "open",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Opens a changed file in the IDE, or its location (`openSidebarGitChangedFileInIde`).
    ///
    /// CDXC:Git 2026-06-24-21:26:
    /// Changed-file IDE opens send Rust only the gxserver project id and a normalized relative file
    /// candidate already present in the current review or Git state; Rust re-validates the file
    /// against gxserver before resolving an absolute path.
    pub(crate) fn git_open_changed_file(
        &mut self,
        request_id: Option<&str>,
        file_path: &str,
        open_location: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let path = normalize_relative_git_file_path(file_path);
        let review = request_id.and_then(|id| self.gx_store.git.reviews.get(id).cloned());
        if let Some(review) = review {
            if review.remote.is_some() {
                if open_location {
                    return self.git_toast(
                        GitToastLevel::Warning,
                        "File location unavailable",
                        ToastOptions::described(
                            "Open File/Folder Location requires a local project on this machine.",
                        ),
                        cx,
                    );
                }
                let listed = path.as_deref().is_some_and(|path| review.has_file(path));
                match (self.git_scope_for_project(&review.project_id), listed, path) {
                    (Ok(scope), true, Some(path)) => self.git_native_project_action(
                        "openRemoteSidebarGitChangedFileInIde",
                        &scope.scoped_id(),
                        Some(&path),
                        cx,
                    ),
                    _ => self.git_toast(
                        GitToastLevel::Warning,
                        "Remote file open unavailable",
                        ToastOptions::described(
                            "Choose a changed file from the current remote Git review.",
                        ),
                        cx,
                    ),
                }
                return;
            }
            let listed = path.as_deref().is_some_and(|path| review.has_file(path));
            return match (listed, path) {
                (true, Some(path)) => self.git_native_project_action(
                    if open_location {
                        "revealSidebarGitChangedFile"
                    } else {
                        "openSidebarGitChangedFileInIde"
                    },
                    &review.project_id,
                    Some(&path),
                    cx,
                ),
                _ => self.git_toast(
                    GitToastLevel::Warning,
                    "Open file unavailable",
                    ToastOptions::described("Choose a changed file from the current Git state."),
                    cx,
                ),
            };
        }
        // No review: the active project's drawn state.
        let listed = path.as_deref().is_some_and(|path| {
            self.gx_store
                .git
                .state
                .files
                .iter()
                .any(|file| file.path == path)
        });
        match (self.git_active_scoped_project_id(), listed, path) {
            (Some(project_id), true, Some(path)) if !project_id.starts_with("remote:") => self
                .git_native_project_action(
                    if open_location {
                        "revealSidebarGitChangedFile"
                    } else {
                        "openSidebarGitChangedFileInIde"
                    },
                    &project_id,
                    Some(&path),
                    cx,
                ),
            _ => self.git_toast(
                GitToastLevel::Warning,
                "Open file unavailable",
                ToastOptions::described("Choose a changed file from the current Git state."),
                cx,
            ),
        }
    }
}

/// The staged and unstaged diff of one file, or for an untracked file its diff against nothing.
async fn read_file_patch(remote: calls::Remote, project_id: String, path: &str) -> Option<String> {
    let file = json!({ "filePath": path });
    let (staged, unstaged) = futures::join!(
        calls::git_action(
            remote.clone(),
            project_id.clone(),
            "diffCachedNoExt",
            file.clone()
        ),
        calls::git_action(
            remote.clone(),
            project_id.clone(),
            "diffNoExt",
            file.clone()
        ),
    );
    let (staged, unstaged) = (staged.ok()?, unstaged.ok()?);
    let parts: Vec<&str> = [calls::stdout(&staged), calls::stdout(&unstaged)]
        .into_iter()
        .map(str::trim_end)
        .filter(|part| !part.trim().is_empty())
        .collect();
    let patch = parts.join("\n\n");
    if !patch.trim().is_empty() {
        return Some(patch);
    }
    let untracked = calls::git_action(
        remote.clone(),
        project_id.clone(),
        "isUntrackedFile",
        file.clone(),
    )
    .await
    .ok()?;
    if calls::stdout(&untracked).trim().is_empty() {
        return Some(patch);
    }
    let no_index = calls::git_action(remote, project_id, "diffNoIndexAgainstNull", file)
        .await
        .ok()?;
    let stdout = calls::stdout(&no_index).trim_end();
    Some(match stdout.is_empty() {
        true => no_index
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_end()
            .to_string(),
        false => stdout.to_string(),
    })
}
