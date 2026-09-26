//! The Add Worktree dialog's list of base branches and existing checkouts (`requestProjectWorktrees`).
//!
//! CDXC:Worktrees 2026-06-24-18:21:
//! Worktree create and open run through gxserver's id-scoped endpoints: the daemon derives target
//! paths, branch refs and Open Existing selections from project ids plus daemon-issued keys, and a
//! renderer path is never mutation authority.
//!
//! CDXC:Worktrees 2026-09-25 WHY:
//! This computer's list used to be assembled client-side from `runWorktreeAction list` and
//! `runGitAction listBranches`, trusted by path, while a remote machine used
//! `/api/listProjectWorktrees` and trusted by key. Both use the endpoint and its keys now, so Open
//! Existing names a daemon key on every machine.

use ghostex_gx_core::git_menu::{
    normalize_existing_worktree_options, normalize_worktree_base_branches, worktree_list_error_text,
};
use ghostex_gx_core::{MachineId, ProjectKey};
use serde_json::{Value, json};

use super::calls;
use super::scope::{GitScope, ScopeMiss};
use crate::GhostexGpuiApp;

const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The existing checkouts the last list offered, which is all Open Existing may pick.
#[derive(Clone, Debug, Default)]
pub(crate) struct TrustedWorktreeList {
    pub(crate) machine: Option<String>,
    pub(crate) source_project_id: String,
    pub(crate) worktree_keys: Vec<String>,
}

/// The project the dialog was opened for.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorktreeDialogScope {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: Option<String>,
    pub(crate) remote_machine_id: Option<String>,
}

impl WorktreeDialogScope {
    pub(crate) fn read(command: &Value) -> Self {
        let text = |key: &str, max: usize| {
            command
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.chars().count() <= max)
                .map(str::to_string)
        };
        Self {
            project_id: text("projectId", 300),
            project_path: text("projectPath", 1024),
            remote_machine_id: text("remoteMachineId", 300),
        }
    }
}

impl GhostexGpuiApp {
    /// The dialog's source project (`resolveWorktreeDialogSourceProject`,
    /// `resolveRemotePresentationProjectScope`).
    ///
    /// CDXC:Worktrees 2026-09-22 WHY:
    /// The dialog names the project whose header was clicked. A scope that resolves to nothing is
    /// an error the dialog shows, never a different project; the active project stands in only for
    /// a command that names no project at all.
    pub(crate) fn git_worktree_dialog_source(
        &self,
        dialog: &WorktreeDialogScope,
    ) -> Result<GitScope, String> {
        if let Some(machine_id) = &dialog.remote_machine_id {
            let project_id = dialog.project_id.clone().unwrap_or_default();
            let key = ProjectKey::parse_workspace_project_id(&project_id)
                .filter(|key| key.machine.remote_id() == Some(machine_id.as_str()))
                .unwrap_or_else(|| ProjectKey::remote(machine_id.clone(), project_id));
            return self
                .git_scope_for_key(key)
                .map_err(|_| "Reconnect the remote machine before loading worktrees.".to_string());
        }
        let projects = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)
            .map(|loaded| loaded.projects().to_vec())
            .unwrap_or_default();
        let by_id = dialog
            .project_id
            .as_deref()
            .and_then(|id| projects.iter().find(|project| project.project_id == id));
        let by_path = || {
            let wanted = dialog.project_path.as_deref()?.trim().trim_end_matches('/');
            projects.iter().find(|project| {
                project
                    .path
                    .as_deref()
                    .is_some_and(|path| path.trim().trim_end_matches('/') == wanted)
            })
        };
        let found = by_id
            .or_else(by_path)
            .map(|project| project.project_id.clone());
        let project_id = match (found, &dialog.project_id, &dialog.project_path) {
            (Some(project_id), _, _) => project_id,
            (None, None, None) => self
                .git_active_scoped_project_id()
                .filter(|id| !id.starts_with("remote:"))
                .ok_or_else(|| "No active gxserver project is available.".to_string())?,
            (None, id, path) => {
                return Err(format!(
                    "Project {} is not registered with gxserver. Close this dialog and open it again from the project header.",
                    id.as_deref().or(path.as_deref()).unwrap_or_default()
                ));
            }
        };
        self.git_scope_for_project(&project_id)
            .map_err(|miss| match miss {
                ScopeMiss::RemoteUnavailable | ScopeMiss::Unknown => {
                    "No active gxserver project is available.".to_string()
                }
            })
    }

    /// Answers the dialog's list request.
    pub(crate) fn git_request_project_worktrees(
        &mut self,
        request_id: String,
        dialog: WorktreeDialogScope,
        cx: &mut gpui::Context<Self>,
    ) {
        let source = match self.git_worktree_dialog_source(&dialog) {
            Ok(source) => source,
            Err(error) => {
                self.gx_store.git.trusted_worktrees = None;
                return self.git_post_worktrees_result(&request_id, Err(error), cx);
            }
        };
        let remote = source.remote.clone();
        let is_remote = source.is_remote();
        let folder = source.path.clone();
        cx.spawn(async move |this, cx| {
            let result = calls::call(
                remote,
                "/api/listProjectWorktrees",
                json!({ "projectId": source.project_id() }),
                Some(LIST_TIMEOUT),
            )
            .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(answer) => {
                    let worktrees = normalize_existing_worktree_options(answer.get("worktrees"));
                    this.gx_store.git.trusted_worktrees = Some(TrustedWorktreeList {
                        machine: source.machine_id().map(str::to_string),
                        source_project_id: answer
                            .get("sourceProjectId")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .unwrap_or_else(|| source.project_id()),
                        worktree_keys: worktrees
                            .iter()
                            .filter_map(|worktree| worktree["worktreeKey"].as_str())
                            .map(str::to_string)
                            .collect(),
                    });
                    this.git_post_worktrees_result(
                        &request_id,
                        Ok((
                            normalize_worktree_base_branches(answer.get("branches")),
                            Value::Array(worktrees),
                        )),
                        cx,
                    );
                }
                Err(error) => {
                    this.gx_store.git.trusted_worktrees = None;
                    let text = match is_remote {
                        true => "Could not load remote gxserver worktrees.".to_string(),
                        false => worktree_list_error_text(&error.message, folder.as_deref()),
                    };
                    this.git_post_worktrees_result(&request_id, Err(text), cx);
                }
            });
        })
        .detach();
    }

    /// `postProjectWorktreesResult`: the answer goes to the open Add Worktree dialog.
    fn git_post_worktrees_result(
        &mut self,
        request_id: &str,
        result: Result<(Value, Value), String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let message = match result {
            Ok((branches, worktrees)) => json!({
                "branches": branches,
                "ok": true,
                "requestId": request_id,
                "type": "projectWorktreesResult",
                "worktrees": worktrees,
            }),
            Err(error) => json!({
                "error": error,
                "ok": false,
                "requestId": request_id,
                "type": "projectWorktreesResult",
            }),
        };
        self.dispatch_open_gpui_app_modal_message(message, cx);
    }

    /// Whether `key` is one the latest list offered for `source`.
    pub(crate) fn git_trusted_worktree_key(&self, source: &GitScope, key: &str) -> bool {
        self.gx_store
            .git
            .trusted_worktrees
            .as_ref()
            .is_some_and(|trusted| {
                trusted.machine.as_deref() == source.machine_id()
                    && trusted.source_project_id == source.project_id()
                    && trusted.worktree_keys.iter().any(|known| known == key)
            })
    }
}
