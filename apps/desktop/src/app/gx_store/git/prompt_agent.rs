//! Starts the prompt agent a Git or worktree workflow hands its work to, on this computer or on a
//! remote machine, and selects it. Ported from the old runtime's `createAgentSessionForProject`,
//! `createAgentSessionRecordForProject` and `createRemoteAgentSessionForProject`.
//!
//! CDXC:Git 2026-07-11-06:14:
//! Match macOS `runSidebarGitPromptAction` + `stageNativeAgentPrompt`: create Git helpers as fresh
//! neutral agent sessions, start the provider, then submit the provider-specific title command and
//! only then the workflow prompt. Persisting `Git: Release` before startup makes the
//! missing-provider attach path treat a brand-new row as a trusted resume title; a failed lookup
//! then leaves the workflow prompt in a plain shell.
//!
//! SEE-ALSO: the create-and-start work of family F4 (gx_store/create/, history in
//! docs/2026-09-25/app-runtime-port/PLAN.md); when it lands as one gxserver call, these legs
//! become that call.

use ghostex_gx_core::SessionKey;
use ghostex_gx_core::git_menu::DEFAULT_PROMPT_AGENT_ID;
use serde_json::{Map, Value, json};

use super::calls::{self, Remote};
use super::scope::GitScope;
use crate::GhostexGpuiApp;
use crate::app::gx_store::GxRpcError;

/// A configured agent from the sidebar HUD.
#[derive(Clone, Debug, Default)]
pub(crate) struct HudAgent {
    pub(crate) agent_id: String,
    pub(crate) command: Option<String>,
    pub(crate) icon: Option<String>,
}

impl HudAgent {
    pub(crate) fn has_command(&self) -> bool {
        self.command
            .as_deref()
            .is_some_and(|command| !command.trim().is_empty())
    }
}

/// What a started agent is for.
#[derive(Clone, Debug, Default)]
pub(crate) struct PromptAgentLaunch {
    pub(crate) prompt: String,
    /// The title the session is renamed to once its provider runs (local), or created with
    /// (remote), when it is not the agent's default.
    pub(crate) title: Option<String>,
    /// A staged composer draft instead of a prompt (Handoff / Export's follow-up conversation).
    pub(crate) first_user_input_draft: Option<String>,
    pub(crate) agent_model: Option<String>,
    pub(crate) agent_effort: Option<String>,
    pub(crate) open_in_chat: bool,
    /// What the failure says when gxserver creates no session.
    pub(crate) missing_session_message: Option<&'static str>,
}

/// `gpuiWorkspaceTerminalTitleCommandForAgent`.
fn title_command_for_agent(agent_id: &str) -> &'static str {
    match agent_id.trim().to_lowercase().as_str() {
        "pi" | "π" => "name",
        "hermes" | "hermes agent" | "hermes-agent" => "title",
        _ => "rename",
    }
}

const TITLE_GENERATION_AGENTS: [&str; 7] = [
    "codex",
    "cursor",
    "claude",
    "grok",
    "pi",
    "antigravity",
    "custom",
];

impl GhostexGpuiApp {
    /// The agents the HUD lists.
    pub(crate) fn git_hud_agents(&self) -> Vec<HudAgent> {
        let text =
            |agent: &Value, key: &str| agent.get(key).and_then(Value::as_str).map(str::to_string);
        self.gx_store
            .runtime_facts
            .hud()
            .and_then(|hud| hud.get("agents"))
            .and_then(Value::as_array)
            .map(|agents| {
                agents
                    .iter()
                    .filter_map(|agent| {
                        Some(HudAgent {
                            agent_id: text(agent, "agentId")?,
                            command: text(agent, "command"),
                            icon: text(agent, "icon"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn git_hud_agent(&self, agent_id: &str) -> Option<HudAgent> {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return None;
        }
        self.git_hud_agents()
            .into_iter()
            .find(|agent| agent.agent_id == agent_id)
    }

    /// `resolveDefaultPromptAgentId`: the dialog's pick, else the Default Prompt Agent setting,
    /// else Codex.
    pub(crate) fn git_default_prompt_agent_id(&self, requested: Option<&str>) -> String {
        if let Some(requested) = requested.map(str::trim).filter(|id| !id.is_empty()) {
            return requested.to_string();
        }
        self.gx_store
            .runtime_facts
            .hud()
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| DEFAULT_PROMPT_AGENT_ID.to_string())
    }

    /// `createFirstPromptTitleRuntimeSettings`.
    ///
    /// CDXC:SessionTitles 2026-07-04-21:52:
    /// Agent sessions carry the gxserver-owned first-prompt title settings before hooks claim the
    /// prompt. The daemon owns eligibility, title generation and command submission; the app only
    /// supplies the saved title-generation agent or command and any already-known first prompt.
    fn git_first_prompt_title_runtime_settings(
        &self,
        first_user_message: Option<&str>,
        first_user_input_draft: Option<&str>,
    ) -> Value {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let object = settings.object();
        let agent = object
            .get("sessionTitleGenerationAgent")
            .and_then(Value::as_str)
            .filter(|agent| TITLE_GENERATION_AGENTS.contains(agent))
            .unwrap_or("codex")
            .to_string();
        let command = match agent.as_str() {
            "custom" => object
                .get("customSessionTitleGenerationCommand")
                .and_then(Value::as_str)
                .map(|command| command.trim().chars().take(240).collect::<String>())
                .filter(|command| !command.is_empty()),
            _ => self
                .git_hud_agent(&agent)
                .and_then(|agent| agent.command)
                .map(|command| command.trim().to_string())
                .filter(|command| !command.is_empty()),
        };
        let mut runtime = Map::new();
        runtime.insert("firstPromptTitleGenerationAgent".into(), json!(agent));
        if let Some(command) = command {
            runtime.insert("firstPromptTitleGenerationCommand".into(), json!(command));
        }
        if let Some(prompt) = first_user_message
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            runtime.insert("firstUserMessage".into(), json!(prompt));
        }
        // CDXC:TranscriptExport 2026-08-20: a draft travels byte for byte; the trailing space of
        // `@<path> ` closes the file mention, so it is not trimmed.
        if let Some(draft) = first_user_input_draft.filter(|text| !text.is_empty()) {
            runtime.insert("firstUserInputDraft".into(), json!(draft));
        }
        Value::Object(runtime)
    }

    /// Creates `agent`'s session in `scope`'s project, starts it with the launch's prompt, and
    /// selects it. The task answers the created session, or the failure the caller toasts.
    pub(crate) fn git_start_prompt_agent(
        &mut self,
        scope: &GitScope,
        agent: HudAgent,
        launch: PromptAgentLaunch,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<SessionKey, String>> {
        let has_prompt = !launch.prompt.trim().is_empty();
        let rename_title = if scope.is_remote() {
            None
        } else {
            launch.title.clone()
        };
        let mut params = Map::new();
        if let Some(effort) = &launch.agent_effort {
            params.insert("agentEffort".into(), json!(effort));
        }
        params.insert("agentId".into(), json!(agent.agent_id));
        if let Some(model) = &launch.agent_model {
            params.insert("agentModel".into(), json!(model));
        }
        // CDXC:Drafts 2026-09-23 WHY: an initial prompt is still unsent until the durable startup
        // queue delivers it, so the new session is a draft while its input box starts.
        if has_prompt || launch.first_user_input_draft.is_some() {
            params.insert("draft".into(), json!(true));
        }
        params.insert("projectId".into(), json!(scope.project_id()));
        // A local workflow renamed after start carries no first message: the rename is what names
        // it, not the prompt.
        let first_message = match rename_title.is_some() {
            true => None,
            false => Some(launch.prompt.as_str()),
        };
        params.insert(
            "runtimeSettings".into(),
            self.git_first_prompt_title_runtime_settings(
                first_message,
                launch.first_user_input_draft.as_deref(),
            ),
        );
        params.insert("surface".into(), json!("workspace"));
        if scope.is_remote() {
            params.insert("requireLaunchCommand".into(), json!(true));
            if let Some(title) = &launch.title {
                params.insert("title".into(), json!(title));
            }
        } else {
            let mut launch_settings = Map::new();
            launch_settings.insert(
                "agentCommand".into(),
                json!(agent.command.clone().unwrap_or_default()),
            );
            if let Some(icon) = &agent.icon {
                launch_settings.insert("icon".into(), json!(icon));
            }
            params.insert("launchSettings".into(), Value::Object(launch_settings));
        }
        let remote = scope.remote.clone();
        let key = scope.key.clone();
        let rename_command = rename_title
            .map(|title| format!("/{} {title}", title_command_for_agent(&agent.agent_id)));
        let open_in_chat = launch.open_in_chat;
        let prompt = launch.prompt;
        let missing_session = launch
            .missing_session_message
            .unwrap_or("Could not create an agent session in the worktree.");
        cx.spawn(async move |this, cx| {
            let created = calls::call(
                remote.clone(),
                "/api/createAgentSession",
                Value::Object(params),
                None,
            )
            .await
            .map_err(|error| error.message)?;
            let session_id = created
                .pointer("/session/sessionId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .ok_or_else(|| missing_session.to_string())?;
            let project_id = created
                .pointer("/session/projectId")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| key.project_id.clone());
            let session = SessionKey {
                machine: key.machine.clone(),
                project_id,
                session_id,
            };
            if key.machine.is_local() {
                let focused = session.clone();
                let _ = this.update(cx, |this, cx| {
                    this.git_select_created_session(&focused, open_in_chat, cx);
                });
                // A promptless local create (Handoff / Export's follow-up) leaves the provider to
                // the attach, as the old runtime did.
                if !prompt.trim().is_empty() || rename_command.is_some() {
                    start_and_send(remote, &session, rename_command, &prompt)
                        .await
                        .map_err(|error| error.message)?;
                }
            } else {
                let started = start_and_send(remote, &session, None, &prompt).await;
                let focused = session.clone();
                let _ = this.update(cx, |this, cx| {
                    if started.is_err() {
                        this.git_toast(
                            ghostex_gx_core::git_menu::GitToastLevel::Warning,
                            "Remote agent prompt failed",
                            super::toasts::ToastOptions::described(
                                "The remote gxserver could not start that agent session or deliver its prompt.",
                            ),
                            cx,
                        );
                    }
                    this.git_select_created_session(&focused, open_in_chat, cx);
                });
            }
            Ok(session)
        })
    }

    /// Selects a session a workflow just created, the way a fork selects its new session.
    fn git_select_created_session(
        &mut self,
        session: &SessionKey,
        open_in_chat: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        match session.machine.remote_id() {
            // A create's own open: the attach counts as the create's and the store's focus holds
            // the new session until its row arrives (create/focus_created.rs).
            None => self.gx_store_focus_created_session(
                &session.project_id,
                &session.session_id,
                false,
                Some(match open_in_chat {
                    true => "chat",
                    false => "terminal",
                }),
                cx,
            ),
            Some(_) => {
                // The same open a remote row's click performs, through the same entry point.
                let payload = ghostex_gx_core::open_remote_session_terminal(
                    &session.to_sidebar_session_id(),
                    false,
                    open_in_chat.then_some("chat"),
                    false,
                );
                self.receive_sidebar_native_project_path_action_payload(&payload.to_string(), cx);
            }
        }
    }
}

/// `startAgentSessionProviderAndSendPrompt`: start the provider, then queue the title command and
/// the prompt, each of which gxserver must acknowledge with a receipt.
async fn start_and_send(
    remote: Remote,
    session: &SessionKey,
    rename_command: Option<String>,
    prompt: &str,
) -> Result<(), GxRpcError> {
    let prompt = prompt.trim();
    calls::call(
        remote.clone(),
        "/api/startSessionProvider",
        json!({ "projectId": session.project_id, "sessionId": session.session_id }),
        None,
    )
    .await?;
    let texts: Vec<String> = rename_command
        .into_iter()
        .chain((!prompt.is_empty()).then(|| prompt.to_string()))
        .collect();
    for text in texts {
        let receipt = calls::call(
            remote.clone(),
            "/api/queueSessionChatPrompt",
            json!({
                "projectId": session.project_id,
                "sessionId": session.session_id,
                "startupSend": true,
                "text": text,
            }),
            None,
        )
        .await?;
        let queued = receipt
            .pointer("/prompt/id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.trim().is_empty());
        if !queued {
            return Err(GxRpcError::transport(
                "/api/queueSessionChatPrompt",
                "gxserver did not confirm the startup prompt was queued. Inspect this session before retrying.",
            ));
        }
    }
    Ok(())
}
