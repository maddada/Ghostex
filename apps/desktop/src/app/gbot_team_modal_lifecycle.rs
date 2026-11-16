//! Import Team… and Export Team… from Main Hub's GBots heading (native_sidebar/gbots_rows.rs):
//! the file pickers, the `/api/gbots` team calls and the approval an import waits for.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! On the desktop Import Team… opens a file panel first and the window opens on the file's preview; Export Team… writes through a save panel into Downloads by default, or copies the JSON. The browser build has neither panel, so its Import Team window takes a pasted file and its Export copies the JSON; the rules are gxserver's either way (server/src/gbots/team_file/).
//! SEE-ALSO: apps/desktop/src/app/window/gbot_team_modal/ (the window).
use crate::app::gx_store::gx_rpc;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};

impl GhostexGpuiApp {
    /// Import Team…: a file panel (the desktop) or the paste box (the browser), then the preview.
    pub(crate) fn open_gpui_gbot_team_import(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(not(ghostex_web))]
        {
            let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Import Team".into()),
            });
            cx.spawn(async move |this, cx| {
                let Ok(Ok(Some(paths))) = receiver.await else {
                    return;
                };
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("Couldn't read {}: {error}", path.display()));
                let preview = match &text {
                    Ok(text) => gx_rpc(
                        None,
                        "/api/gbots",
                        json!({ "op": "teamPreview", "file": text }),
                    )
                    .await
                    .map_err(|error| error.message),
                    Err(message) => Err(message.clone()),
                };
                let _ = this.update(cx, |app, cx| {
                    let start = match (text, preview) {
                        (Ok(text), Ok(answer)) => GbotTeamStart::Preview {
                            text,
                            preview: answer["preview"].clone(),
                        },
                        (_, Err(message)) => GbotTeamStart::Paste {
                            error: Some(message),
                        },
                        (Err(message), _) => GbotTeamStart::Paste {
                            error: Some(message),
                        },
                    };
                    app.open_gpui_gbot_team_window(start, cx);
                });
            })
            .detach();
        }
        #[cfg(ghostex_web)]
        self.open_gpui_gbot_team_window(GbotTeamStart::Paste { error: None }, cx);
    }

    /// Export Team…: reads the GBots and departments, then opens the window.
    pub(crate) fn open_gpui_gbot_team_export(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let list = gx_rpc(None, "/api/gbots", json!({ "op": "list" })).await;
            let _ = this.update(cx, |app, cx| match list {
                Ok(list) => app.open_gpui_gbot_team_window(
                    GbotTeamStart::Export {
                        list,
                        can_save: cfg!(not(ghostex_web)),
                    },
                    cx,
                ),
                Err(error) => app.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Export Team didn't open",
                    &error.message,
                    cx,
                ),
            });
        })
        .detach();
    }

    fn open_gpui_gbot_team_window(&mut self, start: GbotTeamStart, cx: &mut gpui::Context<Self>) {
        let config = GbotTeamModalConfig {
            palette: self.gpui_native_dialog_palette(),
            start,
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            GbotTeamModalCommand::Preview { text, reply } => {
                run_gbots(json!({ "op": "teamPreview", "file": text }), reply, cx)
            }
            GbotTeamModalCommand::Import { text, reply } => {
                run_gbots(json!({ "op": "teamImport", "file": text }), reply, cx)
            }
            GbotTeamModalCommand::KeepPreset { text, reply } => {
                run_gbots(json!({ "op": "presetSave", "file": text }), reply, cx)
            }
            GbotTeamModalCommand::KeepBotPreset { bot, notes, reply } => run_gbots(
                json!({ "op": "presetSave", "bot": bot, "notes": notes }),
                reply,
                cx,
            ),
            GbotTeamModalCommand::Undo { change, reply } => {
                run_gbots(json!({ "op": "undo", "change": change }), reply, cx)
            }
            GbotTeamModalCommand::Export {
                params,
                save,
                reply,
            } => app.run_gbot_team_export(params, save, reply, cx),
            GbotTeamModalCommand::Close => {
                app.release_native_app_modal_window(GpuiAppModalKind::GbotTeam, cx);
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::GbotTeam,
            GBOT_TEAM_MODAL_WIDTH,
            GBOT_TEAM_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiGbotTeamModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// `teamExport`, then a save panel (`save`) or the clipboard.
    fn run_gbot_team_export(
        &mut self,
        params: Value,
        save: bool,
        reply: GbotTeamReply,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let exported = gx_rpc(None, "/api/gbots", params)
                .await
                .map_err(|error| error.message);
            let answer = match exported {
                Ok(answer) => answer,
                Err(message) => {
                    let _ = cx.update(|cx| reply.finish(Err(message), cx));
                    return;
                }
            };
            let text = answer["text"].as_str().unwrap_or_default().to_string();
            if !save {
                let _ = cx.update(|cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    reply.finish(Ok(answer), cx);
                });
                return;
            }
            #[cfg(not(ghostex_web))]
            {
                let name = answer["fileName"]
                    .as_str()
                    .unwrap_or("gbots.gbots-team.json")
                    .to_string();
                let downloads = std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .map(|home| home.join("Downloads"))
                    .filter(|path| path.is_dir())
                    .unwrap_or_else(std::env::temp_dir);
                let Ok(chosen) =
                    this.update(cx, |_, cx| cx.prompt_for_new_path(&downloads, Some(&name)))
                else {
                    return;
                };
                let target = match chosen.await {
                    Ok(Ok(Some(target))) => target,
                    // Cancelled: the window stays as it was.
                    _ => {
                        let _ = cx.update(|cx| reply.finish(Err("Not saved.".to_string()), cx));
                        return;
                    }
                };
                let written = std::fs::write(&target, text);
                let _ = cx.update(|cx| match written {
                    Ok(()) => {
                        let mut answer = answer;
                        answer["savedTo"] = json!(target.display().to_string());
                        reply.finish(Ok(answer), cx);
                    }
                    Err(error) => reply.finish(Err(format!("Couldn't save it: {error}")), cx),
                });
            }
            #[cfg(ghostex_web)]
            {
                let _ = this;
                let _ = cx.update(|cx| {
                    reply.finish(
                        Err("Saving a file needs the Ghostex app; press Copy.".to_string()),
                        cx,
                    )
                });
            }
        })
        .detach();
    }
}

/// Calls `/api/gbots` with `params` and hands the window the answer. An import asks for the
/// user's approval (server/src/server/security_gates.rs): Touch ID shows while the call is out,
/// and when it wasn't used the phone may still approve, after which the same call adds it.
fn run_gbots(params: Value, reply: GbotTeamReply, cx: &mut gpui::Context<GhostexGpuiApp>) {
    cx.spawn(async move |_this, cx| {
        #[allow(unused_mut)]
        let mut answer = gx_rpc(None, "/api/gbots", params.clone()).await;
        #[cfg(not(ghostex_web))]
        if let Some(id) = answer
            .as_ref()
            .err()
            .and_then(|error| error.approval_required())
            .and_then(|approval| approval["id"].as_str())
            .map(str::to_string)
        {
            answer = match crate::app::window::collab_share::wait_for_approval(id.clone()).await {
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
        let result = answer.map_err(|error| error.message);
        let _ = cx.update(|cx| reply.finish(result, cx));
    })
    .detach();
}
