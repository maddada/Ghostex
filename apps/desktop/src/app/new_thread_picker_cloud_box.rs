//! The New Thread picker's Cloud Box chip (D3 "into cloud box menu"): a launch with it, or with
//! This computer while the chip is offered, writes the project's remembered tray choice (Q1010)
//! before the chat opens, so the new chat's tray starts on Cloud Box or off it. The chat itself
//! starts here as a draft; its box starts with the first message, like any Cloud Box chat.
//! SEE-ALSO: apps/desktop/src/app/window/new_thread_picker_run_on.rs (the chip),
//! server/src/server/worktree_ops/draft_cloud_box.rs (the tray's memory and the start).

use std::time::Duration;

use ghostex_gx_protocol::cloud_box::{CLOUD_BOXES_ROUTE, CloudBoxRemember};
use serde_json::{Value, json};

use crate::app::helpers::*;
use crate::*;

/// The Cloud Box providers home gxserver last listed (`providers`), for the picker's rule.
static PROVIDERS: std::sync::Mutex<Vec<ghostex_gx_protocol::cloud_box::CloudProvider>> =
    std::sync::Mutex::new(Vec::new());

impl GhostexGpuiApp {
    /// Whether the picker offers the Cloud Box chip: Cloud Boxes is on and set up (gx-protocol
    /// `cloud_boxes_set_up`, CDXC:CloudBoxes 2026-11-04). The project list shows it only on a
    /// project on this computer.
    pub(crate) fn new_thread_picker_offers_cloud_box(&self) -> bool {
        if !shared_settings::shared_sidebar_settings_snapshot().cloud_boxes_enabled() {
            return false;
        }
        let providers = PROVIDERS
            .lock()
            .map(|providers| providers.clone())
            .unwrap_or_default();
        ghostex_gx_protocol::cloud_box::cloud_boxes_set_up(
            &providers,
            !self.gx_store.core.cloud_boxes().is_empty(),
        )
    }

    /// Reads which Cloud Box providers are connected, for the next picker (asked when this
    /// computer's stream goes live and when the picker opens; the answer is kept until the next).
    pub(crate) fn refresh_new_thread_picker_cloud_providers(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        if !shared_settings::shared_sidebar_settings_snapshot().cloud_boxes_enabled() {
            return;
        }
        cx.spawn(async move |_, _| {
            let Ok(answer) = crate::app::gx_store::gx_rpc(
                None,
                CLOUD_BOXES_ROUTE,
                json!({ "action": "providers" }),
            )
            .await
            else {
                return;
            };
            let providers = answer
                .get("providers")
                .cloned()
                .and_then(|providers| serde_json::from_value(providers).ok())
                .unwrap_or_default();
            if let Ok(mut cache) = PROVIDERS.lock() {
                *cache = providers;
            }
        })
        .detach();
    }

    /// Remembers Cloud Box (or not) for the picked project on this computer (`project_id`, else
    /// the active one), then launches the agent there.
    pub(crate) fn launch_agent_with_cloud_box_choice(
        &mut self,
        agent_id: String,
        account_id: Option<String>,
        cloud: bool,
        project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project_id) = project_id.or_else(|| {
            self.gx_store
                .core
                .focus()
                .active_project
                .as_ref()
                .filter(|project| project.machine.is_local())
                .map(|project| project.project_id.clone())
        }) else {
            self.launch_agent_in_active_project(agent_id, account_id, None, cx);
            return;
        };
        let wanted = serde_json::to_value(if cloud {
            CloudBoxRemember::Cloud
        } else {
            CloudBoxRemember::Local
        })
        .unwrap_or(Value::Null);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background
                .spawn(async move {
                    let read = json!({ "action": "projectSettings", "projectId": project_id });
                    let Ok(current) =
                        gpui_gxserver_rpc_result(CLOUD_BOXES_ROUTE, &read, Duration::from_secs(10))
                    else {
                        return;
                    };
                    let mut settings = current["settings"].clone();
                    if settings.get("remember") == Some(&wanted) || !settings.is_object() {
                        return;
                    }
                    settings["remember"] = wanted;
                    let write = json!({
                        "action": "projectSettings",
                        "projectId": project_id,
                        "settings": settings,
                    });
                    let _ = gpui_gxserver_rpc_result(
                        CLOUD_BOXES_ROUTE,
                        &write,
                        Duration::from_secs(10),
                    );
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.launch_agent_in_active_project(agent_id, account_id, None, cx);
            });
        })
        .detach();
    }
}
