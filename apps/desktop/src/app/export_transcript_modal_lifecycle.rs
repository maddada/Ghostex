//! Open, data, and sidebar bridge plumbing for the native Handoff / Export dialog.
//! SEE-ALSO: apps/desktop/src/app/window/export_transcript_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

use crate::app::session_handoff_modal_lifecycle::gpui_export_transcript_modal_prefs_path;

impl GhostexGpuiApp {
    /*
    CDXC:TranscriptExport 2026-09-25 WHY:
    The session context, the gxserver export call (local and remote), the
    exported path and the follow-up session are Rust's since the app runtime
    port (gx_store/git/export_transcript.rs), which supersedes the 2026-09-15
    note that the sidebar runtime owned them. The dialog still posts the same
    `runExportSessionTranscript`, `startExportedTranscriptConversation` and
    `cancelExportSessionTranscript` commands through the same allowlist and
    receives the same `exportSessionTranscriptResult` answer.
    */
    /// Opens the native dialog for the sidebar's `open` message of the
    /// `exportTranscriptResult` modal kind.
    pub(crate) fn open_gpui_export_transcript_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = message
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|request_id| !request_id.is_empty() && request_id.chars().count() <= 128)
            .map(str::to_string)
        else {
            return;
        };
        let default_agent_id = message
            .get("agentId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|agent_id| !agent_id.is_empty())
            .map(str::to_string);
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let handoff_target = text("targetProvider").map(|family| ExportTranscriptHandoffTarget {
            family,
            model: text("targetModel").unwrap_or_default(),
        });
        let config = ExportTranscriptModalConfig {
            agents: self.gpui_export_transcript_prompt_agents(),
            default_agent_id,
            palette: self.gpui_native_dialog_palette(),
            prefs_path: Some(gpui_export_transcript_modal_prefs_path()),
            // A handover from the chat model picker opens on Handoff without rewriting the remembered mode.
            initial_mode: handoff_target
                .is_some()
                .then_some(ExportTranscriptMode::Handoff),
            handoff_target,
            computers: self.git_export_handoff_computers(),
            restore: self.git_export_take_restore(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_export_transcript_modal_command(&request_id, command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::ExportTranscriptResult,
            EXPORT_TRANSCRIPT_MODAL_WIDTH,
            EXPORT_TRANSCRIPT_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiExportTranscriptModalWindow::new(config, host, window, cx))
            },
            cx,
        );
        // The cached HUD agents open the dialog instantly; a fresh read follows
        // and is pushed into the open dialog when it lands.
        self.refresh_gpui_new_thread_picker_agents(cx);
    }

    fn handle_gpui_export_transcript_modal_command(
        &mut self,
        request_id: &str,
        command: ExportTranscriptModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::ExportTranscriptResult;
        let mut message = serde_json::Map::new();
        message.insert("requestId".to_string(), serde_json::json!(request_id));
        match command {
            ExportTranscriptModalCommand::RunExport {
                include,
                last_turns,
                carry,
            } => {
                message.insert("includeMarkdown".to_string(), serde_json::json!(carry));
                if let Some(last_turns) = last_turns {
                    message.insert("lastTurns".to_string(), serde_json::json!(last_turns));
                }
                message.insert(
                    "includeCommands".to_string(),
                    serde_json::json!(include.commands),
                );
                message.insert(
                    "includePatches".to_string(),
                    serde_json::json!(include.patches),
                );
                message.insert(
                    "includeReasoning".to_string(),
                    serde_json::json!(include.reasoning),
                );
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    "runExportSessionTranscript",
                    &message,
                    cx,
                );
            }
            ExportTranscriptModalCommand::CountTurns(include) => {
                message.insert(
                    "includeCommands".to_string(),
                    serde_json::json!(include.commands),
                );
                message.insert(
                    "includePatches".to_string(),
                    serde_json::json!(include.patches),
                );
                message.insert(
                    "includeReasoning".to_string(),
                    serde_json::json!(include.reasoning),
                );
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    "countExportSessionTranscriptTurns",
                    &message,
                    cx,
                );
            }
            ExportTranscriptModalCommand::StartConversation { agent_id, target } => {
                message.insert("agentId".to_string(), serde_json::json!(agent_id));
                if let Some(target) = target {
                    message.insert(
                        "targetMachineId".to_string(),
                        serde_json::json!(target.machine_id),
                    );
                    message.insert(
                        "targetProjectId".to_string(),
                        serde_json::json!(target.project_id),
                    );
                }
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    "startExportedTranscriptConversation",
                    &message,
                    cx,
                );
                self.pending_export_transcript_reveal_path = None;
                self.release_native_app_modal_window(kind, cx);
            }
            ExportTranscriptModalCommand::Cancel => {
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    "cancelExportSessionTranscript",
                    &message,
                    cx,
                );
                self.pending_export_transcript_reveal_path = None;
                self.release_native_app_modal_window(kind, cx);
            }
            ExportTranscriptModalCommand::Reveal => {
                self.reveal_gpui_exported_transcript(cx);
                self.pending_export_transcript_reveal_path = None;
                self.release_native_app_modal_window(kind, cx);
            }
            // A page has no copy sound; the dialog's own button says Path Copied.
            #[cfg(not(target_arch = "wasm32"))]
            ExportTranscriptModalCommand::PathCopied => gpui_copy_feedback(cx),
            #[cfg(target_arch = "wasm32")]
            ExportTranscriptModalCommand::PathCopied => {}
            ExportTranscriptModalCommand::TargetChanged(target) => {
                self.git_export_read_branches(target, cx);
            }
            ExportTranscriptModalCommand::AddProjectOn { machine_id, picks } => {
                // The dialog closed itself without cancelling: the export request stays parked
                // for the reopen once Add Project closes.
                self.release_native_app_modal_window(kind, cx);
                self.git_export_add_project_on(&machine_id, picks, cx);
            }
        }
    }

    /// Delivers the export's sanitized `exportSessionTranscriptResult`
    /// to the native dialog. Returns false when no native dialog is open so the
    /// caller can hand the result to the React modal host instead.
    pub(crate) fn receive_gpui_export_transcript_result(
        &mut self,
        result: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let text = |key: &str| {
            result
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let ok = result.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
        let can_reveal = result.get("canReveal").and_then(serde_json::Value::as_bool) == Some(true);
        let (path, agent_id, error) = (text("path"), text("agentId"), text("error"));
        self.update_native_app_modal(
            GpuiAppModalKind::ExportTranscriptResult,
            cx,
            |modal: &mut GpuiExportTranscriptModalWindow, window, cx| {
                modal.receive_result(ok, path, can_reveal, agent_id, error, window, cx);
            },
        )
        .is_some()
    }

    /// The computer's turn count for History's hint (gx_store/git/export_transcript.rs).
    pub(crate) fn push_gpui_export_transcript_turn_count(
        &mut self,
        include: ExportTranscriptIncludeOptions,
        count: ExportTranscriptTurnCount,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_native_app_modal(
            GpuiAppModalKind::ExportTranscriptResult,
            cx,
            |modal: &mut GpuiExportTranscriptModalWindow, _window, cx| {
                modal.set_turn_count(include, count, cx);
            },
        );
    }

    /// A handoff's branch notice for `target` (gx_store/git/export_handoff_remote.rs).
    pub(crate) fn push_gpui_export_transcript_branch_notice(
        &mut self,
        target: ExportTranscriptRemoteTarget,
        line: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_native_app_modal(
            GpuiAppModalKind::ExportTranscriptResult,
            cx,
            |modal: &mut GpuiExportTranscriptModalWindow, _window, cx| {
                modal.set_branch_notice(target, line, cx);
            },
        );
    }

    /// Pushes a refreshed HUD agent list into the open dialog.
    pub(crate) fn push_gpui_export_transcript_modal_agents(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let agents = self.gpui_export_transcript_prompt_agents();
        self.update_native_app_modal(
            GpuiAppModalKind::ExportTranscriptResult,
            cx,
            |modal: &mut GpuiExportTranscriptModalWindow, _window, cx| modal.set_agents(agents, cx),
        );
        self.push_gpui_session_handoff_modal_agents(cx);
    }
}
