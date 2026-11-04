//! Open, bridge-command, and awake-agent plumbing for the native Session Automations dialog.
//! SEE-ALSO: apps/desktop/src/app/window/delayed_send_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/delayed_send/ (the bridge command handlers this forwards to), apps/desktop/src/app/delayed_send_sessions.rs (the awake-agent read).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

fn delayed_send_agent_reference(
    value: Option<&serde_json::Value>,
) -> Option<DelayedSendAgentReference> {
    let value = value?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some(DelayedSendAgentReference {
        project_id: text("projectId")?,
        session_id: text("sessionId")?,
    })
}

/// `getRemainingMs`: `None` when the open message carries no deadline, else the
/// time left (zero when the deadline passed or cannot be parsed).
fn delayed_send_deadline_remaining_ms(deadline_at: Option<&str>) -> Option<u64> {
    let deadline_at = deadline_at.filter(|deadline_at| !deadline_at.is_empty())?;
    let remaining = chrono::DateTime::parse_from_rfc3339(deadline_at)
        .ok()
        .map(|deadline| deadline.with_timezone(&chrono::Utc) - chrono::Utc::now())
        .and_then(|remaining| remaining.num_milliseconds().try_into().ok())
        .unwrap_or(0);
    Some(remaining)
}

impl GhostexGpuiApp {
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `delayedSend` modal kind, validating it the way the React host does.
    pub(crate) fn open_gpui_delayed_send_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        let flag = |key: &str| message.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        let Some(session_id) = text("sessionId").filter(|session_id| !session_id.is_empty()) else {
            return;
        };
        // `closeAfterDoneActive ?? sessionsById[id].closeAfterDone ?? commandSessionIndicators[id].closeAfterDone ?? false`.
        let close_after_done_active = message
            .get("closeAfterDoneActive")
            .and_then(serde_json::Value::as_bool)
            .or_else(|| {
                let state = message.get("latestSidebarStateMessage")?;
                state
                    .get("sessionsById")
                    .and_then(|sessions| sessions.get(&session_id))
                    .and_then(|session| session.get("closeAfterDone"))
                    .and_then(serde_json::Value::as_bool)
                    .or_else(|| {
                        state
                            .get("hud")
                            .and_then(|hud| hud.get("commandSessionIndicators"))
                            .and_then(serde_json::Value::as_array)?
                            .iter()
                            .find(|indicator| {
                                indicator
                                    .get("sessionId")
                                    .and_then(serde_json::Value::as_str)
                                    == Some(session_id.as_str())
                            })
                            .and_then(|indicator| indicator.get("closeAfterDone"))
                            .and_then(serde_json::Value::as_bool)
                    })
            })
            .unwrap_or(false);
        let config = DelayedSendModalConfig {
            agent_icon_path: text("agentIcon")
                .and_then(|icon| workspace_tab_agent_icon_path(&icon))
                .map(str::to_string),
            close_after_done_active,
            delayed_send_deadline_remaining_ms: delayed_send_deadline_remaining_ms(
                message
                    .get("delayedSendDeadlineAt")
                    .and_then(serde_json::Value::as_str),
            ),
            delayed_send_remaining_label: text("delayedSendRemainingLabel"),
            send_when_all_project_sessions_stop_active: flag(
                "sendWhenAllProjectSessionsStopActive",
            ),
            send_when_agent_stops_active: flag("sendWhenAgentStopsActive"),
            // Absent (a command pane tab, an older sender) offers the switch.
            supports_close_after_done: message
                .get("supportsCloseAfterDone")
                .and_then(serde_json::Value::as_bool)
                != Some(false),
            send_when_specific_agent_finishes: delayed_send_agent_reference(
                message.get("sendWhenSpecificAgentFinishes"),
            ),
            supports_send_when_agent_stops: flag("supportsSendWhenAgentStops"),
            supports_send_when_all_project_sessions_stop: flag(
                "supportsSendWhenAllProjectSessionsStop",
            ),
            title: text("title"),
            status_accent: None,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_delayed_send_modal_command(&session_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::DelayedSend,
            DELAYED_SEND_MODAL_WIDTH,
            DELAYED_SEND_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiDelayedSendModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Forwards the dialog's commands as the bridge messages the React page
    /// posted: `toggleCloseAfterDone`, `scheduleDelayedSend`,
    /// `cancelDelayedSend` and `requestDelayedSendAgents`.
    fn handle_gpui_delayed_send_modal_command(
        &mut self,
        session_id: &str,
        command: DelayedSendModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::DelayedSend;
        let base = |message_type: &str| {
            let mut message = serde_json::Map::new();
            message.insert("sessionId".to_string(), serde_json::json!(session_id));
            message.insert("type".to_string(), serde_json::json!(message_type));
            message
        };
        match command {
            DelayedSendModalCommand::RequestAgents { request_id } => {
                let mut message = base("requestDelayedSendAgents");
                message.insert("requestId".to_string(), serde_json::json!(request_id));
                self.request_delayed_send_agents(&serde_json::Value::Object(message), cx);
            }
            DelayedSendModalCommand::Cancel => {
                self.release_native_app_modal_window(kind, cx);
            }
            DelayedSendModalCommand::Save {
                toggle_close_after_done,
                send,
            } => {
                if toggle_close_after_done {
                    let message = base("toggleCloseAfterDone");
                    self.handle_gpui_toggle_close_after_done_command(&message, cx);
                }
                match send {
                    DelayedSendModalSend::Schedule {
                        delay_ms,
                        send_when_agent_stops,
                        send_when_all_project_sessions_stop,
                        send_when_specific_agent_finishes,
                    } => {
                        let mut message = base("scheduleDelayedSend");
                        if let Some(delay_ms) = delay_ms {
                            message.insert("delayMs".to_string(), serde_json::json!(delay_ms));
                        }
                        if let Some(reference) = send_when_specific_agent_finishes {
                            message.insert(
                                "sendWhenSpecificAgentFinishes".to_string(),
                                serde_json::json!({
                                    "projectId": reference.project_id,
                                    "sessionId": reference.session_id,
                                }),
                            );
                        }
                        message.insert(
                            "sendWhenAllProjectSessionsStop".to_string(),
                            serde_json::json!(send_when_all_project_sessions_stop),
                        );
                        message.insert(
                            "sendWhenAgentStops".to_string(),
                            serde_json::json!(send_when_agent_stops),
                        );
                        self.handle_gpui_schedule_delayed_send_command(&message, cx);
                    }
                    DelayedSendModalSend::CancelTimer => {
                        let message = base("cancelDelayedSend");
                        self.handle_gpui_cancel_delayed_send_command(&message, cx);
                    }
                    DelayedSendModalSend::Keep => {}
                }
                self.release_native_app_modal_window(kind, cx);
            }
        }
    }

    /// Delivers a `delayedSendAgents` reply to the open native dialog. Returns
    /// false for any other message or when no native Session Automations
    /// dialog is open, so the React host path can have it.
    pub(crate) fn receive_gpui_delayed_send_modal_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if message.get("type").and_then(serde_json::Value::as_str) != Some("delayedSendAgents") {
            return false;
        }
        let Some(request_id) = message
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return false;
        };
        let result = message.get("result");
        let sessions: Vec<DelayedSendAgentOption> = result
            .and_then(|result| result.get("sessions"))
            .and_then(serde_json::Value::as_array)
            .map(|sessions| {
                sessions
                    .iter()
                    .filter_map(|session| {
                        Some(DelayedSendAgentOption {
                            reference: delayed_send_agent_reference(Some(session))?,
                            label: session
                                .get("label")
                                .and_then(serde_json::Value::as_str)?
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = delayed_send_agent_reference(result.and_then(|result| result.get("active")));
        let error = result
            .and_then(|result| result.get("error"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        self.update_native_app_modal(
            GpuiAppModalKind::DelayedSend,
            cx,
            |modal: &mut GpuiDelayedSendModalWindow, window, cx| {
                modal.receive_agents(&request_id, sessions, active, error, window, cx);
            },
        )
        .is_some()
    }
}
