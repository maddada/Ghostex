use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn dispatch_gpui_remote_project_directory_browse_result(
        &mut self,
        request_id: String,
        ok: bool,
        result: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "requestId": request_id,
                "result": result,
                "type": "remoteProjectDirectoryBrowseResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_remote_project_add_result(
        &mut self,
        request_id: String,
        ok: bool,
        project_path: Option<String>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "projectPath": project_path,
                "requestId": request_id,
                "type": "remoteProjectAddResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_preview_result(
        &mut self,
        request_id: String,
        ok: bool,
        preview: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "preview": preview,
                "requestId": request_id,
                "type": "repositoryClonePreviewResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_result(
        &mut self,
        request_id: String,
        ok: bool,
        project_path: Option<String>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "projectPath": project_path,
                "requestId": request_id,
                "type": "repositoryCloneResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        toast_id: Option<String>,
        persistent: bool,
        cancel_request_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let action = cancel_request_id.map(|request_id| {
            serde_json::json!({
                "label": "Cancel",
                "sidebarMessage": {
                    "requestId": request_id,
                    "type": "cancelRepositoryClone",
                },
            })
        });
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "action": action,
                "description": description,
                "level": level,
                "persistent": persistent,
                "title": title,
                "toastId": toast_id,
                "type": "toast",
            }),
            cx,
        );
    }

}

