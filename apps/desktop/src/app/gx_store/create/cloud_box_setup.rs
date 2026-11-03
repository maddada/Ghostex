//! "Set up cloud box" on Settings › Projects › Cloud box (Q1003, guided agent): a new chat in the
//! project with the setup prompt staged in its message box and the tray's Cloud Box segment on a
//! blank box (`/api/draftWorktree` `remember`, which keeps `runtimeSettings.cloudBox.blank` on the
//! draft). Nothing starts or bills until the user sends it; the first message starts the box like
//! any Cloud Box chat, and its box chip then offers Save as snapshot.
//!
//! CDXC:CloudBoxes 2026-11-02 WHY:
//! The prompt is staged, not sent, so the user sees what the agent will do on a machine that bills
//! them and can add to it (which branch, which services) before the box exists.

use ghostex_gx_core::{
    AgentRecordOptions, agent_record_params, created_session, first_prompt_title_runtime_settings,
    resolve_sidebar_agent,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
/// The agents a box runs (Claude and Codex logins go to boxes, D5 and D6).
const BOX_AGENTS: [&str; 2] = ["claude", "codex"];

const SETUP_PROMPT: &str = "Set this project up on this cloud box so new boxes start ready to work:\n\
1. The project is already cloned here. Install its toolchain and dependencies.\n\
2. Run its tests (or its build when it has no tests) and fix anything the box is missing.\n\
3. Write the steps that worked to .ghostex/cloud-setup.sh so they can run again.\n\
4. Run `gh auth login` with me (device code) so this box can fetch and push branches.\n\
Ask me before anything that costs money or changes the repository's remote. When everything works, \
tell me to use Save as snapshot on the box chip so new boxes start from here.";

impl GhostexGpuiApp {
    pub(crate) fn gx_store_create_cloud_box_setup_chat(
        &mut self,
        project_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let hud = self.gx_store_launch_hud();
        let preferred = hud
            .as_deref()
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| BOX_AGENTS.contains(id))
            .unwrap_or(DEFAULT_PROMPT_AGENT_ID)
            .to_string();
        let Some(agent) = resolve_sidebar_agent(hud.as_deref(), &preferred)
            .or_else(|| {
                BOX_AGENTS
                    .iter()
                    .find_map(|id| resolve_sidebar_agent(hud.as_deref(), id))
            })
            .filter(|agent| agent.launch_command().is_some())
        else {
            self.gx_store_create_toast(
                "warning",
                "Couldn't set up a cloud box",
                Some("Turn on Claude or Codex in Settings > Agents first."),
                cx,
            );
            return;
        };
        let runtime_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            Some(SETUP_PROMPT),
        );
        let params = agent_record_params(
            &agent,
            &project_id,
            "",
            runtime_settings,
            &AgentRecordOptions {
                draft: true,
                title: Some("Set up cloud box".to_string()),
                ..AgentRecordOptions::default()
            },
            &ghostex_gx_core::agent_session_default_title(Some(&agent.name)),
        );
        cx.spawn(async move |this, cx| {
            let created = gx_rpc(None, "/api/createAgentSession", params).await;
            let created = match created {
                Ok(response) => created_session(&response, Some(&project_id)),
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.gx_store_create_toast(
                            "error",
                            "Couldn't set up a cloud box",
                            Some(&error.message),
                            cx,
                        )
                    });
                    return;
                }
            };
            let Some((_, session_id)) = created else {
                return;
            };
            // The tray opens on Cloud Box, blank: the box clones the project and the agent sets it up.
            let _ = gx_rpc(
                None,
                "/api/draftWorktree",
                json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "remember": { "mode": "cloudBox", "cloudBox": { "blank": true } },
                }),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store_focus_created_session(
                    &project_id,
                    &session_id,
                    false,
                    Some("chat"),
                    cx,
                );
                this.gx_store_create_toast(
                    "info",
                    "Check the setup prompt, then press Enter to start the cloud box.",
                    None,
                    cx,
                );
            });
        })
        .detach();
    }
}
