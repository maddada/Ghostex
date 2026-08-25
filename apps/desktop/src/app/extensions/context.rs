use crate::*;

use super::{GpuiExtensionPlacement, GpuiExtensionSurfaceContext};

impl GhostexGpuiApp {
    pub(crate) fn extension_surface_context(
        &self,
        placement: GpuiExtensionPlacement,
    ) -> GpuiExtensionSurfaceContext {
        GpuiExtensionSurfaceContext {
            placement,
            start_session: self.active_extension_session_details(),
        }
    }

    pub(crate) fn extension_context_payload(
        &self,
        surface: &GpuiExtensionSurfaceContext,
    ) -> serde_json::Value {
        let project_id = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.as_str());
        let project_metadata = project_id.and_then(|id| self.extension_projects.get(id));
        let snapshot = self.latest_sidebar_project_snapshot.as_ref();
        serde_json::json!({
            "activeSession": self.active_extension_session_details(),
            "startSession": surface.start_session,
            "project": {
                "name": project_metadata
                    .map(|project| project.name.as_str())
                    .filter(|name| !name.is_empty())
                    .or_else(|| snapshot.map(|snapshot| snapshot.display_name.as_str()))
                    .unwrap_or(""),
                "path": project_metadata
                    .and_then(|project| project.path.as_deref())
                    .or_else(|| snapshot.and_then(|snapshot| snapshot.in_memory_project_path.as_deref()).and_then(|path| path.to_str())),
            },
            "worktree": {
                "isWorktree": project_metadata.is_some_and(|project| project.is_worktree),
                "branch": project_metadata.and_then(|project| project.worktree_branch.as_deref()),
            },
            "placement": surface.placement.as_str(),
        })
    }

    pub(crate) fn active_extension_session_details(&self) -> Option<serde_json::Value> {
        self.sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .as_deref()
            .and_then(|id| self.extension_session_details.get(id))
            .cloned()
    }
}
