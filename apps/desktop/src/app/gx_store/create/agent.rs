//! An agent launch from the sidebar's launcher, a project header, a machine tab's launcher, the New
//! Thread picker, a launch with an account and the Hooks Required dialog, performed in Rust
//! (`requestAgentSessionLaunch`, `confirmAgentHookLaunch` and `createAgentSession` in the old
//! runtime).
//!
//! The order is the runtime's: the agent's hooks are checked first (`/api/readAgentHookStatus`),
//! and hooks that are not installed open the Hooks Required dialog instead of launching; a remote
//! project launches on its machine, which resolves the command; this computer creates a draft row
//! with the agent's own command and opens it, and the attach path starts the CLI.
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

use crate::app::gx_store::unified_new_session::{PendingDraftMove, with_moved_draft_text};
use std::sync::Arc;
use std::time::Duration;

use ghostex_gx_core::{
    ProjectKey, SessionKey, TitleGenerationSettings, agent_session_default_title, created_session,
    default_agent_id_for_icon, first_prompt_title_runtime_settings, is_agentbox_run_location,
    local_agent_launch_params, open_remote_session_terminal, remote_agent_launch_params,
    remote_launch_agent_id, replacing_empty_sessions, resolve_sidebar_agent, start_provider_params,
    with_run_location,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use super::terminal::REMOTE_TIMEOUT;
use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_machine_name_from_settings;
use crate::app::remote_conn::sidebar_rpc::{
    GPUI_REMOTE_HOOKS_UPDATE_REQUIRED, GpuiRemoteSidebarRpcMode,
    gpui_remote_sidebar_rpc_failure_reason,
};
use crate::shared_settings;

/// `installAgentHooks` on a remote machine waited this long.
const REMOTE_HOOK_INSTALL_TIMEOUT: Duration = Duration::from_secs(120);
/// `startRemoteAgentSessionAndSendPrompt`'s timeout.
const REMOTE_START_TIMEOUT: Duration = Duration::from_secs(15);

/// What a hook check or an install answered.
enum HookAnswer {
    Status(Value),
    Failed(String),
}

fn hook_failure_message(error: &str, remote_name: Option<&str>) -> String {
    if error == GPUI_REMOTE_HOOKS_UPDATE_REQUIRED {
        return format!(
            "Update Ghostex on {}, then try again.",
            remote_name.unwrap_or("the remote computer")
        );
    }
    let message = error
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(180)
        .collect::<String>();
    if message.is_empty() || message == "Remote gxserver request failed." {
        "Could not reach the remote computer. Reconnect it and try again.".to_string()
    } else {
        message
    }
}

impl GhostexGpuiApp {
    /// The sidebar HUD the launch reads its agents from, the same document the launcher's menus
    /// are built from (runtime_facts.rs).
    pub(crate) fn gx_store_launch_hud(&self) -> Option<Arc<Value>> {
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
    ///
    /// CDXC:AgentLauncher 2026-10-15 WHY:
    /// The app's own field (New Agent Session and the New Thread picker's order) is set here with the key, so every launcher door updates both. Only the picker's own path set it before, so a launch from the project row's agent menu left Cmd+Shift+O starting the previous agent until the app restarted.
    pub(crate) fn gx_store_run_sidebar_agent_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(agent_id) = message.get("agentId").and_then(Value::as_str) else {
            return;
        };
        let agent_id = agent_id.to_string();
        // The pick is remembered for the project and the Space it was made in, and as the last
        // pick anywhere (gx-core's `agent_choice.rs`); the active project when no group is named.
        let place =
            self.gx_store_agent_choice_place(message.get("groupId").and_then(Value::as_str));
        let mut choices = super::super::read_agent_choices();
        choices.remember(&agent_id, &place);
        self.note_primary_agent_launcher(&agent_id, &choices);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            background
                .spawn(async move { super::super::write_agent_choices(&choices) })
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
        let run_location = message
            .get("runLocation")
            .and_then(Value::as_str)
            .filter(|location| is_agentbox_run_location(Some(location)));
        if let Some(run_location) = run_location {
            // CDXC:AgentBox 2026-10-01 WHY: a box session gets no Ghostex hooks (agentbox keeps the
            // host's hooks out of the box), so the Hooks Required check is skipped, and no account
            // goes with it: the box signs in on its own.
            self.gx_store.create.counters.agent_launches += 1;
            self.gx_store_launch_agent_from_sidebar(
                &agent_id,
                group_id,
                None,
                Some(run_location),
                None,
                cx,
            );
            return;
        }
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
        self.gx_store_request_agent_launch_with_draft(agent_id, group_id, account_id, None, cx);
    }

    /// [`Self::gx_store_request_agent_launch`] with text for the new agent's chat box (a sidebar
    /// extension's `newAgent` action). The draft rides this launch alone, through the Hooks
    /// Required dialog too, so a launch that stops early (the dialog closed, the agent missing)
    /// takes it with it instead of leaving it for a later launch.
    pub(crate) fn gx_store_request_agent_launch_with_draft(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        first_input_draft: Option<&str>,
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
            self.gx_store_launch_agent_from_sidebar(
                agent_id,
                group_id,
                account_id,
                None,
                first_input_draft,
                cx,
            );
            return;
        };
        if normalized.is_empty() || hook_agent_id == "zcode" {
            self.gx_store_launch_agent_from_sidebar(
                agent_id,
                group_id,
                account_id,
                None,
                first_input_draft,
                cx,
            );
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
        let first_input_draft = first_input_draft.map(str::to_string);
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
                // An agent whose Connect dialog was answered Not now starts without asking again
                // (CDXC:Onboarding 2026-11-11 in window/onboarding/agents.rs).
                let declined = agent_hooks_declined(hook_agent_id);
                // `notRequired`: Ghostex follows the agent without its hook
                // (CDXC:AgentHooks 2026-11-12 in server/src/agent_hooks/hook_free.rs).
                if declined
                    || matches!(
                        row_status,
                        None | Some("installed" | "cliMissing" | "notRequired")
                    )
                {
                    this.gx_store_launch_agent_from_sidebar(
                        &agent_id,
                        group_id.as_deref(),
                        account_id.as_deref(),
                        None,
                        first_input_draft.as_deref(),
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
                if let Some(draft) = &first_input_draft {
                    open["firstInputDraft"] = json!(draft);
                }
                this.open_app_modal_from_bridge(open, cx);
            });
        })
        .detach();
    }

    /// `confirmAgentHookLaunch(message)`: the Hooks Required dialog's answer.
    ///
    /// CDXC:AgentHooks 2026-11-02 DECISION: User: "make the toast say why it failed (timeout, the remote's error text, or which agent didn't install), short and in plain words."
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
        let first_input_draft = text("firstInputDraft");
        let hud = self.gx_store_launch_hud();
        let agent_name = resolve_sidebar_agent(hud.as_deref(), &agent_id)
            .map(|agent| agent.name)
            .unwrap_or_else(|| agent_id.clone());
        let install_hooks = message.get("installHooks").and_then(Value::as_bool) == Some(true);
        remember_agent_hooks_answer(&hook_agent_id, install_hooks);
        if !install_hooks {
            self.gx_store_create_toast(
                "warning",
                &format!("Install hooks for {agent_name}"),
                Some(HOOKS_DECLINED_DETAIL),
                cx,
            );
            self.gx_store_launch_agent_from_sidebar(
                &agent_id,
                group_id.as_deref(),
                account_id.as_deref(),
                None,
                first_input_draft.as_deref(),
                cx,
            );
            return;
        }
        let remote = group_id
            .as_deref()
            .and_then(ProjectKey::parse_sidebar_group_id)
            .and_then(|project| project.machine.remote_id().map(str::to_string));
        let remote_name = remote
            .as_deref()
            .and_then(gpui_remote_machine_name_from_settings);
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
                let result = match answer {
                    HookAnswer::Status(result) => result,
                    HookAnswer::Failed(error) => {
                        this.gx_store_create_toast(
                            "error",
                            &title,
                            Some(&hook_failure_message(&error, remote_name.as_deref())),
                            cx,
                        );
                        return;
                    }
                };
                let status = result
                    .get("agents")
                    .and_then(Value::as_array)
                    .and_then(|rows| {
                        rows.iter().find(|row| {
                            row.get("agentId").and_then(Value::as_str)
                                == Some(hook_agent_id.as_str())
                        })
                    })
                    .and_then(|row| row.get("status"))
                    .and_then(Value::as_str);
                if status != Some("installed") {
                    let computer = remote_name
                        .clone()
                        .unwrap_or_else(|| "this computer".to_string());
                    let reason = match status {
                        Some("cliMissing") => {
                            format!("Ghostex could not find the {agent_name} CLI on {computer}.")
                        }
                        Some("updateRequired") => {
                            format!("{agent_name} hooks still need an update on {computer}.")
                        }
                        _ => format!("{agent_name} hooks were not installed on {computer}."),
                    };
                    this.gx_store_create_toast("error", &title, Some(&reason), cx);
                    return;
                }
                this.gx_store_launch_agent_from_sidebar(
                    &agent_id,
                    group_id.as_deref(),
                    account_id.as_deref(),
                    None,
                    first_input_draft.as_deref(),
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
                        Err(error) => HookAnswer::Failed(error),
                    }
                })
            }
            None => cx.background_executor().spawn(async move {
                match gx_rpc(None, path, params).await {
                    Ok(value) => HookAnswer::Status(value),
                    Err(error) => HookAnswer::Failed(error.message),
                }
            }),
        }
    }

    /// `createAgentSessionFromSidebarLaunch`: every group launches into its own project.
    /// `run_location` names an agentbox box.
    fn gx_store_launch_agent_from_sidebar(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        run_location: Option<&str>,
        first_input_draft: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_create_agent_session_at(
            agent_id,
            group_id,
            account_id,
            run_location,
            first_input_draft,
            cx,
        )
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
        self.gx_store_create_agent_session_at(agent_id, group_id, account_id, None, None, cx)
    }

    /// `createAgentSession` on this computer or, with `run_location`, in an agentbox box: gxserver
    /// gets the location as `runLocation` and the session opens in the terminal, because a box
    /// keeps no transcript on this computer for Chat View to read.
    fn gx_store_create_agent_session_at(
        &mut self,
        agent_id: &str,
        group_id: Option<&str>,
        account_id: Option<&str>,
        run_location: Option<&str>,
        first_input_draft: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        let group_id = group_id
            .map(str::to_string)
            .or_else(|| self.gx_store_active_group_id());
        let remote = group_id
            .as_deref()
            .and_then(ProjectKey::parse_sidebar_group_id)
            .filter(|project| !project.machine.is_local());
        // The draft this launch replaces, when the tray's project chip asked for it.
        let draft_move = self.gx_store.unified_sidebar.pending_draft_move.take();
        if let Some(project) = remote {
            self.gx_store_create_remote_agent_session(
                agent_id,
                project,
                account_id,
                run_location,
                draft_move,
                cx,
            );
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
        // A Hermes session in a bot is named after the bot: gxserver launches the bot's own profile
        // in place of the HUD agent resolved here, whose name may be another bot's.
        let title_name = self
            .gx_store_bot_name(&project_id, &agent.agent_id)
            .unwrap_or_else(|| agent.name.clone());
        let title = agent_session_default_title(Some(&title_name));
        let mut params = with_moved_draft_text(
            replacing_empty_sessions(with_run_location(
                local_agent_launch_params(&agent, &project_id, title_settings, account_id, &title),
                run_location,
            )),
            &draft_move,
        );
        if let Some(draft) = first_input_draft.filter(|draft| !draft.trim().is_empty()) {
            params["runtimeSettings"]["firstUserInputDraft"] = json!(draft);
        }
        let preferred_interface = if is_agentbox_run_location(run_location) {
            "terminal".to_string()
        } else {
            self.gx_store_preferred_interface(&agent.agent_id)
        };
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
                    this.gx_store_finish_draft_move(draft_move, cx);
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
        run_location: Option<&str>,
        draft_move: Option<PendingDraftMove>,
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
        let launch_agent_id = remote_launch_agent_id(hud.as_deref(), &normalized);
        // A custom agent of this computer that names no built-in agent (by Works like or by its
        // command) has no command on the other computer: say so instead of sending an id it
        // refuses, since only its id may travel (CDXC:RemoteMachines 2026-06-24-17:19).
        if launch_agent_id.starts_with("custom-")
            && resolve_sidebar_agent(hud.as_deref(), &launch_agent_id).is_some()
        {
            self.gx_store_create_toast(
                "warning",
                "Remote agent unavailable",
                Some(&format!(
                    "{agent_name} is a custom command on this computer, so it can't start on another computer. Set its Works like to the agent it runs in Settings > Agents, or use one of the built-in agents."
                )),
                cx,
            );
            return;
        }
        let title = agent_session_default_title(Some(&agent_name));
        let title_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            None,
        );
        // The remote machine's own gxserver runs agentbox for a box launch there.
        let params = with_moved_draft_text(
            replacing_empty_sessions(with_run_location(
                remote_agent_launch_params(
                    &launch_agent_id,
                    &project.project_id,
                    title_settings,
                    account_id,
                    &title,
                ),
                run_location,
            )),
            &draft_move,
        );
        let chat = !is_agentbox_run_location(run_location)
            && self.gx_store_preferred_interface(&normalized) == "chat";
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
                let response = match result {
                    Ok(response) => response,
                    Err(error) => {
                        this.gx_store_create_toast(
                            "warning",
                            "Remote agent failed",
                            Some(&gpui_remote_sidebar_rpc_failure_reason(
                                &error,
                                "The remote gxserver could not create that agent session.",
                            )),
                            cx,
                        );
                        return;
                    }
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
                );
                this.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
                this.gx_store_finish_draft_move(draft_move, cx);
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
                    if let Err(error) = start.await {
                        let _ = this.update(cx, |this, cx| {
                            this.gx_store_create_toast(
                                "warning",
                                "Remote agent failed",
                                Some(&gpui_remote_sidebar_rpc_failure_reason(
                                    &error,
                                    "The remote gxserver could not start that agent session.",
                                )),
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
}

#[cfg(not(ghostex_web))]
const HOOKS_DECLINED_DETAIL: &str = "Ghostex won't ask again when it starts. Install them in Settings > Agents for Chat View, resuming and the working and done indicators.";
#[cfg(ghostex_web)]
const HOOKS_DECLINED_DETAIL: &str = "Install and approve the hooks in order for Chat View to work correctly. Resuming and working/done indicators also require hooks.";

/// Whether the Connect dialog for this agent was answered Not now on this computer. The web build
/// (apps/gpui-web compiles this file) keeps no such memory and always asks.
#[cfg(not(ghostex_web))]
fn agent_hooks_declined(hook_agent_id: &str) -> bool {
    crate::app::helpers::load_gpui_first_run_onboarding_state()
        .agent_hooks_declined
        .iter()
        .any(|id| id == hook_agent_id)
}

#[cfg(ghostex_web)]
fn agent_hooks_declined(_hook_agent_id: &str) -> bool {
    false
}

/// Not now in the Connect dialog is remembered for that agent, and installing forgets it.
#[cfg(not(ghostex_web))]
fn remember_agent_hooks_answer(hook_agent_id: &str, install_hooks: bool) {
    use crate::app::helpers::{
        load_gpui_first_run_onboarding_state, persist_gpui_first_run_onboarding_state,
    };
    let mut state = load_gpui_first_run_onboarding_state();
    let had = state
        .agent_hooks_declined
        .iter()
        .any(|id| id == hook_agent_id);
    if install_hooks == had {
        state.agent_hooks_declined.retain(|id| id != hook_agent_id);
        if !install_hooks {
            state.agent_hooks_declined.push(hook_agent_id.to_string());
        }
        persist_gpui_first_run_onboarding_state(&state);
    }
}

#[cfg(ghostex_web)]
fn remember_agent_hooks_answer(_hook_agent_id: &str, _install_hooks: bool) {}
