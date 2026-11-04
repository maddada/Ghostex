//! An orchestrator's and a thread's rows in the session context menu: Goal & Instructions,
//! Memory, Mark Thread Done and Reopen Thread (`coordinatorSessionAction`), on this computer or a
//! remote one.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/session.rs (`coordinator_rows`), server/src/server/coordinator_resolve.rs (`/api/resolveCoordinatorThread`), apps/desktop/src/app/coordinator_details_modal_lifecycle.rs.

use ghostex_gx_core::{MachineId, SessionKey};
use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::window::CoordinatorDetailsMode;

impl GhostexGpuiApp {
    /// Runs `{ type: 'coordinatorSessionAction', sessionId, action }`; false for any other command.
    pub(crate) fn gx_store_run_coordinator_action(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("coordinatorSessionAction") {
            return false;
        }
        let Some(session) = command
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(SessionKey::parse_sidebar_session_id)
        else {
            return true;
        };
        let remote = match &session.machine {
            MachineId::Local => None,
            MachineId::Remote(machine_id) => {
                match self.gpui_remote_gxserver_request_target(machine_id) {
                    Some(target) => Some(target),
                    None => {
                        self.dispatch_gpui_app_modal_toast(
                            "warning",
                            "That computer is not connected",
                            "",
                            cx,
                        );
                        return true;
                    }
                }
            }
        };
        let ids = json!({ "projectId": session.project_id, "sessionId": session.session_id });
        match command.get("action").and_then(Value::as_str) {
            Some(action @ ("markDone" | "reopen")) => {
                let resolved = action == "markDone";
                let mut params = ids;
                params["resolved"] = json!(resolved);
                cx.spawn(async move |this, cx| {
                    let answer = gx_rpc(remote, "/api/resolveCoordinatorThread", params).await;
                    let _ = this.update(cx, |this, cx| match answer {
                        Ok(_) if !resolved => this.dispatch_gpui_app_modal_toast(
                            "info",
                            "Thread reopened",
                            "It is back under its coordinator, which hears from it again.",
                            cx,
                        ),
                        Ok(_) => {}
                        Err(error) => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            match (resolved, error.code.as_deref()) {
                                // gxserver marked it done; only closing its session failed.
                                (true, Some("sessionNotClosed")) => "Session not closed",
                                (true, _) => "Thread not marked done",
                                (false, _) => "Thread not reopened",
                            },
                            &error.message,
                            cx,
                        ),
                    });
                })
                .detach();
            }
            Some(action @ ("goal" | "memory")) => {
                let mode = if action == "goal" {
                    CoordinatorDetailsMode::Goal
                } else {
                    CoordinatorDetailsMode::Memory
                };
                cx.spawn(async move |this, cx| {
                    let answer = gx_rpc(remote.clone(), "/api/readCoordinator", ids).await;
                    let _ = this.update(cx, |this, cx| match answer {
                        Ok(read) => this.open_gpui_coordinator_details_modal(
                            mode,
                            remote,
                            &read["coordinator"],
                            cx,
                        ),
                        Err(error) => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Could not read the coordinator",
                            &error.message,
                            cx,
                        ),
                    });
                })
                .detach();
            }
            _ => {}
        }
        true
    }
}
