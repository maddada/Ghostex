//! A remote project's Open Folder: the project in Ghostex's Files view, which browses a remote
//! project through its gxserver (`ghostex_gx_core::plan_remote_project_open` decides it).

use serde_json::json;

use crate::app::model::*;
use crate::app::window::toast::{
    GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
};
use crate::*;

impl GhostexGpuiApp {
    /// Shows remote project `project_id` (a workspace project id) in the Files view at its root.
    /// The active project switches to Files at once; another project is focused with Files as
    /// the view it comes back to, so the project switch opens it.
    pub(crate) fn open_remote_project_in_files_view(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project) = ghostex_gx_core::ProjectKey::parse_workspace_project_id(project_id)
            .filter(|project| !project.machine.is_local())
        else {
            return;
        };
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage) {
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: "gpui-remote-project-files-off".to_string(),
                    level: GpuiAppToastLevel::Info,
                    title: "The Files view is turned off".to_string(),
                    description: Some(
                        "Turn on Files in Settings > Extensions to browse remote projects."
                            .to_string(),
                    ),
                    copy_text: None,
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    action: Some(GpuiAppToastAction::OpenViewSettings),
                    epoch: 0,
                },
                cx,
            );
            return;
        }
        if self.active_sidebar_project_id().as_deref() == Some(project_id) {
            if self.activate_mode(TitlebarMode::Manage, cx) {
                self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
                cx.notify();
            }
            return;
        }
        let split_ratio = self.project_editor_shell.workarea_split_ratio;
        let state = self
            .project_view_states_by_project
            .entry(project_id.to_string())
            .or_insert_with(|| GpuiProjectViewState::closed(split_ratio));
        state.active_mode = TitlebarMode::Manage;
        state.last_view_mode = Some(TitlebarMode::Manage);
        state.active_view_awake = true;
        if !state.open_views.contains(&TitlebarMode::Manage) {
            state.open_views.push(TitlebarMode::Manage);
        }
        self.dispatch_native_sidebar_command(
            json!({ "type": "focusGroup", "groupId": project.to_sidebar_group_id() }),
            cx,
        );
    }
}
