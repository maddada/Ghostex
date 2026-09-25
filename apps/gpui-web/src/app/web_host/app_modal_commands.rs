//! The `sidebarCommand` messages the app's own windows send (the desktop's `handle_gpui_app_modal_sidebar_command`, a switch of about 130 arms in `delayed_send.rs`). The page answers the ones its windows can send, which is what Quick Access posts: focus a session or a project, create a terminal or a browser tab, and the Projects and Saved Prompts reads, through `gx_rpc`. What needs the operating system, a CEF page or the desktop's panes answers with a toast.
use std::collections::HashSet;

use ghostex_gx_core::SessionKey;
use gpui::Window;
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;

#[allow(dead_code)]
mod projects_lifted {
    use std::collections::HashMap;

    use crate::app::helpers::gpui_remote_sidebar_project_id_allowed;
    include!(concat!(env!("OUT_DIR"), "/projects_lifted.rs"));
}

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_app_modal_sidebar_command(
        &mut self,
        message: Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(command) = message.get("message").and_then(Value::as_object).cloned() else {
            return;
        };
        let Some(command_type) = command
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let text = |key: &str| command.get(key).and_then(Value::as_str).map(str::to_string);
        match command_type.as_str() {
            "focusSession" => {
                let Some(session_id) = text("sessionId") else {
                    return;
                };
                let session = self
                    .gx_store
                    .session_key_for_row(&session_id)
                    .or_else(|| SessionKey::parse_sidebar_session_id(&session_id));
                if let Some(session) = session {
                    self.close_gpui_quick_access_window(cx);
                    let preferred = match self.show_terminal {
                        true => crate::app::model::GpuiPreferredAgentInterface::Terminal,
                        false => crate::app::model::GpuiPreferredAgentInterface::Chat,
                    };
                    let terminal = self.web_surface_is_terminal(&session, preferred);
                    self.web_open_session_in_work_area(session, terminal, cx);
                }
            }
            "focusRecentProject" => {
                if let Some(project_id) = text("projectId") {
                    self.close_gpui_quick_access_window(cx);
                    self.dispatch_gpui_menu_bar_project_activation(&project_id, cx);
                }
            }
            // Quick Access's Commands rows, as the desktop runs them (`quick_access/commands.rs`).
            command_type
                if crate::app::quick_access::commands::QUICK_ACCESS_COMMAND_ROW_TYPES
                    .contains(&command_type) =>
            {
                self.run_quick_access_command_row(command_type, &command, window, cx);
            }
            "openBrowserChat" => {
                self.gx_store_run_app_modal_create_command(&command_type, cx);
            }
            "requestRecentProjects" if command.get("machineId").is_none_or(Value::is_null) => {
                self.web_answer_recent_projects(cx);
            }
            "requestStashedPrompts" => self.web_answer_stashed_prompts(&command, cx),
            "requestPreviousSessions" => self.web_answer_previous_sessions(&command, cx),
            "setSessionNote"
            | "renameSession"
            | "scheduleDelayedSend"
            | "postponeDelayedSend"
            | "cancelDelayedSend"
            | "updateCustomSessionTags"
            | "confirmAgentHookLaunch"
            | "removeProject" => {
                self.dispatch_gpui_sidebar_host_message(Value::Object(command), cx);
            }
            "toggleCloseAfterDone" => {
                self.handle_gpui_toggle_close_after_done_command(&command, cx)
            }
            // Transcript sizes are an optional column; the page leaves them out.
            "requestSessionTranscriptSizes" => {}
            _ => self.dispatch_gpui_workspace_action_toast(
                "info",
                "Not available in the browser",
                "Use this from the Ghostex app.",
                cx,
            ),
        }
    }

    /// Quick Access's Projects: the open projects of this computer's presentation and its parked ones (`/api/listRecentProjects`), in the desktop's `recentProjectsResult` shape.
    fn web_answer_recent_projects(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let recent = gx_rpc(None, "/api/listRecentProjects", json!({}))
                .await
                .ok();
            let snapshot = gx_rpc(None, "/api/readPresentationSnapshot", json!({}))
                .await
                .ok()
                .and_then(|result| result.get("snapshot").cloned());
            let mut recent_projects: Vec<Value> = recent
                .as_ref()
                .and_then(|result| result.get("recentProjects"))
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(projects_lifted::gpui_recent_project_from_gxserver)
                        .collect()
                })
                .unwrap_or_default();
            for project in &mut recent_projects {
                project["isOpen"] = Value::Bool(false);
            }
            let recent_ids: HashSet<String> = recent_projects
                .iter()
                .filter_map(|project| project.get("projectId").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            let mut projects = snapshot
                .as_ref()
                .map(|snapshot| {
                    projects_lifted::gpui_open_projects_from_presentation_snapshot(
                        snapshot, None, None,
                    )
                })
                .unwrap_or_default();
            projects.retain(|project| {
                project
                    .get("projectId")
                    .and_then(Value::as_str)
                    .is_none_or(|project_id| !recent_ids.contains(project_id))
            });
            projects.extend(recent_projects);
            let payload = json!({ "recentProjects": projects, "type": "recentProjectsResult" });
            let _ = this.update(cx, |this, cx| this.quick_access_receive(payload, cx));
        })
        .detach();
    }

    /// Quick Access's previous sessions: this computer's `/api/listPreviousSessions` page, in the desktop's `previousSessionsResult` shape (a page reads no remote machine's history).
    fn web_answer_previous_sessions(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let request = projects_lifted::gpui_previous_sessions_request_from_command(command);
        let params = projects_lifted::gpui_previous_sessions_list_params(&request);
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/listPreviousSessions", params).await.ok();
            let mut items: Vec<Value> = result
                .as_ref()
                .and_then(|result| result.get("results"))
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(
                            projects_lifted::gpui_gxserver_search_result_to_previous_session_item,
                        )
                        .collect()
                })
                .unwrap_or_default();
            projects_lifted::gpui_sort_previous_session_items_by_closed_time(&mut items);
            let cursor = result
                .as_ref()
                .and_then(|result| result.get("cursor"))
                .and_then(Value::as_str)
                .map(str::to_string);
            let mut payload = projects_lifted::gpui_previous_sessions_result_payload(
                &request.request_id,
                request.query.as_deref(),
                cursor.as_deref(),
                items,
            );
            payload["projects"] = result
                .as_ref()
                .and_then(|result| result.get("projects").cloned())
                .unwrap_or_else(|| json!([]));
            let _ = this.update(cx, |this, cx| this.quick_access_receive(payload, cx));
        })
        .detach();
    }

    /// Quick Access's Saved Prompts (`/api/listStashedPrompts`), in the desktop's `stashedPromptsResult` shape.
    fn web_answer_stashed_prompts(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = command
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut params = Map::new();
        if let Some(project_id) = command.get("projectId").and_then(Value::as_str) {
            params.insert("projectId".into(), json!(project_id));
        }
        params.insert(
            "includeRecovery".into(),
            json!(
                command
                    .get("includeRecovery")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            ),
        );
        params.insert(
            "includeDelivered".into(),
            json!(
                command
                    .get("includeDelivered")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            ),
        );
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/listStashedPrompts", Value::Object(params))
                .await
                .ok();
            let list = |key: &str| {
                result
                    .as_ref()
                    .and_then(|result| result.get(key).cloned())
                    .filter(Value::is_array)
                    .unwrap_or_else(|| Value::Array(Vec::new()))
            };
            let payload = json!({
                "prompts": list("prompts"),
                "requestId": request_id,
                "tags": list("tags"),
                "type": "stashedPromptsResult",
                "deliveredDrafts": result.as_ref().and_then(|result| result.get("deliveredDrafts")),
                "recoveryDrafts": result.as_ref().and_then(|result| result.get("recoveryDrafts")),
                "drafts": result.as_ref().and_then(|result| result.get("drafts")),
            });
            let _ = this.update(cx, |this, cx| this.quick_access_receive(payload, cx));
        })
        .detach();
    }
}
