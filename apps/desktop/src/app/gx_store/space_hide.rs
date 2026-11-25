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
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::window::{GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastLevel};

/// The menu command: `{"type": "setSpaceHidden", "spaceId": …, "hidden": true|false}`.
pub(crate) const SET_SPACE_HIDDEN_COMMAND: &str = "setSpaceHidden";
/// Hide Here and Unhide Here on another workspace's Space shown in this window's row:
/// `{"type": "setSpaceHiddenHere", "spaceId": …, "hidden": true|false}`.
pub(crate) const SET_SPACE_HIDDEN_HERE_COMMAND: &str = "setSpaceHiddenHere";

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

    /// Whether another workspace's Space shown in this window's row is hidden from it with Hide
    /// Here (the window workspace's `hiddenSpaceIds`); `None` for every other Space, which gets
    /// Hide Space instead.
    ///
    /// CDXC:Workspaces 2026-11-26 DECISION:
    /// User (Q5014 B): one of Work's Spaces can be hidden from Personal's row alone, apart from
    /// Hide Space, which hides it in Work too. The list lives on the viewing workspace, so every
    /// window of it (and its phone list) follows.
    pub(crate) fn gx_store_space_hidden_here(&self, space_id: &str) -> Option<bool> {
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        if snapshot.selected_machine_id != ghostex_gx_core::LOCAL_MACHINE_ID {
            return None;
        }
        snapshot
            .spaces
            .iter()
            .find(|space| space.id == space_id)?
            .joined_workspace
            .as_ref()?;
        let state = self.gx_store_workspaces_state()?;
        let window = self.gx_store_resolved_window_workspace_id()?;
        Some(
            state
                .workspaces
                .get(&window)?
                .hidden_space_ids
                .iter()
                .any(|id| id == space_id),
        )
    }

    /// Answers `setSpaceHiddenHere`. Returns `true` when the command is one.
    pub(crate) fn gx_store_run_sidebar_space_hide_here(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some(SET_SPACE_HIDDEN_HERE_COMMAND) {
            return false;
        }
        let (Some(space_id), Some(hidden)) = (
            command.get("spaceId").and_then(Value::as_str),
            command.get("hidden").and_then(Value::as_bool),
        ) else {
            return true;
        };
        if self.gx_store_space_hidden_here(space_id) != Some(!hidden) {
            return true;
        }
        let (Some(state), Some(window)) = (
            self.gx_store_workspaces_state(),
            self.gx_store_resolved_window_workspace_id(),
        ) else {
            return true;
        };
        let Some(workspace) = state.workspaces.get(&window) else {
            return true;
        };
        let mut ids: Vec<String> = workspace
            .hidden_space_ids
            .iter()
            .filter(|id| id.as_str() != space_id)
            .cloned()
            .collect();
        if hidden {
            ids.push(space_id.to_string());
        }
        let params = json!({ "workspaceId": window, "hiddenSpaceIds": ids });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = super::rpc::gxserver_rpc_result_task(
                &background,
                "/api/updateWorkspace",
                params,
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't hide the Space",
                        &message,
                        cx,
                    );
                });
            }
        })
        .detach();
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
