//! A session dragged onto another project: stop it if it works, write its conversation, start the
//! new session there, and close the source when the message's toggle says so.
//!
//! Handoff / Export's own path takes the same file and the same launch; this one has no dialog
//! stage of its own, because the drop already named the project.
//!
//! CDXC:Sidebar 2026-10-31 WHY:
//! A failure at any step ends here with a toast saying why, and the source is closed only after the new session's row has arrived: a close before that would pick a replacement to focus while the store is still holding the focus for the new session.
//!
//! SEE-ALSO: packages/gx-core/src/session_handoff.rs (the order of the steps and the draft),
//! packages/gx-core/src/sidebar_drag/session_handoff.rs (which drops hand off),
//! apps/desktop/src/app/session_handoff_modal_lifecycle.rs (the message).

use std::time::Duration;

use ghostex_gx_core::git_menu::GitToastLevel;
use ghostex_gx_core::session_handoff::{
    SessionHandoffOptions, SessionHandoffOutcome, SessionHandoffStep, next_session_handoff_step,
};
use ghostex_gx_core::{MachineId, ProjectKey, SessionKey};
use serde_json::{Value, json};
use web_time::Instant;

use super::calls;
use super::export_transcript::HandoffCoordinator;
use super::toasts::ToastOptions;
use crate::GhostexGpuiApp;
use crate::app::window::{ExportTranscriptIncludeOptions, load_export_transcript_modal_prefs};

/// How long a stopped agent gets to report that its turn ended.
const STOP_WAIT: Duration = Duration::from_secs(20);
/// How long the new session's row gets to arrive before the source closes anyway.
const NEW_ROW_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(200);

/// The drop the message is about.
#[derive(Clone, Debug)]
pub(crate) struct SessionHandoffRequest {
    pub(crate) session: SessionKey,
    pub(crate) target: ProjectKey,
    pub(crate) target_title: String,
    pub(crate) subgroup: Option<String>,
    pub(crate) agent_id: Option<String>,
    session_title: String,
    source_title: String,
    is_coordinator: bool,
}

impl GhostexGpuiApp {
    /// The drop's message: `handoff` is what `with_native_sidebar_session_drop_landing` put on the
    /// command from the store's plan.
    pub(crate) fn git_open_session_handoff(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drop) = self
            .gx_store_plan_sidebar_session_drop(command)
            .and_then(|drop| drop.handoff)
        else {
            return;
        };
        let presentation = self.gx_store.core.presentation();
        let Some(loaded) = presentation.loaded(&drop.session.machine) else {
            return;
        };
        let Some(row) = loaded.server_session(&drop.session.project_id, &drop.session.session_id)
        else {
            return;
        };
        let request = SessionHandoffRequest {
            agent_id: row.agent_id.clone().filter(|id| !id.trim().is_empty()),
            session_title: row
                .display_title
                .clone()
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| row.title.clone()),
            source_title: self.git_export_project_title(&drop.session.project_key()),
            is_coordinator: row.coordinator_role.as_deref() == Some("coordinator"),
            session: drop.session,
            target: drop.target,
            target_title: drop.target_title,
            subgroup: drop.subgroup,
        };
        self.open_gpui_session_handoff_modal(request, cx);
    }

    /// The agent's name while the session's agent is working.
    pub(crate) fn git_session_handoff_working_agent(&self, session: &SessionKey) -> Option<String> {
        let presentation = self.gx_store.core.presentation();
        let row = presentation
            .loaded(&session.machine)?
            .server_session(&session.project_id, &session.session_id)?;
        if row.activity.as_str() != "working" {
            return None;
        }
        Some(
            row.agent_id
                .as_deref()
                .and_then(|id| {
                    self.gpui_export_transcript_prompt_agents()
                        .into_iter()
                        .find(|agent| agent.agent_id == id)
                })
                .map(|agent| agent.name)
                .unwrap_or_else(|| "The agent".to_string()),
        )
    }

    fn git_session_handoff_row_arrived(&self, session: &SessionKey) -> bool {
        self.gx_store
            .core
            .presentation()
            .loaded(&session.machine)
            .and_then(|loaded| loaded.server_session(&session.project_id, &session.session_id))
            .is_some()
    }

    /// The message's button: runs the steps [`next_session_handoff_step`] orders.
    pub(crate) fn git_run_session_handoff(
        &mut self,
        request: SessionHandoffRequest,
        agent_id: String,
        close_source: bool,
        stop_first: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let options = SessionHandoffOptions {
            stop_first,
            close_source,
        };
        let prefs = load_export_transcript_modal_prefs(
            &crate::app::session_handoff_modal_lifecycle::gpui_export_transcript_modal_prefs_path(),
        );
        let include = prefs.include;
        // The same History the dialog remembers; this computer's gxserver is this app's own.
        let last_turns = prefs.history.last_turns();
        cx.spawn(async move |this, cx| {
            let session = &request.session;
            let mut exported = None;
            let mut exported_coordinator = None;
            let mut created = None;
            let mut step = next_session_handoff_step(options, SessionHandoffOutcome::Confirmed);
            loop {
                let outcome = match step {
                    SessionHandoffStep::StopTurn => {
                        let stop = calls::call(
                            None,
                            "/api/interruptSessionChat",
                            json!({
                                "projectId": session.project_id,
                                "sessionId": session.session_id,
                            }),
                            None,
                        )
                        .await
                        .map_err(|error| error.message);
                        let stopped = match stop {
                            Err(error) => Err(error),
                            Ok(_) => {
                                let deadline = Instant::now() + STOP_WAIT;
                                loop {
                                    let working = this
                                        .read_with(cx, |this, _| {
                                            this.git_session_handoff_working_agent(session)
                                        })
                                        .ok()
                                        .flatten();
                                    if working.is_none() {
                                        break Ok(());
                                    }
                                    if Instant::now() >= deadline {
                                        break Err(format!(
                                            "{} did not stop, so nothing was handed over.",
                                            working.unwrap_or_default()
                                        ));
                                    }
                                    cx.background_executor().timer(POLL).await;
                                }
                            }
                        };
                        match stopped {
                            Ok(()) => SessionHandoffOutcome::Stopped,
                            Err(why) => SessionHandoffOutcome::StopFailed(why),
                        }
                    }
                    SessionHandoffStep::ExportTranscript => {
                        match export_transcript(
                            session,
                            include,
                            last_turns,
                            request.is_coordinator,
                        )
                        .await
                        {
                            Ok((path, coordinator)) => {
                                exported = Some(path);
                                exported_coordinator = coordinator;
                                SessionHandoffOutcome::Exported
                            }
                            Err(why) => SessionHandoffOutcome::ExportFailed(why),
                        }
                    }
                    SessionHandoffStep::StartSession => {
                        let path = exported.clone().unwrap_or_default();
                        let coordinator = exported_coordinator.clone();
                        let started = this.update(cx, |this, cx| {
                            this.git_start_session_handoff(
                                &request,
                                &agent_id,
                                &path,
                                coordinator,
                                cx,
                            )
                        });
                        let started = match started {
                            Ok(Ok(task)) => task.await,
                            Ok(Err(why)) => Err(why),
                            Err(_) => return,
                        };
                        match started {
                            Ok(new_session) => {
                                let _ = this.update(cx, |this, cx| {
                                    if let Some(subgroup) = &request.subgroup {
                                        this.gx_store_place_created_session_in_subgroup(
                                            &request.target,
                                            &new_session.session_id,
                                            subgroup,
                                            cx,
                                        );
                                    }
                                });
                                created = Some(new_session);
                                SessionHandoffOutcome::Started
                            }
                            Err(why) => SessionHandoffOutcome::StartFailed(why),
                        }
                    }
                    SessionHandoffStep::CloseSource => {
                        if let Some(new_session) = &created {
                            let deadline = Instant::now() + NEW_ROW_WAIT;
                            while Instant::now() < deadline
                                && !this
                                    .read_with(cx, |this, _| {
                                        this.git_session_handoff_row_arrived(new_session)
                                    })
                                    .unwrap_or(true)
                            {
                                cx.background_executor().timer(POLL).await;
                            }
                        }
                        // The user chose to hand this session off, which closes it: no second
                        // question (gx_store/sidebar_close_confirm.rs).
                        let _ = this.update(cx, |this, cx| {
                            let confirmed =
                                std::mem::replace(&mut this.gx_store.session_close_confirmed, true);
                            this.dispatch_native_sidebar_command(
                                json!({
                                    "type": "closeSession",
                                    "sessionId": session.to_sidebar_session_id(),
                                    "closeSource": "handoff",
                                }),
                                cx,
                            );
                            this.gx_store.session_close_confirmed = confirmed;
                        });
                        return;
                    }
                    SessionHandoffStep::Finished => return,
                    SessionHandoffStep::Failed(why) => {
                        let _ = this.update(cx, |this, cx| {
                            this.git_toast(
                                GitToastLevel::Error,
                                &format!("Could not hand off to {}", request.target_title),
                                ToastOptions::described(why),
                                cx,
                            );
                        });
                        return;
                    }
                };
                step = next_session_handoff_step(options, outcome);
            }
        })
        .detach();
    }

    /// The new session, or why it can't start. Answers a task so the step waits for it.
    fn git_start_session_handoff(
        &mut self,
        request: &SessionHandoffRequest,
        agent_id: &str,
        path: &str,
        coordinator: Option<HandoffCoordinator>,
        cx: &mut gpui::Context<Self>,
    ) -> Result<gpui::Task<Result<SessionKey, String>>, String> {
        let agent = self
            .git_hud_agent(agent_id)
            .ok_or_else(|| "That agent is no longer available.".to_string())?;
        let scope = self
            .git_scope_for_key(request.target.clone())
            .map_err(|_| format!("{} is no longer available.", request.target_title))?;
        debug_assert_eq!(request.target.machine, MachineId::Local);
        Ok(self.git_start_handoff_session(
            &scope,
            agent,
            ghostex_gx_core::session_handoff::handoff_draft(
                path,
                &request.session_title,
                &request.source_title,
            ),
            coordinator,
            None,
            cx,
        ))
    }
}

/// `/api/exportSessionTranscript` on this computer: the file's path, or the daemon's own message.
async fn export_transcript(
    session: &SessionKey,
    include: ExportTranscriptIncludeOptions,
    last_turns: Option<usize>,
    is_coordinator: bool,
) -> Result<(String, Option<HandoffCoordinator>), String> {
    let mut params = json!({
        "includeCommands": include.commands,
        "includePatches": include.patches,
        "includeReasoning": include.reasoning,
        "projectId": session.project_id,
        "sessionId": session.session_id,
    });
    if let Some(last_turns) = last_turns {
        params["lastTurns"] = json!(last_turns);
    }
    let answer = calls::call(None, "/api/exportSessionTranscript", params, None)
        .await
        .map_err(|error| error.message)?;
    let path = answer
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "gxserver did not return the exported file.".to_string())?;
    let coordinator =
        super::export_transcript::handoff_coordinator(None, &answer, is_coordinator, session).await;
    Ok((path, coordinator))
}
