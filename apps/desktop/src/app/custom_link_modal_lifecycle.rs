//! Open and save for the Add link and Edit link dialog (window/custom_link_modal.rs), from the
//! sidebar's ⋯ menus (app/custom_links/).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the dialog: empty for Add link…, filled from link `id` for its Edit….
    pub(crate) fn open_gpui_custom_link_modal(
        &mut self,
        id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let link = id.and_then(crate::app::custom_links::saved_custom_link);
        if id.is_some() && link.is_none() {
            return;
        }
        let config = CustomLinkModalConfig {
            id: link.as_ref().map(|link| link.id.clone()),
            name: link
                .as_ref()
                .map(|link| link.name.clone())
                .unwrap_or_default(),
            url: link.map(|link| link.url).unwrap_or_default(),
            read_address: ghostex_gx_core::custom_links::normalize_custom_link_url,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.release_native_app_modal_window(GpuiAppModalKind::CustomLink, cx);
            if let CustomLinkModalCommand::Save { id, name, url } = command {
                app.save_custom_link(id, name, url, cx);
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CustomLink,
            CUSTOM_LINK_MODAL_WIDTH,
            CUSTOM_LINK_MODAL_INITIAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiCustomLinkModalWindow::new(config, host, window, cx)),
            cx,
        );
    }
}
