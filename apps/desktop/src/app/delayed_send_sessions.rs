use std::time::Duration;

use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_postpone_delayed_send_command(
        &mut self,
        command: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(gpui_remote_attach_session_reference_from_project_id)
            .is_some()
        {
            self.dispatch_gpui_sidebar_host_message(Value::Object(command.clone()), cx);
            return;
        }
        let Some(session_id) = self.gpui_agents_delayed_send_session_id_from_command(command)
        else {
            return;
        };
        let Some(key) = self.local_workspace_key_for_shell_session(session_id) else {
            return;
        };
        let params = json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "delayMs": command.get("delayMs"),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/postponeDelayedSend",
                        &params,
                        Duration::from_secs(5),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {
                    this.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                    this.dispatch_gpui_app_modal_toast("info", "Delayed Send postponed", "", cx);
                }
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Delayed Send could not be postponed",
                    &error.to_string(),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// CDXC:DelayedSend 2026-09-14 WHY:
    /// The modal's settings hydrate has no session groups. Read the receiving session's daemon so the awake picker also works from native hotkeys and remote panes.
    pub(crate) fn request_delayed_send_agents(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = message.get("sessionId").and_then(Value::as_str) else {
            return;
        };
        let Some(request_id) = message
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let remote = gpui_remote_attach_session_reference_from_project_id(session_id);
        let remote_target = remote.as_ref().and_then(|reference| {
            self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        });
        let local_key = if remote.is_none() {
            self.gpui_titlebar_resource_shell_session_id(session_id)
                .and_then(|session_id| self.local_workspace_key_for_shell_session(session_id))
        } else {
            None
        };
        let target_key = remote
            .as_ref()
            .map(|reference| (reference.project_id.clone(), reference.session_id.clone()))
            .or_else(|| local_key.map(|key| (key.project_id, key.session_id)));
        // This machine's sessions from the live store (gx_store/live_snapshot.rs).
        let live_snapshot = remote
            .is_none()
            .then(|| self.gx_store_local_snapshot_value())
            .flatten();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background.spawn(async move {
                if target_key.is_none() {
                    return Err("The receiving session is unavailable.".to_string());
                }
                let snapshot = if remote.is_some() {
                    let target = remote_target.ok_or_else(|| "The remote computer is disconnected.".to_string())?;
                    gpui_remote_gxserver_rpc_result(&target, "/api/readPresentationSnapshot", &json!({}), Duration::from_secs(10))?
                        .get("snapshot").cloned().ok_or_else(|| "Could not read agent sessions.".to_string())?
                } else {
                    gpui_live_or_read_gxserver_presentation_snapshot(live_snapshot)?
                };
                let sessions = snapshot.get("sessions").and_then(Value::as_array)
                    .ok_or_else(|| "Could not read agent sessions.".to_string())?;
                let mut options: Vec<Value> = sessions.iter().filter_map(|session| {
                    if session.get("lifecycleState").and_then(Value::as_str) != Some("running")
                        || !matches!(session.get("kind").and_then(Value::as_str), Some("terminal" | "agent"))
                        || session.get("surface").and_then(Value::as_str) == Some("commands")
                    {
                        return None;
                    }
                    let project_id = session.get("projectId")?.as_str()?;
                    let session_id = session.get("sessionId")?.as_str()?;
                    let title = session.get("displayTitle").or_else(|| session.get("title"))
                        .and_then(Value::as_str).unwrap_or(session_id);
                    Some(json!({"projectId": project_id, "sessionId": session_id, "label": format!("{title} ({session_id})")}))
                }).collect();
                options.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
                let active = sessions.iter().find(|session| {
                    target_key.as_ref().is_some_and(|(project_id, session_id)| {
                        session["projectId"].as_str() == Some(project_id.as_str())
                            && session["sessionId"].as_str() == Some(session_id.as_str())
                    })
                }).and_then(|session| session.get("sendWhenSpecificAgentFinishes")).cloned();
                Ok((options, active))
            }).await;
            let _ = this.update(cx, |this, cx| {
                let response = match result {
                    Ok((sessions, active)) => json!({"sessions": sessions, "active": active}),
                    Err(_) => json!({"sessions": [], "error": "Could not load agent sessions."}),
                };
                this.dispatch_open_gpui_app_modal_message(json!({
                    "type": "delayedSendAgents", "requestId": request_id, "result": response,
                }), cx);
            });
        }).detach();
    }
}
