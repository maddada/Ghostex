//! Close After Done's toggle, from every place that offers it: a sidebar row's menu and its
//! countdown badge, an agents tab's menu and action bar, the hotkey, the app modals, and the CLI
//! (`ghostex close-after-done`, through the renderer command that lands on the sidebar command).
//! The daemon that owns the session arms it (server/src/close_after_done.rs); the toasts are the
//! runtime's.

use ghostex_gx_core::{MachineId, SessionKey};
use serde_json::Value;

use super::session_calls::toggle_close_after_done;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Claims a `toggleCloseAfterDone` sidebar command. Returns whether it did.
    pub(crate) fn gx_store_run_close_after_done(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        if message.get("type").and_then(Value::as_str) != Some("toggleCloseAfterDone") {
            return false;
        }
        let Some(session_id) = message.get("sessionId").and_then(Value::as_str) else {
            return false;
        };
        self.gx_store_toggle_close_after_done(session_id, cx);
        true
    }

    /// Toggles Close After Done for a sidebar session id (local or remote).
    pub(crate) fn gx_store_toggle_close_after_done(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Close After Done is only available for terminal sessions.",
                "",
                cx,
            );
            return;
        };
        let remote = match &session.machine {
            MachineId::Local => None,
            MachineId::Remote(machine_id) => {
                match self.gpui_remote_gxserver_request_target(machine_id) {
                    Some(target) => Some(target),
                    None => {
                        self.dispatch_gpui_app_modal_toast(
                            "info",
                            "Close After Done is only available for terminal sessions.",
                            "",
                            cx,
                        );
                        return;
                    }
                }
            }
        };
        let project_id = session.project_id.clone();
        let session_id = session.session_id.clone();
        cx.spawn(async move |this, cx| {
            let result = toggle_close_after_done(remote, project_id, session_id, None).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(true) => this.dispatch_gpui_app_modal_toast(
                    "info",
                    "Close After Done enabled",
                    "Closes after Done stays visible for 3m.",
                    cx,
                ),
                Ok(false) => {
                    this.dispatch_gpui_app_modal_toast("info", "Close After Done canceled", "", cx)
                }
                Err(error) if error.code.as_deref() == Some("notFound") => {
                    this.dispatch_gpui_app_modal_toast("info", &error.message, "", cx)
                }
                Err(error) => this.dispatch_gpui_app_modal_toast(
                    "error",
                    "Close After Done unavailable",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }

    /// The sessions the old runtime armed live in client storage
    /// (`ghostex-gpui-close-after-done-session-ids`); they are armed on their daemon once, and the
    /// key is removed when every one of them was taken.
    pub(crate) fn gx_store_migrate_close_after_done_storage(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(Some(raw)) = crate::app::gx_store::read_preference_value(STORED_KEY) else {
            return;
        };
        let ids: Vec<String> = serde_json::from_str::<Vec<Value>>(&raw)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        let mut calls = Vec::new();
        for id in ids {
            let Some(session) = SessionKey::parse_sidebar_session_id(&id) else {
                continue;
            };
            let remote = match &session.machine {
                MachineId::Local => None,
                MachineId::Remote(machine_id) => {
                    match self.gpui_remote_gxserver_request_target(machine_id) {
                        Some(target) => Some(target),
                        // Its machine is not connected in this run: that one is dropped.
                        None => continue,
                    }
                }
            };
            calls.push(toggle_close_after_done(
                remote,
                session.project_id,
                session.session_id,
                Some(true),
            ));
        }
        cx.spawn(async move |_, _| {
            let mut all_taken = true;
            for call in calls {
                // A session that no longer exists is refused and counts as taken.
                if let Err(error) = call.await {
                    all_taken &= error.code.is_some();
                }
            }
            if all_taken {
                let _ = crate::app::gx_store::write_client_document_value(STORED_KEY, None);
            }
        })
        .detach();
    }
}

/// The old runtime's client-storage key for the armed sessions.
const STORED_KEY: &str = "ghostex-gpui-close-after-done-session-ids";
