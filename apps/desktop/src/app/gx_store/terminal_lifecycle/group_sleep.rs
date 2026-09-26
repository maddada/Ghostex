//! A user-made session group's Sleep and Wake (the group menu), performed by the store: gx-core
//! names the members that can move and the bulk path sleeps them 350 ms apart or wakes them
//! together (packages/gx-core/src/sidebar_actions/group_sleep.rs).

use ghostex_gx_core::plan_group_sleep;
use serde_json::{Value, json};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Claims a wrapped `setGroupSleeping` on a user-made group. Returns whether it did.
    pub(crate) fn gx_store_run_group_sleep(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message") else {
            return false;
        };
        let Some((sleeping, session_ids)) = plan_group_sleep(&self.gx_store.core, message) else {
            return false;
        };
        if !session_ids.is_empty() {
            self.gx_store_run_sidebar_bulk(
                &json!({
                    "type": "command",
                    "message": {
                        "type": "setSessionsSleeping",
                        "sessionIds": session_ids,
                        "sleeping": sleeping,
                    },
                }),
                cx,
            );
        }
        true
    }
}
