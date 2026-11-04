//! Open and sidebar bridge plumbing for the native Delete Worktree dialog.
//! SEE-ALSO: apps/desktop/src/app/window/delete_worktree_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/gx_store/git/modal_commands.rs (`forward_gpui_worktree_modal_command_to_sidebar`).
use crate::app::window::*;
use crate::*;

fn draft_string(draft: &serde_json::Value, key: &str) -> Option<String> {
    draft
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn draft_bool(draft: &serde_json::Value, key: &str) -> bool {
    draft.get(key).and_then(serde_json::Value::as_bool) == Some(true)
}

/// The `worktreeDeleteDraft` of the open message, or `None` when the payload
/// is not the shape the React host required (it threw on a missing draft).
pub(crate) fn parse_gpui_delete_worktree_draft(
    message: &serde_json::Value,
) -> Option<DeleteWorktreeDraft> {
    let draft = message.get("worktreeDeleteDraft")?;
    if !draft.is_object() {
        return None;
    }
    Some(DeleteWorktreeDraft {
        branch: draft_string(draft, "branch"),
        can_delete_local_branch: draft_bool(draft, "canDeleteLocalBranch"),
        group_id: draft_string(draft, "groupId")?,
        has_changes: draft_bool(draft, "hasChanges"),
        local_branch_name: draft_string(draft, "localBranchName"),
        project_id: draft_string(draft, "projectId")?,
        remote_branch_disabled_reason: draft_string(draft, "remoteBranchDisabledReason"),
        remote_branch_exists: draft_bool(draft, "remoteBranchExists"),
        remote_branch_name: draft_string(draft, "remoteBranchName"),
        remote_name: draft_string(draft, "remoteName"),
        status_summary: draft_string(draft, "statusSummary").unwrap_or_default(),
        worktree_name: draft_string(draft, "worktreeName")?,
    })
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `deleteWorktree` modal kind. Refuses payloads without a usable draft.
    pub(crate) fn open_gpui_delete_worktree_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = parse_gpui_delete_worktree_draft(message) else {
            return;
        };
        let config = DeleteWorktreeModalConfig {
            draft,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_delete_worktree_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::DeleteWorktree,
            DELETE_WORKTREE_MODAL_WIDTH,
            DELETE_WORKTREE_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiDeleteWorktreeModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Forwards the same `commitWorktreeBeforeDelete` / `confirmDeleteWorktree`
    /// bridge commands the React page posted, then drops the window handle.
    fn handle_gpui_delete_worktree_modal_command(
        &mut self,
        command: DeleteWorktreeModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            DeleteWorktreeModalCommand::Cancel => {}
            DeleteWorktreeModalCommand::Commit { group_id } => {
                let mut message = serde_json::Map::new();
                message.insert("groupId".to_string(), serde_json::json!(group_id));
                self.forward_gpui_worktree_modal_command_to_sidebar(
                    "commitWorktreeBeforeDelete",
                    &message,
                    cx,
                );
            }
            DeleteWorktreeModalCommand::Delete {
                project_id,
                delete_local_branch,
                delete_remote_branch,
            } => {
                let mut message = serde_json::Map::new();
                message.insert("projectId".to_string(), serde_json::json!(project_id));
                message.insert(
                    "deleteLocalBranch".to_string(),
                    serde_json::json!(delete_local_branch),
                );
                message.insert(
                    "deleteRemoteBranch".to_string(),
                    serde_json::json!(delete_remote_branch),
                );
                self.forward_gpui_worktree_modal_command_to_sidebar(
                    "confirmDeleteWorktree",
                    &message,
                    cx,
                );
            }
        }
        self.release_native_app_modal_window(GpuiAppModalKind::DeleteWorktree, cx);
    }
}
