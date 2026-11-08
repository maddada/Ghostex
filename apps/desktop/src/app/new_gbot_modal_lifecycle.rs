//! Open and create for the native New bot dialog (GBots). Nothing opens it until native GBots
//! give it a place under Chief of Staff in Main Hub.
//! SEE-ALSO: apps/desktop/src/app/window/new_gbot_modal.rs (the window), apps/desktop/src/app/gx_store/create/bot.rs (`gx_store_create_gbot`).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the New bot dialog.
    #[allow(dead_code)]
    pub(crate) fn open_gpui_new_gbot_modal(&mut self, cx: &mut gpui::Context<Self>) {
        let config = NewGbotModalConfig {
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let NewGbotModalCommand::Create {
                name,
                instructions,
                agent,
            } = command
            {
                app.gx_store_create_gbot(name, instructions, agent, cx);
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
