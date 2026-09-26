//! A session's rename, note and Delayed Send edits, from the sidebar menus, the Rename and Note
//! dialogs and the Delayed Send dialog: the gxserver calls the old runtime made
//! (`sessions-and-focus.ts` `renameSession`, `saveSessionNote`; `close-after-done.ts`
//! `scheduleRemoteDelayedSend`, `postponeDelayedSend`, `cancelDelayedSend`), with its parameters
//! and its toasts. A remote session's call goes down that machine's tunnel.
//!
//! CDXC:DelayedSend 2026-08-17:
//! Remote delayed sends are owned by the gxserver hosting the target session. The client submits only the canonical trigger and ids; the daemon stores, projects, and eventually fires the send even if this app disappears.
//!
//! CDXC:SessionNotes 2026-08-24:
//! Save (or, with an empty note, clear) a session's note. No optimistic patch: gxserver schedules a presentation delta after a successful save, and a daemon that predates notes or a session without a conversation refuses it, which is a toast rather than a silently lost note.
//!
//! CDXC:SessionTitles 2026-09-15 WHY:
//! Chat view can have no mounted terminal to receive a native rename, so a local rename uses gxserver's queued command submission just like a remote one.

use std::time::Duration;

use ghostex_gx_core::{
    MachineId, REMOTE_AWAITED_TIMEOUT_MS, REMOTE_FIRE_AND_FORGET_TIMEOUT_MS, SessionKey,
};
use serde_json::{Map, Value, json};

use crate::GhostexGpuiApp;
use crate::app::gx_store::gx_rpc;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// `GPUI_DELAYED_SEND_MAX_DELAY_MS`.
const DELAYED_SEND_MAX_DELAY_MS: u64 = 2_147_483_647;

/// How a call reaches its daemon.
enum Route {
    Local,
    Remote(String),
}

impl GhostexGpuiApp {
    /// A wrapped sidebar command (`{type:'command', message}`) for one of these edits.
    pub(crate) fn gx_store_run_session_edit_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        command
            .get("message")
            .is_some_and(|message| self.gx_store_run_session_edit(message, cx))
    }

    /// One of these edits, on the runtime's own message shape. Returns whether it was answered.
    pub(crate) fn gx_store_run_session_edit(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let text = |key: &str| message.get(key).and_then(Value::as_str);
        let Some(session_id) = text("sessionId") else {
            return false;
        };
        match text("type") {
            Some("renameSession") => {
                self.gx_store_rename_session(session_id, message, cx);
                true
            }
            Some("setSessionNote") => {
                let note = text("note").unwrap_or_default().to_string();
                self.gx_store_save_session_note(session_id, note, cx);
                true
            }
            Some("scheduleDelayedSend") => {
                self.gx_store_schedule_remote_delayed_send(session_id, message, cx)
            }
            Some("postponeDelayedSend") => {
                let delay_ms = message.get("delayMs").and_then(Value::as_u64).unwrap_or(0);
                self.gx_store_postpone_delayed_send(session_id, delay_ms, cx);
                true
            }
            Some("cancelDelayedSend") => {
                self.gx_store_cancel_delayed_send(session_id, cx);
                true
            }
            _ => false,
        }
    }

    fn gx_store_rename_session(
        &mut self,
        sidebar_session_id: &str,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            return;
        };
        let title = message
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut params = Map::new();
        if let Some(agent_id) = message
            .get("agentId")
            .and_then(Value::as_str)
            .filter(|agent| !agent.is_empty())
        {
            params.insert("agentName".into(), json!(agent_id));
        }
        let generate = message.get("shouldGenerateTitle").and_then(Value::as_bool) == Some(true);
        if generate && session.machine.is_local() {
            self.gx_store_generate_session_title(&session, message, &title, cx);
            return;
        }
        params.insert("projectId".into(), json!(session.project_id));
        params.insert("reason".into(), json!("gpui-sidebar"));
        params.insert("sessionId".into(), json!(session.session_id));
        params.insert("submitAgentRenameCommand".into(), json!(true));
        params.insert("title".into(), json!(title));
        params.insert("titleSource".into(), json!("user"));
        match session.machine {
            MachineId::Remote(machine_id) => {
                // CDXC:SessionTitles 2026-07-29: a blank direct rename would erase the remote title.
                if title.trim().is_empty() {
                    return;
                }
                // CDXC:RemoteMachines 2026-08-12: the remote gxserver owns that session's provider, so it submits the provider-specific slash command itself.
                let _ = self
                    .start_gpui_remote_sidebar_rpc(
                        &machine_id,
                        "/api/requestSessionRename",
                        Some(Value::Object(params)),
                        Duration::from_millis(REMOTE_FIRE_AND_FORGET_TIMEOUT_MS),
                        GpuiRemoteSidebarRpcMode::FireAndForget,
                        cx,
                    )
                    .detach();
            }
            MachineId::Local => {
                cx.spawn(async move |this, cx| {
                    let result =
                        gx_rpc(None, "/api/requestSessionRename", Value::Object(params)).await;
                    if let Err(error) = result {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_app_modal_toast(
                                "error",
                                "Could not rename session",
                                &error.message,
                                cx,
                            );
                        });
                    }
                })
                .detach();
            }
        }
    }

    /// CDXC:Sessions 2026-07-29:
    /// Generate Name reuses the first-message auto-title UX end to end: gxserver marks the session generating (the card shows the same "Generating title" chrome), summarizes the pasted text with the chosen generation agent, stages the agent rename command through zmx with the same delayed real Enter, and applies the generated title. The long pasted text must never reach `/api/requestSessionRename` as a literal title.
    fn gx_store_generate_session_title(
        &mut self,
        session: &SessionKey,
        message: &Value,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let hud = self.gx_store_sidebar_hud();
        let agent_id = message.get("agentId").and_then(Value::as_str);
        let plan = ghostex_gx_core::plan_generate_session_title(
            hud.as_deref(),
            agent_id,
            &session.project_id,
            &session.session_id,
            text,
        );
        let params = match plan {
            Ok(params) => params,
            Err(reason) => {
                self.dispatch_gpui_app_modal_toast(
                    "error",
                    "Could not generate session name",
                    reason,
                    cx,
                );
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            if let Err(error) = gx_rpc(None, "/api/generateSessionTitle", params).await {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_app_modal_toast(
                        "error",
                        "Could not generate session name",
                        &error.message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    fn gx_store_save_session_note(
        &mut self,
        sidebar_session_id: &str,
        note: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            return;
        };
        let route = route_of(&session);
        let params = json!({
            "note": note,
            "projectId": session.project_id,
            "sessionId": session.session_id,
        });
        let call = self.gx_store_session_edit_call(route, "/api/saveSessionAgentNote", params, cx);
        cx.spawn(async move |this, cx| {
            if call.await.is_err() {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not save the session note",
                        "gxserver refused the note. This session may not have an agent conversation yet.",
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// Schedules a REMOTE session's Delayed Send; a local one is scheduled by
    /// `handle_gpui_schedule_delayed_send_command`, so a local id is not answered here.
    fn gx_store_schedule_remote_delayed_send(
        &mut self,
        sidebar_session_id: &str,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id)
            .filter(|session| !session.machine.is_local())
        else {
            return false;
        };
        let is_terminal = self
            .gx_store
            .core
            .presentation()
            .loaded(&session.machine)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
            .is_some_and(|row| {
                matches!(
                    row.kind,
                    ghostex_gx_core::protocol::SessionKind::Terminal
                        | ghostex_gx_core::protocol::SessionKind::Agent
                )
            });
        if !is_terminal {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Delayed Send is only available for remote terminal sessions.",
                "",
                cx,
            );
            return true;
        }
        let specific = message
            .get("sendWhenSpecificAgentFinishes")
            .filter(|value| is_truthy(value));
        let when_all = message
            .get("sendWhenAllProjectSessionsStop")
            .is_some_and(is_truthy);
        let when_agent = message.get("sendWhenAgentStops").is_some_and(is_truthy);
        let mut params = Map::new();
        let description = if specific.is_some() {
            "Presses Enter after the selected agent has finished working for 10 seconds."
                .to_string()
        } else if when_all {
            "Presses Enter after all agents in the project have finished working for 10 seconds."
                .to_string()
        } else if when_agent {
            "Presses Enter after this agent has finished working for 10 seconds.".to_string()
        } else {
            let delay_ms = message
                .get("delayMs")
                .and_then(Value::as_u64)
                .filter(|delay| (1..=DELAYED_SEND_MAX_DELAY_MS).contains(delay));
            let Some(delay_ms) = delay_ms else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Choose a future send time within 24 days.",
                    "",
                    cx,
                );
                return true;
            };
            params.insert("delayMs".into(), json!(delay_ms));
            format!("Presses Enter in {}.", format_delay(delay_ms))
        };
        params.insert("projectId".into(), json!(session.project_id));
        if specific.is_none() && when_all {
            params.insert("sendWhenAllProjectSessionsStop".into(), json!(true));
        }
        if specific.is_none() && !when_all && when_agent {
            params.insert("sendWhenAgentStops".into(), json!(true));
        }
        if let Some(specific) = specific {
            params.insert("sendWhenSpecificAgentFinishes".into(), specific.clone());
        }
        params.insert("sessionId".into(), json!(session.session_id));
        let call = self.gx_store_session_edit_call(
            route_of(&session),
            "/api/scheduleDelayedSend",
            Value::Object(params),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Delayed Send scheduled",
                    &description,
                    cx,
                ),
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "error",
                    "Delayed Send unavailable",
                    &error,
                    cx,
                ),
            });
        })
        .detach();
        true
    }

    fn gx_store_postpone_delayed_send(
        &mut self,
        sidebar_session_id: &str,
        delay_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "error",
                "Delayed Send could not be postponed",
                "The selected agent session is unavailable.",
                cx,
            );
            return;
        };
        let params = json!({
            "projectId": session.project_id,
            "sessionId": session.session_id,
            "delayMs": delay_ms,
        });
        let call = self.gx_store_session_edit_call(
            route_of(&session),
            "/api/postponeDelayedSend",
            params,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Delayed Send postponed",
                    &format!(
                        "Added {} to the scheduled send time.",
                        format_delay(delay_ms)
                    ),
                    cx,
                ),
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "error",
                    "Delayed Send could not be postponed",
                    &error,
                    cx,
                ),
            });
        })
        .detach();
    }

    fn gx_store_cancel_delayed_send(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "error",
                "Delayed Send could not be canceled",
                "The selected agent session is unavailable.",
                cx,
            );
            return;
        };
        let params = json!({ "projectId": session.project_id, "sessionId": session.session_id });
        let call = self.gx_store_session_edit_call(
            route_of(&session),
            "/api/cancelDelayedSend",
            params,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => this.dispatch_gpui_app_modal_toast(
                    "info",
                    if result.get("changed").and_then(Value::as_bool) == Some(true) {
                        "Delayed Send canceled"
                    } else {
                        "No Delayed Send timer is active"
                    },
                    "",
                    cx,
                ),
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "error",
                    "Delayed Send could not be canceled",
                    &error,
                    cx,
                ),
            });
        })
        .detach();
    }

    /// One awaited call to the session's daemon: the local one through `gx_rpc`, a remote one
    /// through the tunnel's allowlisted request (the runtime's `requestRemoteGxserver`).
    fn gx_store_session_edit_call(
        &mut self,
        route: Route,
        path: &'static str,
        params: Value,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<Value, String>> {
        match route {
            Route::Local => cx.background_executor().spawn(async move {
                gx_rpc(None, path, params)
                    .await
                    .map_err(|error| error.message)
            }),
            Route::Remote(machine_id) => self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                path,
                Some(params),
                Duration::from_millis(REMOTE_AWAITED_TIMEOUT_MS),
                GpuiRemoteSidebarRpcMode::Awaited,
                cx,
            ),
        }
    }
}

fn route_of(session: &SessionKey) -> Route {
    match &session.machine {
        MachineId::Local => Route::Local,
        MachineId::Remote(machine_id) => Route::Remote(machine_id.clone()),
    }
}

/// JavaScript truthiness of a JSON value.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(text) => !text.is_empty(),
        _ => true,
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
