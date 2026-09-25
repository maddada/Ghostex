//! Handoff / Export: the dialog's session, its export, and the follow-up conversation. Ported from
//! the old runtime's `export-transcript.ts`.
//!
//! CDXC:TranscriptExport 2026-08-20 / CDXC:TranscriptExport 2026-08-24:
//! Export Transcript opens its dialog on the include-toggle options stage; the daemon call only runs
//! when the user confirms it there. This host parks which session the dialog is about (local or
//! remote), because the dialog is a separate child window with no gxserver client of its own; the
//! dialog sends only its toggles and agent choice, and the exported path and its project stay
//! here. The export is a daemon operation on the machine that runs the agent, so the path it
//! returns is absolute on THAT machine, and a remote export has nothing to reveal here.
//!
//! CDXC:TranscriptExport 2026-09-21 DECISION:
//! User: "if I am in a Claude session and I select a Codex one, then we should just automatically show the handover modal. The user can select what to hand over to, but in the handover modal, we need to select the one that we are going towards. Once we open that session, we need to set the model automatically for the user."
//! The chat model picker's pick of another agent's model arrives as the handoff target. It is parked
//! with the request, the dialog opens on Handoff with that agent selected, and the follow-up session
//! launches on the model when the user keeps an agent of that family. Every open replaces the
//! parked request, so a plain Handoff / Export never inherits an earlier target.
//!
//! SEE-ALSO: apps/desktop/src/app/window/export_transcript_modal.rs (`targetAgentId`),
//! server/src/agents/launch_plan.rs (`agentModel` / `agentEffort`, Claude and Codex only).

use ghostex_gx_core::git_menu::GitToastLevel;
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
use serde_json::{Value, json};

use super::calls;
use super::prompt_agent::PromptAgentLaunch;
use super::scope::GitScope;
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;

const EXPORT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// The chat model picker's pick of another agent's model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HandoffTarget {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) effort: Option<String>,
}

/// The session the open dialog is about.
#[derive(Clone, Debug)]
struct ExportRequest {
    request_id: String,
    session: SessionKey,
    agent_id: Option<String>,
    session_title: String,
    handoff_target: Option<HandoffTarget>,
}

/// What the dialog's export returned.
#[derive(Clone, Debug)]
struct ExportedTranscript {
    request_id: String,
    project: ProjectKey,
    agent_id: Option<String>,
    path: String,
    session_title: String,
    handoff_target: Option<HandoffTarget>,
}

#[derive(Default)]
pub(crate) struct ExportTranscriptHost {
    request: Option<ExportRequest>,
    exported: Option<ExportedTranscript>,
    sequence: u64,
}

/// `modelPickerProvider`: the model family an agent's logo belongs to.
fn model_picker_provider(icon: Option<&str>) -> Option<&'static str> {
    match icon? {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "cursor-cli" | "cursor" => Some("cursor"),
        "grok-build" | "grok" => Some("grok"),
        "antigravity-cli" | "antigravity" => Some("antigravity"),
        _ => None,
    }
}

/// `sessionChatFileReference` + `sessionChatHandoffDraft`.
///
/// CDXC:SessionChat 2026-09-06 DECISION:
/// User: Ghostex-generated file references use descriptive Markdown links instead of @filepath,
/// with a purpose and reference number or a session-title Handoff label.
/// SEE-ALSO: packages/shared/session-chat-file-references.ts.
fn handoff_draft(path: &str, session_title: &str) -> String {
    let label = format!("{session_title} Handoff");
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut escaped_label = String::new();
    for ch in label.chars() {
        if matches!(ch, '\\' | '[' | ']') {
            escaped_label.push('\\');
        }
        escaped_label.push(ch);
    }
    let destination = if path
        .chars()
        .any(|ch| ch.is_whitespace() || ch == '<' || ch == '>')
    {
        let mut escaped = String::from("<");
        for ch in path.chars() {
            if matches!(ch, '\\' | '<' | '>') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        escaped.push('>');
        escaped
    } else {
        let mut escaped = String::new();
        for ch in path.chars() {
            if matches!(ch, '\\' | '(' | ')') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        escaped
    };
    // The trailing space closes the link and separates it from the prompt the user writes next,
    // so it must survive untrimmed all the way to the daemon.
    format!("[{escaped_label}]({destination}) ")
}

impl GhostexGpuiApp {
    /// Opens the Handoff / Export dialog for a sidebar session id (`exportSessionTranscript`).
    pub(crate) fn git_export_session_transcript(
        &mut self,
        sidebar_session_id: &str,
        handoff_target: Option<HandoffTarget>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session) = SessionKey::parse_sidebar_session_id(sidebar_session_id) else {
            return;
        };
        let presentation = self.gx_store.core.presentation();
        let row = presentation
            .loaded(&session.machine)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
            .cloned();
        let remote = !session.machine.is_local();
        let (agent_id, session_title) = match (&row, remote) {
            (Some(row), _) => (
                row.agent_id.clone().filter(|id| !id.trim().is_empty()),
                row.display_title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| row.title.clone()),
            ),
            (None, true) => (None, "Session".to_string()),
            (None, false) => return,
        };
        let session_title = match (remote, session_title.is_empty()) {
            (true, true) => "Session".to_string(),
            _ => session_title,
        };
        // A remote row's agent is not preselected: the old runtime never read it.
        let agent_id = if remote { None } else { agent_id };
        let host = &mut self.gx_store.git.export;
        host.sequence += 1;
        let request_id = format!(
            "export-transcript-{:x}-{}",
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis())
                .unwrap_or_default(),
            host.sequence
        );
        let target_agent_id = handoff_target.as_ref().and_then(|target| {
            self.git_hud_agents()
                .into_iter()
                .find(|agent| {
                    agent.has_command()
                        && model_picker_provider(agent.icon.as_deref())
                            == Some(target.provider.as_str())
                })
                .map(|agent| agent.agent_id)
        });
        let host = &mut self.gx_store.git.export;
        host.exported = None;
        host.request = Some(ExportRequest {
            request_id: request_id.clone(),
            session,
            agent_id: agent_id.clone(),
            session_title,
            handoff_target,
        });
        let mut message = json!({
            "canReveal": !remote,
            "modal": "exportTranscriptResult",
            "requestId": request_id,
            "type": "open",
        });
        if let Some(agent_id) = agent_id {
            message["agentId"] = json!(agent_id);
        }
        if let Some(target) = target_agent_id {
            message["targetAgentId"] = json!(target);
        }
        self.open_app_modal_from_bridge(message, cx);
    }

    /// A command from the dialog (`handleGpuiExportTranscriptModalCommand`), after the dialog's
    /// field allowlist.
    pub(crate) fn git_export_transcript_modal_command(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = command
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        match command.get("type").and_then(Value::as_str) {
            Some("cancelExportSessionTranscript") => {
                let host = &mut self.gx_store.git.export;
                if request_id.is_some()
                    && host.request.as_ref().map(|request| &request.request_id)
                        == request_id.as_ref()
                {
                    host.request = None;
                    if host.exported.as_ref().map(|exported| &exported.request_id)
                        == request_id.as_ref()
                    {
                        host.exported = None;
                    }
                }
            }
            Some("runExportSessionTranscript") => {
                if let Some(request_id) = request_id {
                    let toggle = |key: &str| command.get(key).and_then(Value::as_bool);
                    self.git_run_export(
                        &request_id,
                        toggle("includeCommands") != Some(false),
                        toggle("includePatches") != Some(false),
                        toggle("includeReasoning") == Some(true),
                        cx,
                    );
                }
            }
            Some("startExportedTranscriptConversation") => {
                let agent_id = command
                    .get("agentId")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string);
                if let Some(request_id) = request_id {
                    self.git_start_exported_conversation(&request_id, agent_id, cx);
                }
            }
            _ => {}
        }
    }

    /// The dialog's Export button (`runExportSessionTranscriptForDialog`). A failure surfaces the
    /// daemon's own message inside the dialog.
    fn git_run_export(
        &mut self,
        request_id: &str,
        include_commands: bool,
        include_patches: bool,
        include_reasoning: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request) = self
            .gx_store
            .git
            .export
            .request
            .clone()
            .filter(|request| request.request_id == request_id)
        else {
            return;
        };
        let remote = !request.session.machine.is_local();
        let target = match request.session.machine.remote_id() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    return self.git_post_export_result(
                        &request.request_id,
                        Err("Remote gxserver request failed.".to_string()),
                        cx,
                    );
                }
            },
            None => None,
        };
        let params = json!({
            "includeCommands": include_commands,
            "includePatches": include_patches,
            "includeReasoning": include_reasoning,
            "projectId": request.session.project_id,
            "sessionId": request.session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let result = calls::call(
                target,
                "/api/exportSessionTranscript",
                params,
                remote.then_some(EXPORT_TIMEOUT),
            )
            .await;
            let _ = this.update(cx, |this, cx| {
                let still_open = this
                    .gx_store
                    .git
                    .export
                    .request
                    .as_ref()
                    .is_some_and(|open| open.request_id == request.request_id);
                if !still_open {
                    return;
                }
                let answer = result.map_err(|error| error.message).and_then(|answer| {
                    let path = answer
                        .get("path")
                        .and_then(Value::as_str)
                        .filter(|path| !path.trim().is_empty())
                        .map(str::to_string)
                        .ok_or_else(|| "gxserver did not return the exported file.".to_string())?;
                    Ok((
                        path,
                        answer
                            .get("agent")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ))
                });
                match answer {
                    Ok((path, agent)) => {
                        let agent_id = request
                            .agent_id
                            .clone()
                            .or_else(|| this.git_export_agent_id(agent.as_deref()));
                        this.gx_store.git.export.exported = Some(ExportedTranscript {
                            request_id: request.request_id.clone(),
                            project: ProjectKey {
                                machine: request.session.machine.clone(),
                                project_id: request.session.project_id.clone(),
                            },
                            agent_id: agent_id.clone(),
                            path: path.clone(),
                            session_title: request.session_title.clone(),
                            handoff_target: request.handoff_target.clone(),
                        });
                        this.git_post_export_result(
                            &request.request_id,
                            Ok((path, agent_id, !remote)),
                            cx,
                        );
                    }
                    Err(error) => this.git_post_export_result(&request.request_id, Err(error), cx),
                }
            });
        })
        .detach();
    }

    /// `resolveExportTranscriptAgentId`: the daemon's transcript format name, as one of the user's
    /// agents, so the dialog preselects the agent the exported session used.
    fn git_export_agent_id(&self, agent: Option<&str>) -> Option<String> {
        let agent = agent?.trim().to_lowercase();
        if agent.is_empty() {
            return None;
        }
        let agents = self.git_hud_agents();
        agents
            .iter()
            .find(|candidate| candidate.agent_id.to_lowercase() == agent)
            .or_else(|| {
                agents.iter().find(|candidate| {
                    candidate.icon.as_deref().map(str::to_lowercase).as_deref()
                        == Some(agent.as_str())
                })
            })
            .map(|candidate| candidate.agent_id.clone())
    }

    /// The export's answer, into the open dialog; the path is held here for Reveal in Finder, so
    /// Reveal never trusts a path posted back by the dialog (`CDXC:TranscriptExport 2026-08-24`).
    fn git_post_export_result(
        &mut self,
        request_id: &str,
        result: Result<(String, Option<String>, bool), String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let message = match result {
            Ok((path, agent_id, can_reveal)) => {
                self.pending_export_transcript_reveal_path = can_reveal.then(|| path.clone());
                let mut message = json!({
                    "canReveal": can_reveal,
                    "ok": true,
                    "path": path,
                    "requestId": request_id,
                    "type": "exportSessionTranscriptResult",
                });
                if let Some(agent_id) = agent_id {
                    message["agentId"] = json!(agent_id);
                }
                message
            }
            Err(error) => {
                self.pending_export_transcript_reveal_path = None;
                json!({
                    "canReveal": false,
                    "error": error,
                    "ok": false,
                    "requestId": request_id,
                    "type": "exportSessionTranscriptResult",
                })
            }
        };
        if self.receive_gpui_export_transcript_result(&message, cx) {
            return;
        }
        self.dispatch_open_gpui_app_modal_message(message, cx);
    }

    /// The dialog's Start Conversation (`startExportedTranscriptConversation`).
    ///
    /// CDXC:TranscriptExport 2026-08-20:
    /// The new session is created with a staged input draft, never a first user message: gxserver
    /// types the mention into the agent's composer after the provider starts and stops there, so
    /// the conversation only begins when the user writes their own prompt around the mention.
    ///
    /// CDXC:Drafts 2026-09-02:
    /// The row is a draft, like every other promptless launch, and when the user's Default Agent
    /// View for this agent is Chat, the session opens straight in Chat.
    fn git_start_exported_conversation(
        &mut self,
        request_id: &str,
        agent_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let host = &mut self.gx_store.git.export;
        let Some(exported) = host
            .exported
            .clone()
            .filter(|exported| exported.request_id == request_id)
        else {
            return;
        };
        host.exported = None;
        host.request = None;
        let agent_id = agent_id.or_else(|| exported.agent_id.clone());
        let agent = agent_id.as_deref().and_then(|id| self.git_hud_agent(id));
        let Some(agent) = agent else {
            return self.git_toast(
                GitToastLevel::Warning,
                "Could not start the conversation",
                ToastOptions::described("Choose a configured agent for the new session."),
                cx,
            );
        };
        let scope = match self.git_scope_for_key(exported.project.clone()) {
            Ok(scope) => scope,
            Err(_) if exported.project.machine == MachineId::Local => return,
            Err(_) => GitScope {
                key: exported.project.clone(),
                remote: exported
                    .project
                    .machine
                    .remote_id()
                    .and_then(|machine| self.gpui_remote_gxserver_request_target(machine)),
                title: String::new(),
                path: None,
                preferences: Default::default(),
                worktree: None,
                projectless: false,
            },
        };
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let object = settings.object();
        let open_in_chat =
            crate::app::helpers::gpui_preferred_agent_interface_override_from_settings(
                object,
                &agent.agent_id,
            )
            .unwrap_or_else(|| {
                crate::app::helpers::gpui_preferred_agent_interface_from_settings(object)
            }) == crate::app::model::GpuiPreferredAgentInterface::Chat;
        // The user may pick another agent in the dialog; the model only means something to the
        // family that offers it.
        let launch_model = exported.handoff_target.as_ref().filter(|target| {
            matches!(target.provider.as_str(), "claude" | "codex")
                && model_picker_provider(agent.icon.as_deref()) == Some(target.provider.as_str())
        });
        let remote = scope.is_remote();
        let started = self.git_start_prompt_agent(
            &scope,
            agent,
            PromptAgentLaunch {
                first_user_input_draft: Some(handoff_draft(
                    &exported.path,
                    &exported.session_title,
                )),
                agent_model: launch_model.map(|target| target.model.clone()),
                agent_effort: launch_model.and_then(|target| target.effort.clone()),
                open_in_chat,
                missing_session_message: Some("Could not create the new agent session."),
                ..PromptAgentLaunch::default()
            },
            cx,
        );
        cx.spawn(async move |this, cx| {
            if let Err(error) = started.await {
                let _ = this.update(cx, |this, cx| {
                    let description = match (remote, error.is_empty()) {
                        (false, true) => "Could not create the new agent session.".to_string(),
                        _ => error,
                    };
                    this.git_toast(
                        GitToastLevel::Error,
                        "Could not start the conversation",
                        ToastOptions::described(description),
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}
