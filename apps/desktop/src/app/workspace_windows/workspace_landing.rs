//! Keeps a window on its own workspace: a window that opens on a workspace, or that finds itself
//! showing a project outside the one it shows (a slot restored at launch, a new window, a project
//! moved away), lands the way a workspace switch does (`land_on_window_workspace`).
//!
//! CDXC:Workspaces 2026-10-10 DECISION:
//! User, on a ShortPoint window opened from the Personal one that showed a Ghostex session: "it's showing me a session that's not part of any of the projects that are in the work workspace". A window opened on a workspace starts the way switching to it does (the session that window last had open there, else nothing selected), it never takes the source window's session or project from another workspace, and a window never shows a session, view or pane whose project is outside its workspace. File > New Window on the same workspace still opens on the source window's project (CDXC:AppWindows 2026-10-01).
//! SEE-ALSO: apps/desktop/src/app/workspace_windows/window_workspace.rs (the switch's restore), apps/desktop/src/app/workspace_windows/open.rs (what a new window starts on), apps/desktop/src/app/gx_store/sidebar_list.rs (where this runs).

use crate::*;

/// What this window still owes its workspace.
#[derive(Default)]
pub(crate) struct WindowWorkspaceLanding {
    /// A window opened with no project to start on: it lands once its list is built.
    pub(crate) pending: bool,
    /// The workspace and the outside project the last landing was for, so a landing that could not
    /// move the focus (an empty workspace) is not run again on every update.
    last_outside: Option<(String, String)>,
}

impl GhostexGpuiApp {
    /// Whether this window shows `workspace_id` (`None` = the default workspace).
    pub(crate) fn shows_same_workspace_as(&self, workspace_id: Option<&str>) -> bool {
        let own = self.gx_store_window_workspace_id();
        match self.gx_store_workspaces_state() {
            Some(state) => state.resolve(workspace_id) == state.resolve(own.as_deref()),
            None => workspace_id == own.as_deref(),
        }
    }

    /// The project this window has active or on screen when it is outside the workspace the
    /// window shows.
    fn project_outside_window_workspace(&self) -> Option<String> {
        let active = self
            .gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .filter(|project| project.machine.is_local())
            .map(|project| project.project_id.clone());
        [active, self.agents_workspace_project_id.clone()]
            .into_iter()
            .flatten()
            .find(|project_id| self.gx_store_project_outside_window_workspace(project_id))
    }

    /// Runs after every sidebar list update: lands the window on its workspace when it opened on
    /// one with nothing to show yet, or when the project it has active or on screen is outside it.
    pub(crate) fn keep_window_inside_workspace(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.gx_store_sidebar_list_ready() {
            return;
        }
        let Some(workspace_id) = self.gx_store_resolved_window_workspace_id() else {
            return;
        };
        let outside = self.project_outside_window_workspace();
        let landing = &mut self.window_workspace_landing;
        let pending = std::mem::take(&mut landing.pending);
        match outside {
            Some(project_id) => {
                let key = (workspace_id.clone(), project_id);
                if landing.last_outside.as_ref() == Some(&key) {
                    return;
                }
                landing.last_outside = Some(key);
            }
            None => {
                landing.last_outside = None;
                if !pending {
                    return;
                }
            }
        }
        // Not inside the list update that found it: the landing focuses rows, which updates the
        // list again. A focus that moved in the meantime (a workspace switch's own landing, a
        // focus routed here from another workspace, whose project may reach the work area only
        // once its attach returns) has chosen what to show, so the landing gives way to it; a new
        // window's first landing does not.
        let stamp = self.gx_store.core.focus().local_stamp;
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                let focus_moved = this.gx_store.core.focus().local_stamp != stamp;
                let still_owed =
                    pending || (!focus_moved && this.project_outside_window_workspace().is_some());
                if still_owed
                    && this.gx_store_resolved_window_workspace_id().as_deref()
                        == Some(workspace_id.as_str())
                {
                    this.land_on_window_workspace(&workspace_id, cx);
                }
            });
        })
        .detach();
    }
}
