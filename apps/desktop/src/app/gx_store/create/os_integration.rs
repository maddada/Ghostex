//! The OS integration commands: `ghostex://terminal` and the Finder or Services "New Terminal",
//! `ghostex://open` and Open With, and the Help menu's questions, performed in Rust
//! (`handleGpuiOsIntegrationCommand`, `createOsIntegrationTerminal`, `openOsIntegrationProjectPaths`,
//! `createGhostexHelpChat` in the old runtime).
//!
//! Rust already owned the URL and file parsing, the script Run/Edit/Cancel consent dialog, the
//! existence checks and the git-root resolution; the runtime registered the daemon projects and
//! created and focused the sessions through the reviewed paths. Both halves are here now, with the
//! runtime's calls (`/api/addProjectPath`, `/api/createSession` or `/api/createAgentSession`) and
//! toasts.
//!
//! `ghostex://terminal` parity note: macOS creates a client-side projectless Quick project per
//! invocation; GPUI's sidebar is daemon-derived, so the terminal lands in the daemon project
//! registered (or reused) at the resolved cwd. A provided command launches the session with it
//! (the Search-by-Text `gx f` launcher contract) instead of macOS's typed `command\r` into a shell.
//!
//! SEE-ALSO: apps/desktop/src/app/sidebar_dispatch.rs (`dispatch_gpui_os_integration_command_message`),
//! apps/desktop/src/app/titlebar/help_menu.rs.

use ghostex_gx_core::{
    AgentRecordOptions, ProjectKey, agent_record_params, created_session,
    first_prompt_title_runtime_settings, normalize_project_path, os_integration_command_params,
    project_name_from_path, resolve_sidebar_agent, terminal_create_params,
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
                Some("ghostex://terminal needs the local gxserver."),
                cx,
            );
            return;
        };
        cx.spawn(async move |this, cx| {
            let name = project_name_from_path(&cwd);
            let registered = gx_rpc(
                None,
                "/api/addProjectPath",
                json!({ "name": name, "path": cwd }),
            )
            .await;
            let Some(project_id) = registered.ok().as_ref().and_then(project_id_of) else {
                let _ = this.update(cx, |this, cx| this.gx_store_open_terminal_failed(cx));
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_focus_project_group(&project_id, cx)
            });
            let title = title
                .or_else(|| Some(name.clone()).filter(|name| !name.trim().is_empty()))
                .unwrap_or_else(|| ghostex_gx_core::DEFAULT_TERMINAL_SESSION_TITLE.to_string());
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
                            None,
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
        cx.spawn(async move |this, cx| {
            let mut focus_project_id = None;
            let mut failed = 0usize;
            for path in paths {
                let result = gx_rpc(
                    None,
                    "/api/addProjectPath",
                    json!({ "name": project_name_from_path(&path), "path": path }),
                )
                .await;
                match result.ok().as_ref().and_then(project_id_of) {
                    Some(project_id) => focus_project_id = Some(project_id),
                    None => failed += 1,
                }
            }
            let _ = this.update(cx, |this, cx| {
                if failed > 0 {
                    this.gx_store_create_toast(
                        "error",
                        "Open failed",
                        Some("gxserver could not open a requested folder as a project."),
                        cx,
                    );
                }
                if let Some(project_id) = focus_project_id {
                    this.gx_store_focus_project_group(&project_id, cx);
                }
            });
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
        cx.spawn(async move |this, cx| {
            let project_id = match known_project {
                Some(project_id) => Some(project_id),
                None => gx_rpc(
                    None,
                    "/api/addProjectPath",
                    json!({ "name": "Ghostex", "path": project_path }),
                )
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

/// `response.project.projectId`.
fn project_id_of(response: &Value) -> Option<String> {
    response
        .get("project")?
        .get("projectId")?
        .as_str()
        .map(str::to_string)
}
