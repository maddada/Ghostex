//! Open and answers for the message a session dropped on another project opens.
//! SEE-ALSO: apps/desktop/src/app/window/session_handoff_modal.rs (the window entity and its decision record), apps/desktop/src/app/gx_store/git/session_handoff.rs (the handoff).
use crate::app::gx_store::git::SessionHandoffRequest;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;
use std::path::PathBuf;

/// GPUI-owned replacement for the React dialog's `ghostex.exportTranscript.mode`
/// and `ghostex.exportTranscript.includeOptions` localStorage keys.
pub(crate) fn gpui_export_transcript_modal_prefs_path() -> PathBuf {
    ghostex_state_root().join("gpui-export-transcript-modal.json")
}

impl GhostexGpuiApp {
    /// The configured agents that can take a handoff: the sidebar HUD buttons with a launch
    /// command, the same filter the React dialog applied.
    pub(crate) fn gpui_export_transcript_prompt_agents(&self) -> Vec<ExportTranscriptAgent> {
        self.new_thread_picker_agents
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter_map(|value| {
                let field = |key: &str| {
                    value
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                };
                field("command")?;
                Some(ExportTranscriptAgent {
                    agent_id: field("agentId")?,
                    name: field("name")?,
                    // The chat core's own mapping, so the dialog and the model picker agree on
                    // which agent a picked model belongs to.
                    family:
                        ghostex_gx_chat_core::menus::picker::model_picker::model_picker_provider(
                            field("icon").as_deref(),
                        )
                        .map(|family| family.as_str().to_string()),
                })
            })
            .collect()
    }

    /// Opens the message for a drop the sidebar planned.
    pub(crate) fn open_gpui_session_handoff_modal(
        &mut self,
        request: SessionHandoffRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = SessionHandoffModalConfig {
            project_title: request.target_title.clone(),
            agents: self.gpui_export_transcript_prompt_agents(),
            default_agent_id: request.agent_id.clone(),
            working_agent: self.git_session_handoff_working_agent(&request.session),
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_session_handoff_modal_command(&request, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::SessionHandoff,
            SESSION_HANDOFF_MODAL_WIDTH,
            SESSION_HANDOFF_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiSessionHandoffModalWindow::new(config, host, window, cx))
            },
            cx,
        );
        // The cached HUD agents open the message instantly; a fresh read follows and is pushed
        // into it when it lands.
        self.refresh_gpui_new_thread_picker_agents(cx);
    }

    fn handle_gpui_session_handoff_modal_command(
        &mut self,
        request: &SessionHandoffRequest,
        command: SessionHandoffModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::SessionHandoff;
        match command {
            SessionHandoffModalCommand::Cancel => self.release_native_app_modal_window(kind, cx),
            SessionHandoffModalCommand::Confirm {
                agent_id,
                close_source,
                warned,
            } => {
                let working = self.git_session_handoff_working_agent(&request.session);
                if let (Some(agent), false) = (working.clone(), warned) {
                    self.update_native_app_modal(
                        kind,
                        cx,
                        |modal: &mut GpuiSessionHandoffModalWindow, _window, cx| {
                            modal.set_working_agent(agent, cx);
                        },
                    );
                    return;
                }
                self.update_native_app_modal(
                    kind,
                    cx,
                    |_: &mut GpuiSessionHandoffModalWindow, window, _cx| window.remove_window(),
                );
                self.release_native_app_modal_window(kind, cx);
                self.git_run_session_handoff(
                    request.clone(),
                    agent_id,
                    close_source,
                    working.is_some(),
                    cx,
                );
            }
        }
    }

    /// Pushes a refreshed HUD agent list into the open message.
    pub(crate) fn push_gpui_session_handoff_modal_agents(&mut self, cx: &mut gpui::Context<Self>) {
        let agents = self.gpui_export_transcript_prompt_agents();
        self.update_native_app_modal(
            GpuiAppModalKind::SessionHandoff,
            cx,
            |modal: &mut GpuiSessionHandoffModalWindow, _window, cx| modal.set_agents(agents, cx),
        );
    }
}
