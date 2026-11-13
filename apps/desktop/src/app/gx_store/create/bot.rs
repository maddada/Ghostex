//! A bot's "+", its Edit SOUL and Edit config, New bot's create, and the sync that brings the bot
//! projects up to date.
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
    /// New bot's Create (`/api/createBot` on the computer the Bots list shows): Hermes makes the
    /// profile and gxserver adds its row. A refusal goes back to the dialog in Hermes's words; on
    /// success the dialog closes and the new bot's row becomes the selected one, with its Edit
    /// SOUL button there.
    pub(crate) fn gx_store_create_bot(
        &mut self,
        remote: Option<crate::app::model::GpuiRemoteGxserverRequestTarget>,
        machine_id: String,
        profile: String,
        clone_from: Option<String>,
        description: Option<String>,
        reply: crate::app::window::NewBotCreateReply,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut params = json!({ "profile": profile });
        if let Some(source) = clone_from {
            params["cloneFrom"] = json!(source);
        }
        if let Some(description) = description {
            params["description"] = json!(description);
        }
        cx.spawn(async move |this, cx| {
            let created = super::super::gx_rpc_with_timeout(
                remote,
                "/api/createBot",
                params,
                std::time::Duration::from_secs(90),
            )
            .await;
            let project_id = match created {
                Ok(answer) => answer["projectId"].as_str().map(str::to_string),
                Err(error) => {
                    let message = if error.unanswered {
                        "Ghostex couldn't reach that computer. Try again.".to_string()
                    } else {
                        error.message
                    };
                    let _ = this.update(cx, |this, cx| {
                        if !reply.finish(Err(message.clone()), cx) {
                            this.gx_store_create_toast(
                                "warning",
                                "Bot not created",
                                Some(&message),
                                cx,
                            );
                        }
                    });
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                if reply.finish(Ok(()), cx) {
                    this.release_native_app_modal_window(
                        crate::app::model::GpuiAppModalKind::NewBot,
                        cx,
                    );
                }
            });
            let Some(project_id) = project_id else {
                return;
            };
            let project = if machine_id == ghostex_gx_core::LOCAL_MACHINE_ID {
                ghostex_gx_core::ProjectKey::local(project_id)
            } else {
                ghostex_gx_core::ProjectKey::remote(machine_id, project_id)
            };
            // gxserver published the row before it answered; wait for it to reach the list (a
            // remote computer's comes over its own connection), then select it.
            for _ in 0..30 {
                let arrived = this
                    .update(cx, |this, _| {
                        this.gx_store
                            .core
                            .presentation()
                            .loaded(&project.machine)
                            .and_then(|loaded| loaded.project(&project.project_id))
                            .is_some()
                    })
                    .unwrap_or(false);
                if arrived {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
            }
            let _ = this.update(cx, |this, cx| this.gx_store_select_bot_row(project, cx));
        })
        .detach();
    }

    /// Makes a bot's row the selected one, as a click on its header does, without opening or
    /// starting a session (a remote group's `focusGroup` would open one, and a new bot has none).
    fn gx_store_select_bot_row(
        &mut self,
        project: ghostex_gx_core::ProjectKey,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(not(ghostex_web))]
        self.gx_store_focus_project(&project.to_workspace_project_id(), cx);
        // The browser build has no focus publisher; the store's own focus moves the list.
        #[cfg(ghostex_web)]
        {
            let output = self.gx_store.core.handle(
                ghostex_gx_core::Event::Intent(ghostex_gx_core::Intent::FocusProject { project }),
                super::super::host::now_ms(),
            );
            if !output.changes.is_empty() {
                self.gx_store.sidebar_list.note_changes(&output.changes);
                self.gx_store_update_sidebar_list(cx);
            }
        }
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
            #[cfg(not(ghostex_web))]
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
