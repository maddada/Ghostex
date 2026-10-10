//! Which window opens a session or a project of this computer: the window that shows its
//! workspace. Every focus of a session or a group ends in the store's one focus
//! (gx_store/focus_perform.rs), and a create in `gx_store_focus_created_session`; both ask here
//! first, so a session of another workspace opens in the window that shows that workspace, or
//! this window switches to it when none does, and never opens in a window whose list does not
//! hold it.
//!
//! CDXC:Workspaces 2026-10-10 DECISION:
//! User: "a click that silently snaps back is a bug. Route EVERY way of opening a session to the window showing that session's workspace (or switch this window to it)", at the shared session-activation entry point rather than per caller, so the guard in workspace_landing.rs only catches truly stray cases.
//! SEE-ALSO: apps/desktop/src/app/gx_store/focus_perform.rs, apps/desktop/src/app/gx_store/create/focus_created.rs, apps/desktop/src/app/workspace_windows/workspace_landing.rs.

use ghostex_gx_core::SessionKey;

use crate::*;

/// Where a project's session or group opens.
pub(crate) enum ProjectWindowRoute {
    /// This window shows the project's workspace.
    Here,
    /// This window switched to the project's workspace; the caller shows what was asked for and
    /// then `settle_routed_window_workspace`.
    Switched,
    /// Another window shows the project's workspace and was brought forward; the caller repeats
    /// the open there.
    Other(gpui::WeakEntity<GhostexGpuiApp>),
}

impl GhostexGpuiApp {
    /// The workspace of this computer's project `project_id` when it is not the one this window
    /// shows; `None` for a project in this window's workspace, in every workspace, or unknown.
    fn foreign_workspace_of_local_project(&self, project_id: &str) -> Option<String> {
        let workspace_id = self.gx_store_local_project_workspace_id(project_id)?;
        (self.gx_store_resolved_window_workspace_id().as_deref() != Some(workspace_id.as_str()))
            .then_some(workspace_id)
    }

    /// Routes an open of this computer's project `project_id` to the window that shows its
    /// workspace.
    pub(crate) fn route_local_project_to_its_window(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> ProjectWindowRoute {
        let Some(workspace_id) = self.foreign_workspace_of_local_project(project_id) else {
            return ProjectWindowRoute::Here;
        };
        if let Some(other) = self.other_window_showing_workspace(&workspace_id, cx) {
            return ProjectWindowRoute::Other(other);
        }
        self.enter_window_workspace(&workspace_id, cx);
        ProjectWindowRoute::Switched
    }

    /// Another window that shows the workspace of this computer's project `project_id` when this
    /// one does not, brought forward: for an open that must run whole in that window (a project
    /// activation, a create). `None` leaves the open here, where its focus routes itself.
    pub(crate) fn other_window_for_local_project(
        &self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::WeakEntity<GhostexGpuiApp>> {
        let workspace_id = self.foreign_workspace_of_local_project(project_id)?;
        self.other_window_showing_workspace(&workspace_id, cx)
    }

    /// After a switch-routed open has shown what it was asked for: the views of projects outside
    /// the new workspace go to sleep, as after a workspace switch.
    pub(crate) fn settle_routed_window_workspace(&mut self, cx: &mut gpui::Context<Self>) {
        self.sleep_views_outside_window_workspace(cx);
        cx.notify();
    }

    /// Runs `f` in the window that takes the session behind `row_id` (a sidebar row id): another
    /// window when it shows the session's workspace and this one does not, on the next turn, after
    /// the focus routed there; else this window, now. For what a caller does beside the focus
    /// (Find's reveal, Quick Access's floating sessions).
    pub(crate) fn run_in_session_window(
        &mut self,
        row_id: &str,
        cx: &mut gpui::Context<Self>,
        f: impl FnOnce(&mut Self, &mut gpui::Context<Self>) + 'static,
    ) {
        let other = SessionKey::parse_sidebar_session_id(row_id.trim())
            .filter(|session| session.machine.is_local())
            .and_then(|session| self.foreign_workspace_of_local_project(&session.project_id))
            .and_then(|workspace_id| self.find_other_window_showing_workspace(&workspace_id, cx));
        match other {
            Some((_, app)) => cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| f(app, cx));
            }),
            None => f(self, cx),
        }
    }
}
