//! The page's stand-in for the desktop's `app/workspace_windows/session_routing.rs`: a page is one
//! window, so a session or project of another workspace opens here, with the page switched to
//! that workspace.

use gpui::Context;

use crate::GhostexGpuiApp;

/// Where a project's session or group opens (the desktop's `ProjectWindowRoute`). A page has no
/// other window, so it never routes to one.
#[allow(dead_code)]
pub(crate) enum ProjectWindowRoute {
    Here,
    Switched,
    Other(gpui::WeakEntity<GhostexGpuiApp>),
}

impl GhostexGpuiApp {
    pub(crate) fn route_local_project_to_its_window(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) -> ProjectWindowRoute {
        let Some(workspace_id) = self.gx_store_local_project_workspace_id(project_id) else {
            return ProjectWindowRoute::Here;
        };
        if self.gx_store_resolved_window_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return ProjectWindowRoute::Here;
        }
        self.gx_store_set_window_workspace(Some(workspace_id), cx);
        ProjectWindowRoute::Switched
    }

    pub(crate) fn settle_routed_window_workspace(&mut self, _cx: &mut Context<Self>) {}
}
