//! Dialog confirms: remote machines, Add Project, missing project folders, session notes, Spaces, custom session tags, agent hook launch, project removal, worktree, Git commit and transcript export dialogs, pinned prompts, Rename, Delayed Send and Close After Done.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_dialog_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "saveRemoteMachinePassword" => {
                self.handle_gpui_save_remote_machine_password_message(command, cx);
            }
            "reconnectRemoteMachine" => {
                self.handle_gpui_reconnect_remote_machine_message(command, cx);
            }
            "probeRemoteGxserverInstall" => {
                self.handle_gpui_probe_remote_gxserver_install_message(command, cx);
            }
            "remoteAppUpdate" => {
                self.handle_remote_app_update_command(command, cx);
            }
            "requestRemoteMachineConnectStates" => {
                self.handle_request_remote_machine_connect_states(cx);
            }
            "addProjectDialogRequest" => {
                self.handle_gpui_add_project_dialog_request_message(command, cx);
            }
            "pickReplacementProjectFolder" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                    .map(str::to_string)
                else {
                    return;
                };
                self.handle_gpui_pick_replacement_project_folder_message(project_id, cx);
            }
            /*
            CDXC:SessionNotes 2026-08-24:
            The Session Note dialog's confirm. Like `removeProject`, this is a
            sidebar-owned write that happens to be issued from an app-modal
            window, so it is handed to the Rust store rather than acted
            on here: the store (gx_store/terminal_lifecycle/session_edits.rs) owns the
            gxserver call and the local/remote machine routing. Only the sidebar session id and the note text
            cross this boundary, and the note is never logged.
            */
            "setSessionNote" => {
                let Some(session_id) = command
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|session_id| gpui_app_modal_sidebar_session_id_allowed(session_id))
                else {
                    return;
                };
                let Some(note) = command.get("note").and_then(serde_json::Value::as_str) else {
                    return;
                };
                let mut message = serde_json::json!({
                    "note": note,
                    "sessionId": session_id,
                    "type": "setSessionNote",
                });
                if let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                {
                    message["projectId"] = serde_json::json!(project_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            /*
            CDXC:Spaces 2026-08-27:
            The New/Edit Space dialog's confirm and delete. Like `setSessionNote`
            this is a sidebar-owned write issued from an app-modal window, so it
            is not acted on here. Its owner was SidebarApp until 2026-09-21; the
            store applies it to the CURRENT Space document now
            (gx_store/space_editor.rs, see `CDXC:Spaces 2026-09-21`).
            */
            "sidebarSpaceEditorResult" => {
                self.forward_gpui_sidebar_space_editor_result_to_sidebar(command, cx);
            }
            /*
            CDXC:Sessions 2026-09-25 WHY:
            The Settings modal creates custom session tags from the app-modal
            host window, so its catalog write arrives here instead of from the
            sidebar page. It is bounded and then pushed from Rust
            (gx_store/custom_tags_sync.rs); supersedes the 2026-09-11 note that
            forwarded it to the sidebar runtime.
            */
            "updateCustomSessionTags" => {
                self.forward_gpui_custom_session_tags_update_to_sidebar(command, cx);
            }
            "confirmAgentHookLaunch" => {
                let bounded_text = |key: &str, max_len: usize| {
                    command
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty() && value.len() <= max_len)
                };
                let Some(agent_id) = bounded_text("agentId", 128) else {
                    return;
                };
                let Some(hook_agent_id) = bounded_text("hookAgentId", 128) else {
                    return;
                };
                let Some(install_hooks) = command
                    .get("installHooks")
                    .and_then(serde_json::Value::as_bool)
                else {
                    return;
                };
                let mut message = serde_json::json!({
                    "agentId": agent_id,
                    "hookAgentId": hook_agent_id,
                    "installHooks": install_hooks,
                    "type": "confirmAgentHookLaunch",
                });
                if let Some(group_id) = bounded_text("groupId", 512) {
                    message["groupId"] = serde_json::json!(group_id);
                }
                if let Some(account_id) = bounded_text("accountId", 256) {
                    message["accountId"] = serde_json::json!(account_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            "removeProject" => {
                let Some(project_id) = command
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                else {
                    return;
                };
                self.dispatch_gpui_sidebar_host_message(
                    serde_json::json!({
                        "projectId": project_id,
                        "type": "removeProject",
                    }),
                    cx,
                );
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "requestProjectWorktrees"
            | "createProjectWorktree"
            | "confirmDeleteWorktree"
            | "confirmRenameWorktree"
            | "commitWorktreeBeforeDelete" => {
                self.forward_gpui_worktree_modal_command_to_sidebar(command_type, command, cx);
            }
            "revealExportedTranscript" => {
                self.reveal_gpui_exported_transcript(cx);
            }
            "cancelExportSessionTranscript"
            | "startExportedTranscriptConversation"
            | "runExportSessionTranscript" => {
                self.forward_gpui_export_transcript_modal_command_to_sidebar(
                    command_type,
                    command,
                    cx,
                );
            }
            "savePinnedPrompt" => {
                self.handle_gpui_save_pinned_prompt_command(command, cx);
            }
            "renameSession" => {
                self.handle_gpui_rename_command_session_command(command, cx);
            }
            "scheduleDelayedSend" => {
                self.handle_gpui_schedule_delayed_send_command(command, cx);
            }
            "postponeDelayedSend" => {
                self.handle_gpui_postpone_delayed_send_command(command, cx);
            }
            "cancelDelayedSend" => {
                self.handle_gpui_cancel_delayed_send_command(command, cx);
            }
            "toggleCloseAfterDone" => {
                self.handle_gpui_toggle_close_after_done_command(command, cx);
            }
            _ => {}
        }
    }
}
