//! Open and create for the native New GBot dialog (its presets read first), opened from Main Hub's GBots heading
//! (native_sidebar/gbots_rows.rs).
//! SEE-ALSO: apps/desktop/src/app/window/new_gbot_modal.rs (the window), apps/desktop/src/app/gx_store/create/bot.rs (`gx_store_create_gbot`).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the New GBot dialog.
    /// Opens the New GBot dialog, with the presets it can start from (read first; none when
    /// gxserver can't say).
    pub(crate) fn open_gpui_new_gbot_modal(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let presets = crate::app::gx_store::gx_rpc(
                None,
                "/api/gbots",
                serde_json::json!({ "op": "presets" }),
            )
            .await
            .ok()
            .and_then(|answer| answer["presets"].as_array().cloned())
            .unwrap_or_default();
            let _ = this.update(cx, |app, cx| app.open_gpui_new_gbot_window(presets, cx));
        })
        .detach();
    }

    fn open_gpui_new_gbot_window(
        &mut self,
        presets: Vec<serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = NewGbotModalConfig {
            palette: self.gpui_native_dialog_palette(),
            presets,
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let NewGbotModalCommand::Create {
                name,
                title,
                instructions,
                agent,
                preset,
            } = command
            {
                app.gx_store_create_gbot(name, title, instructions, agent, preset, cx);
            }
            app.release_native_app_modal_window(GpuiAppModalKind::NewGbot, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::NewGbot,
            NEW_GBOT_MODAL_WIDTH,
            NEW_GBOT_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiNewGbotModalWindow::new(config, host, window, cx)),
            cx,
        );
    }
}
