//! What each rail icon shows and does in the desktop app. The web build has its own
//! (apps/gpui-web/src/app/web_host/sidebar_rail.rs) for the items a page can open.

use gpui::Window;
use serde_json::json;

use super::model::{RailBadge, RailItemState};
use crate::GhostexGpuiApp;
use crate::app::claude_web::{TakeoverPage, WebPlace};
use crate::app::native_sidebar::actions::NativeSidebarAction;

fn rail_web_place(key: &str) -> Option<WebPlace> {
    match key {
        "claude" => Some(WebPlace::Claude),
        "grok" => Some(WebPlace::Grok),
        _ => None,
    }
}

impl GhostexGpuiApp {
    /// What item `key` shows now: whether it applies here, whether its page or view is on screen,
    /// and its count or dot.
    pub(crate) fn sidebar_rail_item_state(&self, key: &str, _cx: &gpui::App) -> RailItemState {
        let mut state = RailItemState::default();
        let snapshot = self.native_sidebar.snapshot.as_ref();
        if let Some(place) = rail_web_place(key) {
            state.available = place.enabled() && self.titlebar_mode_available(place.mode());
            state.showing = self.web_place_on() == Some(place);
            return state;
        }
        match key {
            "search" | "commands" | "connectPhone" | "settings" => state.available = true,
            "mainHub" => {
                let chief = snapshot
                    .and_then(|snapshot| snapshot.chiefs.iter().find(|chief| chief.is_local));
                state.available = crate::app::native_sidebar::chief::chief_on()
                    && snapshot.is_some_and(|snapshot| snapshot.chief_enabled);
                state.showing = chief.is_some_and(|chief| chief.is_focused) && !self.claude_on();
                if let Some(chief) = chief {
                    if chief.awaiting_approval || chief.has_question {
                        state.badge = Some(RailBadge::Dot(
                            gpui::rgb(crate::app::native_sidebar::status::APPROVAL_COLOR).into(),
                        ));
                    } else if chief.activity == "working" {
                        state.badge = Some(RailBadge::Dot(gpui::rgb(0xd29a2e).into()));
                    }
                }
            }
            "agentsHub" => {
                state.available = true;
                state.showing = self.takeover_page_on() == Some(TakeoverPage::AgentsHub);
            }
            "allAutomations" => {
                state.available = true;
                state.showing = self.takeover_page_on() == Some(TakeoverPage::AllAutomations);
            }
            "hermes" => {
                state.available = snapshot.is_some_and(|snapshot| snapshot.bots_enabled);
                state.showing = snapshot.is_some_and(|snapshot| snapshot.bots_mode);
            }
            "voiceCall" => {
                state.available = crate::app::voice_agent::voice_agent_on();
                state.showing = self.voice_agent.active();
            }
            _ => {}
        }
        state
    }

    /// A click on item `key`, on the rail or its ⋯ menu.
    pub(crate) fn run_sidebar_rail_item(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(place) = rail_web_place(key) {
            self.toggle_web_place(place, window, cx);
            return;
        }
        let sidebar_action = |action: &str| json!({"type": "sidebarAction", "action": action});
        match key {
            "search" => self.dispatch_native_sidebar_ui(sidebar_action("sessions"), cx),
            "commands" => self
                .dispatch_native_sidebar_ui(sidebar_action(ghostex_gx_core::COMMANDS_ACTION), cx),
            "connectPhone" => self.dispatch_native_sidebar_ui(sidebar_action("remoteSetup"), cx),
            "settings" => self.dispatch_native_sidebar_ui(sidebar_action("settings"), cx),
            "mainHub" => self.handle_native_sidebar_action(
                &NativeSidebarAction {
                    command: json!({ "type": "chief", "op": "open" }),
                },
                window,
                cx,
            ),
            "agentsHub" => self.toggle_takeover_page(TakeoverPage::AgentsHub, window, cx),
            "allAutomations" => self.toggle_takeover_page(TakeoverPage::AllAutomations, window, cx),
            "hermes" => self.toggle_native_sidebar_bots_mode(cx),
            "voiceCall" => self.toggle_voice_agent_conversation(cx),
            _ => {}
        }
    }
}
