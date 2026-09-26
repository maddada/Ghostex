//! The fixed native bridge for a project's path actions (`receive_sidebar_native_project_path_action_payload` on the desktop). What a page can do is done here: Copy Path copies the project folder gxserver reports, and Open Pull Request opens the pull request gxserver reads in a new browser tab. Finder, an IDE, a terminal app and every remote-machine action need the operating system or a tunnel, so they answer with a toast.
use ghostex_gx_core::{MachineId, ProjectKey};
use gpui::ClipboardItem;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::helpers::gpui_copy_to_clipboard;

impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_native_project_path_action_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        let action = message
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let project_id = message
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match action {
            "copyWorkspaceProjectPath" => {
                let path = ProjectKey::parse_workspace_project_id(&project_id)
                    .filter(|key| key.machine == MachineId::Local)
                    .and_then(|key| self.gx_store.core.presentation().project(&key).cloned())
                    .and_then(|project| project.path)
                    .filter(|path| !path.trim().is_empty());
                match path {
                    Some(path) => gpui_copy_to_clipboard(ClipboardItem::new_string(path), cx),
                    None => self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Native action unavailable",
                        "Ghostex could not resolve this project's saved folder.",
                        cx,
                    ),
                }
            }
            "openExistingPullRequestInBrowser" => {
                cx.spawn(async move |this, cx| {
                    let url = gx_rpc(
                        None,
                        "/api/readProjectGitState",
                        json!({ "projectId": project_id, "gitHubOnly": true }),
                    )
                    .await
                    .ok()
                    .and_then(|result| result["gitHub"]["pr"]["url"].as_str().map(str::to_string))
                    .filter(|url| url.starts_with("https://"));
                    let _ = this.update(cx, |this, cx| match url {
                        Some(url) => {
                            if let Some(window) = web_sys::window() {
                                let _ = window.open_with_url_and_target(&url, "_blank");
                            }
                        }
                        None => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Native action unavailable",
                            "No pull request was found for this project.",
                            cx,
                        ),
                    });
                })
                .detach();
            }
            _ => self.dispatch_gpui_app_modal_toast(
                "info",
                "Not available in the browser",
                "Opening Finder, an editor or a terminal app needs the Ghostex app.",
                cx,
            ),
        }
    }
}
