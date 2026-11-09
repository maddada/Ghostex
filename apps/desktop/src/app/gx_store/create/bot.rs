//! A bot's "+", its Edit SOUL and Edit config, and the sync that brings the bot projects up to
//! date.
//!
//! The launch is the ordinary Hermes launch into the bot's project: gxserver swaps in the project's
//! own `hermes -p <profile>` command (server/src/bot_projects.rs). It skips only what belongs to the
//! agent launcher, which a bot has none of: the launcher's highlighted agent is left alone.

use ghostex_gx_core::MachineId;
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Asks gxserver to add a project for every Hermes profile that has none yet. The new rows
    /// arrive through the presentation like any added project, so nothing waits on the answer;
    /// gxserver itself refuses while Bots is switched off.
    pub(crate) fn gx_store_sync_bot_projects(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |_, _| {
            let _ = gx_rpc(None, "/api/syncBotProjects", json!({})).await;
        })
        .detach();
    }

    /// An `openBotFile` sidebar message: the bot's own project comes forward and the file opens
    /// in its Code view. The paths come from gx-core's bot row (sidebar_menu/header.rs).
    pub(super) fn gx_store_open_bot_file_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_string);
        let (Some(group_id), Some(project_path), Some(file_path)) =
            (text("groupId"), text("projectPath"), text("filePath"))
        else {
            return;
        };
        self.open_bot_file_in_code_view(group_id, project_path.into(), file_path.into(), cx);
    }

    /// The bot's name when this is a Hermes launch into a local bot, which gxserver runs as the
    /// bot (`bot_agent_config` in server/src/bot_projects.rs); `None` for any other agent.
    pub(super) fn gx_store_bot_name(&self, project_id: &str, agent_id: &str) -> Option<String> {
        let agent_id = agent_id.trim();
        if !agent_id.eq_ignore_ascii_case("hermes-agent") {
            return None;
        }
        let project = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)?
            .project(project_id)?;
        project
            .bot_profile
            .is_some()
            .then(|| project.title.trim().to_string())
            .filter(|name| !name.is_empty())
    }

    /// A `runSidebarBot` host message.
    pub(super) fn gx_store_run_sidebar_bot_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let group_id = message.get("groupId").and_then(Value::as_str);
        self.gx_store_request_agent_launch("hermes-agent", group_id, None, cx);
    }
}

impl GhostexGpuiApp {
    /// Makes a GBot (`/api/gbots` op `create`). Its row arrives under Chief of Staff through the
    /// presentation like any added project, so nothing waits on the answer but a failure, which
    /// shows as a toast with gxserver's sentence.
    pub(crate) fn gx_store_create_gbot(
        &mut self,
        name: String,
        title: String,
        instructions: String,
        agent: String,
        preset: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = json!({
            "op": "create",
            "name": name,
            "title": title,
            "soul": instructions,
            "agentId": agent,
        });
        // A preset fills the look, description and starter notes (server/src/gbots/team_file/).
        if let Some(preset) = preset {
            params["preset"] = json!(preset);
        }
        cx.spawn(async move |this, cx| {
            #[allow(unused_mut)]
            let mut created = gx_rpc(None, "/api/gbots", params.clone()).await;
            // Adding a GBot asks for the owner's approval (server/src/server/security_gates.rs):
            // Touch ID shows while the call is out; when it wasn't used the phone may still
            // approve, and the repeated call then adds it.
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(id) = created
                .as_ref()
                .err()
                .and_then(|error| error.approval_required())
                .and_then(|approval| approval["id"].as_str())
                .map(str::to_string)
            {
                created = match crate::app::window::collab_share::wait_for_approval(id.clone())
                    .await
                {
                    Ok(()) => {
                        let mut again = params;
                        again["approvalId"] = json!(id);
                        gx_rpc(None, "/api/gbots", again).await
                    }
                    Err(message) => Err(super::super::GxRpcError::transport("/api/gbots", message)),
                };
            }
            if let Err(error) = created {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_create_toast(
                        "warning",
                        "GBot not created",
                        Some(&error.message),
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}
