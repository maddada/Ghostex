//! Open and save for the Add link, Edit link, and Change icon dialog
//! (window/custom_link_modal.rs), from the rail's ⋯ menu and a pinned link's right-click menu
//! (app/custom_links/).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the dialog: empty for Add link…, filled from link `id` for its Edit… or Change icon….
    pub(crate) fn open_gpui_custom_link_modal(
        &mut self,
        id: Option<&str>,
        mode: CustomLinkModalMode,
        cx: &mut gpui::Context<Self>,
    ) {
        let link = id.and_then(crate::app::custom_links::saved_custom_link);
        if id.is_some() && link.is_none() {
            return;
        }
        let site_favicon = link
            .as_ref()
            .and_then(|link| crate::browser_favicon_cache_lookup(&link.url, None))
            .map(|favicon| favicon.image);
        let config = CustomLinkModalConfig {
            id: link.as_ref().map(|link| link.id.clone()),
            mode,
            name: link
                .as_ref()
                .map(|link| link.name.clone())
                .unwrap_or_default(),
            url: link
                .as_ref()
                .map(|link| link.url.clone())
                .unwrap_or_default(),
            icon: link.and_then(|link| link.icon),
            site_favicon,
            read_address: ghostex_gx_core::custom_links::normalize_custom_link_url,
            palette: self.gpui_native_dialog_palette(),
        };
        let initial_height = match mode {
            CustomLinkModalMode::Full => CUSTOM_LINK_MODAL_INITIAL_HEIGHT,
            CustomLinkModalMode::IconOnly => CUSTOM_LINK_ICON_MODAL_INITIAL_HEIGHT,
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.release_native_app_modal_window(GpuiAppModalKind::CustomLink, cx);
            if let CustomLinkModalCommand::Save {
                id,
                name,
                url,
                icon,
            } = command
            {
                app.save_custom_link(id, name, url, icon, cx);
            }
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CustomLink,
            CUSTOM_LINK_MODAL_WIDTH,
            initial_height,
            move |window, cx| cx.new(|cx| GpuiCustomLinkModalWindow::new(config, host, window, cx)),
            cx,
        );
    }
}
