//! Delete Worktree: the dialog's draft, its confirm, and the delete a completed Git action asks for
//! (`promptDeleteWorktreeForGroup`, `confirmDeleteWorktree`, `deleteWorktreeAfterCompletedGitAction`
//! and their remote twins).

use ghostex_gx_core::ProjectKey;
use ghostex_gx_core::git_menu::{
    GitToastLevel, short_status_has_changes, worktree_branch_metadata, worktree_branch_name,
    worktree_delete_warning_title,
};
use serde_json::{Value, json};

use super::calls;
use super::scope::{GitScope, ScopeMiss};
use super::toasts::{ToastOptions, new_toast_id};
use crate::GhostexGpuiApp;

const DELETE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// The branch facts a worktree dialog opens with: its current branch, `git status --short
/// --branch`, and whether `origin/<branch>` exists.
pub(crate) struct WorktreeBranchFacts {
    pub(crate) branch: Option<String>,
    pub(crate) status: String,
    pub(crate) remote_branch_exists: bool,
}

/// Reads the branch facts, or the failure's message.
pub(crate) async fn read_worktree_branch_facts(
    remote: calls::Remote,
    project_id: String,
    fallback_branch: Option<String>,
    failure: &'static str,
) -> Result<WorktreeBranchFacts, String> {
    let (branch, status) = futures::join!(
        calls::git_action(remote.clone(), project_id.clone(), "branch", json!({})),
        calls::git_action(remote.clone(), project_id.clone(), "status", json!({})),
    );
    let (branch, status) = (
        branch.map_err(|error| error.message)?,
        status.map_err(|error| error.message)?,
    );
    if calls::exit_code(&branch) != 0 || calls::exit_code(&status) != 0 {
        return Err(failure.to_string());
    }
    let branch = worktree_branch_name(Some(calls::stdout(&branch)), fallback_branch.as_deref());
    let remote_branch_exists = match &branch {
        Some(branch) => calls::git_action(
            remote,
            project_id,
            "remoteBranchExists",
            json!({ "branch": branch, "remoteName": "origin" }),
        )
        .await
        .map_err(|error| error.message)
        .map(|result| calls::exit_code(&result) == 0)?,
        None => false,
    };
    Ok(WorktreeBranchFacts {
        branch,
        status: calls::stdout(&status).to_string(),
        remote_branch_exists,
    })
}

impl GhostexGpuiApp {
    /// A project menu's Delete Worktree: read the branch and status first, so a dirty checkout can
    /// offer Commit before the destructive removal, then open the dialog.
    pub(crate) fn git_prompt_delete_worktree(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = group_id.starts_with("remote:");
        let scope = match self.git_scope_for_group(group_id) {
            Ok(scope) if scope.worktree.is_some() => scope,
            Err(ScopeMiss::RemoteUnavailable) | Ok(_) | Err(ScopeMiss::Unknown) if remote => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Remote worktree unavailable",
                    ToastOptions::described(
                        "Reconnect the remote machine and try deleting the worktree again.",
                    ),
                    cx,
                );
            }
            _ => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Not a worktree",
                    ToastOptions::described("Only worktree projects can be deleted."),
                    cx,
                );
            }
        };
        let worktree = scope.worktree.clone().unwrap_or_default();
        let group_id = group_id.to_string();
        let failure = match remote {
            true => "Could not read remote worktree status.",
            false => "Could not read worktree status.",
        };
        let facts = read_worktree_branch_facts(
            scope.remote.clone(),
            scope.project_id(),
            worktree.branch.clone(),
            failure,
        );
        cx.spawn(async move |this, cx| {
            let facts = facts.await;
            let _ = this.update(cx, |this, cx| match facts {
                Ok(facts) => {
                    let mut draft = worktree_branch_metadata(
                        facts.branch.as_deref(),
                        facts.remote_branch_exists,
                    );
                    draft.insert("groupId".into(), json!(group_id));
                    draft.insert(
                        "hasChanges".into(),
                        json!(short_status_has_changes(&facts.status)),
                    );
                    draft.insert("projectId".into(), json!(scope.scoped_id()));
                    draft.insert("statusSummary".into(), json!(facts.status.trim()));
                    let name = Some(scope.title.clone())
                        .filter(|title| !title.is_empty())
                        .or(worktree.name.clone())
                        .unwrap_or_else(|| "worktree".to_string());
                    draft.insert("worktreeName".into(), json!(name));
                    this.open_app_modal_from_bridge(
                        json!({
                            "modal": "deleteWorktree",
                            "type": "open",
                            "worktreeDeleteDraft": Value::Object(draft),
                        }),
                        cx,
                    );
                }
                Err(message) => this.git_toast(
                    GitToastLevel::Error,
                    match remote {
                        true => "Could not inspect remote worktree",
                        false => "Could not inspect worktree",
                    },
                    ToastOptions::described(message),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// The Delete Worktree dialog's confirm.
    ///
    /// CDXC:Git 2026-07-29:
    /// Worktree removal is a Git write outside the ship workflow: gxserver removes the worktree
    /// from the parent repository and, when asked, deletes the branch there too. The parent's lease
    /// and the removed project's own are dropped before the call, so neither can be drawn again.
    pub(crate) fn git_confirm_delete_worktree(
        &mut self,
        project_id: &str,
        delete_local_branch: bool,
        delete_remote_branch: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = project_id.starts_with("remote:");
        let scope = match self.git_scope_for_project(project_id) {
            Ok(scope) if remote || scope.worktree.is_some() => scope,
            _ if remote => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Remote worktree unavailable",
                    ToastOptions::described(
                        "Reconnect the remote machine and try deleting the worktree again.",
                    ),
                    cx,
                );
            }
            _ => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Worktree unavailable",
                    ToastOptions::described("The selected worktree no longer exists."),
                    cx,
                );
            }
        };
        let toast_id = new_toast_id("worktree");
        let started = match remote {
            true => ToastOptions::persistent(&toast_id),
            false => ToastOptions::persistent(&toast_id).with_description(scope.title.clone()),
        };
        self.git_toast(
            GitToastLevel::Info,
            match remote {
                true => "Deleting remote worktree",
                false => "Deleting worktree",
            },
            started,
            cx,
        );
        self.git_forget_worktree_leases(&scope);
        let params = json!({
            "deleteLocalBranch": delete_local_branch,
            "deleteRemoteBranch": delete_remote_branch,
            "projectId": scope.project_id(),
        });
        let remote_target = scope.remote.clone();
        cx.spawn(async move |this, cx| {
            let result = calls::call(
                remote_target,
                "/api/deleteWorktreeProject",
                params,
                remote.then_some(DELETE_TIMEOUT),
            )
            .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(answer) => {
                    this.git_post_worktree_delete_warnings(&answer, cx);
                    this.git_focus_worktree_parent(&scope, cx);
                    let done = match remote {
                        true => ToastOptions::keyed(&toast_id),
                        false => {
                            ToastOptions::keyed(&toast_id).with_description(scope.title.clone())
                        }
                    };
                    this.git_toast(
                        GitToastLevel::Success,
                        match remote {
                            true => "Remote worktree deleted",
                            false => "Worktree deleted",
                        },
                        done,
                        cx,
                    );
                }
                Err(_) => this.git_toast(
                    GitToastLevel::Error,
                    match remote {
                        true => "Could not delete remote worktree",
                        false => "Could not delete worktree",
                    },
                    ToastOptions::keyed(&toast_id).with_description(match remote {
                        true => "Remote gxserver worktree removal failed.",
                        false => "gxserver worktree removal failed.",
                    }),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// The worktree delete a completed commit, push, pull request or merge asked for. The task ends
    /// when the delete has answered.
    pub(crate) fn git_delete_worktree_after_completed(
        &mut self,
        scope: &GitScope,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<()> {
        let remote = scope.is_remote();
        let scope = self
            .git_scope_for_key(scope.key.clone())
            .unwrap_or_else(|_| scope.clone());
        if scope.worktree.is_none() {
            self.git_toast(
                GitToastLevel::Warning,
                match remote {
                    true => "Remote worktree cleanup skipped",
                    false => "Worktree cleanup skipped",
                },
                ToastOptions::described(match remote {
                    true => "The selected remote project is no longer a worktree.",
                    false => "The selected gxserver project is no longer a worktree.",
                }),
                cx,
            );
            return gpui::Task::ready(());
        }
        let toast_id = new_toast_id("git");
        self.git_toast(
            GitToastLevel::Info,
            match remote {
                true => "Removing remote worktree",
                false => "Removing worktree",
            },
            ToastOptions::persistent(&toast_id),
            cx,
        );
        self.git_forget_worktree_leases(&scope);
        let target = scope.remote.clone();
        let params = json!({
            "deleteLocalBranch": false,
            "deleteRemoteBranch": false,
            "projectId": scope.project_id(),
        });
        cx.spawn(async move |this, cx| {
            let result = calls::call(
                target,
                "/api/deleteWorktreeProject",
                params,
                remote.then_some(DELETE_TIMEOUT),
            )
            .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(answer) => {
                    this.git_post_worktree_delete_warnings(&answer, cx);
                    this.git_focus_worktree_parent(&scope, cx);
                    this.git_toast(
                        GitToastLevel::Success,
                        match remote {
                            true => "Remote worktree removed",
                            false => "Worktree removed",
                        },
                        ToastOptions::keyed(&toast_id),
                        cx,
                    );
                }
                Err(_) => this.git_toast(
                    GitToastLevel::Error,
                    match remote {
                        true => "Could not remove remote worktree",
                        false => "Could not remove worktree",
                    },
                    ToastOptions::keyed(&toast_id).with_description(match remote {
                        true => "Remote gxserver worktree cleanup failed.",
                        false => "gxserver worktree cleanup failed.",
                    }),
                    cx,
                ),
            });
        })
    }

    fn git_forget_worktree_leases(&mut self, scope: &GitScope) {
        if scope.is_remote() {
            return;
        }
        if let Some(worktree) = &scope.worktree {
            self.git_forget_leases(&worktree.parent_project_id, false);
        }
        self.git_forget_leases(&scope.project_id(), true);
    }

    /// `postGxserverWorktreeDeleteWarnings`.
    fn git_post_worktree_delete_warnings(&mut self, answer: &Value, cx: &mut gpui::Context<Self>) {
        let warnings = answer
            .get("warnings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for warning in warnings {
            if let Some(title) = warning
                .get("kind")
                .and_then(Value::as_str)
                .and_then(worktree_delete_warning_title)
            {
                self.git_toast(GitToastLevel::Warning, title, ToastOptions::default(), cx);
            }
        }
    }

    /// After a delete on this computer the parent project is selected (`focusProjectId(parent)`).
    fn git_focus_worktree_parent(&mut self, scope: &GitScope, cx: &mut gpui::Context<Self>) {
        if scope.is_remote() {
            return;
        }
        if let Some(worktree) = &scope.worktree {
            self.git_focus_project(&ProjectKey::local(worktree.parent_project_id.clone()), cx);
        }
    }
}
