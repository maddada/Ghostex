//! The `web` block: a small page of the extension's own. The desktop shows it as a web page in the
//! block's frame (app/extensions/web_blocks.rs); the GPUI web build, which draws no extension
//! pages (Q27), shows an Open on computer row in its place, as the phone does.

use ghostex_gx_protocol::extension_panel::{
    PanelBlock, WEB_BLOCK_HEIGHT, WEB_BLOCK_HEIGHT_MAX, WEB_BLOCK_HEIGHT_MIN,
};
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};

use super::PanelPlace;
use crate::GhostexGpuiApp;
use crate::app::native_sidebar::appearance::SidebarAppearance;

impl GhostexGpuiApp {
    pub(super) fn render_native_panel_web(
        &self,
        block: &PanelBlock,
        path: &str,
        height: Option<u32>,
        place: &PanelPlace,
        appearance: &SidebarAppearance,
    ) -> AnyElement {
        let height = height
            .unwrap_or(WEB_BLOCK_HEIGHT)
            .clamp(WEB_BLOCK_HEIGHT_MIN, WEB_BLOCK_HEIGHT_MAX) as f32;
        #[cfg(not(ghostex_web))]
        if let Some(url) = self.native_panel_web_url(block, path, place) {
            let key = crate::app::extensions::WebBlockKey {
                extension_id: place.extension_id.clone(),
                surface: place.surface,
                session: place.session.clone(),
                block_id: block.id.clone(),
            };
            return self.render_extension_web_block(
                key,
                url,
                px(height * appearance.scale),
                appearance,
            );
        }
        let _ = (block, path, place, height);
        open_on_computer_row(appearance)
    }

    /// The block's page: the extension's page base (`/ext/<id>/`) plus its path, told which block
    /// and surface it is.
    #[cfg(not(ghostex_web))]
    fn native_panel_web_url(
        &self,
        block: &PanelBlock,
        path: &str,
        place: &PanelPlace,
    ) -> Option<String> {
        let base = self
            .native_extension_panels()?
            .0
            .get(&place.extension_id)?
            .page_base
            .clone()?;
        let mut url = url::Url::parse(&base).ok()?.join(path).ok()?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("surface", place.surface);
            query.append_pair("block", &block.id);
            if let Some(session) = &place.session {
                query.append_pair("session", session);
            }
        }
        // Never leave the extension's own folder (gxserver checks the path too).
        url.path()
            .starts_with(&format!("/ext/{}/", place.extension_id))
            .then(|| url.to_string())
    }
}

/// What a client that draws no extension pages shows in a web block's place.
fn open_on_computer_row(appearance: &SidebarAppearance) -> AnyElement {
    let scale = appearance.scale;
    div()
        .w_full()
        .min_w_0()
        .px(px(6.0 * scale))
        .py(px(4.0 * scale))
        .text_size(px(12.0 * scale))
        .text_color(appearance.muted)
        .child("Open on computer")
        .into_any_element()
}
