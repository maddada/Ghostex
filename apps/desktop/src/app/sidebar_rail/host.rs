//! What each rail icon shows and does in the desktop app. The web build has its own
//! (apps/gpui-web/src/app/web_host/sidebar_rail.rs) for the items a page can open.

use gpui::Window;
use serde_json::json;

use super::model::{RailExtensionView, RailItemState, RailLinkPin};
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
    /// What item `key` shows now: whether it applies here and whether its page or view is on screen.
    pub(crate) fn sidebar_rail_item_state(&self, key: &str, _cx: &gpui::App) -> RailItemState {
        let mut state = RailItemState::default();
        let snapshot = self.native_sidebar.snapshot.as_ref();
        if let Some(place) = rail_web_place(key) {
            state.available = place.enabled() && self.titlebar_mode_available(place.mode());
            state.showing = self.web_place_on() == Some(place);
            return state;
        }
        match key {
            // Settings and the compact Theme editor are windows over the app, never a view on the
            // rail, so neither shows as on (CDXC:Sidebar 2026-11-25 on gx-core `RAIL_FOOT_ITEM`).
            "search" | "findPrompts" | "commands" | "settings" | "theme" => state.available = true,
            // CDXC:Chief 2026-11-27 DECISION:
            // User: "icon in the rail for a Ask Ghostex AI shows red dot when it needs something lets not show dot on thhat rail icon pls". The rail's Ask Ghostex AI button draws no dot or badge in any state of its session (needs you or working); its sidebar row, notifications, Needs you and Inbox still show it.
            "mainHub" => {
                let chief = snapshot
                    .and_then(|snapshot| snapshot.chiefs.iter().find(|chief| chief.is_local));
                state.available = true;
                state.showing = chief.is_some_and(|chief| chief.is_focused) && !self.claude_on();
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

    /// The pinned link `key` (`link:<id>`) as its rail button shows it, while the link is saved.
    pub(crate) fn sidebar_rail_link_pin(&self, key: &str) -> Option<RailLinkPin> {
        let id = ghostex_gx_core::sidebar_rail::rail_link_pin_id(key)?;
        let link = crate::app::custom_links::saved_custom_link(id)?;
        let icon = link.icon.as_deref().map(|icon_id| {
            crate::app::helpers::gpui_sidebar_command_icon_asset_path(Some(icon_id))
        });
        let image = if icon.is_some() {
            None
        } else {
            crate::browser_favicon_cache_lookup(&link.url, None).map(|favicon| favicon.image)
        };
        let showing = self.web_place_on() == Some(WebPlace::Link)
            && crate::app::custom_links::custom_link_page_link()
                .is_some_and(|showing| showing.id == link.id);
        Some(RailLinkPin {
            label: link.label(),
            id: link.id,
            icon,
            image,
            showing,
        })
    }

    /// Every enabled extension with a side panel view this project can show, by title: the rail's
    /// Extensions rows in ⋯ (CDXC:Sidebar 2026-11-26 on gx-core `RAIL_EXTENSION_PIN_PREFIX`).
    pub(crate) fn sidebar_rail_extension_views(&self) -> Vec<RailExtensionView> {
        let mut views: Vec<RailExtensionView> = self
            .extensions_snapshot
            .installed
            .keys()
            .filter_map(|id| self.sidebar_rail_extension_view(id))
            .collect();
        views.sort_by(|left, right| {
            left.title
                .to_lowercase()
                .cmp(&right.title.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        views
    }

    /// The pinned extension view `key` (`ext:<id>`) while its extension is on and its view can
    /// open here; a pin of one that is off, hidden here or uninstalled is not drawn.
    pub(crate) fn sidebar_rail_extension_pin(&self, key: &str) -> Option<RailExtensionView> {
        let id = ghostex_gx_core::sidebar_rail::rail_extension_pin_id(key)?;
        self.sidebar_rail_extension_view(id)
    }

    fn sidebar_rail_extension_view(&self, id: &str) -> Option<RailExtensionView> {
        let extension_id = crate::ExtensionId::new(id)?;
        let mode = crate::TitlebarMode::Extension(extension_id);
        if !self.extension_view_installed(extension_id) || !self.titlebar_mode_available(mode) {
            return None;
        }
        Some(RailExtensionView {
            id: id.to_string(),
            title: self.extension_view_title(extension_id),
            image: self
                .extensions_snapshot
                .installed
                .get(id)
                .map(|extension| extension.icon_image.clone()),
            showing: self.active_mode == mode,
        })
    }

    /// A click on a pinned extension view, or its ⋯ row: opens or focuses its view in the side
    /// panel, through the same path as `ghostex extensions open` (app/extensions/open_view.rs).
    pub(crate) fn run_sidebar_rail_extension(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(extension_id) = crate::ExtensionId::new(id) else {
            return;
        };
        let mode = crate::TitlebarMode::Extension(extension_id);
        if self.active_mode == mode {
            self.focus_project_editor_surface(mode, window, cx);
            return;
        }
        if let Err(error) = self.open_extension_view_in_side_panel(
            id,
            None,
            ghostex_gx_core::ExtensionViewOpenMode::Show,
            false,
            cx,
        ) {
            log::warn!("Rail: could not open the extension view {id}: {error}");
        }
    }

    /// A click on a pinned link: its page opens, or closes when it is the one showing.
    pub(crate) fn run_sidebar_rail_link(
        &mut self,
        id: &str,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let showing = self.web_place_on() == Some(WebPlace::Link)
            && crate::app::custom_links::custom_link_page_link()
                .is_some_and(|showing| showing.id == id);
        if showing {
            self.turn_claude_off(cx);
        } else {
            self.open_custom_link(id, cx);
        }
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
            "findPrompts" => self.open_gpui_find_prompts_modal(cx),
            "commands" => self
                .dispatch_native_sidebar_ui(sidebar_action(ghostex_gx_core::COMMANDS_ACTION), cx),
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
            // The ☰ menu's Settings row and the Settings hotkey open it the same way.
            "settings" => {
                self.open_app_modal_from_bridge(json!({ "modal": "settings", "type": "open" }), cx)
            }
            // Beside the pinned Theme button that was clicked (render.rs), or beside ⋯ for its
            // menu's Theme row; the editor keeps inside the window (native_app_modal_lifecycle.rs).
            "theme" => {
                if self.sidebar_rail.theme_editor_anchor.get().is_none() {
                    self.sidebar_rail
                        .theme_editor_anchor
                        .set(self.sidebar_rail.more_bounds.get());
                }
                self.open_gpui_theme_editor_modal(&serde_json::Value::Null, cx);
            }
            _ => {}
        }
    }

    /// The pointer is on the rail's top half (`Some(true)`), its bottom half (`Some(false)`) or
    /// off it (`None`, also on a press). Resting `RAIL_REVEAL_DWELL` on a half while the sidebar is
    /// hidden floats out what the left edge's same half does (`floating_reveal_edge_want`).
    ///
    /// CDXC:Sidebar 2026-11-21 DECISION:
    /// User (Q4322): "top half when sidebar is hidden shows the sidebar, bottom half shows the chat". With the rail on and the sidebar hidden, resting on the rail's top half floats out the sidebar exactly as the left edge's hover does (`arm_floating_reveal_from_strip`): the same panel, slide, grace and dismissal; the panel's keep zone runs from the window's left edge, so the rail and the panel keep it out, and the pointer can move onto it. It waits a short dwell first (coordinator: "the reveal shouldn't fight clicks on icons"), so a click or a passing pointer opens nothing and icons keep their tooltips; a press on the rail starts the wait again. The bottom half is "the chat": the sidebar has no chat view of its own (the rail's Claude and Grok Bot open full-window pages), so, coordinator's call, the rail mirrors the left edge's halves (`floating_reveal_edge_want`): the bottom half floats the Agents Panel while an expanded view has folded it away, and while it is on screen the whole rail floats the sidebar.
    pub(crate) fn sidebar_rail_reveal_hover(
        &mut self,
        top_half: Option<bool>,
        cx: &mut gpui::Context<Self>,
    ) {
        const RAIL_REVEAL_DWELL: std::time::Duration = std::time::Duration::from_millis(300);
        if self.sidebar_rail.reveal_half == top_half {
            return;
        }
        self.sidebar_rail.reveal_half = top_half;
        self.sidebar_rail.reveal_generation += 1;
        if std::mem::take(&mut self.sidebar_rail.reveal_armed) {
            // A panel already out leaves through its own dismissal.
            self.disarm_floating_reveal_edge();
        }
        let Some(half) = top_half else {
            return;
        };
        if !self.sidebar_collapsed || self.floating_reveal.panel.is_some() {
            return;
        }
        let generation = self.sidebar_rail.reveal_generation;
        cx.spawn(async move |app, cx| {
            cx.background_executor().timer(RAIL_REVEAL_DWELL).await;
            let _ = app.update(cx, |app, cx| {
                if app.sidebar_rail.reveal_generation != generation
                    || app.sidebar_rail.reveal_half != Some(half)
                    || !app.sidebar_collapsed
                    || app.floating_reveal.panel.is_some()
                {
                    return;
                }
                app.sidebar_rail.reveal_armed = true;
                // The left edge's halves: the sidebar from the top, the Agents Panel from the
                // bottom while it is folded away, the sidebar from either otherwise.
                let want = app.floating_reveal_edge_want(half);
                app.arm_floating_reveal_from_strip(want);
                // Wakes the reveal's sweep, which opens the panel.
                cx.notify();
            });
        })
        .detach();
    }
}
