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
//! SEE-ALSO: apps/desktop/src/app/window/export_transcript_modal.rs (`targetProvider`, `handoff_target_agent`),
//! server/src/agents/launch_plan.rs (`agentModel` / `agentEffort`, Claude, Codex and Antigravity CLI).

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

/// The coordinator role and standing context carried across a coordinator handoff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HandoffCoordinator {
    pub(crate) goal: String,
    pub(crate) instructions: String,
    /// The source computer's gxserver gave neither the export's `coordinator` nor a
    /// `/api/readCoordinator` answer, so the new orchestrator starts without its Goal and Standing
    /// instructions and the user is told so.
    pub(crate) context_lost: bool,
    /// Sent with the orchestrator's first message (`coordinator.firstRequest`) instead of staged as
    /// a draft: the handoff's lead-in, for a destination gxserver that would type a staged draft
    /// into the terminal while the skill starts (`coordinatorHandoffDraft`).
    pub(crate) first_request: Option<String>,
}

impl HandoffCoordinator {
    fn from_object(coordinator: &serde_json::Map<String, Value>) -> Self {
        let text = |key: &str| {
            coordinator
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            goal: text("goal"),
            instructions: text("instructions"),
            ..Self::default()
        }
    }

    /// The export's own `coordinator` (gxserver 10.43 and later).
    pub(crate) fn from_export_answer(answer: &Value) -> Option<Self> {
        answer
            .get("coordinator")
            .and_then(Value::as_object)
            .map(Self::from_object)
    }

    /// `/api/readCoordinator`'s answer, only when it is this session's own record: the endpoint
    /// answers a thread with its orchestrator's.
    pub(crate) fn from_read_answer(answer: &Value, session_id: &str) -> Option<Self> {
        answer
            .get("coordinator")
            .and_then(Value::as_object)
            .filter(|coordinator| {
                coordinator.get("sessionId").and_then(Value::as_str) == Some(session_id)
            })
            .map(Self::from_object)
    }
}

/// The orchestrator a handoff of `session` carries, read from its export's answer. An older
/// gxserver's export has no `coordinator`, so an orchestrator row's Goal and Standing instructions
/// are read with `/api/readCoordinator` on the same computer; when that fails too, the handoff
/// still starts an orchestrator, marked [`HandoffCoordinator::context_lost`].
pub(super) async fn handoff_coordinator(
    remote: calls::Remote,
    answer: &Value,
    is_coordinator: bool,
    session: &SessionKey,
) -> Option<HandoffCoordinator> {
    if let Some(coordinator) = HandoffCoordinator::from_export_answer(answer) {
        return Some(coordinator);
    }
    if !is_coordinator {
        return None;
    }
    let read = calls::call(
        remote,
        "/api/readCoordinator",
        json!({ "projectId": session.project_id, "sessionId": session.session_id }),
        None,
    )
    .await;
    Some(
        read.ok()
            .and_then(|answer| HandoffCoordinator::from_read_answer(&answer, &session.session_id))
            .unwrap_or(HandoffCoordinator {
                context_lost: true,
                ..HandoffCoordinator::default()
            }),
    )
}

/// The session the open dialog is about.
#[derive(Clone, Debug)]
pub(super) struct ExportRequest {
    pub(super) request_id: String,
    pub(super) session: SessionKey,
    agent_id: Option<String>,
    session_title: String,
    is_coordinator: bool,
    handoff_target: Option<HandoffTarget>,
    /// The `open` message, sent again when the dialog reopens after Add Project.
    pub(super) open_message: Value,
}

/// What the dialog's export returned.
#[derive(Clone, Debug)]
pub(super) struct ExportedTranscript {
    request_id: String,
    pub(super) project: ProjectKey,
    agent_id: Option<String>,
    pub(super) path: String,
    pub(super) session_title: String,
    pub(super) coordinator: Option<HandoffCoordinator>,
    pub(super) handoff_target: Option<HandoffTarget>,
    /// The file's text, when the dialog asked for it (a handoff to another computer).
    pub(super) markdown: Option<String>,
}

#[derive(Default)]
pub(crate) struct ExportTranscriptHost {
    pub(super) request: Option<ExportRequest>,
    exported: Option<ExportedTranscript>,
    sequence: u64,
    /// The dialog's picks while it is closed for Add Project, given back when it reopens.
    pub(super) restore: Option<crate::app::window::ExportTranscriptPicks>,
    /// Add Project opened from a handoff's destination (export_handoff_remote.rs).
    pub(super) add_project: Option<super::export_handoff_remote::HandoffAddProject>,
}

impl GhostexGpuiApp {
    /// The picks Handoff / Export reopens with after Add Project, once.
    pub(crate) fn git_export_take_restore(
        &mut self,
    ) -> Option<crate::app::window::ExportTranscriptPicks> {
        self.gx_store.git.export.restore.take()
    }
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
        let is_coordinator = row
            .as_ref()
            .is_some_and(|row| row.coordinator_role.as_deref() == Some("coordinator"));
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
        // The dialog resolves the target's agent from its own list (`handoff_target_agent`).
        let target = handoff_target
            .as_ref()
            .map(|target| (target.provider.clone(), target.model.clone()));
        let mut message = json!({
            "canReveal": !remote,
            "modal": "exportTranscriptResult",
            "requestId": request_id,
            "type": "open",
        });
        if let Some(agent_id) = &agent_id {
            message["agentId"] = json!(agent_id);
        }
        if let Some((provider, model)) = target {
            message["targetProvider"] = json!(provider);
            message["targetModel"] = json!(model);
        }
        let host = &mut self.gx_store.git.export;
        host.exported = None;
        host.restore = None;
        host.add_project = None;
        host.request = Some(ExportRequest {
            request_id: request_id.clone(),
            session,
            agent_id,
            session_title,
            is_coordinator,
            handoff_target,
            open_message: message.clone(),
        });
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
                        toggle("includeMarkdown") == Some(true),
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
                let text = |key: &str| {
                    command
                        .get(key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                let target = text("targetMachineId").zip(text("targetProjectId"));
                if let Some(request_id) = request_id {
                    match target {
                        Some((machine_id, project_id)) => self.git_start_remote_handoff(
                            &request_id,
                            agent_id,
                            &machine_id,
                            &project_id,
                            cx,
                        ),
                        None => self.git_start_exported_conversation(&request_id, agent_id, cx),
                    }
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
        include_markdown: bool,
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
            "includeMarkdown": include_markdown,
            "includePatches": include_patches,
            "includeReasoning": include_reasoning,
            "projectId": request.session.project_id,
            "sessionId": request.session.session_id,
        });
        cx.spawn(async move |this, cx| {
            let result = calls::call(
                target.clone(),
                "/api/exportSessionTranscript",
                params,
                remote.then_some(EXPORT_TIMEOUT),
            )
            .await;
            let coordinator = match &result {
                Ok(answer) => {
                    handoff_coordinator(target, answer, request.is_coordinator, &request.session)
                        .await
                }
                Err(_) => None,
            };
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
                    let markdown = answer
                        .get("markdown")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if include_markdown && markdown.is_none() {
                        // A gxserver from before this carrying ignores `includeMarkdown`.
                        return Err(
                            "That computer's Ghostex is too old to hand off to another computer. Update it and try again."
                                .to_string(),
                        );
                    }
                    Ok((
                        path,
                        answer
                            .get("agent")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        coordinator,
                        markdown,
                    ))
                });
                match answer {
                    Ok((path, agent, coordinator, markdown)) => {
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
                            coordinator,
                            handoff_target: request.handoff_target.clone(),
                            markdown,
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
        let Some(exported) = self.git_take_exported(request_id) else {
            return;
        };
        let Some(agent) = self.git_handoff_agent(agent_id, &exported, cx) else {
            return;
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
        let path = exported.path.clone();
        self.git_launch_handoff(&scope, agent, &exported, &path, cx);
    }

    /// The parked export the dialog's Start is about, taken with its request.
    pub(super) fn git_take_exported(&mut self, request_id: &str) -> Option<ExportedTranscript> {
        let host = &mut self.gx_store.git.export;
        let exported = host
            .exported
            .clone()
            .filter(|exported| exported.request_id == request_id)?;
        host.exported = None;
        host.request = None;
        Some(exported)
    }

    /// The agent the follow-up session runs, or a toast saying none was chosen.
    pub(super) fn git_handoff_agent(
        &mut self,
        agent_id: Option<String>,
        exported: &ExportedTranscript,
        cx: &mut gpui::Context<Self>,
    ) -> Option<super::prompt_agent::HudAgent> {
        let agent_id = agent_id.or_else(|| exported.agent_id.clone());
        let agent = agent_id.as_deref().and_then(|id| self.git_hud_agent(id));
        if agent.is_none() {
            self.git_toast(
                GitToastLevel::Warning,
                "Could not start the conversation",
                ToastOptions::described("Choose a configured agent for the new session."),
                cx,
            );
        }
        agent
    }

    /// Starts the follow-up session in `scope` with a link to `transcript_path` (a path on the
    /// machine `scope` is on) typed into its composer.
    pub(super) fn git_launch_handoff(
        &mut self,
        scope: &GitScope,
        agent: super::prompt_agent::HudAgent,
        exported: &ExportedTranscript,
        transcript_path: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let source_title = self.git_export_project_title(&exported.project);
        let remote = scope.is_remote();
        let started = self.git_start_handoff_session(
            scope,
            agent,
            ghostex_gx_core::session_handoff::handoff_draft(
                transcript_path,
                &exported.session_title,
                &source_title,
            ),
            exported.coordinator.clone(),
            exported.handoff_target.as_ref(),
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

    /// The name of the project a handoff's lead-in says it came from.
    pub(super) fn git_export_project_title(&self, project: &ProjectKey) -> String {
        self.gx_store
            .core
            .presentation()
            .loaded(&project.machine)
            .and_then(|loaded| loaded.project(&project.project_id))
            .map(|project| project.title.clone())
            .unwrap_or_default()
    }

    /// Creates a handoff's follow-up session with its message box staged.
    pub(super) fn git_start_handoff_session(
        &mut self,
        scope: &GitScope,
        agent: super::prompt_agent::HudAgent,
        draft: String,
        coordinator: Option<HandoffCoordinator>,
        handoff_target: Option<&HandoffTarget>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<SessionKey, String>> {
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let object = settings.object();
        let mut coordinator_launch = coordinator.and_then(|coordinator| {
            agent
                .coordinator_family()
                .map(|family| (family, coordinator))
        });
        let mut draft = Some(draft);
        if let Some((_, coordinator)) = coordinator_launch.as_mut() {
            if coordinator.context_lost {
                self.git_toast(
                    GitToastLevel::Warning,
                    "Goal and Standing instructions not kept",
                    ToastOptions::described(
                        "The orchestrator's computer did not say what they were, so the new orchestrator starts without them. Add them again in its details.",
                    ),
                    cx,
                );
            }
            // An older gxserver there would type a staged draft into the terminal while
            // `/ghostex-orchestrator` starts (CDXC:Coordinators `coordinatorHandoffDraft`).
            if scope
                .remote
                .as_ref()
                .is_some_and(|target| !target.stages_coordinator_handoff_draft())
            {
                coordinator.first_request = draft.take();
                self.git_toast(
                    GitToastLevel::Info,
                    "Handoff sent with the orchestrator's first message",
                    ToastOptions::described(
                        "That computer's Ghostex is older, so the conversation link goes with the orchestrator's first message instead of waiting in its message box. Update Ghostex there to keep it as a draft.",
                    ),
                    cx,
                );
            }
        }
        let open_in_chat = coordinator_launch.is_some()
            || crate::app::helpers::gpui_preferred_agent_interface_override_from_settings(
                object,
                &agent.agent_id,
            )
            .unwrap_or_else(|| {
                crate::app::helpers::gpui_preferred_agent_interface_from_settings(object)
            }) == crate::app::model::GpuiPreferredAgentInterface::Chat;
        // The user may pick another agent in the dialog; the model only means something to the
        // family that offers it.
        let launch_model = handoff_target.filter(|target| {
            matches!(target.provider.as_str(), "claude" | "codex" | "antigravity")
                && model_picker_provider(agent.icon.as_deref()) == Some(target.provider.as_str())
        });
        let agent_model = launch_model.map(|target| target.model.clone()).or_else(|| {
            coordinator_launch
                .as_ref()
                .filter(|(family, _)| *family == "claude")
                .map(|_| crate::app::window::DEFAULT_CLAUDE_COORDINATOR_MODEL.to_string())
        });
        let agent_effort = launch_model
            .and_then(|target| target.effort.clone())
            .or_else(|| {
                coordinator_launch.as_ref().and_then(|(family, _)| {
                    matches!(*family, "claude" | "codex" | "antigravity")
                        .then(|| "high".to_string())
                })
            });
        self.git_start_prompt_agent(
            scope,
            agent,
            PromptAgentLaunch {
                first_user_input_draft: draft,
                agent_model,
                agent_effort,
                open_in_chat,
                coordinator: coordinator_launch.map(|(_, coordinator)| coordinator),
                missing_session_message: Some("Could not create the new agent session."),
                ..PromptAgentLaunch::default()
            },
            cx,
        )
    }
}

#[cfg(test)]
mod handoff_coordinator_tests {
    use super::HandoffCoordinator;
    use serde_json::json;

    #[test]
    fn an_export_carries_the_orchestrators_goal_and_instructions() {
        let answer = json!({
            "path": "/x.md",
            "coordinator": { "goal": "Ship it", "instructions": "One module per thread." },
        });
        assert_eq!(
            HandoffCoordinator::from_export_answer(&answer),
            Some(HandoffCoordinator {
                goal: "Ship it".to_string(),
                instructions: "One module per thread.".to_string(),
                ..HandoffCoordinator::default()
            })
        );
        assert_eq!(
            HandoffCoordinator::from_export_answer(&json!({ "path": "/x.md" })),
            None
        );
    }

    #[test]
    fn a_read_coordinator_answer_counts_only_for_the_session_itself() {
        let answer = json!({
            "coordinator": {
                "sessionId": "S1",
                "goal": "Ship it",
                "instructions": "Keep it small.",
            },
        });
        assert_eq!(
            HandoffCoordinator::from_read_answer(&answer, "S1").map(|c| c.goal),
            Some("Ship it".to_string())
        );
        // `/api/readCoordinator` answers a thread with its orchestrator's record.
        assert_eq!(HandoffCoordinator::from_read_answer(&answer, "T9"), None);
    }
}
