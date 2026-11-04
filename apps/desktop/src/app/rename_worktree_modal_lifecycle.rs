//! Open and sidebar bridge plumbing for the native Rename Worktree dialog.
//! SEE-ALSO: apps/desktop/src/app/window/rename_worktree_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/gx_store/git/modal_commands.rs (`forward_gpui_worktree_modal_command_to_sidebar`).
use crate::app::window::*;
use crate::*;

fn draft_string(draft: &serde_json::Value, key: &str) -> Option<String> {
    draft
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn draft_strings(draft: &serde_json::Value, key: &str) -> Vec<String> {
    draft
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The `worktreeRenameDraft` of the open message, or `None` when the payload
/// is not the shape the React host required (it threw on a missing draft).
pub(crate) fn parse_gpui_rename_worktree_draft(
    message: &serde_json::Value,
) -> Option<RenameWorktreeDraft> {
    let draft = message.get("worktreeRenameDraft")?;
    if !draft.is_object() {
        return None;
    }
    Some(RenameWorktreeDraft {
        blocking_reason: draft_string(draft, "blockingReason"),
        branch: draft_string(draft, "branch"),
        current_name: draft_string(draft, "currentName")?,
        current_path: draft_string(draft, "currentPath")?,
        parent_folder_name: draft_string(draft, "parentFolderName")?,
        parent_project_path: draft_string(draft, "parentProjectPath")?,
        project_id: draft_string(draft, "projectId")?,
        registered_project_paths: draft_strings(draft, "registeredProjectPaths"),
        rename_branch_default: draft
            .get("renameBranchDefault")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        warnings: draft_strings(draft, "warnings"),
        worktree_name: draft_string(draft, "worktreeName")?,
    })
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `renameWorktree` modal kind. Refuses payloads without a usable draft.
    pub(crate) fn open_gpui_rename_worktree_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(draft) = parse_gpui_rename_worktree_draft(message) else {
            return;
        };
        let config = RenameWorktreeModalConfig {
            draft,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_rename_worktree_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::RenameWorktree,
            RENAME_WORKTREE_MODAL_WIDTH,
            RENAME_WORKTREE_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiRenameWorktreeModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Forwards the same `confirmRenameWorktree` bridge command the React page
    /// posted, then drops the window handle.
    fn handle_gpui_rename_worktree_modal_command(
        &mut self,
        command: RenameWorktreeModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        if let RenameWorktreeModalCommand::Rename {
            project_id,
            name,
            rename_branch,
        } = command
        {
            let mut message = serde_json::Map::new();
            message.insert("projectId".to_string(), serde_json::json!(project_id));
            message.insert("name".to_string(), serde_json::json!(name));
            message.insert("renameBranch".to_string(), serde_json::json!(rename_branch));
            self.forward_gpui_worktree_modal_command_to_sidebar(
                "confirmRenameWorktree",
                &message,
                cx,
            );
        }
        self.release_native_app_modal_window(GpuiAppModalKind::RenameWorktree, cx);
    }
}
