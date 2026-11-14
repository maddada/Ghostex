//! Sidebar extensions in the native sidebar and the view panel: an extension's own sidebar mode
//! (a top-row button that swaps the list, like Bots), its sections in the Projects list, and its
//! side panel view, all drawn from the panel gxserver keeps (`extensionPanels`, gx-core
//! `extension_panel.rs` for the layout rules). One file per v1 block; `actions.rs` runs taps.
//!
//! CDXC:Extensions 2026-11-14 DECISION:
//! Q23 (option C, native blocks first): Ghostex draws an extension's panel with its own rows, the sidebar's menu builder (menus.rs) and the frosted tooltip host, so it looks like Ghostex in both themes; a panel picks no colours or fonts. A `{project}` or `{session}` item gets Ghostex's name, status dot, hover text and right-click menu, and a click opens it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/extension_panel.rs, server/src/extensions/panel.rs,
//! apps/gpui-web/src/app/native_sidebar/extension_panel (the web build's symlink).

mod actions;
mod buttons;
mod checklist;
mod empty;
mod icons;
mod map;
mod mode;
mod progress;
mod refs;
mod rows;
mod surface;
mod text;

use std::sync::Arc;

use ghostex_gx_core::extension_panel::{
    ExtensionPanelUiState, ExtensionSidebarView, extension_sidebar_view,
};
use ghostex_gx_core::{MachineId, SidebarMode};
use ghostex_gx_protocol::extension_panel::ExtensionPanels;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

/// Which document a drawn block belongs to, and what its taps may do.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PanelPlace {
    pub(crate) extension_id: String,
    /// `sidebar` or `sidePanel`.
    pub(crate) surface: &'static str,
    /// `<projectId>:<sessionId>` for a per-session side panel.
    pub(crate) session: Option<String>,
    pub(crate) revision: u64,
    pub(crate) granted_permissions: Arc<Vec<String>>,
}

impl PanelPlace {
    pub(crate) fn ui_key(&self) -> String {
        ExtensionPanelUiState::surface_key(&self.extension_id, self.surface)
    }

    /// What gxserver's panel calls take to name this document.
    pub(crate) fn wire(&self) -> Value {
        let mut place = json!({ "extension": self.extension_id, "surface": self.surface });
        if let Some(session) = &self.session {
            place["session"] = json!(session);
        }
        place
    }

    pub(crate) fn from_wire(value: &Value) -> Option<Self> {
        Some(Self {
            extension_id: value["extension"].as_str()?.to_string(),
            surface: match value["surface"].as_str()? {
                "sidePanel" => "sidePanel",
                _ => "sidebar",
            },
            session: value["session"].as_str().map(str::to_string),
            revision: value["revision"].as_u64().unwrap_or_default(),
            granted_permissions: Arc::new(
                value["granted"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
        })
    }

    /// `wire` plus the revision and grants, for menu commands that come back later.
    pub(crate) fn wire_with_grants(&self) -> Value {
        let mut place = self.wire();
        place["revision"] = json!(self.revision);
        place["granted"] = json!(*self.granted_permissions);
        place
    }
}

impl GhostexGpuiApp {
    /// This computer's sidebar extensions' panels, as the store last received them.
    pub(crate) fn native_extension_panels(&self) -> Option<&ExtensionPanels> {
        self.gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .and_then(|machine| machine.side_state().extension_panels.as_ref())
    }

    /// The sidebar's extension modes and sections for the mode and width on screen.
    pub(crate) fn native_extension_sidebar_view(&self) -> ExtensionSidebarView {
        extension_sidebar_view(
            self.native_extension_panels(),
            &self.gx_store_sidebar_mode(),
            &self.native_sidebar.extension_panel_ui,
            self.sidebar_width,
        )
    }

    /// Shows an extension's sidebar mode, or the projects again when it is already showing.
    pub(crate) fn toggle_native_sidebar_extension_mode(
        &mut self,
        extension_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let showing = self.gx_store_sidebar_mode().extension_id() == Some(extension_id);
        let mode = if showing {
            SidebarMode::Projects
        } else {
            SidebarMode::Extension(extension_id.to_string())
        };
        self.dispatch_native_sidebar_ui(
            json!({"type": "setSidebarMode", "mode": mode.as_wire()}),
            cx,
        );
    }
}
