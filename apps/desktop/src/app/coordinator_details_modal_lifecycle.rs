//! Open and save for the native Goal & Instructions, Memory and Playbook overrides editors of an
//! orchestrator.
//! SEE-ALSO: apps/desktop/src/app/window/coordinator_details_modal.rs (the window), apps/desktop/src/app/gx_store/coordinator_actions.rs (the menu rows that open it).
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// Opens the editor for an orchestrator read from `/api/readCoordinator` (`coordinator`); the
    /// Overrides mode also takes the `/api/readCoordinatorPlaybook` answer (`playbook`) and the
    /// guide that coordinator reads (`guide`, from `/api/readCoordinatorGuide`) for View playbook,
    /// whose page it opens on when `on_playbook` (the menus' View playbook rows).
    pub(crate) fn open_gpui_coordinator_details_modal(
        &mut self,
        mode: CoordinatorDetailsMode,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        coordinator: &Value,
        playbook: Option<&Value>,
        guide: Option<Result<String, String>>,
        on_playbook: bool,
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
            overrides: playbook
                .and_then(|view| view["overrides"]["text"].as_str())
                .unwrap_or_default()
                .trim()
                .to_string(),
            old_copies_note: playbook
                .and_then(|view| old_copies_note_elsewhere(&old_copy_places(view))),
            guide,
            on_playbook,
            palette: self.gpui_native_dialog_palette(),
        };
        // The editor stays open while the save runs and closes only when gxserver accepted it
        // (CDXC:Coordinators 2026-10-14 in window/coordinator_details_modal.rs).
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            let ids = json!({ "projectId": project_id, "sessionId": session_id });
            let (endpoint, params, reply) = match command {
                CoordinatorDetailsModalCommand::SaveGoal {
                    goal,
                    instructions,
                    reply,
                } => {
                    let mut params = ids;
                    params["goal"] = json!(goal);
                    params["instructions"] = json!(instructions);
                    ("/api/updateCoordinator", params, reply)
                }
                CoordinatorDetailsModalCommand::SaveMemory { notes, reply } => {
                    let mut params = ids;
                    params["memory"] = json!(notes);
                    ("/api/updateCoordinator", params, reply)
                }
                CoordinatorDetailsModalCommand::SaveOverrides { overrides, reply } => (
                    "/api/setCoordinatorPlaybook",
                    json!({ "overrides": overrides }),
                    reply,
                ),
                CoordinatorDetailsModalCommand::Cancel => {
                    app.release_native_app_modal_window(GpuiAppModalKind::CoordinatorDetails, cx);
                    return;
                }
            };
            let remote = remote.clone();
            cx.spawn(async move |this, cx| {
                let saved = gx_rpc(remote, endpoint, params)
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
