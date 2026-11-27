//! Delete Worktree preview. States (`GHOSTEX_NATIVE_MODAL_DEMO_STATE`):
//! default (uncommitted changes, both branch options enabled), `clean` (no
//! local changes, so no Commit button), `disabled` (neither branch can be
//! deleted, with the daemon's remote reason), `nolocal` (only the remote
//! branch is offered), `long` (a status summary past the 220px cap).
use super::delete_worktree_modal::*;
use gpui::{App, AppContext as _};
use std::rc::Rc;

const STATUS_SUMMARY: &str = " M packages/core-ui/styles/modals.css\n M apps/desktop/src/app/window/native_modal_kit.rs\n?? packages/core-ui/modal-gallery/\n?? apps/desktop/assets/modals/delete-worktree/";

pub(super) fn open(demo: &super::DemoEnv, cx: &mut App) {
    let state = demo.state.as_str();
    let has_changes = !matches!(state, "clean");
    let status_summary = if state == "long" {
        (0..40)
            .map(|index| format!(" M packages/core-ui/styles/file-{index}.css"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        STATUS_SUMMARY.to_string()
    };
    let draft = DeleteWorktreeDraft {
        branch: Some("feat/modal-gallery".to_string()),
        can_delete_local_branch: !matches!(state, "disabled" | "nolocal"),
        group_id: "demo-worktree-group".to_string(),
        has_changes,
        local_branch_name: Some("feat/modal-gallery".to_string()),
        project_id: "demo-worktree-project".to_string(),
        remote_branch_disabled_reason: (state == "disabled")
            .then(|| "The remote branch has commits that are not in this worktree.".to_string()),
        remote_branch_exists: state != "disabled",
        remote_branch_name: Some("feat/modal-gallery".to_string()),
        remote_name: Some("origin".to_string()),
        status_summary,
        worktree_name: "Ghostex-modal-gallery".to_string(),
        repos: Vec::new(),
    };
    let host: DeleteWorktreeModalHost = Rc::new(|command, cx: &mut App| {
        match command {
            DeleteWorktreeModalCommand::Cancel => eprintln!("cancel"),
            DeleteWorktreeModalCommand::Commit { group_id } => eprintln!("commit: {group_id}"),
            DeleteWorktreeModalCommand::Delete {
                project_id,
                delete_local_branch,
                delete_remote_branch,
            } => eprintln!(
                "delete: {project_id} local={delete_local_branch} remote={delete_remote_branch}"
            ),
        }
        cx.quit();
    });
    let config = DeleteWorktreeModalConfig {
        draft,
        palette: demo.palette,
    };
    let _ = super::open_modal_window(
        DELETE_WORKTREE_MODAL_WIDTH,
        DELETE_WORKTREE_MODAL_INITIAL_HEIGHT,
        move |window, cx| cx.new(|cx| GpuiDeleteWorktreeModalWindow::new(config, host, window, cx)),
        cx,
    );
}
