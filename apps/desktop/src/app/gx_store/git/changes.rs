//! The Changes view's commit bar and the Git menu's entry points into it. Each commit reads the
//! project's Git state and confirms a review built from it, so the commit, push, pull request,
//! Multiple Commits and merge flows the commit review window ran are the same code; only the window
//! is gone.
//!
//! CDXC:Changes 2026-10-25 DECISION:
//! User: "we need to remove the old floating diff thing we had from before and just use this one instead". The commit review window and the standalone file diff window are gone; every place that opened them (the header's Commit button and Git menu, the Git hotkeys, the sidebar's Git rows, Delete worktree's Commit first, Session details' Commit and Create PR) opens the Changes tab instead with its commit bar ready, on Uncommitted for Commit and Push and on Against main for a pull request. This supersedes the 2026-09-28 Git decision that kept that window "looking and working exactly as now".

use ghostex_gx_core::git_menu::{
    GitRemoteProject, GitReviewAction, GitState, GitToastLevel, PendingGitReview,
};
use serde_json::Value;

use super::review::GitReviewConfirm;
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;
use crate::app::native_changes::{ChangesScope, ChangesTarget, CommitAction};

fn changes_request_id(remote: bool) -> String {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    match remote {
        true => format!("gpui-remote-changes-{millis:x}"),
        false => format!("gpui-changes-{millis:x}"),
    }
}

impl GhostexGpuiApp {
    /// The project the titlebar shows, as the Changes view's target.
    pub(crate) fn changes_target_for_active_project(&self) -> Option<ChangesTarget> {
        let key = self.git_active_scoped_project_id()?;
        let scope = self.git_scope_for_project(&key).ok()?;
        if scope.projectless {
            return None;
        }
        Some(ChangesTarget {
            project_key: scope.scoped_id(),
            rpc_project_id: scope.project_id(),
            remote: scope.remote.clone(),
        })
    }

    /// The folder of a project on this computer (`None` for a remote one).
    pub(crate) fn changes_project_root(&self, project_key: &str) -> Option<std::path::PathBuf> {
        let scope = self.git_scope_for_project(project_key).ok()?;
        if scope.is_remote() {
            return None;
        }
        scope.path.map(std::path::PathBuf::from)
    }

    /// `(agent id, name)` of the agents that can write a commit message (the ones with a launch
    /// command), and the Default Prompt Agent.
    pub(crate) fn changes_commit_agents(&self) -> (Vec<(String, String)>, Option<String>) {
        let agents = self
            .gx_store
            .runtime_facts
            .hud()
            .and_then(|hud| hud.get("agents"))
            .and_then(Value::as_array)
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|agent| {
                        let text = |key: &str| {
                            agent
                                .get(key)
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty())
                                .map(str::to_string)
                        };
                        text("command")?;
                        let id = text("agentId")?;
                        // CDXC:Changes 2026-10-25 DECISION: User: "remove Claude (cloud) from agents who can be used for this (or any cloud agent)". The commit message is written on this computer, so only local agents are offered.
                        if ghostex_gx_core::is_claude_cloud_agent(&id) {
                            return None;
                        }
                        let name = text("name").unwrap_or_else(|| id.clone());
                        Some((id, name))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let default = Some(self.git_default_prompt_agent_id(None))
            .filter(|id| agents.iter().any(|agent| &agent.0 == id));
        (agents, default)
    }

    /// The review a commit bar action confirms, built from a fresh read of the project.
    fn changes_review_then(
        &mut self,
        project_key: &str,
        action: GitReviewAction,
        cx: &mut gpui::Context<Self>,
        then: impl FnOnce(&mut Self, String, &mut gpui::Context<Self>) + 'static,
    ) {
        let scope = match self.git_scope_for_project(project_key) {
            Ok(scope) => scope,
            Err(_) => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Git unavailable",
                    ToastOptions::described("This project is no longer available."),
                    cx,
                );
            }
        };
        let read = self.git_read_for_confirm(&scope, cx);
        cx.spawn(async move |this, cx| {
            let state: GitState = read.await;
            let _ = this.update(cx, |this, cx| {
                if !state.is_repo {
                    return this.git_toast(
                        GitToastLevel::Warning,
                        "Git unavailable",
                        ToastOptions::described("Open a Git repository to use Git actions."),
                        cx,
                    );
                }
                let request_id = changes_request_id(scope.is_remote());
                this.gx_store.git.reviews.insert(
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
                then(this, request_id, cx);
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn changes_commit(
        &mut self,
        project_key: &str,
        action: CommitAction,
        message: String,
        agent_id: Option<String>,
        file_paths: Option<Vec<String>>,
        new_branch: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let review_action = match action {
            CommitAction::Commit => GitReviewAction::Commit,
            CommitAction::CommitPush => GitReviewAction::Push,
            CommitAction::CommitPushPr => GitReviewAction::Pr,
        };
        self.changes_review_then(
            project_key,
            review_action,
            cx,
            move |this, request_id, cx| {
                this.git_confirm_review(
                    GitReviewConfirm {
                        request_id,
                        message,
                        agent_id,
                        file_paths,
                        commit_on_new_ref: new_branch,
                        delete_worktree_after: false,
                    },
                    cx,
                );
            },
        );
    }

    pub(crate) fn changes_multiple_commits(
        &mut self,
        project_key: &str,
        agent_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.changes_review_then(
            project_key,
            GitReviewAction::Commit,
            cx,
            move |this, request_id, cx| {
                this.git_run_multiple_commits(&request_id, agent_id, cx);
            },
        );
    }

    pub(crate) fn changes_merge_into_main(
        &mut self,
        project_key: &str,
        message: String,
        agent_id: Option<String>,
        file_paths: Option<Vec<String>>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.changes_review_then(
            project_key,
            GitReviewAction::Commit,
            cx,
            move |this, request_id, cx| {
                this.git_confirm_direct_merge(
                    GitReviewConfirm {
                        request_id,
                        message,
                        agent_id,
                        file_paths,
                        commit_on_new_ref: false,
                        delete_worktree_after: false,
                    },
                    cx,
                );
            },
        );
    }

    /// Where an entry point of the Git menu used to open the commit review: the scope its action
    /// is about.
    pub(crate) fn changes_preset_for_review(
        action: GitReviewAction,
    ) -> (ChangesScope, CommitAction) {
        match action {
            GitReviewAction::Commit => (ChangesScope::Uncommitted, CommitAction::Commit),
            GitReviewAction::Push => (ChangesScope::Uncommitted, CommitAction::CommitPush),
            GitReviewAction::Pr => (ChangesScope::AgainstMain, CommitAction::CommitPushPr),
        }
    }
}
