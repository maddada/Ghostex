//! Open and save for the native Goal & Instructions and Memory editors of an orchestrator.
//! SEE-ALSO: apps/desktop/src/app/window/coordinator_details_modal.rs (the window), apps/desktop/src/app/gx_store/coordinator_actions.rs (the menu rows that open it).
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// Opens the editor for an orchestrator read from `/api/readCoordinator` (`coordinator`).
    pub(crate) fn open_gpui_coordinator_details_modal(
        &mut self,
        mode: CoordinatorDetailsMode,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        coordinator: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            coordinator
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let project_id = text("projectId");
        let session_id = text("sessionId");
        let title = text("title");
        let config = CoordinatorDetailsModalConfig {
            mode,
            title: if title.trim().is_empty() {
                "this coordinator".to_string()
            } else {
                title
            },
            goal: text("goal"),
            instructions: text("instructions"),
            notes: coordinator
                .get("memory")
                .and_then(Value::as_array)
                .map(|notes| {
                    notes
                        .iter()
                        .filter_map(|note| note.get("text").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            palette: self.gpui_native_dialog_palette(),
        };
        // The editor stays open while the save runs and closes only when gxserver accepted it
        // (CDXC:Coordinators 2026-10-14 in window/coordinator_details_modal.rs).
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            let ids = json!({ "projectId": project_id, "sessionId": session_id });
            let (params, reply) = match command {
                CoordinatorDetailsModalCommand::SaveGoal {
                    goal,
                    instructions,
                    reply,
                } => {
                    let mut params = ids;
                    params["goal"] = json!(goal);
                    params["instructions"] = json!(instructions);
                    (params, reply)
                }
                CoordinatorDetailsModalCommand::SaveMemory { notes, reply } => {
                    let mut params = ids;
                    params["memory"] = json!(notes);
                    (params, reply)
                }
                CoordinatorDetailsModalCommand::Cancel => {
                    app.release_native_app_modal_window(GpuiAppModalKind::CoordinatorDetails, cx);
                    return;
                }
            };
            let remote = remote.clone();
            cx.spawn(async move |this, cx| {
                let saved = gx_rpc(remote, "/api/updateCoordinator", params)
                    .await
                    .map(|_| ())
                    .map_err(|error| error.message);
                let _ = this.update(cx, |app, cx| {
                    let succeeded = saved.is_ok();
                    let failure = saved.as_ref().err().cloned();
                    let shown = reply.finish(saved, cx);
                    if succeeded && shown {
                        app.release_native_app_modal_window(
                            GpuiAppModalKind::CoordinatorDetails,
                            cx,
                        );
                    } else if let (false, Some(message)) = (shown, failure) {
                        // Cancelled while the save ran: nothing is left to show the refusal in.
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Coordinator not updated",
                            &message,
                            cx,
                        );
                    }
                });
            })
            .detach();
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CoordinatorDetails,
            COORDINATOR_DETAILS_MODAL_WIDTH,
            COORDINATOR_DETAILS_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiCoordinatorDetailsModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }
}
