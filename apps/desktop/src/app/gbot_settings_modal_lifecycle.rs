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
        self.open_gpui_gbot_settings_modal_on(bot_id, machine_id, GbotSettingsTab::Profile, cx);
    }

    /// The same, opened on `tab` (the row menu's Memory… opens the Memory tab).
    pub(crate) fn open_gpui_gbot_settings_modal_on(
        &mut self,
        bot_id: String,
        machine_id: String,
        tab: GbotSettingsTab,
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
                json!({ "op": "show", "bot": bot_id.clone() }),
            )
            .await;
            // Without the agent list the picker still shows the bot's own agent.
            let status = match &shown {
                Ok(_) => gx_rpc(remote.clone(), "/api/chief", json!({ "op": "status" }))
                    .await
                    .ok(),
                Err(_) => None,
            };
            // Opening on Memory reads it first, so the tab draws filled.
            let memory = match (&shown, tab) {
                (Ok(_), GbotSettingsTab::Memory) => gx_rpc(
                    remote.clone(),
                    "/api/gbots",
                    json!({ "op": "memoryShow", "bot": bot_id }),
                )
                .await
                .ok(),
                _ => None,
            };
            let _ = this.update(cx, |app, cx| {
                let record = shown.map_err(|error| error.message).and_then(|answer| {
                    GbotSettingsRecord::from_show(&answer["bot"])
                        .ok_or_else(|| "Ghostex couldn't read this GBot.".to_string())
                });
                match record {
                    Ok(record) => {
                        app.open_gpui_gbot_settings_window(record, status, remote, tab, memory, cx)
                    }
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
        tab: GbotSettingsTab,
        memory: Option<Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = GbotSettingsModalConfig {
            memory,
            departments: None,
            agents: gbot_agent_choices(status.as_ref(), &record.agent_id),
            record,
            palette: self.gpui_native_panel_palette(),
            tab,
        };
        // The window stays open while the save runs and closes only when gxserver accepted it
        // (CDXC:GBots 2026-11-09 in window/gbot_settings_modal/window.rs).
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            GbotSettingsModalCommand::Save {
                bot_id,
                changes,
                reply,
            } => {
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
            }
            // The Memory tab: the op, then the memory as it is afterwards (memory.rs).
            GbotSettingsModalCommand::MemoryLoad { bot_id, reply } => {
                run_gbot_memory_op(remote.clone(), None, bot_id, reply, cx)
            }
            GbotSettingsModalCommand::MemorySave {
                bot_id,
                text,
                base,
                reply,
            } => run_gbot_memory_op(
                remote.clone(),
                Some(json!({ "op": "memoryEdit", "bot": bot_id, "text": text, "base": base })),
                bot_id,
                reply,
                cx,
            ),
            GbotSettingsModalCommand::MemoryUndo {
                bot_id,
                change,
                reply,
            } => run_gbot_memory_op(
                remote.clone(),
                Some(json!({ "op": "memoryUndo", "bot": bot_id, "change": change })),
                bot_id,
                reply,
                cx,
            ),
            // The Routines tab: the op, then the routines as they are afterwards (routines.rs).
            GbotSettingsModalCommand::Routines { bot_id, op, reply } => {
                run_gbot_routine_op(remote.clone(), op, bot_id, reply, cx)
            }
            GbotSettingsModalCommand::Departments { bot_id, ops, reply } => {
                run_gbot_department_ops(remote.clone(), ops, bot_id, reply, cx)
            }
            // The Computer tab: the op, then where it works as it is afterwards (computer.rs).
            GbotSettingsModalCommand::Computer { bot_id, op, reply } => {
                run_gbot_computer_op(remote.clone(), op, bot_id, reply, cx)
            }
            // The Bot API tab: the op (turning it on or a new key waits for the user's approval),
            // else a plain read (bot_api.rs).
            GbotSettingsModalCommand::BotApi { bot_id, op, reply } => {
                run_gbot_api_op(remote.clone(), op, bot_id, reply, cx)
            }
            GbotSettingsModalCommand::Reveal { path } => {
                app.reveal_gbot_memory_file(remote.is_some(), &path, cx)
            }
            GbotSettingsModalCommand::Cancel => {
                app.release_native_app_modal_window(GpuiAppModalKind::GbotSettings, cx);
            }
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

    /// Show in Finder for a GBot's memory file; only on the computer the file is on.
    fn reveal_gbot_memory_file(&mut self, remote: bool, path: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(not(target_arch = "wasm32"))]
        if !remote {
            if let Err(message) = gpui_reveal_path_in_finder(std::path::Path::new(path)) {
                self.dispatch_gpui_app_modal_toast("warning", "Couldn't show it", &message, cx);
            }
            return;
        }
        let _ = (remote, path);
        self.dispatch_gpui_app_modal_toast(
            "info",
            "It's on the GBot's computer",
            "Show in Finder works in Ghostex on the computer the GBot lives on.",
            cx,
        );
    }
}

/// Runs memory op `op` (none for a plain read), then reads the memory again, and hands the window
/// the read or the op's refusal.
fn run_gbot_memory_op(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    op: Option<Value>,
    bot_id: String,
    reply: GbotMemoryReply,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    cx.spawn(async move |_this, cx| {
        let mut result = Ok(());
        if let Some(op) = op {
            result = gx_rpc(remote.clone(), "/api/gbots", op)
                .await
                .map(|_| ())
                .map_err(|error| error.message);
        }
        let shown = match result {
            Ok(()) => gx_rpc(
                remote,
                "/api/gbots",
                json!({ "op": "memoryShow", "bot": bot_id }),
            )
            .await
            .map_err(|error| error.message),
            Err(message) => Err(message),
        };
        let _ = cx.update(|cx| reply.finish(shown, cx));
    })
    .detach();
}

/// Runs department ops `ops` in order (stopping at the first refusal), then reads the departments
/// and the GBot again, and hands the window `{departments, bot}` or the refusal.
fn run_gbot_department_ops(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    ops: Vec<Value>,
    bot_id: String,
    reply: GbotDepartmentReply,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    cx.spawn(async move |_this, cx| {
        let mut result = Ok(());
        for op in ops {
            result = gx_rpc(remote.clone(), "/api/gbots", op)
                .await
                .map(|_| ())
                .map_err(|error| error.message);
            if result.is_err() {
                break;
            }
        }
        let read = match result {
            Ok(()) => {
                let departments =
                    gx_rpc(remote.clone(), "/api/gbots", json!({ "op": "departments" })).await;
                let shown =
                    gx_rpc(remote, "/api/gbots", json!({ "op": "show", "bot": bot_id })).await;
                match (departments, shown) {
                    (Ok(departments), Ok(shown)) => {
                        Ok(json!({ "departments": departments, "bot": shown["bot"] }))
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error.message),
                }
            }
            Err(message) => Err(message),
        };
        let _ = cx.update(|cx| reply.finish(read, cx));
    })
    .detach();
}

/// Runs routine op `op` (none for a plain read), then lists the GBot's routines again, and hands
/// the window the list or the op's refusal.
fn run_gbot_routine_op(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    op: Option<Value>,
    bot_id: String,
    reply: GbotRoutinesReply,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    cx.spawn(async move |_this, cx| {
        let mut result = Ok(());
        if let Some(op) = op {
            result = gx_rpc(remote.clone(), "/api/gbots", op)
                .await
                .map(|_| ())
                .map_err(|error| error.message);
        }
        let listed = match result {
            Ok(()) => gx_rpc(
                remote,
                "/api/gbots",
                json!({ "op": "routines", "bot": bot_id }),
            )
            .await
            .map_err(|error| error.message),
            Err(message) => Err(message),
        };
        let _ = cx.update(|cx| reply.finish(listed, cx));
    })
    .detach();
}

/// Runs computer op `op` (none for a plain read), then reads where the GBot works again, and
/// hands the window that answer or the op's refusal.
fn run_gbot_computer_op(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    op: Option<Value>,
    bot_id: String,
    reply: crate::app::window::gbot_settings_modal::GbotComputerReply,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    cx.spawn(async move |_this, cx| {
        let mut result = Ok(());
        if let Some(op) = op {
            #[allow(unused_mut)]
            let mut answer = gx_rpc(remote.clone(), "/api/gbots", op.clone()).await;
            // Letting the GBot do more asks for the owner's approval (server/src/server/
            // security_gates.rs `gbots_computer_action`): Touch ID shows while the call is out,
            // and the same call is made again once it was approved (the phone may approve it too).
            #[cfg(not(target_arch = "wasm32"))]
            if remote.is_none() {
                if let Some(id) = answer
                    .as_ref()
                    .err()
                    .and_then(|error| error.approval_required())
                    .and_then(|approval| approval["id"].as_str())
                    .map(str::to_string)
                {
                    answer = match crate::app::window::collab_share::wait_for_approval(id.clone())
                        .await
                    {
                        Ok(()) => {
                            let mut again = op;
                            again["approvalId"] = json!(id);
                            gx_rpc(None, "/api/gbots", again).await
                        }
                        Err(message) => Err(crate::app::gx_store::GxRpcError::transport(
                            "/api/gbots",
                            message,
                        )),
                    };
                }
            }
            result = answer.map(|_| ()).map_err(|error| error.message);
        }
        let read = match result {
            Ok(()) => gx_rpc(
                remote,
                "/api/gbots",
                json!({ "op": "computer", "bot": bot_id }),
            )
            .await
            .map_err(|error| error.message),
            Err(message) => Err(message),
        };
        let _ = cx.update(|cx| reply.finish(read, cx));
    })
    .detach();
}

/// Runs Bot API op `op` (none for a plain read of `api`) and hands the window gxserver's answer:
/// every API op answers the API's state, and turning it on or a new key carries the key, shown
/// once. Turning it on and a new key ask for the user's approval (server/src/server/
/// security_gates.rs): Touch ID shows while the call is out, and the same call is made again
/// once it was approved (the phone may approve it too).
fn run_gbot_api_op(
    remote: Option<GpuiRemoteGxserverRequestTarget>,
    op: Option<Value>,
    bot_id: String,
    reply: crate::app::window::gbot_settings_modal::GbotApiReply,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) {
    cx.spawn(async move |_this, cx| {
        let mut params = op.unwrap_or_else(|| json!({ "op": "api", "bot": bot_id }));
        // A one-time nonce for this call and its approved repeat, so gxserver shows the key only
        // to this call (server/src/server/security_gates.rs `gbots_api_key_action`).
        params["keyNonce"] = json!(crate::app::gx_chat::random_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>());
        #[allow(unused_mut)]
        let mut answer = gx_rpc(remote.clone(), "/api/gbots", params.clone()).await;
        #[cfg(not(target_arch = "wasm32"))]
        if remote.is_none() {
            if let Some(id) = answer
                .as_ref()
                .err()
                .and_then(|error| error.approval_required())
                .and_then(|approval| approval["id"].as_str())
                .map(str::to_string)
            {
                answer = match crate::app::window::collab_share::wait_for_approval(id.clone()).await
                {
                    Ok(()) => {
                        let mut again = params;
                        again["approvalId"] = json!(id);
                        gx_rpc(None, "/api/gbots", again).await
                    }
                    Err(message) => Err(crate::app::gx_store::GxRpcError::transport(
                        "/api/gbots",
                        message,
                    )),
                };
            }
        }
        let result = answer.map_err(|error| error.message);
        let _ = cx.update(|cx| reply.finish(result, cx));
    })
    .detach();
}
