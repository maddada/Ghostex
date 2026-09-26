//! The review dialog's direct merge into main (`confirmSidebarGitDirectMerge` and its remote twin).
//!
//! CDXC:Git 2026-09-25 WHY:
//! This computer's merge used to be re-implemented client-side (verify main, check the parent is
//! clean, check out main, merge) while a remote machine already used gxserver's
//! `/api/mergeWorktreeIntoMain`. Both use the endpoint now, so the daemon derives main, the parent
//! and the branch for every client (`CDXC:Git 2026-06-24-18:55`). The conflict agent's prompt no
//! longer quotes git's merge output, which the endpoint does not return; the agent reads the
//! conflict state itself.

use ghostex_gx_core::ProjectKey;
use ghostex_gx_core::git_menu::{
    GitState, GitToastLevel, REMOTE_MERGE_CONFLICT_PROMPT, agent_workflow_title,
    merge_conflict_prompt, resolve_trusted_file_selection, user_visible_git_error,
};
use serde_json::{Value, json};

use super::calls;
use super::prompt_agent::PromptAgentLaunch;
use super::review::GitReviewConfirm;
use super::scope::GitScope;
use super::toasts::{ToastOptions, new_toast_id};
use crate::GhostexGpuiApp;

const MERGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

impl GhostexGpuiApp {
    pub(crate) fn git_confirm_direct_merge(
        &mut self,
        confirm: GitReviewConfirm,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(review) = self.gx_store.git.reviews.remove(&confirm.request_id) else {
            self.git_publish_menu_state(cx);
            return;
        };
        let remote = review.remote.is_some();
        let scope = match self.git_scope_for_project(&review.project_id) {
            Ok(scope) => scope,
            Err(_) if remote => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Remote merge unavailable",
                    ToastOptions::described(
                        "Reconnect the remote machine before merging this worktree.",
                    ),
                    cx,
                );
            }
            Err(_) => {
                self.git_toast(
                    GitToastLevel::Error,
                    "Direct merge unavailable",
                    ToastOptions::described(
                        "The selected gxserver project is no longer available.",
                    ),
                    cx,
                );
                return self.git_publish_menu_state(cx);
            }
        };
        let Some(worktree) = scope.worktree.clone() else {
            let (title, description) = match remote {
                true => (
                    "Remote worktree unavailable",
                    "Direct merge is only available from a remote worktree project.",
                ),
                false => (
                    "Worktree unavailable",
                    "Direct merge is only available from a gxserver worktree project.",
                ),
            };
            self.git_toast(
                GitToastLevel::Warning,
                title,
                ToastOptions::described(description),
                cx,
            );
            if !remote {
                self.git_publish_menu_state(cx);
            }
            return;
        };
        let conflict_agent = self.git_workflow_agent(confirm.agent_id.as_deref());
        if !remote && !conflict_agent.has_command() {
            self.git_toast(
                GitToastLevel::Error,
                "Agent unavailable",
                ToastOptions::described("Choose a configured prompt agent before merging."),
                cx,
            );
            return self.git_publish_menu_state(cx);
        }
        let read = match remote {
            true => gpui::Task::ready(GitState {
                is_repo: true,
                ..GitState::default()
            }),
            false => self.git_read_for_confirm(&scope, cx),
        };
        cx.spawn(async move |this, cx| {
            let state = read.await;
            let _ = this.update(cx, |this, cx| {
                if !state.is_repo {
                    return this.git_toast(
                        GitToastLevel::Warning,
                        "Git unavailable",
                        ToastOptions::described("Open a Git repository before merging this worktree."),
                        cx,
                    );
                }
                let trusted = match review.has_commit {
                    false => None,
                    true => match resolve_trusted_file_selection(
                        &review,
                        confirm.file_paths.as_deref(),
                    ) {
                        Ok(selection) => Some(selection),
                        Err(_) => {
                            let description = match remote {
                                true => "Choose files from the current remote Git review before merging.",
                                false => "Choose files from the current Git review before merging.",
                            };
                            this.git_toast(
                                GitToastLevel::Warning,
                                "Invalid file selection",
                                ToastOptions::described(description),
                                cx,
                            );
                            if !remote {
                                this.git_set_busy(false, cx);
                            }
                            return;
                        }
                    },
                };
                let commit = trusted.map(|selection| {
                    json!({
                        "agentId": this.git_default_prompt_agent_id(confirm.agent_id.as_deref()),
                        "filePaths": selection.file_paths,
                        "message": confirm.message,
                    })
                });
                let branch = state.branch.clone().or_else(|| worktree.branch.clone());
                this.git_run_direct_merge(
                    scope,
                    commit,
                    branch,
                    worktree.name.clone(),
                    confirm.agent_id.clone(),
                    confirm.delete_worktree_after,
                    cx,
                );
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn git_run_direct_merge(
        &mut self,
        scope: GitScope,
        commit: Option<Value>,
        branch: Option<String>,
        worktree_name: Option<String>,
        agent_id: Option<String>,
        delete_after: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = scope.is_remote();
        let toast_id = new_toast_id("git");
        self.git_toast(
            GitToastLevel::Info,
            match remote {
                true => "Merging remote worktree",
                false => "Merging worktree into main",
            },
            ToastOptions::persistent(&toast_id),
            cx,
        );
        if !remote {
            self.git_set_busy(true, cx);
            self.git_forget_leases(&scope.project_id(), false);
        }
        let target = scope.remote.clone();
        let project_id = scope.project_id();
        cx.spawn(async move |this, cx| {
            let result = async {
                if let Some(commit) = commit {
                    calls::run_ship_workflow(
                        target.clone(),
                        json!({ "commit": commit, "projectId": project_id }),
                    )
                    .await?;
                }
                calls::call(
                    target.clone(),
                    "/api/mergeWorktreeIntoMain",
                    json!({ "projectId": project_id }),
                    Some(MERGE_TIMEOUT),
                )
                .await
            }
            .await;
            let follow_up = this.update(cx, |this, cx| {
                this.git_after_direct_merge(
                    &scope,
                    result,
                    &toast_id,
                    branch.as_deref(),
                    worktree_name.as_deref(),
                    agent_id.as_deref(),
                    delete_after,
                    cx,
                )
            });
            if let Ok(Some(follow_up)) = follow_up {
                follow_up.await;
                let _ = this.update(cx, |this, cx| {
                    this.git_toast(
                        GitToastLevel::Success,
                        "Worktree merged to main",
                        ToastOptions::keyed(&toast_id),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// What follows the merge. A local merge that went through finishes with its toast after the
    /// worktree delete it asked for; the task that returns is that delete.
    #[allow(clippy::too_many_arguments)]
    fn git_after_direct_merge(
        &mut self,
        scope: &GitScope,
        result: Result<Value, crate::app::gx_store::GxRpcError>,
        toast_id: &str,
        branch: Option<&str>,
        worktree_name: Option<&str>,
        agent_id: Option<&str>,
        delete_after: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<()>> {
        let remote = scope.is_remote();
        let answer = match result {
            Ok(answer) => answer,
            Err(error) => {
                if !remote {
                    self.git_set_busy(false, cx);
                }
                let (title, fallback) = match remote {
                    true => (
                        "Remote direct merge failed",
                        "Remote gxserver could not merge the selected worktree.",
                    ),
                    false => (
                        "Direct merge failed",
                        "gxserver could not merge the selected worktree.",
                    ),
                };
                self.git_toast(
                    GitToastLevel::Error,
                    title,
                    ToastOptions::keyed(toast_id)
                        .with_description(user_visible_git_error(&error.message, fallback)),
                    cx,
                );
                return None;
            }
        };
        let parent_project_id = answer
            .get("parentProjectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let conflicts = answer.get("status").and_then(Value::as_str) == Some("conflicts");
        if !remote {
            // CDXC:Git 2026-07-29: the merge's writes land in the PARENT, and a parent read already
            // in flight would otherwise store its pre-merge answer; the GitHub lease goes with it
            // because it is per branch and this flow checks the parent out onto main.
            if let Some(parent) = parent_project_id.as_deref() {
                self.git_forget_leases(parent, true);
            }
            self.git_set_busy(false, cx);
        }
        if conflicts {
            self.git_toast(
                GitToastLevel::Warning,
                match remote {
                    true => "Remote merge conflicts need resolution",
                    false => "Merge conflicts need resolution",
                },
                ToastOptions::keyed(toast_id),
                cx,
            );
            if let Some(parent) = parent_project_id {
                self.git_start_merge_conflict_agent(
                    scope,
                    &parent,
                    branch,
                    worktree_name,
                    agent_id,
                    cx,
                );
            }
            return None;
        }
        if remote {
            self.git_toast(
                GitToastLevel::Success,
                "Remote worktree merged",
                ToastOptions::keyed(toast_id),
                cx,
            );
            if delete_after {
                self.git_delete_worktree_after_completed(scope, cx).detach();
            }
            return None;
        }
        Some(match delete_after {
            true => self.git_delete_worktree_after_completed(scope, cx),
            false => gpui::Task::ready(()),
        })
    }

    /// `launchMergeConflictAgent`: a prompt agent on the parent project, resolving the conflicts.
    fn git_start_merge_conflict_agent(
        &mut self,
        scope: &GitScope,
        parent_project_id: &str,
        branch: Option<&str>,
        worktree_name: Option<&str>,
        agent_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let parent_key = ProjectKey {
            machine: scope.key.machine.clone(),
            project_id: parent_project_id.to_string(),
        };
        let Ok(parent) = self.git_scope_for_key(parent_key) else {
            return;
        };
        let agent = self.git_workflow_agent(agent_id);
        let prompt = match scope.is_remote() {
            true => REMOTE_MERGE_CONFLICT_PROMPT.to_string(),
            false => merge_conflict_prompt(
                &parent.title,
                branch.unwrap_or_default(),
                worktree_name.unwrap_or(&scope.title),
                "",
            ),
        };
        self.git_start_prompt_agent(
            &parent,
            agent,
            PromptAgentLaunch {
                prompt,
                title: Some(agent_workflow_title("Merge Conflicts")),
                ..PromptAgentLaunch::default()
            },
            cx,
        )
        .detach();
    }
}
