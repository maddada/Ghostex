//! Hide Space and Unhide Space on a Space's menu (CDXC:Spaces 2026-11-21 in gx-core
//! sidebar_view/spaces.rs `Space::shown`): this computer's Spaces document with the Space's
//! `hidden` set, written like the Space editor's edits.
//!
//! CDXC:Spaces 2026-11-21 WHY:
//! The brief asked for a Show hidden button on the "Space hidden" toast, but a toast with a button
//! opens the centred notice modal (CDXC:AppModal 2026-11-08 DECISION in window/notice_modal.rs),
//! which would stop the user after every Hide; the toast names the menu row instead. Only this
//! computer's Spaces offer Hide, because Show Hidden (the way back) is offered on this computer's
//! list only (gx-core sidebar_menu/navigation.rs `more_menu`).

use ghostex_gx_core::{SpacesDocument, plan_space_hidden};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::window::{GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastLevel};

/// The menu command: `{"type": "setSpaceHidden", "spaceId": …, "hidden": true|false}`.
pub(crate) const SET_SPACE_HIDDEN_COMMAND: &str = "setSpaceHidden";

impl GhostexGpuiApp {
    /// Whether this computer's Space is hidden; `None` when the Space is not one of this
    /// computer's (Other, Active projects, another computer's tab), which gets no Hide row.
    pub(crate) fn gx_store_space_hidden(&self, space_id: &str) -> Option<bool> {
        if self.gx_store.sidebar_ui.selected_machine_id() != ghostex_gx_core::LOCAL_MACHINE_ID
            || !self.gx_store.spaces.sync.has_document()
        {
            return None;
        }
        self.gx_store
            .spaces
            .sync
            .document()
            .state
            .spaces
            .get(space_id)
            .map(|space| space.hidden)
    }

    /// Answers `setSpaceHidden`. Returns `true` when the command is one.
    pub(crate) fn gx_store_run_sidebar_space_hide(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some(SET_SPACE_HIDDEN_COMMAND) {
            return false;
        }
        let (Some(space_id), Some(hidden)) = (
            command.get("spaceId").and_then(Value::as_str),
            command.get("hidden").and_then(Value::as_bool),
        ) else {
            return true;
        };
        if self.gx_store_space_hidden(space_id).is_none() {
            return true;
        }
        let held: SpacesDocument = self.gx_store.spaces.sync.document().clone();
        let Some(document) = plan_space_hidden(&held, space_id, hidden) else {
            return true;
        };
        self.gx_document_edit::<SpacesDocument>(document, cx);
        // Hidden while Show Hidden is on, the Space stays on the row (dimmed), so there is nothing
        // to explain.
        if hidden && !self.gx_store.sidebar_list.last_inputs.ui.show_hidden {
            self.gx_store_show_space_hidden_toast(cx);
        }
        true
    }

    fn gx_store_show_space_hidden_toast(&mut self, cx: &mut gpui::Context<Self>) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-app-toast-space-hidden".to_string(),
                level: GpuiAppToastLevel::Info,
                title: "Space hidden".to_string(),
                description: Some(
                    "Turn on Show Hidden in the sidebar menu's Sort & Filter to see it again."
                        .to_string(),
                ),
                copy_text: None,
                action: None,
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }
}
