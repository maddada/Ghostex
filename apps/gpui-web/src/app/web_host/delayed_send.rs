//! Delayed Send and Close After Done from the page's Delayed Send dialog. The desktop keeps a local session's timer beside its terminal pane and asks gxserver to persist it; the page has no panes, so a local session's Delayed Send goes to gxserver the way a remote one does (`/api/scheduleDelayedSend`, which owns the timer), and its row shows the deadline from the presentation.
use ghostex_gx_core::{MachineId, SessionKey};
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;

/// The Delayed Send dialog's longest delay (`GPUI_DELAYED_SEND_MAX_DELAY_MS`).
const DELAYED_SEND_MAX_DELAY_MS: u64 = 2_147_483_647;

impl GhostexGpuiApp {
    /// The dialog's Schedule. A remote id is the store's (`session_edits.rs`); a local one is scheduled here with the parameters the desktop sends for an Agents session.
    pub(crate) fn handle_gpui_schedule_delayed_send_command(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let message = Value::Object(command.clone());
        if self.gx_store_run_session_edit(&message, cx) {
            return;
        }
        let Some(session) = command
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(SessionKey::parse_sidebar_session_id)
            .filter(|session| session.machine == MachineId::Local)
        else {
            return;
        };
        let delay_ms = command.get("delayMs").and_then(Value::as_u64);
        let flag = |key: &str| command.get(key).and_then(Value::as_bool) == Some(true);
        let send_when_agent_stops = flag("sendWhenAgentStops");
        let send_when_all = flag("sendWhenAllProjectSessionsStop");
        let watched = command
            .get("sendWhenSpecificAgentFinishes")
            .filter(|value| !value.is_null())
            .cloned();
        let triggers = usize::from(delay_ms.is_some())
            + usize::from(send_when_agent_stops)
            + usize::from(send_when_all)
            + usize::from(watched.is_some());
        if triggers != 1
            || delay_ms.is_some_and(|delay| delay == 0 || delay > DELAYED_SEND_MAX_DELAY_MS)
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Delayed Send unavailable",
                "Choose exactly one valid Delayed Send trigger.",
                cx,
            );
            return;
        }
        let description = if watched.is_some() {
            "Presses Enter after the selected agent has finished working for 10 seconds."
                .to_string()
        } else if send_when_agent_stops {
            "Presses Enter after the agent has finished working for 10 seconds.".to_string()
        } else if send_when_all {
            "Presses Enter after all agents in the project have finished working for 10 seconds."
                .to_string()
        } else {
            format!(
                "Presses Enter in {}.",
                format_delay(delay_ms.unwrap_or_default())
            )
        };
        let params = json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "delayMs": delay_ms,
            "sendWhenAgentStops": send_when_agent_stops,
            "sendWhenSpecificAgentFinishes": watched,
            "sendWhenAllProjectSessionsStop": send_when_all,
        });
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/scheduleDelayedSend", params).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Delayed Send scheduled",
                    &description,
                    cx,
                ),
                Err(_) => this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Delayed Send unavailable",
                    "gxserver could not persist this Delayed Send.",
                    cx,
                ),
            });
        })
        .detach();
    }

    /// The dialog's Cancel Timer: the store's `cancelDelayedSend`, which calls the session's own daemon.
    pub(crate) fn handle_gpui_cancel_delayed_send_command(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_run_session_edit(&Value::Object(command.clone()), cx);
    }

    /// The dialog's Close After Done switch: gxserver owns the timer (`/api/toggleCloseAfterDone`).
    pub(crate) fn handle_gpui_toggle_close_after_done_command(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = command
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(SessionKey::parse_sidebar_session_id)
            .filter(|session| session.machine == MachineId::Local)
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result =
                crate::app::gx_store::terminal_lifecycle::session_calls::toggle_close_after_done(
                    None,
                    session.project_id,
                    session.session_id,
                    None,
                )
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_app_modal_toast(
                        "error",
                        "Close After Done unavailable",
                        &error.message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// The dialog's "when another agent finishes" list: the running agent and terminal sessions of this computer, from the store's own presentation (the desktop reads the same list with `/api/readPresentationSnapshot`).
    pub(crate) fn request_delayed_send_agents(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = message.get("requestId").and_then(Value::as_str) else {
            return;
        };
        let target = message
            .get("sessionId")
            .and_then(Value::as_str)
            .and_then(SessionKey::parse_sidebar_session_id);
        let result = match self.gx_store.core.presentation().loaded(&MachineId::Local) {
            Some(loaded) => {
                use ghostex_gx_core::protocol::{LifecycleState, SessionKind, SessionSurface};
                let mut sessions: Vec<Value> = loaded
                    .server_sessions()
                    .filter(|session| {
                        session.lifecycle_state == LifecycleState::Running
                            && matches!(session.kind, SessionKind::Terminal | SessionKind::Agent)
                            && session.surface != SessionSurface::Commands
                    })
                    .map(|session| {
                        let title = session.display_title.as_deref().unwrap_or(&session.title);
                        json!({
                            "projectId": session.project_id,
                            "sessionId": session.session_id,
                            "label": format!("{title} ({})", session.session_id),
                        })
                    })
                    .collect();
                sessions.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
                let active = target.as_ref().and_then(|target| {
                    loaded
                        .server_session(&target.project_id, &target.session_id)
                        .and_then(|session| session.send_when_specific_agent_finishes.clone())
                });
                json!({ "sessions": sessions, "active": active })
            }
            None => json!({ "sessions": [], "error": "Could not load agent sessions." }),
        };
        self.receive_native_app_modal_message(
            &json!({ "type": "delayedSendAgents", "requestId": request_id, "result": result }),
            cx,
        );
    }
}

/// `formatGpuiDelayedSendDelay`: `1h 2m 3s`, zero parts left out, at least one second.
fn format_delay(delay_ms: u64) -> String {
    let total_seconds = delay_ms.div_ceil(1_000).max(1);
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    [
        (hours > 0).then(|| format!("{hours}h")),
        (minutes > 0).then(|| format!("{minutes}m")),
        (seconds > 0).then(|| format!("{seconds}s")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}
