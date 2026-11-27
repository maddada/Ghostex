//! The rail's items in the web build: what a page can open. The desktop's are in
//! apps/desktop/src/app/sidebar_rail/host.rs; a page has no Claude or Grok embed, no Hubs, no
//! side panel views and no bell, so those items do not apply here. Nor do Settings and Theme: a
//! page opens no Settings window and no Theme editor, so the rail's gear and those ⋯ rows stay out.

use gpui::Window;
use serde_json::json;

use crate::GhostexGpuiApp;
use crate::app::sidebar_rail::RailItemState;

impl GhostexGpuiApp {
    /// The page never floats the sidebar, so resting on the rail opens nothing (the desktop's
    /// `sidebar_rail_reveal_hover`).
    pub(crate) fn sidebar_rail_reveal_hover(
        &mut self,
        _top_half: Option<bool>,
        _cx: &mut gpui::Context<Self>,
    ) {
    }

    pub(crate) fn sidebar_rail_item_state(&self, key: &str, _cx: &gpui::App) -> RailItemState {
        let snapshot = self.native_sidebar.snapshot.as_ref();
        let mut state = RailItemState::default();
        match key {
            "search" => state.available = true,
            "mainHub" => {
                state.available = true;
            }
            "hermes" => {
                state.available = snapshot.is_some_and(|snapshot| snapshot.bots_enabled);
                state.showing = snapshot.is_some_and(|snapshot| snapshot.bots_mode);
            }
            _ => {}
        }
        state
    }

    pub(crate) fn run_sidebar_rail_item(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match key {
            "search" => self.dispatch_native_sidebar_ui(
                json!({"type": "sidebarAction", "action": "sessions"}),
                cx,
            ),
            "mainHub" => self.handle_native_sidebar_action(
                &crate::app::native_sidebar::actions::NativeSidebarAction {
                    command: json!({ "type": "chief", "op": "open" }),
                },
                window,
                cx,
            ),
            "hermes" => self.toggle_native_sidebar_bots_mode(cx),
            _ => {}
        }
    }

    /// A page in a browser has no links' page (app/custom_links/ on the desktop), so it shows no
    /// pinned link.
    pub(crate) fn sidebar_rail_link_pin(
        &self,
        _key: &str,
    ) -> Option<crate::app::sidebar_rail::RailLinkPin> {
        None
    }

    pub(crate) fn run_sidebar_rail_link(
        &mut self,
        _id: &str,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
    }

    /// A page in a browser has no extension views in its side panel, so the rail lists and draws
    /// none (`ext:<id>` pins, packages/gx-core/src/sidebar_rail.rs).
    pub(crate) fn sidebar_rail_extension_views(
        &self,
    ) -> Vec<crate::app::sidebar_rail::RailExtensionView> {
        Vec::new()
    }

    pub(crate) fn sidebar_rail_extension_pin(
        &self,
        _key: &str,
    ) -> Option<crate::app::sidebar_rail::RailExtensionView> {
        None
    }

    pub(crate) fn run_sidebar_rail_extension(
        &mut self,
        _id: &str,
        _window: &mut Window,
        _cx: &mut gpui::Context<Self>,
    ) {
    }
}
