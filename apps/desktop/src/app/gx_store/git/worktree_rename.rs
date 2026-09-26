//! Rename Worktree: the dialog's draft and its confirm (`promptRenameWorktreeForGroup`,
//! `confirmRenameWorktree`).
//!
//! CDXC:Worktrees 2026-08-09-18:40:
//! Everything the dialog needs to decide whether a rename can happen is gathered before the
//! window opens, because it has no channel to ask gxserver anything once it is up: answers that do
//! not depend on the typed name (submodules, pushed branch, uncommitted changes, live sessions,
//! agent history) ride the draft; answers that are pure computation over draft data (a folder that
//! collides with the main checkout or another registered project) are recomputed live in the
//! dialog; answers that need git for a name nobody has typed yet are enforced by gxserver at submit
//! and surface as the error toast. Remote worktrees get an honest refusal: the rename endpoint is
//! remote-allowed, but the remote presentation indirection has no rename counterpart yet.

use ghostex_gx_core::git_menu::{
    GitToastLevel, is_managed_worktree_branch, normalize_project_path, project_name_from_path,
    short_status_has_changes, worktree_folder_suffix, worktree_rename_user_visible_error,
};
use ghostex_gx_core::{MachineId, ProjectKey};
use serde_json::{Value, json};

use super::calls;
use super::toasts::{ToastOptions, new_toast_id};
use super::worktree_delete::read_worktree_branch_facts;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn git_prompt_rename_worktree(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if group_id.starts_with("remote:") {
            return self.git_toast(
                GitToastLevel::Warning,
                "Not available for remote worktrees",
                ToastOptions::described("Rename a remote worktree from the machine that owns it."),
                cx,
            );
        }
        let scope = match self.git_scope_for_group(group_id) {
            Ok(scope) if scope.worktree.is_some() && scope.path.is_some() => scope,
            _ => {
                return self.git_toast(
                    GitToastLevel::Warning,
                    "Not a worktree",
                    ToastOptions::described("Only worktree projects can be renamed."),
                    cx,
                );
            }
        };
        let worktree = scope.worktree.clone().unwrap_or_default();
        let project_path = scope.path.clone().unwrap_or_default();
        let parent = self
            .git_scope_for_key(ProjectKey::local(worktree.parent_project_id.clone()))
            .ok()
            .filter(|parent| parent.path.is_some());
        let Some(parent) = parent else {
            return self.git_toast(
                GitToastLevel::Warning,
                "Parent project unavailable",
                ToastOptions::described("The worktree's parent project is not registered."),
                cx,
            );
        };
        let parent_path = parent.path.clone().unwrap_or_default();
        // The sessions the warnings count, read now as the old runtime read its presentation.
        let (running_sessions, has_agent_history) = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)
            .map(|loaded| {
                let sessions: Vec<_> = loaded
                    .server_sessions()
                    .filter(|session| session.project_id == scope.key.project_id)
                    .collect();
                let running = sessions
                    .iter()
                    .filter(|session| {
                        serde_json::to_value(&session.lifecycle_state)
                            .ok()
                            .and_then(|state| state.as_str().map(str::to_string))
                            .as_deref()
                            == Some("running")
                    })
                    .count();
                let agents = sessions.iter().any(|session| {
                    session
                        .agent_id
                        .as_deref()
                        .is_some_and(|agent| !agent.is_empty())
                });
                (running, agents)
            })
            .unwrap_or_default();
        let registered_paths = self.git_registered_project_paths(&scope.key.project_id);
        let project_id = scope.project_id();
        let facts = read_worktree_branch_facts(
            None,
            project_id.clone(),
            worktree.branch.clone(),
            "Could not read worktree status.",
        );
        // CDXC:Worktrees 2026-08-09-18:40: the submodule probe is an early warning, not the guard.
        // A daemon that does not know the action refuses it, and that must not cost the user the
        // whole dialog; gxserver re-checks submodules inside the rename itself.
        let submodules = calls::call(
            None,
            "/api/runWorktreeAction",
            json!({
                "action": "hasPopulatedSubmodules",
                "projectId": parent.project_id(),
                "worktreePath": project_path,
            }),
            None,
        );
        let worktree_name = Some(scope.title.clone())
            .filter(|title| !title.is_empty())
            .or(worktree.name.clone());
        cx.spawn(async move |this, cx| {
            let (facts, submodules) = futures::join!(facts, submodules);
            let _ = this.update(cx, |this, cx| {
                let facts = match facts {
                    Ok(facts) => facts,
                    Err(message) => {
                        return this.git_toast(
                            GitToastLevel::Error,
                            "Could not inspect worktree",
                            ToastOptions::described(message),
                            cx,
                        );
                    }
                };
                let branch = facts.branch.clone();
                let mut warnings: Vec<String> = Vec::new();
                if let (true, Some(branch)) = (facts.remote_branch_exists, &branch) {
                    warnings.push(format!(
                        "Renaming here only renames the local branch. origin/{branch} keeps its old name, and your next push will be rejected until you set a new upstream."
                    ));
                }
                if short_status_has_changes(&facts.status) {
                    warnings.push("This worktree has uncommitted changes. They move with the folder and are not touched.".to_string());
                }
                if running_sessions > 0 {
                    warnings.push(format!(
                        "{running_sessions} running session(s) will keep working, but their shell still thinks it is in the old folder until you cd or restart them."
                    ));
                }
                if has_agent_history {
                    warnings.push("Agent history (Claude/Cursor) is filed under the old folder path and will not follow the rename.".to_string());
                }
                let parent_folder = project_name_from_path(&parent_path);
                let current_folder = project_name_from_path(&project_path);
                let mut draft = serde_json::Map::new();
                if submodules.is_ok_and(|result| calls::exit_code(&result) == 0) {
                    draft.insert(
                        "blockingReason".into(),
                        json!("This worktree has initialised submodules, and git cannot move those. Remove them (git submodule deinit --all) or move the folder yourself."),
                    );
                }
                if let Some(branch) = &branch {
                    draft.insert("branch".into(), json!(branch));
                }
                draft.insert(
                    "currentName".into(),
                    json!(worktree_folder_suffix(&current_folder, &parent_folder)),
                );
                draft.insert("currentPath".into(), json!(project_path));
                draft.insert("parentFolderName".into(), json!(parent_folder));
                draft.insert("parentProjectPath".into(), json!(parent_path));
                draft.insert("projectId".into(), json!(project_id));
                draft.insert("registeredProjectPaths".into(), json!(registered_paths));
                draft.insert(
                    "renameBranchDefault".into(),
                    json!(is_managed_worktree_branch(branch.as_deref())),
                );
                draft.insert("warnings".into(), json!(warnings));
                draft.insert(
                    "worktreeName".into(),
                    json!(worktree_name.clone().unwrap_or(current_folder)),
                );
                this.open_app_modal_from_bridge(
                    json!({
                        "modal": "renameWorktree",
                        "type": "open",
                        "worktreeRenameDraft": Value::Object(draft),
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    /// Every registered project folder on this computer but `except`'s, for the dialog's live
    /// collision check.
    fn git_registered_project_paths(&self, except: &str) -> Vec<String> {
        let Some(machine) = self.gx_store.core.presentation().machine(&MachineId::Local) else {
            return Vec::new();
        };
        let mut paths: Vec<String> = machine
            .domain_projects()
            .filter(|(project_id, _)| project_id.as_str() != except)
            .filter_map(|(_, project)| {
                normalize_project_path(project.get("path").and_then(Value::as_str))
            })
            .collect();
        if paths.is_empty() {
            paths = machine
                .loaded()
                .map(|loaded| {
                    loaded
                        .projects()
                        .iter()
                        .filter(|project| project.project_id != except)
                        .filter_map(|project| normalize_project_path(project.path.as_deref()))
                        .collect()
                })
                .unwrap_or_default();
        }
        paths
    }

    /// The Rename Worktree dialog's confirm.
    ///
    /// CDXC:Git 2026-07-29 (extended for rename): the rename is a Git write outside the ship
    /// workflow (gxserver moves the checkout and can rename the branch in the parent repository),
    /// so the parent's lease and the project's own are dropped before the call.
    pub(crate) fn git_confirm_rename_worktree(
        &mut self,
        project_id: &str,
        name: &str,
        rename_branch: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let scope = match self.git_scope_for_project(project_id) {
            Ok(scope) if !scope.is_remote() && scope.worktree.is_some() => scope,
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
        self.git_toast(
            GitToastLevel::Info,
            "Renaming worktree",
            ToastOptions::persistent(&toast_id).with_description(scope.title.clone()),
            cx,
        );
        if let Some(worktree) = &scope.worktree {
            self.git_forget_leases(&worktree.parent_project_id, false);
        }
        self.git_forget_leases(&scope.project_id(), true);
        let params = json!({
            "name": name,
            "projectId": scope.project_id(),
            "renameBranch": rename_branch,
        });
        let title = scope.title.clone();
        cx.spawn(async move |this, cx| {
            let result = calls::call(None, "/api/renameWorktreeProject", params, None).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(answer) => {
                    let renamed = answer
                        .pointer("/project/name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or(title);
                    this.git_toast(
                        GitToastLevel::Success,
                        "Worktree renamed",
                        ToastOptions::keyed(&toast_id).with_description(renamed),
                        cx,
                    );
                }
                Err(error) => this.git_toast(
                    GitToastLevel::Error,
                    "Could not rename worktree",
                    ToastOptions::keyed(&toast_id)
                        .with_description(worktree_rename_user_visible_error(&error.message)),
                    cx,
                ),
            });
        })
        .detach();
    }
}
