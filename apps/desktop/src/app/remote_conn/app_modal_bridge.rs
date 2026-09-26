// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_app_modal_host_bridge_event(
        &mut self,
        event: cef::AppModalHostBridgeEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let cef::AppModalHostBridgeEvent::Message(payload) = event;
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&payload) else {
            return;
        };
        let Some(message_type) = message.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };

        match message_type {
            "requestDelayedSendAgents" => self.request_delayed_send_agents(&message, cx),
            "browserHistoryQuery" | "browserHistoryOpen" => {
                self.receive_browser_history_message(&message, cx);
            }
            "accountTitlebarChanged" => self.update_titlebar_account_from_ui(&message, window, cx),
            // CDXC:Settings 2026-09-06 DECISION: Account setup runs its displayed sign-in command with one click in an interactive terminal, using the existing terminal launcher.
            "accountSetup" => {
                if message.get("machineId").and_then(serde_json::Value::as_str) != Some("local") {
                    return;
                }
                let title = match message.get("provider").and_then(serde_json::Value::as_str) {
                    Some("claude") => "Claude account sign-in",
                    Some("codex") => "Codex account sign-in",
                    _ => return,
                };
                let Some(command) = message
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                else {
                    return;
                };
                let snapshot = self.latest_sidebar_project_snapshot.as_ref();
                if gpui_active_project_id_from_snapshot(snapshot)
                    .is_some_and(|id| id.starts_with("remote:"))
                {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Account sign-in unavailable",
                        "Open a local project to sign in to an account on this computer.",
                        cx,
                    );
                    return;
                }
                // First-launch account setup happens before the user has chosen a project.
                let Some(cwd) = gpui_active_local_project_directory(snapshot)
                    .map(std::path::Path::to_path_buf)
                    .or_else(|| {
                        gpui_active_project_id_from_snapshot(snapshot)
                            .is_none()
                            .then(|| std::env::var_os("HOME").map(std::path::PathBuf::from))
                            .flatten()
                    })
                    .filter(|path| path.is_absolute())
                else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Account sign-in unavailable",
                        "The active project's folder is unavailable. Open a local project first.",
                        cx,
                    );
                    return;
                };
                if self.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "createQuickTerminal", "command": command, "cwd": cwd, "title": title,
                    }),
                    cx,
                ) {
                    self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                }
            }
            "findPromptsHostAction" => {
                self.receive_find_prompts_modal_host_action(&message, window, cx);
            }
            #[cfg(target_os = "windows")]
            "downloadGhostexUpdate" => {
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                self.download_windows_update(cx);
            }
            #[cfg(target_os = "windows")]
            "restartAndUpdateGhostex" => {
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                self.restart_and_apply_windows_update(cx);
            }
            "open" => {
                self.open_app_modal_from_bridge(message, cx);
            }
            "ready" | "presented" | "contentHeightMeasured" => {
                if let Some(handle) = self.app_modal_window.clone() {
                    let _ = handle.update(cx, |host, modal_window, cx| {
                        host.receive_bridge_message(message, modal_window, cx);
                    });
                }
            }
            "updateSettings" => {
                self.handle_gpui_app_modal_update_settings_message(&message, cx);
            }
            "updateSettingsPatch" => {
                self.handle_gpui_app_modal_update_settings_patch_message(&message, cx);
            }
            "listAppIcons" => {
                self.handle_gpui_list_app_icons_message(cx);
            }
            "setAppIcon" => {
                self.handle_gpui_set_app_icon_message(&message, cx);
            }
            "pickAppIconFile" => {
                self.handle_gpui_pick_app_icon_file_message(cx);
            }
            "pickTerminalBackgroundImageFile" => {
                self.handle_gpui_pick_terminal_background_image_message(cx);
            }
            "pickWindowGlassImageFile" => {
                self.handle_gpui_pick_window_glass_image_message(&message, cx);
            }
            "listGlassVideoLibrary" => {
                self.handle_gpui_list_glass_video_library_message(cx);
            }
            "downloadGlassVideo" => {
                self.handle_gpui_download_glass_video_message(&message, cx);
            }
            "cancelGlassVideoDownload" => {
                self.handle_gpui_cancel_glass_video_download_message(&message);
            }
            "removeGlassVideo" => {
                self.handle_gpui_remove_glass_video_message(&message, cx);
            }
            "listWindowGlassVideos" => {
                self.handle_gpui_list_window_glass_videos_message(cx);
            }
            "pickWindowGlassVideoFile" => {
                self.handle_gpui_pick_window_glass_video_message(&message, cx);
            }
            "pickFirstLaunchProjectFolder" => {
                self.handle_gpui_pick_first_launch_project_folder_message(cx);
            }
            "firstLaunchCreateProjectSession" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_first_launch_create_project_session_message(command, cx);
                }
            }
            "revealAppIconsFolder" => {
                app_icon::reveal_icons_directory();
            }
            "saveRemoteMachinePassword" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_save_remote_machine_password_message(command, cx);
                }
            }
            "reconnectRemoteMachine" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_reconnect_remote_machine_message(command, cx);
                }
            }
            "probeRemoteGxserverInstall" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_probe_remote_gxserver_install_message(command, cx);
                }
            }
            "browseRemoteProjectDirectories" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_browse_remote_project_directories_message(command, cx);
                }
            }
            "addRemoteProjectPath" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_add_remote_project_path_message(command, cx);
                }
            }
            "addProjectDialogRequest" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_add_project_dialog_request_message(command, cx);
                }
            }
            "pickWorkspaceFolder" => {
                self.handle_gpui_pick_workspace_folder_message(cx);
            }
            "pickRepositoryFolder" => {
                self.handle_gpui_pick_repository_folder_message(cx);
            }
            "completeFirstLaunchSetup" => {
                let is_first_launch_setup = self.app_modal_window.clone().is_some_and(|handle| {
                    handle
                        .update(cx, |host, _window, _cx| {
                            matches!(
                                host.current_modal,
                                GpuiAppModalKind::FirstLaunchSetup | GpuiAppModalKind::Onboarding
                            )
                        })
                        .unwrap_or(false)
                });
                if !is_first_launch_setup {
                    return;
                }
                self.complete_first_launch_setup();
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "close" => self.close_app_modal_from_bridge(cx),
            "toastDismissed" => {
                if message.get("keepOpen").and_then(serde_json::Value::as_bool) == Some(true)
                    || !self.remote_repository_clone_requests.is_empty()
                {
                    return;
                }
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "sidebarCommand" => {
                self.handle_gpui_app_modal_sidebar_command(message, window, cx);
            }
            "firstLaunchCreateProjectSessionResult" => {
                let Some(command) = message.as_object() else {
                    return;
                };
                let Some(request_id) = gpui_remote_request_id_from_command(command) else {
                    return;
                };
                let ok = message.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
                let mut result = serde_json::json!({
                    "ok": ok,
                    "requestId": request_id,
                    "type": "firstLaunchCreateProjectSessionResult",
                });
                if let Some(error) = message
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|error| !error.is_empty())
                {
                    result["error"] = serde_json::json!(error);
                }
                self.dispatch_open_gpui_app_modal_message(result, cx);
            }
            "pickWorktreeImages" => {
                self.handle_gpui_pick_worktree_images_message(cx);
            }
            "toast" => {
                self.receive_gpui_app_toast_bridge_message(&message, cx);
            }
            // CDXC:Clipboard 2026-09-15 SEE-ALSO: packages/core-ui/copy-sound.ts posts this from every React copy site; Rust owns the sound and its setting, and since 2026-09-22 the "Copied!" bubble too (gpui_copy_feedback).
            "playCopySound" => gpui_copy_feedback(cx),
            /*
            CDXC:Settings 2026-07-29:
            The shared React modal host already reports its uncaught renderer
            exceptions (`logError`, installed by
            `installAppModalGlobalErrorLogging`) and its Settings lifecycle
            breadcrumbs (`debugLog`) over this same app-owned bridge, and the CEF
            shim installs the `ghostexAppModalHost` handler both helpers post
            through. GPUI had no arm for either message, so both fell through to
            the no-op below: a render error that blanked the Settings window left
            no trace anywhere under the resolved Ghostex logs directory, which is why a blank
            Settings report could not be diagnosed from a user's machine at all.

            Persist both through the existing sanitized AppModal writer. The error
            event name contains `error`, so `event_is_important_diagnostic` keeps
            recording it even while the routine `gpui.app.modal` scenario is off,
            while routine breadcrumbs stay opt-in behind that scenario. Stacks
            carry paths and URLs, so they are reported as a presence flag and
            never stored; `details` is parsed back into structured JSON so the
            writer can sanitize each bounded field instead of redacting one long
            string wholesale.
            */
            "logError" => {
                let current_modal_id = self.app_modal_window.clone().and_then(|handle| {
                    handle
                        .update(cx, |host, _window, _cx| host.current_modal.modal_id())
                        .ok()
                });
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    "gpui.appModal.rendererError",
                    serde_json::json!({
                        "area": message.get("area").and_then(serde_json::Value::as_str),
                        "errorMessage": message.get("message").and_then(serde_json::Value::as_str),
                        "errorName": message.get("name").and_then(serde_json::Value::as_str),
                        "hasStack": message
                            .get("stack")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|stack| !stack.trim().is_empty()),
                        "modal": current_modal_id,
                    }),
                );
            }
            "debugLog" => {
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    message
                        .get("event")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("gpui.appModal.debugLog"),
                    message
                        .get("details")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|details| serde_json::from_str::<serde_json::Value>(details).ok())
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            _ => {}
        }
    }

    pub(crate) fn receive_gpui_titlebar_resources_open_external_url_message(
        &self,
        message: &serde_json::Value,
    ) {
        let Some(url) = message
            .get("url")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
        else {
            return;
        };
        let _ = gpui_open_external_http_url(url);
    }

    pub(crate) fn focus_gpui_titlebar_resource_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if let Some(key) = gpui_combined_presentation_session_key(session_id) {
            return self.focus_existing_gpui_local_workspace_terminal(&key, cx);
        }
        let Some(shell_session_id) = gpui_agents_session_id_from_external_id(session_id) else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return false;
        };
        self.change_active_mode_with_pane_state(TitlebarMode::Agents, cx);
        focus_existing_local_workspace_terminal_tab_model(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            pane_id,
            shell_session_id,
        );
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        self.set_sidebar_focus_border_handoff_target(shell_session_id);
        self.request_agents_session_text_focus_handoff(
            AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id: shell_session_id,
            },
            cx,
        );
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn gpui_titlebar_resource_shell_session_id(
        &mut self,
        session_id: &str,
    ) -> Option<TerminalSessionId> {
        self.prune_local_workspace_session_mappings();
        if let Some(key) = gpui_combined_presentation_session_key(session_id) {
            return self.local_workspace_session_mappings.get(&key).copied();
        }
        gpui_agents_session_id_from_external_id(session_id)
            .filter(|shell_session_id| self.agents_workspace.has_session(*shell_session_id))
    }
}

/// The string fields an `open` message may carry into the modal host, for the
/// modals whose payload is a flat field set. `None` means "forward the sidebar's
/// message unchanged", which is what the draft-carrying dialogs need.
fn gpui_app_modal_open_message_allowed_fields(
    modal: GpuiAppModalKind,
) -> Option<&'static [&'static str]> {
    match modal {
        GpuiAppModalKind::MermaidDiagram | GpuiAppModalKind::MarkdownTable => Some(&["source"]),
        GpuiAppModalKind::RecentProjects => Some(&["machineId", "machineName"]),
        GpuiAppModalKind::SidebarSpaceEditor => Some(&[
            "memberCollectionId",
            "memberProjectId",
            "mode",
            "remoteMachineId",
            "sectionKey",
            "spaceColor",
            "spaceIcon",
            "spaceId",
            "spaceName",
        ]),
        _ => None,
    }
}

impl GhostexGpuiApp {
    /// What the bridge's `open` arm does: validate the modal, rebuild its open message from the
    /// allowlist, attach the sidebar state, and OPEN THE WINDOW.
    ///
    /// CDXC:AppModal 2026-09-21 WHY:
    /// Moved out of the `open` arm so the sidebar store opens a dialog through the same code the old runtime's `openAppModal` reaches, instead of through `dispatch_open_gpui_app_modal_message`, which only delivers a message INTO a window that already exists and returns silently when none does. The store's Rename, Note, More menu rows, Configure, Space editor, Add Worktree and History all called that one, so every one of them did nothing unless a dialog happened to be open already, while their gates (which compare the planned call, not the host that performs it) stayed clean.
    pub(crate) fn open_app_modal_from_bridge(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(modal) = message
            .get("modal")
            .and_then(serde_json::Value::as_str)
            .and_then(GpuiAppModalKind::from_modal_id)
        else {
            return;
        };
        /*
        CDXC:Git 2026-07-26:
        The commit review dialog asks native for each changed file's
        patch while it opens, and native answers with a `gitFileDiff`
        open message. For an open commit modal that payload is inline
        right-pane state, not a second dialog: the React host consumes
        it without changing `activeModal`. GPUI runs one reusable
        app-modal window, so routing it through the normal open path
        retitled and replaced the commit window with the standalone
        File Diff modal. Deliver it into the live window instead.
        */
        /*
        CDXC:TranscriptExport 2026-08-20:
        Capture the exported file's path from the dialog's own open
        message so Reveal in Finder runs against Rust-held state. A
        remote export never lands on this machine, so the sidebar
        marks it `canReveal: false` and Rust holds nothing to reveal.
        */
        if modal == GpuiAppModalKind::ExportTranscriptResult {
            self.pending_export_transcript_reveal_path = message
                .get("canReveal")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                .then(|| message.get("path").and_then(serde_json::Value::as_str))
                .flatten()
                .map(str::to_string);
        }
        if modal == GpuiAppModalKind::GitFileDiff
            && self.gpui_app_modal_current_modal(cx) == Some(GpuiAppModalKind::GitCommit)
        {
            self.dispatch_open_gpui_app_modal_message(message, cx);
            return;
        }
        let has_live_command_session =
            gpui_app_modal_has_required_live_command_session(modal, &message, &self.command_pane);
        if !has_live_command_session {
            let Some(external_session_id) =
                message.get("sessionId").and_then(serde_json::Value::as_str)
            else {
                return;
            };
            if !matches!(
                modal,
                GpuiAppModalKind::DelayedSend | GpuiAppModalKind::RenameSession
            ) || !gpui_app_modal_sidebar_session_id_allowed(external_session_id)
            {
                return;
            }
            if modal == GpuiAppModalKind::DelayedSend {
                // Activating the exact terminal prepares its mounted
                // send target, but presentation must not depend on it.
                let _ = self.focus_gpui_titlebar_resource_session(external_session_id, cx);
            }
        }
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        /*
        CDXC:Spaces 2026-08-28:
        A modal whose whole open payload is a known, flat set of string
        fields is rebuilt from its own `open_message()` template plus the
        allowlist below, so nothing else the sidebar page put on the
        message can reach the modal host. Modals whose payload is a
        structured draft (the worktree and diff dialogs) still forward
        their message verbatim, because there is no flat field list to
        enumerate for them.
        */
        let mut open_message =
            if let Some(allowed_fields) = gpui_app_modal_open_message_allowed_fields(modal) {
                let mut open_message = modal.open_message();
                for field in allowed_fields {
                    if let Some(value) = message.get(*field).and_then(serde_json::Value::as_str) {
                        open_message[*field] = serde_json::json!(value);
                    }
                }
                open_message
            } else {
                message
            };
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        if modal == GpuiAppModalKind::DelayedSend && !has_live_command_session {
            let external_session_id = open_message
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            if let Some(session_id) =
                external_session_id
                    .as_deref()
                    .and_then(|external_session_id| {
                        self.gpui_titlebar_resource_shell_session_id(external_session_id)
                    })
            {
                self.enrich_gpui_agents_delayed_send_open_message(&mut open_message, session_id);
            }
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
    /// What `closeAppModal(...)` does. Moved out of the bridge's `close` arm so the sidebar's own
    /// Rename and Note, which close the open dialog before they open theirs, run the SAME close
    /// rather than a second copy of it (gx_store/sidebar_modals.rs).
    pub(crate) fn close_app_modal_from_bridge(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.remote_repository_clone_requests.is_empty() {
            /*
            CDXC:AddProject 2026-06-24-19:35:
            The shared Clone Repository modal clears its React dialog immediately after submit. While a GPUI remote clone is pending, keep the native app-modal host alive so the real daemon job can show cancel/final toasts; close the host only after the final toast dismisses instead of dropping visible progress.
            */
            return;
        }
        if self.app_modal_window.is_none() && self.close_native_app_modal_from_bridge(cx) {
            return;
        }
        let closing_modal_id = self.app_modal_window.clone().and_then(|handle| {
            handle
                .update(cx, |host, _window, _cx| host.current_modal.modal_id())
                .ok()
        });
        if matches!(
            closing_modal_id.as_deref(),
            Some("firstLaunchSetup") | Some("onboarding")
        ) {
            return;
        }
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.appModal.lifecycle",
            serde_json::json!({ "action": "close", "modal": closing_modal_id }),
        );
        self.close_gpui_app_modal_window_and_restore_command_focus(cx);
    }
}
