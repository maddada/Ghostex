//! The group facts every menu of a group and of its rows reads.

use crate::sidebar_view::view::ProjectContextView;

use super::clipboard::DetailsGroup;

/// One drawn group, as its menus see it.
pub struct MenuGroup<'a> {
    pub group_id: &'a str,
    /// The id every per-project UI state is keyed by.
    pub storage_id: &'a str,
    pub title: &'a str,
    /// The group belongs to a remote machine's section.
    pub is_remote: bool,
    pub remote_machine_name: Option<&'a str>,
    /// A remote machine whose stream is down keeps drawing its last rows, faded.
    pub is_stale: bool,
    /// The group can take a session out into a user-made group of its own.
    pub can_create_session_group: bool,
    /// The host can put the group into focus mode. The gxserver projection never sets it, so on
    /// this client it is always false and the Focus item never appears.
    pub can_focus_mode: bool,
    /// The host has `postWorkspaceTerminalFocus`, which Split Right needs. The web app does not.
    pub workspace_focus_bridge: bool,
    /// A project group's project; absent for the Chats group and for a user-made session group.
    pub project: Option<&'a ProjectContextView>,
    /// A project the sidebar may take out of the workspace.
    pub can_remove_project: bool,
    /// The project's git origin, when the daemon has probed one.
    pub git_remote_origin_url: Option<&'a str>,
    /// The machine's daemon id, which Copy Details joins into the session's global ref.
    pub server_id: Option<String>,
}

impl<'a> MenuGroup<'a> {
    /// The project facts Copy Details quotes.
    pub(crate) fn details(&self) -> DetailsGroup<'_> {
        let worktree = self.project.and_then(|project| project.worktree.as_ref());
        DetailsGroup {
            title: self.title,
            project_path: self.project.map(|project| project.path.as_str()),
            worktree_name: worktree.map(|worktree| worktree.name.as_str()),
            worktree_branch: worktree.map(|worktree| worktree.branch.as_str()),
            remote_machine_name: self.remote_machine_name,
            server_id: self.server_id.as_deref(),
        }
    }
}
