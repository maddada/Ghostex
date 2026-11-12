//! The Active projects star in the Spaces row: its right-click menu and its Remove.
//!
//! CDXC:Spaces 2026-11-12 DECISION in packages/gx-core/src/sidebar_view/active_projects.rs: the
//! star leads the Spaces row (selectors.rs draws it as a Space tile that never folds into More,
//! takes no drag and no project drop), and its right-click menu offers Remove Active projects,
//! which turns `sidebarActiveProjects` off; Settings > Sidebar & Sessions > Spaces turns it back on.

use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// The star's right-click menu: New Space, then Remove Active projects with where to bring it back.
pub(crate) fn active_projects_menu() -> Value {
    json!([
        {"label": "Manage", "heading": true},
        {"label": "New Space", "icon": "plus", "command": {"type": "editSpace"}},
        {"separator": true},
        {
            "label": "Remove Active projects",
            "icon": "eyeOff",
            "detail": "Turn it back on in Settings > Sidebar & Sessions > Spaces",
            "command": {"type": "removeActiveProjects"},
        },
    ])
}

impl GhostexGpuiApp {
    /// Turns Active projects off the way Settings does.
    pub(crate) fn remove_native_active_projects(&mut self, cx: &mut gpui::Context<Self>) {
        // The page answers this with a toast: it saves no settings (apps/gpui-web web_host).
        let mut patch = serde_json::Map::new();
        patch.insert(
            ghostex_settings_catalog::spaces::ACTIVE_PROJECTS_KEY.to_string(),
            json!(false),
        );
        self.handle_gpui_app_modal_update_settings_patch_message(
            &json!({
                "patch": patch,
                "source": "sidebar:activeProjects",
                "type": "updateSettingsPatch",
            }),
            cx,
        );
    }
}
