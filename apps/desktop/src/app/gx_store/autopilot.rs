//! A project's Advanced > Autopilot row (gx-core `sidebar_menu/project.rs`): puts the project and
//! its worktrees in Autopilot or takes them out through `/api/autopilot` `setProject`
//! (server/src/server/autopilot_http.rs). gxserver republishes the project, which re-ticks the row.

use serde_json::{Value, json};

use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn gx_store_run_autopilot(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let message = &command["message"];
        if command["type"] != "command" || message["type"] != "setProjectAutopilot" {
            return false;
        }
        let (Some(project_id), Some(on)) = (
            message["projectId"].as_str().map(str::to_string),
            message["on"].as_bool(),
        ) else {
            return true;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/autopilot",
                json!({ "op": "setProject", "projectId": project_id, "on": on, "limit": 0 }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't change Autopilot for this project",
                        &message,
                        cx,
                    );
                });
            }
        })
        .detach();
        true
    }
}
