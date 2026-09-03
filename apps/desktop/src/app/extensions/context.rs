use crate::*;

use super::{GpuiExtensionPlacement, GpuiExtensionSurfaceContext};

impl GhostexGpuiApp {
    pub(crate) fn extension_launch_context_value(&self) -> serde_json::Value {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref();
        let project_id = snapshot
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.as_str());
        let project = project_id.and_then(|id| self.extension_projects.get(id));
        serde_json::json!({
            "sessionId": self.active_extension_session_details()
                .and_then(|details| details.get("sessionId").cloned())
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default(),
            "projectPath": project
                .and_then(|project| project.path.as_deref())
                .or_else(|| snapshot.and_then(|snapshot| snapshot.in_memory_project_path.as_deref()).and_then(|path| path.to_str()))
                .unwrap_or(""),
            "projectName": project
                .map(|project| project.name.as_str())
                .filter(|name| !name.is_empty())
                .or_else(|| snapshot.map(|snapshot| snapshot.display_name.as_str()))
                .unwrap_or(""),
            "worktree": project.is_some_and(|project| project.is_worktree),
            "worktreeBranch": project
                .and_then(|project| project.worktree_branch.as_deref())
                .unwrap_or(""),
        })
    }

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
        let active_session = if surface.placement == GpuiExtensionPlacement::ChatBar {
            surface.start_session.clone()
        } else {
            self.active_extension_session_details()
        };
        let project_id = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.as_str());
        let project_metadata = project_id.and_then(|id| self.extension_projects.get(id));
        let snapshot = self.latest_sidebar_project_snapshot.as_ref();
        serde_json::json!({
            "activeSession": active_session,
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

    pub(crate) fn broadcast_extension_context_changes(&mut self, cx: &mut gpui::Context<Self>) {
        let view_message = cef::sidebar_bridge_manifest::extension_bridge_context_changed_message(
            self.extension_context_payload(
                &self.extension_surface_context(GpuiExtensionPlacement::View),
            ),
        );
        let view_surfaces = self
            .project_workarea_runtime_cef_surfaces
            .iter()
            .filter_map(|(slot, owned)| {
                let ProjectWorkareaCefSurfaceSlotKey::Extension(id) = slot else {
                    return None;
                };
                if gpui_custom_view(*id).is_some() {
                    return None;
                }
                self.extensions_snapshot
                    .installed
                    .contains_key(id.as_str())
                    .then(|| owned.surface.clone())
            })
            .collect::<Vec<_>>();
        for surface in view_surfaces {
            surface.update(cx, |surface, _| {
                surface.dispatch_extension_bridge_message(&view_message);
            });
        }

        if let Some(panel) = self
            .titlebar_extension_popup
            .as_ref()
            .and_then(|state| state.panel.clone())
        {
            let popup_message =
                cef::sidebar_bridge_manifest::extension_bridge_context_changed_message(
                    self.extension_context_payload(
                        &self.extension_surface_context(GpuiExtensionPlacement::Popup),
                    ),
                );
            panel.update(cx, |panel, cx| {
                panel.dispatch_bridge_message(&popup_message, cx);
            });
        }

        if let Some(handle) = self.app_modal_window.clone() {
            let modal_message =
                cef::sidebar_bridge_manifest::extension_bridge_context_changed_message(
                    self.extension_context_payload(
                        &self.extension_surface_context(GpuiExtensionPlacement::Modal),
                    ),
                );
            let _ = handle.update(cx, |host, _window, cx| {
                if !matches!(host.current_modal, GpuiAppModalKind::Extension(_)) {
                    return;
                }
                if let Some(surface) = host.surface.clone() {
                    surface.update(cx, |surface, _| {
                        surface.dispatch_extension_bridge_message(&modal_message);
                    });
                }
            });
        }
        self.broadcast_chat_bar_extension_context_changes(cx);
    }
}
