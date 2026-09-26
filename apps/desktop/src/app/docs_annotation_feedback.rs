//! Docs annotation feedback: where the Send button in the Docs review puts the
//! annotations, and how Docs learns that target ahead of time.
//!
//! CDXC:Docs 2026-09-15 DECISION:
//! User: "just send for the chat that's currently last active in the currently active project basically so the user can change that agent session by just clicking in the sidebar", "add the annotations either to the chat or the agent cli depending on what's currently active in that agent session", and "if we're not sure anyways we can just show a toast that we copied the annotations to the clipboard" with the copy sound, both for agents whose input box cannot be detected and when detection says the box is not available.
//! The text is added to the composer or pasted into the terminal without pressing Enter, the same way Add to Session Context works, so the user can read it and add to it before sending.
//! User: sending must not switch the app to the Agents view ("why does submitting switch to the agents view forcefully? Please disable that behavior"); Docs is a side pane, so the notes land in the session's chat or terminal while Docs stays on screen, and only the target tab is selected in its pane.
//! SEE-ALSO: apps/desktop/views/manage/manage-app.tsx (`sendAnnotationFeedback`), apps/desktop/views/manage/annotation-feedback.ts (the text), server/src/session_chat_composer.rs (`read_session_terminal_tail`, the input-box verdict).

use std::time::Duration;

use gpui::ClipboardItem;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

const DOCS_ANNOTATION_FEEDBACK_COPIED_TOAST_ID: &str = "gpui-docs-annotation-feedback-copied";
const DOCS_ANNOTATION_FEEDBACK_READINESS_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocsAnnotationFeedbackSurface {
    Chat,
    Terminal,
}

impl DocsAnnotationFeedbackSurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DocsAnnotationFeedbackTarget {
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) agent_label: String,
    pub(crate) session_title: String,
    pub(crate) surface: DocsAnnotationFeedbackSurface,
}

/// The gxserver identity of the target session, for the input-box verdict.
enum DocsAnnotationFeedbackRpcTarget {
    Local {
        project_id: String,
        session_id: String,
    },
    Remote {
        target: GpuiRemoteGxserverRequestTarget,
        project_id: String,
        session_id: String,
    },
}

fn docs_annotation_feedback_agent_label(agent_icon: &str) -> String {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == agent_icon)
        .map(|agent| agent.name.to_string())
        .unwrap_or_else(|| {
            let mut characters = agent_icon.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect(),
                None => agent_icon.to_string(),
            }
        })
}

impl GhostexGpuiApp {
    /// The session the user last clicked in the sidebar for the active project,
    /// when it is a running agent session. Nothing else is considered: the user
    /// changes the target by clicking another session.
    pub(crate) fn docs_annotation_feedback_target(&self) -> Option<DocsAnnotationFeedbackTarget> {
        let active_project_id = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|project_id| project_id.0.as_str())?;
        let latest_key = self.local_workspace_latest_focus_key.as_ref()?;
        if latest_key.project_id != active_project_id {
            return None;
        }
        let shell_session_id = *self.local_workspace_session_mappings.get(latest_key)?;
        self.docs_annotation_feedback_target_for_session(shell_session_id)
    }

    /// `shell_session_id` as a feedback target, when it is a running agent session.
    pub(crate) fn docs_annotation_feedback_target_for_session(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> Option<DocsAnnotationFeedbackTarget> {
        let session = self.agents_workspace.session(shell_session_id)?;
        if session.presentation_state != TerminalSessionPresentationState::Running {
            return None;
        }
        let agent_icon = session.agent_icon?;
        let surface = if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            DocsAnnotationFeedbackSurface::Chat
        } else {
            DocsAnnotationFeedbackSurface::Terminal
        };
        Some(DocsAnnotationFeedbackTarget {
            shell_session_id,
            agent_label: docs_annotation_feedback_agent_label(agent_icon),
            session_title: session.title.clone(),
            surface,
        })
    }

    pub(crate) fn docs_annotation_send_target_response(
        &self,
        action: &str,
        request_id: &str,
    ) -> serde_json::Value {
        let target = self.docs_annotation_feedback_target().map(|target| {
            serde_json::json!({
                "agentLabel": target.agent_label,
                "sessionTitle": target.session_title,
                "surface": target.surface.as_str(),
            })
        });
        serde_json::json!({
            "action": action,
            "annotationTarget": target,
            "requestId": request_id,
        })
    }

    fn docs_annotation_feedback_rpc_target(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> Option<DocsAnnotationFeedbackRpcTarget> {
        match self.workspace_terminal_key_for_shell_session(shell_session_id)? {
            GpuiWorkspaceTerminalSessionKey::Local(key) => {
                match gpui_remote_project_reference_from_project_id(key.project_id.as_str()) {
                    Some(reference) => {
                        let target = self.gpui_remote_gxserver_request_target(
                            reference.remote_machine_id.as_str(),
                        )?;
                        Some(DocsAnnotationFeedbackRpcTarget::Remote {
                            target,
                            project_id: reference.project_id,
                            session_id: key.session_id,
                        })
                    }
                    None => Some(DocsAnnotationFeedbackRpcTarget::Local {
                        project_id: key.project_id,
                        session_id: key.session_id,
                    }),
                }
            }
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                let target =
                    self.gpui_remote_gxserver_request_target(key.remote_machine_id.as_str())?;
                Some(DocsAnnotationFeedbackRpcTarget::Remote {
                    target,
                    project_id: key.project_id,
                    session_id: key.session_id,
                })
            }
        }
    }

    /// Handles the Docs page's `sendAnnotationFeedback` request end to end and
    /// dispatches the bridge response itself.
    pub(crate) fn send_docs_annotation_feedback(
        &mut self,
        action: String,
        request_id: String,
        content: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.deliver_docs_annotation_feedback(content, None, cx, move |this, delivery, cx| {
            let response = match delivery {
                Ok(delivery) => {
                    docs_annotation_feedback_delivery_response(&action, &request_id, delivery)
                }
                Err(error) => manage_files_bridge_error_response(&action, &request_id, &error),
            };
            this.dispatch_docs_annotation_feedback_response(&response, cx);
        });
    }

    /// Where feedback goes: the session the document came from when there is one, else the
    /// session last clicked in the sidebar.
    ///
    /// CDXC:Docs 2026-09-25 DECISION:
    /// User chose that notes go back to the agent a document came from: an agent reply opened with Reply by Annotating, or a file opened by clicking it in an agent's chat, sends to that agent's session even after another session is clicked in the sidebar ("12a makes sense"). A file opened from the Docs files list has no origin and keeps the sidebar rule below. When the origin agent is no longer running the notes are copied, never sent to a different agent. This refines the 2026-09-15 rule that every send went to the last sidebar click.
    pub(crate) fn docs_annotation_feedback_target_or_origin(
        &self,
        origin: Option<TerminalSessionId>,
    ) -> Option<DocsAnnotationFeedbackTarget> {
        match origin {
            Some(origin) => self.docs_annotation_feedback_target_for_session(origin),
            None => self.docs_annotation_feedback_target(),
        }
    }

    /// Delivers annotation feedback to the target session's chat or agent CLI (see
    /// `docs_annotation_feedback_target_or_origin`), or copies it, and reports where it went
    /// (`chat`, `terminal` or `clipboard`) to `done`. The terminal path first asks gxserver
    /// whether the agent's input box is on screen.
    pub(crate) fn deliver_docs_annotation_feedback(
        &mut self,
        content: String,
        origin: Option<TerminalSessionId>,
        cx: &mut gpui::Context<Self>,
        done: impl FnOnce(&mut Self, Result<&'static str, String>, &mut gpui::Context<Self>) + 'static,
    ) {
        if content.trim().is_empty() {
            done(self, Err("There is nothing to send.".to_string()), cx);
            return;
        }
        let Some(target) = self.docs_annotation_feedback_target_or_origin(origin) else {
            self.copy_docs_annotation_feedback_to_clipboard(
                &content,
                if origin.is_some() {
                    "The agent session this came from is not running."
                } else {
                    "No running agent session is selected in the sidebar."
                },
                cx,
            );
            done(self, Ok("clipboard"), cx);
            return;
        };
        if target.surface == DocsAnnotationFeedbackSurface::Chat {
            let delivery = if self.insert_docs_annotation_feedback_into_chat(&target, &content, cx)
            {
                "chat"
            } else {
                self.copy_docs_annotation_feedback_to_clipboard(
                    &content,
                    "The chat for that session is not available right now.",
                    cx,
                );
                "clipboard"
            };
            done(self, Ok(delivery), cx);
            return;
        }
        let Some(rpc_target) = self.docs_annotation_feedback_rpc_target(target.shell_session_id)
        else {
            self.copy_docs_annotation_feedback_to_clipboard(
                &content,
                "The selected session's machine cannot be reached.",
                cx,
            );
            done(self, Ok("clipboard"), cx);
            return;
        };
        let background = cx.background_executor().clone();
        let expected_session_id = target.shell_session_id;
        cx.spawn(async move |this, cx| {
            let verdict = background
                .spawn(async move { docs_annotation_feedback_composer_state(rpc_target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                // The target is re-read after the round trip: the user may have
                // clicked another session while gxserver looked at the screen.
                let current = this.docs_annotation_feedback_target_or_origin(origin);
                let delivery = match (current, verdict.as_deref()) {
                    (Some(current), Some("ready"))
                        if current.shell_session_id == expected_session_id
                            && current.surface == DocsAnnotationFeedbackSurface::Terminal
                            && this.insert_manage_file_context_into_agents_session(
                                current.shell_session_id,
                                &content,
                                false,
                                cx,
                            ) =>
                    {
                        "terminal"
                    }
                    (_, Some("notReady")) => {
                        this.copy_docs_annotation_feedback_to_clipboard(
                            &content,
                            "The agent's input box is not available right now.",
                            cx,
                        );
                        "clipboard"
                    }
                    _ => {
                        this.copy_docs_annotation_feedback_to_clipboard(
                            &content,
                            "Ghostex cannot tell whether this agent's input box is ready.",
                            cx,
                        );
                        "clipboard"
                    }
                };
                done(this, Ok(delivery), cx);
            });
        })
        .detach();
    }

    fn insert_docs_annotation_feedback_into_chat(
        &mut self,
        target: &DocsAnnotationFeedbackTarget,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let session_id = target.shell_session_id;
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return false;
        }
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
            return false;
        };
        self.agents_workspace.select_tab(pane_id, session_id);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.deliver_session_chat_composer_insert(session_id, content.to_string(), cx);
        cx.notify();
        true
    }

    fn copy_docs_annotation_feedback_to_clipboard(
        &mut self,
        content: &str,
        reason: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        gpui_copy_to_clipboard(ClipboardItem::new_string(content.to_string()), cx);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: DOCS_ANNOTATION_FEEDBACK_COPIED_TOAST_ID.to_string(),
                level: GpuiAppToastLevel::Info,
                title: "Annotations copied to the clipboard".to_string(),
                description: Some(format!("{reason} Paste them into the agent yourself.")),
                copy_text: None,
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    fn dispatch_docs_annotation_feedback_response(
        &mut self,
        response: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_project_workarea_json_event(
            ProjectWorkareaCefSurfaceSlotKey::Manage,
            "ghostex-manage-files-response",
            &response.to_string(),
            cx,
        );
    }
}

fn docs_annotation_feedback_delivery_response(
    action: &str,
    request_id: &str,
    delivery: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "annotationDelivery": delivery,
        "requestId": request_id,
    })
}

/// `ready`, `notReady`, or `unknown` from gxserver's input-box detector, or
/// `None` when the daemon could not be asked. `unknown` is the normal answer for
/// agents without a measured signature and is treated exactly like a failed
/// read: the text is copied instead of pasted blind.
fn docs_annotation_feedback_composer_state(
    target: DocsAnnotationFeedbackRpcTarget,
) -> Option<String> {
    let result = match target {
        DocsAnnotationFeedbackRpcTarget::Local {
            project_id,
            session_id,
        } => gpui_gxserver_rpc_result(
            "/api/readSessionTerminalTail",
            &serde_json::json!({ "projectId": project_id, "sessionId": session_id }),
            DOCS_ANNOTATION_FEEDBACK_READINESS_TIMEOUT,
        ),
        DocsAnnotationFeedbackRpcTarget::Remote {
            target,
            project_id,
            session_id,
        } => gpui_remote_gxserver_rpc_result(
            &target,
            "/api/readSessionTerminalTail",
            &serde_json::json!({ "projectId": project_id, "sessionId": session_id }),
            DOCS_ANNOTATION_FEEDBACK_READINESS_TIMEOUT,
        ),
    };
    result
        .ok()?
        .get("composerState")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

const PENDING_DOCS_REVIEW_OPEN_MAX_ATTEMPTS: usize = 50;
const PENDING_DOCS_REVIEW_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(100);

impl GhostexGpuiApp {
    /// The chat's Annotate action: opens an agent reply in the Docs review as a
    /// document with no file behind it. The session stays the sidebar's latest
    /// focus, so the review's Send button targets this same session.
    pub(crate) fn open_session_chat_reply_in_docs_review(
        &mut self,
        session_id: TerminalSessionId,
        markdown: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Manage)
            || !self.titlebar_mode_available(TitlebarMode::Manage)
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open the reply for review",
                "The Docs view is not available for this project.",
                cx,
            );
            return;
        }
        let (agent_label, session_title) = self
            .agents_workspace
            .session(session_id)
            .map(|session| {
                (
                    session
                        .agent_icon
                        .map(docs_annotation_feedback_agent_label)
                        .unwrap_or_else(|| "Agent".to_string()),
                    session.title.clone(),
                )
            })
            .unwrap_or_else(|| ("Agent".to_string(), String::new()));
        let id = format!(
            "reply-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis())
                .unwrap_or_default()
        );
        let payload = serde_json::json!({
            "content": markdown,
            "id": id,
            "sessionTitle": session_title,
            "title": format!("Reply from {agent_label}"),
        });
        self.pending_docs_review_open = Some(payload.to_string());
        self.native_docs.pending_origin = Some(session_id);
        self.switch_workarea_from_hotkey(TitlebarMode::Manage, window, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Manage, cx);
        self.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
        if !self.deliver_pending_docs_review_open(cx) {
            self.schedule_pending_docs_review_open_delivery(cx);
        }
    }

    /// Same shape as `deliver_pending_docs_file_open`: the Docs page may not
    /// have mounted yet, so the injected script waits for the page's own hook.
    pub(crate) fn deliver_pending_docs_review_open(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(payload) = self.pending_docs_review_open.clone() else {
            return false;
        };
        if crate::app::native_docs::render::native_docs_enabled() {
            self.pending_docs_review_open = None;
            self.native_docs_open_review(&payload, cx);
            return true;
        }
        let Some(surface) = self
            .project_workarea_runtime_cef_surfaces
            .get(&ProjectWorkareaCefSurfaceSlotKey::Manage)
            .map(|owned_surface| owned_surface.surface.clone())
        else {
            return false;
        };
        let literal = payload
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        let script = format!(
            "(function(){{var p={literal};var a=0;var send=function(){{var open=window.ghostexOpenDocsReview;if(typeof open==='function'){{open(p);return;}}if(++a<250){{setTimeout(send,20);}}}};send();}})(); undefined;"
        );
        let dispatched = surface.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        if dispatched {
            self.pending_docs_review_open = None;
        }
        dispatched
    }

    fn schedule_pending_docs_review_open_delivery(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            for _ in 0..PENDING_DOCS_REVIEW_OPEN_MAX_ATTEMPTS {
                cx.background_executor()
                    .timer(PENDING_DOCS_REVIEW_OPEN_RETRY_INTERVAL)
                    .await;
                match this.update(cx, |this, cx| this.deliver_pending_docs_review_open(cx)) {
                    Ok(false) => {}
                    _ => return,
                }
            }
            let _ = this.update(cx, |this, _| {
                this.pending_docs_review_open = None;
            });
        })
        .detach();
    }
}
