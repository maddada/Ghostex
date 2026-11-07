//! The Session resume hooks card with a remote machine picked: its hook status, Install all and
//! Uninstall all go to that machine's gxserver (remote_conn/settings_rpc.rs), which installs the
//! hooks in its own agent CLIs' config files.
//!
//! CDXC:AgentHooks 2026-11-07 FIX:
//! Reporter (GitHub issue #431): "Settings > Agents > Machine: dell installs agent CLIs there, but Install all for Session resume hooks doesn't reach it". The card showed only with This computer picked, so a computer that runs only gxserver had no way to get its hooks; it now follows the Machine picker like the CLI rows above it.
use std::time::Duration;

use gpui::Context;
use serde_json::{Map, Value, json};

use super::super::super::store::store_machine_gxserver_rpc;
use super::AgentsTab;
use super::model::HookStatus;

/// A remote install probes every agent CLI there and writes their config files.
const REMOTE_HOOKS_TIMEOUT: Duration = Duration::from_secs(120);

impl AgentsTab {
    /// The picked machine's hook status, with a failed request as its error line.
    pub(super) fn remote_hook_status(&self) -> Option<HookStatus> {
        if let Some(error) = &self.remote_hooks_error {
            return Some(HookStatus {
                error_message: Some(error.clone()),
                ..HookStatus::default()
            });
        }
        self.remote_hooks.as_ref().map(HookStatus::parse)
    }

    /// Sends a hook call to the picked machine; `agent_ids: None` means every agent. An answer
    /// that comes back after another machine was picked is dropped.
    pub(super) fn remote_hook_call(
        &mut self,
        path: &'static str,
        agent_ids: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(machine) = self.shown_machine.clone() else {
            return;
        };
        let mut params = Map::new();
        if let Some(agent_ids) = agent_ids {
            params.insert("agentIds".into(), json!(agent_ids));
        }
        self.remote_hooks_loading = true;
        let weak = cx.entity().downgrade();
        let target = machine.clone();
        store_machine_gxserver_rpc(
            &self.store.clone(),
            Some(target.as_str()),
            path,
            Value::Object(params),
            REMOTE_HOOKS_TIMEOUT,
            move |result, cx| {
                let _ = weak.update(cx, |page, cx| {
                    if page.shown_machine.as_deref() != Some(machine.as_str()) {
                        return;
                    }
                    page.remote_hooks_loading = false;
                    match result {
                        Ok(status) => {
                            page.remote_hooks = Some(status);
                            page.remote_hooks_error = None;
                        }
                        Err(error) => page.remote_hooks_error = Some(error),
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }
}
