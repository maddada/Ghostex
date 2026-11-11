// The app's own panels that open from the sidebar's menu: Ask Ghostex, Tips & Tricks, Resources,
// Dev servers and Extensions. Each is a native child window hung from the sidebar's menu button,
// and each is offered only while its Settings switch (and, for the last two, the project's scope)
// says so.

use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;
use gpui::px;

use crate::app::consts::*;
use crate::app::native_sidebar::navigation::{
    SIDEBAR_TOP_EDGE_INSET, SIDEBAR_TOP_ROW_BUTTON_HEIGHT, SIDEBAR_TOP_ROW_BUTTON_WIDTH,
};
use crate::*;

/// CDXC:Titlebar 2026-11-11 DECISION:
/// User (main menu page, Q2500 B): "we have this one but i think should be merged and remove the 3 dots from above chat". The header's trailing ⋯ button and its menu are gone; Ask Ghostex and Tips & Tricks are rows of the sidebar menu's Help submenu, and Extensions, Dev servers and Resources rows of its Tools submenu (packages/gx-core/src/sidebar_menu/navigation.rs). Each row opens the same panel as before, hung from the sidebar's menu button. This supersedes the 2026-09-20 ruling that gave them one trailing ⋯ menu in the header and the 2026-09-21 Customize row (Q2501 C: no row of its own; the gear at the sidebar's bottom and the Extensions panel's "Browse extensions…" open that page).
/// What stands from 2026-09-21: every row opens its overlay rather than a view tab ("i dont like that they are opening as full tabs in the right side view panel"), a blank Browser tab does not list the dev servers, an item switched off in Settings or scoped away from this project is not listed at all, and Ask Ghostex, Tips & Tricks and Resources stay app-wide: a Settings switch each, never scoped away from a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppMenuPanel {
    AskGhostex,
    Tips,
    Resources,
    DevServers,
    Extensions,
}

impl AppMenuPanel {
    /// The panel a menu row names (`MENU_PANEL_*` in gx-core).
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        match id {
            ghostex_gx_core::MENU_PANEL_HELP => Some(Self::AskGhostex),
            ghostex_gx_core::MENU_PANEL_TIPS => Some(Self::Tips),
            ghostex_gx_core::MENU_PANEL_RESOURCES => Some(Self::Resources),
            ghostex_gx_core::MENU_PANEL_DEV_SERVERS => Some(Self::DevServers),
            ghostex_gx_core::MENU_PANEL_EXTENSIONS => Some(Self::Extensions),
            _ => None,
        }
    }

    /// The Settings switch and the view scope key that decide whether the panel is offered.
    fn visibility_gate(self) -> (&'static str, &'static str) {
        match self {
            Self::AskGhostex => (HELP_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "help"),
            Self::Tips => (TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "tips"),
            Self::Resources => (RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "resources"),
            Self::DevServers => (
                DEV_SERVERS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "devServers",
            ),
            Self::Extensions => (
                EXTENSIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "extensionsButton",
            ),
        }
    }

    /// Ask Ghostex, Tips & Tricks and Resources belong to the app rather than to a project, so only
    /// their Settings switch can take them away, never a view scope.
    fn app_wide(self) -> bool {
        matches!(self, Self::AskGhostex | Self::Tips | Self::Resources)
    }

    pub(crate) fn popup_kind(self) -> GpuiTitlebarPopupKind {
        match self {
            Self::AskGhostex => GpuiTitlebarPopupKind::Help,
            Self::Tips => GpuiTitlebarPopupKind::Tips,
            Self::Resources => GpuiTitlebarPopupKind::Resources,
            Self::DevServers => GpuiTitlebarPopupKind::RemoteSites,
            Self::Extensions => GpuiTitlebarPopupKind::Extensions,
        }
    }
}

impl GhostexGpuiApp {
    /// Whether the sidebar's menu lists this panel right now.
    fn sidebar_menu_panel_shown(&self, panel: AppMenuPanel) -> bool {
        let (settings_key, official_extension_id) = panel.visibility_gate();
        if panel.app_wide() {
            !shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get(settings_key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        } else {
            !self.titlebar_button_hidden(settings_key, official_extension_id)
        }
    }

    /// The same for a menu row's panel id. The sidebar's menu asks when it opens
    /// (`native_sidebar_more_menu_now`), since the menu itself is built without the switches.
    pub(crate) fn sidebar_menu_panel_offered(&self, panel_id: &str) -> bool {
        AppMenuPanel::from_id(panel_id).is_some_and(|panel| self.sidebar_menu_panel_shown(panel))
    }

    /// Where a panel of the sidebar's menu hangs from: the menu button, where it was last drawn.
    ///
    /// CDXC:Titlebar 2026-11-11 WHY:
    /// The button is drawn by the docked sidebar and by the sidebar that slides out while it is hidden, at the same place in the window, so a panel opened from either (the slid-out sidebar leaves once the pointer does) keeps the place its row was picked from. Only a window whose sidebar has not been drawn yet has no such place (the Ask Ghostex hotkey with the sidebar hidden since the window opened): the panel then opens at the window's top left, where the top row's first button would be, rather than waiting for a button that may never be drawn.
    pub(crate) fn sidebar_menu_panel_trigger_bounds(&self) -> Bounds<Pixels> {
        self.native_sidebar
            .more_button_bounds
            .get()
            .unwrap_or_else(|| Bounds {
                origin: gpui::point(px(SIDEBAR_TOP_EDGE_INSET), px(SIDEBAR_TOP_EDGE_INSET)),
                size: gpui::size(
                    px(SIDEBAR_TOP_ROW_BUTTON_WIDTH),
                    px(SIDEBAR_TOP_ROW_BUTTON_HEIGHT),
                ),
            })
    }

    /// Opens a menu row's panel. The gate is asked again because the row and the click are a
    /// frame apart.
    pub(crate) fn open_sidebar_menu_panel(
        &mut self,
        panel_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(panel) = AppMenuPanel::from_id(panel_id) else {
            return;
        };
        if !self.sidebar_menu_panel_shown(panel) {
            return;
        }
        let trigger_bounds = self.sidebar_menu_panel_trigger_bounds();
        self.set_gpui_titlebar_popup_open(
            panel.popup_kind(),
            true,
            Some(trigger_bounds),
            window,
            cx,
        );
    }
}
