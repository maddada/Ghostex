//! Previous Sessions commands: the list and transcript sizes, restoring and deleting a previous session, and the retired search-by-text row.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(super) fn handle_gpui_app_modal_previous_sessions_command(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        match command_type {
            "requestPreviousSessions" => {
                let request = gpui_previous_sessions_request_from_command(command);
                let remote_sources = self.connected_gpui_remote_previous_session_sources();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_previous_sessions_result_message(request, remote_sources),
                    cx,
                );
            }
            "requestSessionTranscriptSizes" => {
                let request = gpui_session_transcript_sizes_request_from_command(command);
                let remote_sources = self.connected_gpui_remote_previous_session_sources();
                self.run_gpui_app_modal_sidebar_status_task(
                    move || gpui_session_transcript_sizes_result_message(request, remote_sources),
                    cx,
                );
            }
            "restorePreviousSession" => {
                if let Some(history_id) = command
                    .get("historyId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let remote_sources = self.connected_gpui_remote_previous_session_sources();
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |this, cx| {
                        let restored = background
                            .spawn(async move {
                                gpui_restore_previous_session_from_history_id(
                                    &history_id,
                                    &remote_sources,
                                )
                            })
                            .await;
                        let Some(restored) = restored else {
                            return;
                        };
                        let _ = this.update(cx, |this, cx| {
                            match restored {
                                GpuiPreviousSessionRestoreResult::Local {
                                    project_id,
                                    session_id,
                                } => {
                                    match gpui_combined_presentation_session_focus_id(
                                        &project_id,
                                        &session_id,
                                    ) {
                                        // In the window that shows the session's workspace
                                        // (workspace_windows/session_routing.rs).
                                        Some(focus_id) => this.run_in_session_window(
                                            &focus_id,
                                            cx,
                                            move |app, cx| {
                                                app.restore_local_previous_session(
                                                    project_id, session_id, cx,
                                                )
                                            },
                                        ),
                                        None => this.restore_local_previous_session(
                                            project_id, session_id, cx,
                                        ),
                                    }
                                }
                                GpuiPreviousSessionRestoreResult::Remote {
                                    remote_machine_id,
                                    project_id,
                                    session_id,
                                } => {
                                    this.refresh_gpui_remote_gxserver_presentation_in_background(&remote_machine_id);
                                    let scoped_session_id = gpui_remote_scoped_session_id(
                                        remote_machine_id.as_str(),
                                        project_id.as_str(),
                                        session_id.as_str(),
                                    );
                                    if let Some(reference) =
                                        gpui_remote_attach_session_reference_from_project_id(
                                            scoped_session_id.as_str(),
                                        )
                                    {
                                        this.arm_default_view_chat_launch_intent(
                                            GpuiWorkspaceTerminalSessionKey::Remote(
                                                GpuiRemoteAttachSessionKey::from(&reference),
                                            ),
                                        );
                                    }
                                    this.handle_gpui_remote_session_native_action(
                                        GpuiSidebarNativeProjectPathActionMessage {
                                            action:
                                                GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal,
                                            file_path: None,
                                            preferred_interface:
                                                GpuiPreferredAgentInterface::Terminal,
                                            project_id: gpui_remote_scoped_session_id(
                                                remote_machine_id.as_str(),
                                                project_id.as_str(),
                                                session_id.as_str(),
                                            ),
                                            keep_view: false,
                                            target_id: None,
                                        },
                                        cx,
                                    );
                                }
                            }
                        });
                    })
                    .detach();
                }
            }
            "deletePreviousSession" => {
                if let Some(history_id) = command
                    .get("historyId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    let remote_sources = self.connected_gpui_remote_previous_session_sources();
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |_this, _cx| {
                        background
                            .spawn(async move {
                                gpui_delete_previous_session_from_history_id(
                                    &history_id,
                                    &remote_sources,
                                );
                            })
                            .await;
                    })
                    .detach();
                }
            }
            "searchPreviousSessionsByText" => {
                /*
                CDXC:Sessions 2026-06-24-11:53:
                The shared Previous Sessions modal no longer renders Search by Text launch buttons, and GPUI does not yet have enough current-project launch authority here to recreate macOS's direct text-search terminal honestly. Keep the legacy command harmless and response-free instead of faking a terminal launch or claiming success.
                */
            }
            _ => {}
        }
    }

    /// Restores one of this computer's previous sessions in this window: its row, then the
    /// normal attach sequence.
    fn restore_local_previous_session(
        &mut self,
        project_id: String,
        session_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Sessions 2026-07-11:
        macOS restores a previous terminal by creating its
        replacement row and then running the normal attach
        sequence. A focus-only dispatch lets the presentation
        reconciler create a placeholder, but does not provide
        that placeholder with gxserver's resume/attach payload,
        leaving an empty shell. Start the same local attach path
        directly here, using the currently focused Agents pane
        as the restore placement target.
        */
        // The restored session opens in its agent's Default Agent View
        // (`CDXC:SessionChat 2026-09-30 DECISION` in session_chat_launch.rs).
        self.arm_default_view_chat_launch_intent(GpuiWorkspaceTerminalSessionKey::Local(
            GpuiLocalWorkspaceSessionKey {
                project_id: project_id.clone(),
                session_id: session_id.clone(),
            },
        ));
        if let Some(focus_id) =
            gpui_combined_presentation_session_focus_id(&project_id, &session_id)
        {
            let _ = self.dispatch_gpui_command_palette_session_focus(&focus_id, cx);
        }
        let key = GpuiLocalWorkspaceSessionKey {
            project_id,
            session_id,
        };
        self.local_workspace_latest_focus_key = Some(key.clone());
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        let requested_pane_id = self.agents_workspace.focused_pane;
        if self.focus_existing_gpui_local_workspace_terminal(&key, cx) {
            return;
        }
        let attach_intent = self.local_workspace_attach_intent_for_key(&key);
        if !self.local_workspace_attach_pending.insert(key.clone()) {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let prepare_key = key.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_local_workspace_attach_terminal_plan(&prepare_key, attach_intent)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.local_workspace_attach_pending.remove(&key);
                if this.local_workspace_latest_focus_key.as_ref() != Some(&key) {
                    return;
                }
                match result {
                    Ok(plan) => {
                        let _ = this.open_gpui_local_workspace_terminal(
                            key,
                            plan,
                            requested_pane_id,
                            false,
                            cx,
                        );
                    }
                    Err(message) => this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Session restore unavailable",
                        message.as_str(),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }
}
