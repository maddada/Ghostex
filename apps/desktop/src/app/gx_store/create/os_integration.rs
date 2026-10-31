//! The OS integration commands: a script's Run and Find's resume, the Finder or Services "New
//! Terminal", `ghostex://open` and Open With, and the Help menu's questions, performed in Rust
//! (`handleGpuiOsIntegrationCommand`, `createOsIntegrationTerminal`, `openOsIntegrationProjectPaths`,
//! `createGhostexHelpChat` in the old runtime).
//!
//! Rust already owned the URL and file parsing, the script Run/Edit/Cancel consent dialog, the
//! existence checks and the git-root resolution; the runtime registered the daemon projects and
//! created and focused the sessions through the reviewed paths. Both halves are here now, with the
//! runtime's calls (`/api/addProjectPath`, `/api/createSession` or `/api/createAgentSession`) and
//! toasts.
//!
//! Terminal parity note: macOS creates a client-side projectless Quick project per
//! invocation; GPUI's sidebar is daemon-derived, so the terminal lands in the daemon project
//! registered (or reused) at the resolved cwd. A provided command launches the session with it
//! (the Search-by-Text `gx f` launcher contract) instead of macOS's typed `command\r` into a shell.
//!
//! SEE-ALSO: apps/desktop/src/app/sidebar_dispatch/folder_pickers_and_os_integration.rs (`dispatch_gpui_os_integration_command_message`),
//! apps/desktop/src/app/titlebar/help_menu.rs.

use ghostex_gx_core::{
    AgentRecordOptions, ProjectKey, agent_record_params, check_startup_prompt_receipt,
    created_session, first_prompt_title_runtime_settings, normalize_project_path,
    os_integration_command_params, project_name_from_path, queue_startup_prompt_params,
    resolve_sidebar_agent, start_provider_params, terminal_create_params,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

/// `DEFAULT_GPUI_PROMPT_AGENT_ID`.
const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
/// Most folders one `openProjectPaths` registers.
const MAX_OPENED_PATHS: usize = 16;

impl GhostexGpuiApp {
    /// `handleGpuiOsIntegrationCommand(payload)`. Returns whether the payload was an object with an
    /// action, which is what the runtime answered at all.
    pub(crate) fn gx_store_run_os_integration_command(
        &mut self,
        payload: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let text = |key: &str| {
            payload
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let Some(action) = text("action") else {
            return false;
        };
        self.gx_store.create.counters.os_integration_commands += 1;
        match action.as_str() {
            "createQuickTerminal" => {
                self.gx_store_create_os_integration_terminal(
                    text("command"),
                    text("cwd"),
                    text("title"),
                    cx,
                );
            }
            "openProjectPaths" => {
                let entries = payload
                    .get("projects")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                self.gx_store_open_os_integration_project_paths(entries, cx);
            }
            "createGhostexHelpChat" => {
                let question = payload
                    .get("question")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.gx_store_create_ghostex_help_chat(question, text("projectPath"), cx);
            }
            "createAgentboxSetupChat" => {
                self.gx_store_create_agentbox_setup_chat(text("projectPath"), cx);
            }
            _ => self.gx_store_create_toast(
                "warning",
                "Unsupported OS integration action.",
                None,
                cx,
            ),
        }
        true
    }

    /// `createOsIntegrationTerminal({ command, cwd, title })`.
    fn gx_store_create_os_integration_terminal(
        &mut self,
        command: Option<String>,
        cwd: Option<String>,
        title: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(cwd) = cwd else {
            self.gx_store_create_toast(
                "warning",
                "Open Terminal failed",
                Some("Opening a terminal needs the local gxserver."),
                cx,
            );
            return;
        };
        let name = project_name_from_path(&cwd);
        // A folder that is already a project stays in its own workspace; a new one joins this
        // window's (`gx_store_window_for_existing_project`).
        let existing = self.gx_store_local_project_at_path(&cwd).is_some();
        let add_params = json!({ "name": name, "path": cwd });
        let add_params = if existing {
            add_params
        } else {
            self.gx_store_with_window_workspace(add_params)
        };
        cx.spawn(async move |this, cx| {
            let registered = gx_rpc(None, "/api/addProjectPath", add_params).await;
            let Some(project_id) = registered.ok().as_ref().and_then(project_id_of) else {
                let _ = this.update(cx, |this, cx| this.gx_store_open_terminal_failed(cx));
                return;
            };
            // The window that shows the project's workspace runs the rest.
            let Ok(this) = this.update(cx, |this, cx| {
                if existing {
                    this.gx_store_window_for_existing_project(&project_id, cx)
                } else {
                    cx.entity().downgrade()
                }
            }) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_focus_project_group(&project_id, cx)
            });
            let title = title
                .or_else(|| Some(name.clone()).filter(|name| !name.trim().is_empty()))
                .unwrap_or_else(|| ghostex_gx_core::DEFAULT_TERMINAL_SESSION_TITLE.to_string());
            // A command may start an agent (Find's resume), whose view
            // gxserver's metadata decides; a plain shell has none to follow.
            let view = command.is_none().then_some("terminal");
            let created = match command {
                Some(command) => {
                    gx_rpc(
                        None,
                        "/api/createAgentSession",
                        os_integration_command_params(&command, &project_id, &title),
                    )
                    .await
                }
                None => {
                    gx_rpc(
                        None,
                        "/api/createSession",
                        terminal_create_params(Some(&project_id), Some(&title)),
                    )
                    .await
                }
            };
            let _ = this.update(cx, |this, cx| match created {
                Ok(response) => {
                    if let Some((created_project, session_id)) =
                        created_session(&response, Some(&project_id))
                    {
                        this.gx_store_focus_created_session(
                            created_project.as_deref().unwrap_or(&project_id),
                            &session_id,
                            false,
                            view,
                            cx,
                        );
                    }
                }
                Err(_) => this.gx_store_open_terminal_failed(cx),
            });
        })
        .detach();
    }

    fn gx_store_open_terminal_failed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_create_toast(
            "error",
            "Open Terminal failed",
            Some("gxserver could not create the requested terminal."),
            cx,
        );
    }

    /// `openOsIntegrationProjectPaths(entries)`: every folder registered in order, the last one
    /// made active.
    fn gx_store_open_os_integration_project_paths(
        &mut self,
        entries: Vec<Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let paths: Vec<String> = entries
            .iter()
            .take(MAX_OPENED_PATHS)
            .filter_map(|entry| entry.get("path").and_then(Value::as_str))
            .filter(|path| !path.trim().is_empty())
            .map(str::to_string)
            .collect();
        // A folder that is already a project stays in its own workspace and is shown there; a new
        // one joins this window's workspace (`gx_store_window_for_existing_project`).
        let opens: Vec<(Value, bool)> = paths
            .into_iter()
            .map(|path| {
                let existing = self.gx_store_local_project_at_path(&path).is_some();
                let params = json!({ "name": project_name_from_path(&path), "path": path });
                if existing {
                    (params, true)
                } else {
                    (self.gx_store_with_window_workspace(params), false)
                }
            })
            .collect();
        cx.spawn(async move |this, cx| {
            let mut focus_project = None;
            let mut failed = 0usize;
            for (params, existing) in opens {
                let result = gx_rpc(None, "/api/addProjectPath", params).await;
                match result.ok().as_ref().and_then(project_id_of) {
                    Some(project_id) => focus_project = Some((project_id, existing)),
                    None => failed += 1,
                }
            }
            let target = this.update(cx, |this, cx| {
                if failed > 0 {
                    this.gx_store_create_toast(
                        "error",
                        "Open failed",
                        Some("gxserver could not open a requested folder as a project."),
                        cx,
                    );
                }
                match &focus_project {
                    Some((project_id, true)) => {
                        Some(this.gx_store_window_for_existing_project(project_id, cx))
                    }
                    Some((_, false)) => Some(cx.entity().downgrade()),
                    None => None,
                }
            });
            if let (Ok(Some(target)), Some((project_id, _))) = (target, focus_project) {
                let _ = target.update(cx, |this, cx| {
                    this.gx_store_focus_project_group(&project_id, cx)
                });
            }
        })
        .detach();
    }

    /// `focusProjectId(projectId)` and the publish, through the runtime's `focusGroup`.
    fn gx_store_focus_project_group(&mut self, project_id: &str, cx: &mut gpui::Context<Self>) {
        self.dispatch_native_sidebar_command(
            json!({
                "type": "focusGroup",
                "groupId": ProjectKey::local(project_id).to_sidebar_group_id(),
            }),
            cx,
        );
    }

    /// `createGhostexHelpChat(question, projectPath)`.
    ///
    /// CDXC:Onboarding 2026-09-09 DECISION:
    /// User: picking a Help menu row must not send the prompt. The Quick agent chat opens with `$ghostex-help <question>` staged as an editable draft, a toast says "Edit the prompt and press Enter to learn more about Ghostex.", and only the user's Enter submits it, because they may want to reword the question.
    /// User: do not create a new Quick project per question. Every Help chat lives in one project rooted at the Ghostex config folder (the OS-specific directory Rust resolves through ghostex_paths and passes as projectPath), registered on first use and reused after that.
    /// Rust owns the menu and the skill install; this owns the project lookup, the default prompt agent, and focusing the new session, so the chat takes the same draft launch path as Export transcript.
    fn gx_store_create_ghostex_help_chat(
        &mut self,
        question: String,
        project_path: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        // The open-ended row stages `$ghostex-help ` with its trailing space so the user types
        // straight after the skill mention; only the leading whitespace goes.
        let draft = question.trim_start().to_string();
        if draft.trim().is_empty() {
            return;
        }
        let hud = self.gx_store_launch_hud();
        let agent_id = hud
            .as_deref()
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(DEFAULT_PROMPT_AGENT_ID)
            .to_string();
        let Some(agent) = resolve_sidebar_agent(hud.as_deref(), &agent_id)
            .filter(|agent| agent.launch_command().is_some())
        else {
            self.gx_store_create_toast(
                "warning",
                "Ghostex Help unavailable",
                Some("Choose a default prompt agent in Settings > Agents first."),
                cx,
            );
            return;
        };
        let Some(project_path) = project_path else {
            self.gx_store_create_toast(
                "error",
                "Ghostex Help failed",
                Some(
                    "The Ghostex config folder is unknown, so no project could host the help chat.",
                ),
                cx,
            );
            return;
        };
        let preferred_interface = self.gx_store_preferred_interface(&agent.agent_id);
        let known_project = self.gx_store_local_project_at_path(&project_path);
        let runtime_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            None,
            Some(&draft),
        );
        // The config folder's project shows in every workspace (gxserver
        // `project_in_every_workspace`), so it is not placed in this window's.
        let add_params = json!({ "name": "Ghostex", "path": project_path.clone() });
        cx.spawn(async move |this, cx| {
            let project_id = match known_project {
                Some(project_id) => Some(project_id),
                None => gx_rpc(None, "/api/addProjectPath", add_params)
                    .await
                    .ok()
                    .as_ref()
                    .and_then(project_id_of),
            };
            let Some(project_id) = project_id else {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_help_chat_failed("gxserver is unavailable.", cx)
                });
                return;
            };
            let params = agent_record_params(
                &agent,
                &project_id,
                "",
                runtime_settings,
                &AgentRecordOptions {
                    draft: true,
                    title: Some("Ghostex Help".to_string()),
                    ..AgentRecordOptions::default()
                },
                &ghostex_gx_core::agent_session_default_title(Some(&agent.name)),
            );
            let created = gx_rpc(None, "/api/createAgentSession", params).await;
            let _ = this.update(cx, |this, cx| {
                let created = match created {
                    Ok(response) => created_session(&response, Some(&project_id)),
                    Err(error) => {
                        this.gx_store_help_chat_failed(&error.message, cx);
                        return;
                    }
                };
                let Some((_, session_id)) = created else {
                    this.gx_store_help_chat_failed("Ghostex could not start the help chat.", cx);
                    return;
                };
                this.gx_store_focus_created_session(
                    &project_id,
                    &session_id,
                    false,
                    (preferred_interface == "chat").then_some("chat"),
                    cx,
                );
                this.gx_store_create_toast(
                    "info",
                    "Edit the prompt and press Enter to learn more about Ghostex.",
                    None,
                    cx,
                );
            });
        })
        .detach();
    }

    fn gx_store_help_chat_failed(&mut self, message: &str, cx: &mut gpui::Context<Self>) {
        self.gx_store_create_toast("error", "Ghostex Help failed", Some(message), cx);
    }

    /// "Set it up for me" on Settings > Cloud Boxes: the default prompt agent, in the Ghostex
    /// folder project the Help chats use, starts on `AGENTBOX_SETUP_PROMPT` straight away.
    ///
    /// CDXC:AgentBox 2026-10-01 WHY:
    /// The user's decision lives on the Cloud Boxes page (settings_modal/tabs/cloud_boxes.rs): "a prompt to an agent that configures it with computer use". The prompt is sent, not staged as an editable draft like a Help question, because the button's whole point is to start the setup; the prompt itself makes the agent ask before anything that costs money. `createAgentSession` only records the prompt as `firstUserMessage` (and marks the session a draft until it is delivered); nothing types it, so the create is followed by `startSessionProvider` and one `queueSessionChatPrompt` with `startupSend`, exactly as the Project Board's create-with-prompt does, and the startup queue's delivery promotes the draft.
    fn gx_store_create_agentbox_setup_chat(
        &mut self,
        project_path: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let hud = self.gx_store_launch_hud();
        let agent_id = hud
            .as_deref()
            .and_then(|hud| hud.get("settings"))
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(DEFAULT_PROMPT_AGENT_ID)
            .to_string();
        let Some(agent) = resolve_sidebar_agent(hud.as_deref(), &agent_id)
            .filter(|agent| agent.launch_command().is_some())
        else {
            self.gx_store_create_toast(
                "warning",
                "Couldn't start the setup agent",
                Some("Choose a default prompt agent in Settings > Agents first."),
                cx,
            );
            return;
        };
        let Some(project_path) = project_path else {
            self.gx_store_create_toast(
                "error",
                "Couldn't start the setup agent",
                Some("The Ghostex config folder is unknown, so no project could host the setup."),
                cx,
            );
            return;
        };
        let preferred_interface = self.gx_store_preferred_interface(&agent.agent_id);
        let known_project = self.gx_store_local_project_at_path(&project_path);
        let runtime_settings = first_prompt_title_runtime_settings(
            &self.gx_store_title_generation_settings(),
            hud.as_deref(),
            Some(AGENTBOX_SETUP_PROMPT),
            None,
        );
        // The config folder's project shows in every workspace (gxserver
        // `project_in_every_workspace`), so it is not placed in this window's.
        let add_params = json!({ "name": "Ghostex", "path": project_path.clone() });
        cx.spawn(async move |this, cx| {
            let project_id = match known_project {
                Some(project_id) => Some(project_id),
                None => gx_rpc(None, "/api/addProjectPath", add_params)
                    .await
                    .ok()
                    .as_ref()
                    .and_then(project_id_of),
            };
            let Some(project_id) = project_id else {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_agentbox_setup_failed("gxserver is unavailable.", cx)
                });
                return;
            };
            let params = agent_record_params(
                &agent,
                &project_id,
                AGENTBOX_SETUP_PROMPT,
                runtime_settings,
                &AgentRecordOptions {
                    title: Some("Set up Cloud Boxes".to_string()),
                    ..AgentRecordOptions::default()
                },
                &ghostex_gx_core::agent_session_default_title(Some(&agent.name)),
            );
            let created = match gx_rpc(None, "/api/createAgentSession", params).await {
                Ok(response) => created_session(&response, Some(&project_id)),
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.gx_store_agentbox_setup_failed(&error.message, cx)
                    });
                    return;
                }
            };
            let Some((_, session_id)) = created else {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_agentbox_setup_failed(
                        "Ghostex could not start the setup session.",
                        cx,
                    )
                });
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_focus_created_session(
                    &project_id,
                    &session_id,
                    false,
                    (preferred_interface == "chat").then_some("chat"),
                    cx,
                );
            });
            let delivered: Result<(), String> = async {
                gx_rpc(
                    None,
                    "/api/startSessionProvider",
                    start_provider_params(&project_id, &session_id),
                )
                .await
                .map_err(|error| error.message)?;
                let receipt = gx_rpc(
                    None,
                    "/api/queueSessionChatPrompt",
                    queue_startup_prompt_params(&project_id, &session_id, AGENTBOX_SETUP_PROMPT),
                )
                .await
                .map_err(|error| error.message)?;
                check_startup_prompt_receipt(&receipt).map_err(str::to_string)
            }
            .await;
            if let Err(message) = delivered {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_agentbox_setup_failed(&message, cx)
                });
            }
        })
        .detach();
    }

    fn gx_store_agentbox_setup_failed(&mut self, message: &str, cx: &mut gpui::Context<Self>) {
        self.gx_store_create_toast("error", "Couldn't start the setup agent", Some(message), cx);
    }

    /// `resolveDomainProjectScope({ projectPath })`: this computer's project registered at that
    /// folder, as the store holds it.
    fn gx_store_local_project_at_path(&self, path: &str) -> Option<String> {
        let wanted = normalize_project_path(path)?;
        let machine = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)?;
        machine
            .projects()
            .iter()
            .find(|project| {
                project
                    .path
                    .as_deref()
                    .and_then(normalize_project_path)
                    .is_some_and(|candidate| candidate == wanted)
            })
            .map(|project| project.project_id.clone())
    }
}

/// The first prompt of the "Set it up for me" session on Settings > Cloud Boxes.
///
/// CDXC:AgentBox 2026-10-01 WHY:
/// agentbox's provider logins, `claude login` and its install wizard refuse to run without a terminal, and an agent's own shell has none, so the prompt has the agent run them in Ghostex terminals (`ghostex terminal`, `read-text`, `send-text`) and save provider tokens straight into `~/.agentbox/secrets.env`, which is where `agentbox <provider> login` keeps them. `agentbox install -p docker -y` sets up Docker without the interactive wizard.
const AGENTBOX_SETUP_PROMPT: &str = "Set up Cloud Boxes for me on this computer. Ghostex runs agent sessions in isolated boxes through agentbox, a free, open-source command line tool (https://github.com/madarco/agentbox), either on this computer with Docker or in the cloud. Work through the steps below and tell me briefly what you did after each one.

Ground rules:
- Ask me which places I want boxes to run before you create anything that costs money. Docker on this computer is free; Hetzner, Vercel, Daytona, E2B, DigitalOcean and my own server bill while a box exists.
- Never show an API token, key or sign-in code in this chat or in a command you print. Save provider tokens straight into ~/.agentbox/secrets.env as KEY=value lines (keep the file mode 600) and check them with `agentbox <provider> login --status`, which shows them masked.
- agentbox logins and wizards need a real terminal. Run them with `ghostex terminal --title \"<title>\" -- <command>`, read the screen with `ghostex read-text \"<title>\"`, and answer prompts with `ghostex send-text` and `ghostex send-enter` (see `ghostex --help`).
- Use $ghostex-browser-use to work in my browser, or $ghostex-computer-use when a page needs the desktop. If neither skill is installed, open the page and tell me exactly what to click. When a page asks for my password or a two-factor code, stop and let me type it.

Steps:
1. See what is already done: `ghostex agentbox status --json` and `agentbox --version`. Skip every step that is already done.
2. If agentbox is missing, install it: `npm install -g @madarco/agentbox`. If npm is missing, install Node.js from Ghostex Settings > Integrations > Tools first, or ask me.
3. Docker on this computer: if Docker is running, run `agentbox install -p docker -y` (it builds the box image once and skips agentbox's interactive wizard). If Docker is missing or stopped, tell me and suggest Docker Desktop, OrbStack or Colima.
4. Ask me which cloud providers I want, if any. For each one I pick, create an API token in my browser and save it:
   - Hetzner: Cloud Console, open or create a project, Security > API tokens, generate a token with Read & Write. Save it as HCLOUD_TOKEN.
   - Vercel: Account Settings > Tokens (https://vercel.com/account/settings/tokens), create a token for the right team. Also copy the Team ID (team Settings > General) and a Project ID (project Settings > General). Save them as VERCEL_TOKEN, VERCEL_TEAM_ID and VERCEL_PROJECT_ID.
   - Daytona: the Daytona dashboard > API Keys, create a key. Save it as DAYTONA_API_KEY.
   - E2B: the E2B dashboard > API Keys (https://e2b.dev/dashboard?tab=keys). Save it as E2B_API_KEY.
   - DigitalOcean: API > Tokens (https://cloud.digitalocean.com/account/api/tokens), generate a token with read and write scopes. Save it as DIGITALOCEAN_TOKEN.
   Confirm each with `agentbox <provider> login --status`. If that does not show the token as set, run `agentbox <provider> login` in a Ghostex terminal and enter the token there instead.
   Then run `agentbox prepare --provider <provider>` once for each provider in a Ghostex terminal. It builds the base image and can take several minutes; wait for it to finish.
5. My own server over SSH, only if I ask for it: run `agentbox remote-docker add <name> <ssh>` in a Ghostex terminal (ssh is user@host, host:port, or a name from ~/.ssh/config), then `agentbox remote-docker doctor <name>`.
6. Claude in boxes: boxes keep their own Claude sign-in so Claude on this computer stays signed in. Run `agentbox claude login` in a Ghostex terminal, open the sign-in link it prints in my browser, let me approve it, then enter the code the page shows into that terminal.
7. Codex in boxes: boxes on this computer reuse my Codex sign-in. If the status from step 1 says Codex is not signed in for boxes and I picked a cloud provider, run `agentbox codex login` in a Ghostex terminal and finish the device sign-in in my browser.
8. Ask whether I want box web apps at https://<box>.localhost addresses. If yes, run `agentbox install portless`.
9. Finish with `ghostex agentbox status` and a short summary: which places are ready, what still needs me, and a reminder that cloud boxes keep billing until I stop or destroy them in Ghostex Settings > Cloud Boxes.";

/// `response.project.projectId`.
fn project_id_of(response: &Value) -> Option<String> {
    response
        .get("project")?
        .get("projectId")?
        .as_str()
        .map(str::to_string)
}
