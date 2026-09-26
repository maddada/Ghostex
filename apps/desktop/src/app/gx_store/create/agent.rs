//! An agent launch from the sidebar's launcher, a project header, a machine tab's launcher, the New
//! Thread picker, a launch with an account and the Hooks Required dialog, performed in Rust
//! (`requestAgentSessionLaunch`, `confirmAgentHookLaunch`, `createAgentSession`,
//! `createQuickAgentSession` in the old runtime).
//!
//! The order is the runtime's: the agent's hooks are checked first (`/api/readAgentHookStatus`),
//! and hooks that are not installed open the Hooks Required dialog instead of launching; the
//! Chats collection launches into a new Quick project; a remote project launches on its machine,
//! which resolves the command; this computer creates a draft row with the agent's own command and
//! opens it, and the attach path starts the CLI.
//!
//! CDXC:AgentLauncher 2026-09-14 DECISION:
//! User: starting a new agent from Docs, Code, Browser, or any other non-Agents view must keep that view selected.
//! Both local and remote launches use keepView through native attachment so startup completion preserves the view too.
//!
//! CDXC:PlatformSupport 2026-08-11:
//! Windows agent creation and attachment must stay in the Rust-owned WSL gxserver path. Splitting
//! creation across two transports can address different backend state during WSL bootstrap and
//! leaves the project-header click with no materialized terminal. Only the bounded project and
//! agent ids plus the user's interface preference go to `receive_sidebar_create_project_agent_payload`,
//! which resolves the configured command, starts the provider, obtains its attach plan, and opens
//! the exact tab.
//!
//! SEE-ALSO: packages/gx-core/src/session_create/agents.rs and params.rs,
//! apps/desktop/src/app/gx_store/create/focus_created.rs.

use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_core::{
    CHATS_GROUP_ID, ProjectKey, SessionKey, TitleGenerationSettings, agent_session_default_title,
    created_session, default_agent_id_for_icon, first_prompt_title_runtime_settings,
    local_agent_launch_params, open_remote_session_terminal, remote_agent_launch_params,
    resolve_sidebar_agent, start_provider_params,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use super::terminal::REMOTE_TIMEOUT;
use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;
use crate::shared_settings;

/// `ghostex-sidebar-project-terminal-launcher` (packages/core-ui/primary-agent-launcher.ts), which
/// the project header's agent name and the launcher's highlighted row are read from.
const PRIMARY_AGENT_LAUNCHER_STORAGE_KEY: &str = "ghostex-sidebar-project-terminal-launcher";
/// `installAgentHooks` on a remote machine waited this long.
const REMOTE_HOOK_INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
/// `startRemoteAgentSessionAndSendPrompt`'s timeout.
const REMOTE_START_TIMEOUT: Duration = Duration::from_secs(15);

/// What a hook check or an install answered.
enum HookAnswer {
    Status(Value),
    Failed,
}

impl GhostexGpuiApp {
    /// The sidebar HUD the launch reads its agents from, the same document the launcher's menus
    /// are built from (runtime_facts.rs).
    pub(super) fn gx_store_launch_hud(&self) -> Option<Arc<Value>> {
        self.gx_store.runtime_facts.hud.clone()
    }

    pub(super) fn gx_store_title_generation_settings(&self) -> TitleGenerationSettings {
        let snapshot = shared_settings::shared_sidebar_settings_snapshot();
        TitleGenerationSettings::from_settings(&Value::Object(snapshot.object().clone()))
    }

    /// `resolveEffectivePreferredAgentInterface(settings, agentId)`.
    pub(crate) fn gx_store_preferred_interface(&self, agent_id: &str) -> String {
        self.gx_store_preferred_interface_settings()
            .resolve(Some(agent_id))
            .unwrap_or("terminal")
            .to_string()
    }

    /// A `runSidebarAgent` host message (the New Thread picker, the launcher's run, a launch with an
    /// account). The launched agent becomes the launcher's highlighted one first, as the host
    /// message door did before the handler ran.
    ///
    /// CDXC:AgentLauncher 2026-09-09 WHY:
    /// A launch forwarded from the New Thread picker must also become the sidebar's highlighted
    /// default agent, which is read from the launcher key in client storage.
    pub(super) fn gx_store_run_sidebar_agent_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(agent_id) = message.get("agentId").and_then(Value::as_str) else {
            return;
        };
        let agent_id = agent_id.to_string();
        let background = cx.background_executor().clone();
        let stored = agent_id.clone();
        cx.spawn(async move |this, cx| {
            let _ = background
                .spawn(async move {
                    super::super::write_client_document_value(
                        PRIMARY_AGENT_LAUNCHER_STORAGE_KEY,
                        Some(&stored),
                    )
                })
                .await;
            // The launcher key is one of the menu host's two values; a launch is the write that
            // command names, so the next install re-reads it.
            let _ = this.update(cx, |this, _| {
                this.gx_store_note_menu_host_write(
                    &json!({ "type": "projectAction", "action": "agent" }),
                );
            });
        })
        .detach();
        let group_id = message.get("groupId").and_then(Value::as_str);
        let account_id = message.get("accountId").and_then(Value::as_str);
        self.gx_store_request_agent_launch(&agent_id, group_id, account_id, cx);
    }

    /// `requestAgentSessionLaunch(agentId, groupId, accountId)`: the hook check, then the dialog or
    /// the launch.
    pub(crate) fn gx_store_request_agent_launch(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.create.counters.agent_launches += 1;
        let hud = self.gx_store_launch_hud();
        let normalized = agent_id.trim().to_string();
        let agent = resolve_sidebar_agent(hud.as_deref(), agent_id);
        let hook_agent_id = agent
            .as_ref()
            .and_then(|agent| default_agent_id_for_icon(agent.icon.as_deref()));
        let (Some(agent), Some(hook_agent_id)) = (agent, hook_agent_id) else {
            self.gx_store_launch_agent_from_sidebar(agent_id, group_id, account_id, cx);
            return;
        };
        if normalized.is_empty() || hook_agent_id == "zcode" {
            self.gx_store_launch_agent_from_sidebar(agent_id, group_id, account_id, cx);
            return;
        }
        let remote = group_id
            .and_then(ProjectKey::parse_sidebar_group_id)
            .and_then(|project| project.machine.remote_id().map(str::to_string));
        let params = json!({ "agentIds": [hook_agent_id] });
        let status = self.gx_store_hook_call(remote, "/api/readAgentHookStatus", params, None, cx);
        let agent_id = agent_id.to_string();
        let group_id = group_id.map(str::to_string);
        let account_id = account_id.map(str::to_string);
        cx.spawn(async move |this, cx| {
            let answer = status.await;
            let _ = this.update(cx, |this, cx| {
                let HookAnswer::Status(status) = answer else {
                    this.gx_store_create_toast(
                        "warning",
                        "Unable to check agent hooks",
                        Some(&format!(
                            "Ghostex could not verify {} hooks. Try opening the agent again.",
                            agent.name
                        )),
                        cx,
                    );
                    return;
                };
                let row_status = status
                    .get("agents")
                    .and_then(Value::as_array)
                    .and_then(|rows| {
                        rows.iter().find(|row| {
                            row.get("agentId").and_then(Value::as_str) == Some(hook_agent_id)
                        })
                    })
                    .map(|row| {
                        row.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    });
                if matches!(row_status, None | Some("installed") | Some("cliMissing")) {
                    this.gx_store_launch_agent_from_sidebar(
                        &agent_id,
                        group_id.as_deref(),
                        account_id.as_deref(),
                        cx,
                    );
                    return;
                }
                this.gx_store.create.counters.hook_dialogs += 1;
                let mut open = json!({
                    "agentId": normalized,
                    "agentName": agent.name,
                    "hookAgentId": hook_agent_id,
                    "modal": "agentHooksRequired",
                    "type": "open",
                });
                if let Some(group_id) = &group_id {
                    open["groupId"] = json!(group_id);
                }
                if let Some(account_id) = &account_id {
                    open["accountId"] = json!(account_id);
                }
                this.open_app_modal_from_bridge(open, cx);
            });
        })
        .detach();
    }

    /// `confirmAgentHookLaunch(message)`: the Hooks Required dialog's answer.
    pub(super) fn gx_store_confirm_agent_hook_launch(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_string);
        let (Some(agent_id), Some(hook_agent_id)) = (text("agentId"), text("hookAgentId")) else {
            return;
        };
        let group_id = text("groupId");
        let account_id = text("accountId");
        let hud = self.gx_store_launch_hud();
        let agent_name = resolve_sidebar_agent(hud.as_deref(), &agent_id)
            .map(|agent| agent.name)
            .unwrap_or_else(|| agent_id.clone());
        if message.get("installHooks").and_then(Value::as_bool) != Some(true) {
            self.gx_store_create_toast(
                "warning",
                &format!("Install hooks for {agent_name}"),
                Some(
                    "Install and approve the hooks in order for Chat View to work correctly. Resuming and working/done indicators also require hooks.",
                ),
                cx,
            );
            self.gx_store_launch_agent_from_sidebar(
                &agent_id,
                group_id.as_deref(),
                account_id.as_deref(),
                cx,
            );
            return;
        }
        let remote = group_id
            .as_deref()
            .and_then(ProjectKey::parse_sidebar_group_id)
            .and_then(|project| project.machine.remote_id().map(str::to_string));
        let install = self.gx_store_hook_call(
            remote,
            "/api/installAgentHooks",
            json!({ "agentIds": [hook_agent_id] }),
            Some(REMOTE_HOOK_INSTALL_TIMEOUT),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let answer = install.await;
            let _ = this.update(cx, |this, cx| {
                let title = format!("Could not install {agent_name} hooks");
                let HookAnswer::Status(result) = answer else {
                    this.gx_store_create_toast(
                        "error",
                        &title,
                        Some("Open Settings > Agents > Agent Hooks and try again."),
                        cx,
                    );
                    return;
                };
                let installed =
                    result
                        .get("agents")
                        .and_then(Value::as_array)
                        .is_some_and(|rows| {
                            rows.iter().any(|row| {
                                row.get("agentId").and_then(Value::as_str)
                                    == Some(hook_agent_id.as_str())
                                    && row.get("status").and_then(Value::as_str)
                                        == Some("installed")
                            })
                        });
                if !installed {
                    this.gx_store_create_toast(
                        "error",
                        &title,
                        Some("Open Settings > Agents > Agent Hooks to review the hook status."),
                        cx,
                    );
                    return;
                }
                this.gx_store_launch_agent_from_sidebar(
                    &agent_id,
                    group_id.as_deref(),
                    account_id.as_deref(),
                    cx,
                );
            });
        })
        .detach();
    }

    /// A hook status read or install, on this computer or down the project's machine tunnel.
    fn gx_store_hook_call(
        &mut self,
        remote: Option<String>,
        path: &'static str,
        params: Value,
        remote_timeout: Option<Duration>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<HookAnswer> {
        match remote {
            Some(machine_id) => {
                let task = self.start_gpui_remote_sidebar_rpc(
                    &machine_id,
                    path,
                    Some(params),
                    remote_timeout.unwrap_or(REMOTE_TIMEOUT),
                    GpuiRemoteSidebarRpcMode::Awaited,
                    cx,
                );
                cx.background_executor().spawn(async move {
                    match task.await {
                        Ok(value) => HookAnswer::Status(value),
                        Err(_) => HookAnswer::Failed,
                    }
                })
            }
            None => cx.background_executor().spawn(async move {
                match gx_rpc(None, path, params).await {
                    Ok(value) => HookAnswer::Status(value),
                    Err(_) => HookAnswer::Failed,
                }
            }),
        }
    }

    /// `createAgentSessionFromSidebarLaunch`: the Chats collection launches into a new Quick
    /// project; every other group into its own project.
    fn gx_store_launch_agent_from_sidebar(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        if group_id == Some(CHATS_GROUP_ID) {
            let agent_id = agent_id.to_string();
            let account_id = account_id.map(str::to_string);
            // CDXC:AgentLauncher 2026-07-11:
            // A Quick agent never launches inside the active code project: it gets a new
            // projectless chat workspace, then the same configured-agent launch path as project
            // headers.
            self.gx_store_create_quick_project("agent", cx, move |this, project_id, cx| {
                let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
                this.gx_store_create_agent_session(
                    &agent_id,
                    Some(&group_id),
                    account_id.as_deref(),
                    cx,
                )
                .detach();
            });
            return;
        }
        self.gx_store_create_agent_session(agent_id, group_id, account_id, cx)
            .detach();
    }

    /// `createAgentSession(agentId, groupId = this.activeGroupId, accountId)`.
    pub(super) fn gx_store_create_agent_session(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        let group_id = group_id
            .map(str::to_string)
            .or_else(|| self.gx_store_active_group_id());
        let remote = group_id
            .as_deref()
            .and_then(ProjectKey::parse_sidebar_group_id)
            .filter(|project| !project.machine.is_local());
        if let Some(project) = remote {
            self.gx_store_create_remote_agent_session(agent_id, project, account_id, cx);
            return gpui::Task::ready(Ok(()));
        }
        // `parseGxserverPresentationProjectGroupId(groupId)` when a group was given, the active
        // project only when none was: a user-made group's id names no project here, as it did.
        let project_id = match group_id.as_deref() {
            Some(group_id) => ProjectKey::parse_sidebar_group_id(group_id)
                .filter(|project| project.machine.is_local())
                .map(|project| project.project_id),
            None => self.gx_store_active_local_project_id(),
        };
        if let Some(project_id) = project_id.as_deref()
            && !self.gx_store_ensure_local_project_path_available(project_id, cx)
        {
            return gpui::Task::ready(Ok(()));
        }
        let normalized = agent_id.trim().to_string();
        if cfg!(target_os = "windows") {
            let Some(project_id) = project_id.filter(|_| !normalized.is_empty()) else {
                self.gx_store_create_toast("warning", "Agent unavailable", None, cx);
                return gpui::Task::ready(Ok(()));
            };
            let mut payload = json!({
                "agentId": normalized,
                "preferredInterface": self.gx_store_preferred_interface(&normalized),
                "projectId": project_id,
                "type": "ghostex.gpui.sidebar.createProjectAgent",
                "version": 1,
            });
            if let Some(account_id) = account_id {
                payload["accountId"] = json!(account_id);
            }
            self.receive_sidebar_create_project_agent_payload(&payload.to_string(), cx);
            return gpui::Task::ready(Ok(()));
        }
        let hud = self.gx_store_launch_hud();
        let (Some(project_id), Some(agent)) =
            (project_id, resolve_sidebar_agent(hud.as_deref(), agent_id))
        else {
            return gpui::Task::ready(Ok(()));
        };
        if agent.launch_command().is_none() {
            return gpui::Task::ready(Ok(()));
        }
        let title_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            None,
        );
        let title = agent_session_default_title(Some(&agent.name));
        let params =
            local_agent_launch_params(&agent, &project_id, title_settings, account_id, &title);
        let preferred_interface = self.gx_store_preferred_interface(&agent.agent_id);
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/createAgentSession", params).await;
            let failure = result.as_ref().err().and_then(|error| {
                (error.code.as_deref() != Some("projectPathUnavailable"))
                    .then(|| error.message.clone())
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(response) => {
                    let Some((created_project, session_id)) =
                        created_session(&response, Some(&project_id))
                    else {
                        return;
                    };
                    this.gx_store_focus_created_session(
                        created_project.as_deref().unwrap_or(&project_id),
                        &session_id,
                        true,
                        Some(&preferred_interface),
                        cx,
                    );
                }
                Err(error) => {
                    if error.code.as_deref() == Some("projectPathUnavailable") {
                        this.gx_store_answer_project_path_unavailable(&project_id, cx);
                    }
                }
            });
            failure.map_or(Ok(()), Err)
        })
    }

    /// The remote half of `createAgentSession`: the machine resolves the command, a Chat-first agent
    /// opens its chat through the native attach, a terminal agent starts its provider.
    fn gx_store_create_remote_agent_session(
        &mut self,
        agent_id: &str,
        project: ProjectKey,
        account_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(machine_id) = project.machine.remote_id().map(str::to_string) else {
            return;
        };
        let normalized = agent_id.trim().to_string();
        if normalized.is_empty() {
            self.gx_store_create_toast(
                "warning",
                "Remote agent unavailable",
                Some("Choose a configured agent for this remote project."),
                cx,
            );
            return;
        }
        let hud = self.gx_store_launch_hud();
        let agent_name = resolve_sidebar_agent(hud.as_deref(), &normalized)
            .map(|agent| agent.name)
            .unwrap_or_else(|| normalized.clone());
        let title = agent_session_default_title(Some(&agent_name));
        let title_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            None,
        );
        let params = remote_agent_launch_params(
            &normalized,
            &project.project_id,
            title_settings,
            account_id,
            &title,
        );
        let chat = self.gx_store_preferred_interface(&normalized) == "chat";
        let task = self.start_gpui_remote_sidebar_rpc(
            &machine_id,
            "/api/createAgentSession",
            Some(params),
            REMOTE_TIMEOUT,
            GpuiRemoteSidebarRpcMode::Awaited,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let Ok(response) = result else {
                    this.gx_store_create_toast(
                        "warning",
                        "Remote agent failed",
                        Some("The remote gxserver could not create that agent session."),
                        cx,
                    );
                    return;
                };
                let Some((created_project, session_id)) =
                    created_session(&response, Some(&project.project_id))
                else {
                    return;
                };
                let created_project = created_project.unwrap_or(project.project_id.clone());
                // `setRemotePresentationSessionFocus` put the session on screen through the tab
                // reconcile; the native open is that same attach, with the view kept.
                let session = SessionKey::remote(
                    machine_id.as_str(),
                    created_project.clone(),
                    session_id.clone(),
                );
                let payload = open_remote_session_terminal(
                    &session.to_sidebar_session_id(),
                    true,
                    chat.then_some("chat"),
                    false,
                );
                this.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
                if chat {
                    return;
                }
                let start = this.start_gpui_remote_sidebar_rpc(
                    &machine_id,
                    "/api/startSessionProvider",
                    Some(start_provider_params(&created_project, &session_id)),
                    REMOTE_START_TIMEOUT,
                    GpuiRemoteSidebarRpcMode::Awaited,
                    cx,
                );
                cx.spawn(async move |this, cx| {
                    if start.await.is_err() {
                        let _ = this.update(cx, |this, cx| {
                            this.gx_store_create_toast(
                                "warning",
                                "Remote agent failed",
                                Some("The remote gxserver could not start that agent session."),
                                cx,
                            );
                        });
                    }
                })
                .detach();
            });
        })
        .detach();
    }

    /// `createQuickProject(kind)`: a new projectless chat workspace, made active, then `then`.
    pub(super) fn gx_store_create_quick_project(
        &mut self,
        kind: &'static str,
        cx: &mut gpui::Context<Self>,
        then: impl FnOnce(&mut Self, &str, &mut gpui::Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/createQuickProject", json!({ "kind": kind })).await;
            let _ = this.update(cx, |this, cx| {
                let project_id = result.ok().and_then(|response| {
                    response
                        .get("project")
                        .and_then(|project| project.get("projectId"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
                let Some(project_id) = project_id else {
                    this.gx_store_create_toast(
                        "error",
                        "Quick action failed",
                        Some("Ghostex could not create the Quick workspace."),
                        cx,
                    );
                    return;
                };
                // `focusProjectId(project)` and the publish.
                this.dispatch_native_sidebar_command(
                    json!({
                        "type": "focusGroup",
                        "groupId": ProjectKey::local(project_id.as_str()).to_sidebar_group_id(),
                    }),
                    cx,
                );
                then(this, &project_id, cx);
            });
        })
        .detach();
    }
}
