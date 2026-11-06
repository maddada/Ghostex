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
        if !agent_id.eq_ignore_ascii_case("hermes-agent") && !agent_id.eq_ignore_ascii_case("gbots")
        {
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
        // A GBot's "+" opens a new conversation with it (server/src/gbots/), a Hermes bot's a
        // Hermes session.
        let gbot = group_id
            .and_then(|group_id| {
                self.native_sidebar
                    .snapshot
                    .as_ref()?
                    .groups
                    .iter()
                    .find(|group| group.group_id == group_id)
            })
            .is_some_and(|group| group.is_gbot());
        if gbot {
            if let Some(project) =
                group_id.and_then(ghostex_gx_core::ProjectKey::parse_sidebar_group_id)
            {
                self.gx_store_new_gbot_conversation(project.project_id, cx);
            }
            return;
        }
        self.gx_store_request_agent_launch("hermes-agent", group_id, None, cx);
    }
}

impl GhostexGpuiApp {
    /// Makes a GBot (`/api/gbots create`). Its row arrives through the presentation like any
    /// added project, so nothing waits on the answer but a failure, which shows as a toast.
    pub(crate) fn gx_store_create_gbot(
        &mut self,
        name: String,
        instructions: String,
        agent: String,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(
                None,
                "/api/gbots",
                json!({
                    "action": "create",
                    "name": name,
                    "instructions": instructions,
                    "agent": agent,
                }),
            )
            .await;
            if let Err(error) = created {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_create_toast(
                        "warning",
                        "Bot not created",
                        Some(&error.message),
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}

impl GhostexGpuiApp {
    /// A GBot's "+": gxserver starts the conversation and its session (`/api/gbots
    /// newConversation`), which opens in the chat. GBots is no launcher agent, so this is not the
    /// agent launch a Hermes bot's "+" takes.
    fn gx_store_new_gbot_conversation(&mut self, project_id: String, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(
                None,
                "/api/gbots",
                json!({ "action": "newConversation", "projectId": project_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| match created {
                Ok(created) => {
                    let text =
                        |key: &str| created.get(key).and_then(Value::as_str).map(str::to_string);
                    if let (Some(project_id), Some(session_id)) =
                        (text("projectId"), text("sessionId"))
                    {
                        this.gx_store_focus_created_session(
                            &project_id,
                            &session_id,
                            true,
                            Some("chat"),
                            cx,
                        );
                    }
                }
                Err(error) => this.gx_store_create_toast(
                    "warning",
                    "Conversation not started",
                    Some(&error.message),
                    cx,
                ),
            });
        })
        .detach();
    }
}
