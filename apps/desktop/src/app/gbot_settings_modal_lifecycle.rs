//! Open and save for the native GBot Settings window, opened from a GBot's row.
//! SEE-ALSO: apps/desktop/src/app/window/gbot_settings_modal/ (the window), server/src/gbots/endpoint.rs (`show`, `edit`).
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// Opens the settings of GBot `bot_id` on `machine_id` (`ghostex_gx_core::LOCAL_MACHINE_ID`
    /// for this computer, or a remote machine's id): reads the bot with `/api/gbots {op:"show"}`
    /// and the agents it may run on with `/api/chief {op:"status"}`, then opens the window.
    #[allow(dead_code)]
    pub(crate) fn open_gpui_gbot_settings_modal(
        &mut self,
        bot_id: String,
        machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote = if machine_id.is_empty() || machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
            None
        } else {
            match self.gpui_remote_gxserver_request_target(&machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Computer not connected",
                        "Connect to that computer again, then open the GBot's settings.",
                        cx,
                    );
                    return;
                }
            }
        };
        cx.spawn(async move |this, cx| {
            let shown = gx_rpc(
                remote.clone(),
                "/api/gbots",
                json!({ "op": "show", "bot": bot_id }),
            )
            .await;
            // Without the agent list the picker still shows the bot's own agent.
            let status = match &shown {
                Ok(_) => gx_rpc(remote.clone(), "/api/chief", json!({ "op": "status" }))
                    .await
                    .ok(),
                Err(_) => None,
            };
            let _ = this.update(cx, |app, cx| {
                let record = shown.map_err(|error| error.message).and_then(|answer| {
                    GbotSettingsRecord::from_show(&answer["bot"])
                        .ok_or_else(|| "Ghostex couldn't read this GBot.".to_string())
                });
                match record {
                    Ok(record) => app.open_gpui_gbot_settings_window(record, status, remote, cx),
                    Err(message) => app.dispatch_gpui_app_modal_toast(
                        "warning",
                        "GBot settings didn't open",
                        &message,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    fn open_gpui_gbot_settings_window(
        &mut self,
        record: GbotSettingsRecord,
        status: Option<Value>,
        remote: Option<GpuiRemoteGxserverRequestTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = GbotSettingsModalConfig {
            agents: gbot_agent_choices(status.as_ref(), &record.agent_id),
            record,
            palette: self.gpui_native_panel_palette(),
            tab: GbotSettingsTab::Profile,
        };
        // The window stays open while the save runs and closes only when gxserver accepted it
        // (CDXC:GBots 2026-11-09 in window/gbot_settings_modal/window.rs).
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            let GbotSettingsModalCommand::Save {
                bot_id,
                changes,
                reply,
            } = command
            else {
                app.release_native_app_modal_window(GpuiAppModalKind::GbotSettings, cx);
                return;
            };
            let mut params = changes;
            params.insert("op".to_string(), json!("edit"));
            params.insert("bot".to_string(), json!(bot_id));
            let remote = remote.clone();
            cx.spawn(async move |this, cx| {
                let saved = gx_rpc(remote, "/api/gbots", Value::Object(params))
                    .await
                    .map(|_| ())
                    .map_err(|error| error.message);
                let _ = this.update(cx, |app, cx| {
                    let succeeded = saved.is_ok();
                    let failure = saved.as_ref().err().cloned();
                    let shown = reply.finish(saved, cx);
                    if succeeded && shown {
                        app.release_native_app_modal_window(GpuiAppModalKind::GbotSettings, cx);
                    } else if let (false, Some(message)) = (shown, failure) {
                        // Cancelled while the save ran: nothing is left to show the refusal in.
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "GBot not saved",
                            &message,
                            cx,
                        );
                    }
                });
            })
            .detach();
        });
        self.open_native_app_modal(
            GpuiAppModalKind::GbotSettings,
            GBOT_SETTINGS_MODAL_WIDTH,
            GBOT_SETTINGS_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiGbotSettingsModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }
}
