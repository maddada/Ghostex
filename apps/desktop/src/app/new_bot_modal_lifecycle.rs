//! Open and create for the native New bot dialog, opened from the Bots list's New bot row
//! (native_sidebar/bots.rs). The bot is made on the computer the list shows: the selected
//! computer with Separate by machine on, this one with the computers combined.
//! SEE-ALSO: apps/desktop/src/app/window/new_bot_modal.rs (the window), apps/desktop/src/app/gx_store/create/bot.rs (`gx_store_create_bot`, the gxserver call and the new row selected).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the New bot dialog for the computer the Bots list shows.
    pub(crate) fn open_gpui_new_bot_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.native_sidebar.snapshot.as_ref() else {
            return;
        };
        let machine_id = Some(snapshot.selected_machine_id.clone())
            .filter(|machine| !machine.is_empty())
            .unwrap_or_else(|| ghostex_gx_core::LOCAL_MACHINE_ID.to_string());
        let local = machine_id == ghostex_gx_core::LOCAL_MACHINE_ID;
        // The bots on that computer, the one open in the list first.
        let mut bots: Vec<(bool, NewBotSource)> = snapshot
            .groups
            .iter()
            .filter(|group| {
                let group_machine = group
                    .remote_machine_context
                    .as_ref()
                    .and_then(|context| context.get("machineId"))
                    .and_then(serde_json::Value::as_str);
                match group_machine {
                    Some(remote) => remote == machine_id,
                    None => local,
                }
            })
            .filter_map(|group| {
                let profile = group.bot_profile()?.to_string();
                Some((
                    group.is_active,
                    NewBotSource {
                        profile,
                        title: group.title.clone(),
                    },
                ))
            })
            .collect();
        bots.sort_by_key(|(active, bot)| (!active, bot.profile != "default"));
        let source = bots.into_iter().next().map(|(_, bot)| bot);
        let remote = if local {
            None
        } else {
            match self.gpui_remote_gxserver_request_target(&machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Computer not connected",
                        "Connect to that computer again, then make the bot.",
                        cx,
                    );
                    return;
                }
            }
        };
        let computer = (!local).then(|| {
            snapshot
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .map(|machine| machine.label.clone())
                .unwrap_or_else(|| "that computer".to_string())
        });
        let config = NewBotModalConfig {
            palette: self.gpui_native_dialog_palette(),
            source,
            computer,
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| match command {
            NewBotModalCommand::Create {
                profile,
                clone_from,
                description,
                reply,
            } => app.gx_store_create_bot(
                remote.clone(),
                machine_id.clone(),
                profile,
                clone_from,
                description,
                reply,
                cx,
            ),
            NewBotModalCommand::Cancel => {
                app.release_native_app_modal_window(GpuiAppModalKind::NewBot, cx)
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::NewBot,
            NEW_BOT_MODAL_WIDTH,
            NEW_BOT_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiNewBotModalWindow::new(config, host, window, cx)),
            cx,
        );
    }
}
