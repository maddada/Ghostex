//! Move to project, from the menu of a chat in No project.
//!
//! CDXC:Projects 2026-11-27 DECISION:
//! User (Q5262 B, Q5284 A): the chat moves at once, with no confirm, and opens in its new
//! project. gxserver does the move (`/api/moveSessionToProject`, server/src/no_project/
//! move_chat.rs): a Claude or Codex chat resumes in the project's folder under a new ref, any
//! other agent starts there from a handover of the chat (the user's messages and the agent's
//! final replies). The row the menu came from goes away with the gxserver update; this opens the
//! chat it became.
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/session.rs (`move_to_project_item`).

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use ghostex_gx_core::SessionKey;
use serde_json::{Value, json};
use std::time::Duration;

impl GhostexGpuiApp {
    /// `{type: "moveSessionToProject", sessionId: <sidebar session id>, targetProjectId}`.
    pub(crate) fn move_native_sidebar_session_to_project(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = message["sessionId"]
            .as_str()
            .and_then(SessionKey::parse_sidebar_session_id)
        else {
            return;
        };
        let Some(target_project_id) = message["targetProjectId"].as_str().map(str::to_string)
        else {
            return;
        };
        // A connected computer's chat moves on that computer's gxserver.
        let remote = match key.machine.remote_id() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't move the chat",
                        "That computer isn't connected.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let machine = key.machine.clone();
        let params = json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "targetProjectId": target_project_id,
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(remote, "/api/moveSessionToProject", params).await;
            let moved = match result {
                Ok(answer) => {
                    let body = answer.get("result").unwrap_or(&answer);
                    let session = &body["session"];
                    match (session["projectId"].as_str(), session["sessionId"].as_str()) {
                        (Some(project_id), Some(session_id)) => Ok(SessionKey {
                            machine: machine.clone(),
                            project_id: project_id.to_string(),
                            session_id: session_id.to_string(),
                        }),
                        _ => Ok(SessionKey {
                            machine: machine.clone(),
                            project_id: target_project_id.clone(),
                            session_id: key.session_id.clone(),
                        }),
                    }
                }
                Err(error) => Err(error.message),
            };
            match moved {
                Ok(moved) => {
                    // The moved chat's row arrives with gxserver's project update; give it a
                    // moment so the focus finds it.
                    cx.background_executor()
                        .timer(Duration::from_millis(400))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_native_sidebar_command(
                            json!({
                                "type": "focusSession",
                                "sessionId": moved.to_sidebar_session_id(),
                            }),
                            cx,
                        );
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't move the chat",
                            &error,
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }
}
