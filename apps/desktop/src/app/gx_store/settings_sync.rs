//! The desktop's part in settings sync (server/src/settings_sync/): an Ungroup deletes the group on
//! every computer, a conflict this computer lost shows the "changed on X · Undo" toast, and Undo
//! puts this computer's value back everywhere.

use serde_json::{Value, json};

use super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::window::toast::{
    GPUI_APP_TOAST_DEFAULT_DURATION_MS, GpuiAppToast, GpuiAppToastAction, GpuiAppToastLevel,
};

impl GhostexGpuiApp {
    /// The user's Ungroup: the group goes on every computer in the sync group, its projects staying
    /// in its Space. Asked of the computer whose document the Ungroup edits.
    pub(crate) fn gx_store_delete_synced_group(
        &mut self,
        remote_machine_id: Option<&str>,
        collection_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = match remote_machine_id {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => return,
            },
            None => None,
        };
        let params = json!({ "collectionId": collection_id });
        cx.spawn(async move |_, _| {
            let _ = gx_rpc(target, "/api/deleteSyncedProjectGroup", params).await;
        })
        .detach();
    }

    /// A note from this computer's gxserver: a conflict this computer lost gets the toast with Undo
    /// (CDXC:SettingsSync 2026-11-06 in gx-core settings_sync); one another computer lost is only
    /// listed in Settings > Sync there. A first sync that merged only the Spaces named alike asks
    /// to name them the same (CDXC:Spaces 2026-11-06 in gx-core settings_sync/spaces.rs).
    pub(super) fn gx_store_show_settings_sync_note(
        &mut self,
        note: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.receive_native_settings_modal_payload(&json!({ "type": "settingsSyncChanged" }), cx);
        if note.get("kind").and_then(Value::as_str) == Some("spacesNamedApart") {
            self.dispatch_gpui_workspace_action_toast(
                "info",
                "Spaces from all your computers are one list now",
                "Spaces with the same name on two computers became one. Name a Space the same on each computer to keep them together.",
                cx,
            );
            return;
        }
        let text = |key: &str| note.get(key).and_then(Value::as_str).unwrap_or_default();
        if note.get("lostHere").and_then(Value::as_bool) != Some(true) || text("id").is_empty() {
            return;
        }
        self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: format!("gpui-app-toast-{}", self.app_toast_id_counter),
                level: GpuiAppToastLevel::Info,
                title: format!("Changed on {}", text("keptOn")),
                description: Some(text("text").to_string()),
                copy_text: None,
                action: Some(GpuiAppToastAction::UndoSyncedChange {
                    id: text("id").to_string(),
                }),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS * 2,
                epoch: 0,
            },
            cx,
        );
    }

    /// Undo on the toast or in Settings > Sync: this computer's value comes back, as a new edit, on
    /// every computer.
    pub(crate) fn gx_store_undo_settings_sync_change(
        &mut self,
        id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = json!({ "id": id });
        cx.spawn(async move |this, cx| {
            let undone = gx_rpc(None, "/api/undoSettingsSyncChange", params).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = undone {
                    this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Couldn't undo the change",
                        &error.to_string(),
                        cx,
                    );
                }
                this.receive_native_settings_modal_payload(
                    &json!({ "type": "settingsSyncChanged" }),
                    cx,
                );
            });
        })
        .detach();
    }
}
