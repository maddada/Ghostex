//! The rail's items in the web build: what a page can open. The desktop's are in
//! apps/desktop/src/app/sidebar_rail/host.rs; a page has no Claude or Grok embed, no Hubs, no
//! side panel views and no bell, so those items do not apply here.

use gpui::Window;
use serde_json::json;

use crate::GhostexGpuiApp;
use crate::app::sidebar_rail::RailItemState;

impl GhostexGpuiApp {
    pub(crate) fn sidebar_rail_item_state(&self, key: &str, _cx: &gpui::App) -> RailItemState {
        let snapshot = self.native_sidebar.snapshot.as_ref();
        let mut state = RailItemState::default();
        match key {
            "search" => state.available = true,
            "mainHub" => {
                state.available = crate::app::native_sidebar::chief::chief_on()
                    && snapshot.is_some_and(|snapshot| snapshot.chief_enabled);
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
}
