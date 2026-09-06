use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::ClipboardItem;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_app_modal_host_bridge_event(
        &mut self,
        event: cef::AppModalHostBridgeEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let payload = match event {
            cef::AppModalHostBridgeEvent::Message(payload) => payload,
            cef::AppModalHostBridgeEvent::NativeHostMessage(payload) => {
                self.receive_gpui_titlebar_native_host_message(&payload, window, cx);
                return;
            }
        };
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&payload) else {
            return;
        };
        let Some(message_type) = message.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };

        match message_type {
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
                let has_live_command_session = gpui_app_modal_has_required_live_command_session(
                    modal,
                    &message,
                    &self.command_pane,
                );
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
                let sidebar_state_message =
                    self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
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
                let mut open_message = if let Some(allowed_fields) =
                    gpui_app_modal_open_message_allowed_fields(modal)
                {
                    let mut open_message = modal.open_message();
                    for field in allowed_fields {
                        if let Some(value) = message.get(*field).and_then(serde_json::Value::as_str)
                        {
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
                        self.enrich_gpui_agents_delayed_send_open_message(
                            &mut open_message,
                            session_id,
                        );
                    }
                }
                self.open_gpui_app_modal_window(
                    modal,
                    open_message,
                    sidebar_state_message,
                    None,
                    cx,
                );
            }
            "ready" | "presented" | "contentHeightMeasured" => {
                if let Some(handle) = self.app_modal_window.clone() {
                    let _ = handle.update(cx, |host, modal_window, cx| {
                        host.receive_bridge_message(message, modal_window, cx);
                    });
                }
            }
            "gpuiTitlebarTipsUnreadCount" => {
                self.receive_gpui_titlebar_tips_unread_count_message(&message, cx);
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
            "remoteGxserverSubscribePresentation" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_remote_gxserver_subscribe_presentation_message(command, cx);
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
            "copySessionDetails" => {
                if let Some(details_text) = message
                    .get("detailsText")
                    .and_then(serde_json::Value::as_str)
                    .filter(|details_text| !details_text.trim().is_empty())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(details_text.to_string()));
                }
            }
            "gpuiRemoteGxserverSidebarRequest" => {
                if let Some(command) = message.as_object() {
                    self.handle_gpui_remote_gxserver_sidebar_request_message(command, cx);
                }
            }
            "completeFirstLaunchSetup" => {
                let is_first_launch_setup = self.app_modal_window.clone().is_some_and(|handle| {
                    handle
                        .update(cx, |host, _window, _cx| {
                            host.current_modal == GpuiAppModalKind::FirstLaunchSetup
                        })
                        .unwrap_or(false)
                });
                if !is_first_launch_setup {
                    return;
                }
                self.complete_first_launch_setup();
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "close" => {
                if !self.remote_repository_clone_requests.is_empty() {
                    /*
                    CDXC:AddProject 2026-06-24-19:35:
                    The shared Clone Repository modal clears its React dialog immediately after submit. While a GPUI remote clone is pending, keep the native app-modal host alive so the real daemon job can show cancel/final toasts; close the host only after the final toast dismisses instead of dropping visible progress.
                    */
                    return;
                }
                let closing_modal_id = self.app_modal_window.clone().and_then(|handle| {
                    handle
                        .update(cx, |host, _window, _cx| host.current_modal.modal_id())
                        .ok()
                });
                if closing_modal_id.as_deref() == Some("firstLaunchSetup") {
                    return;
                }
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    "gpui.appModal.lifecycle",
                    serde_json::json!({ "action": "close", "modal": closing_modal_id }),
                );
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
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
            "projectWorktreesResult" => {
                // The sidebar runtime answers the Worktree modal's existing
                // worktree/branch list request through the app-modal host, the
                // same route macOS uses. Forward only the shared result fields
                // into the open modal window.
                let Some(request_id) = message
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|request_id| !request_id.trim().is_empty())
                else {
                    return;
                };
                let mut result = serde_json::json!({
                    "ok": message.get("ok").and_then(serde_json::Value::as_bool) == Some(true),
                    "requestId": request_id,
                    "type": "projectWorktreesResult",
                });
                if let Some(error) = message.get("error").and_then(serde_json::Value::as_str) {
                    result["error"] = serde_json::json!(error);
                }
                if let Some(branches) = message.get("branches").filter(|value| value.is_array()) {
                    result["branches"] = branches.clone();
                }
                if let Some(worktrees) = message.get("worktrees").filter(|value| value.is_array()) {
                    result["worktrees"] = worktrees.clone();
                }
                self.dispatch_open_gpui_app_modal_message(result, cx);
            }
            "exportSessionTranscriptResult" => {
                /*
                CDXC:TranscriptExport 2026-08-24:
                The sidebar runtime's answer to the Export Transcript dialog's
                `runExportSessionTranscript` request. Forward only the shared
                result fields into the open modal window, and capture the
                exported path for Reveal in Finder here — the same Rust-held
                state the dialog's old done-stage open message used to seed —
                so Reveal never trusts a path posted back by the modal page.
                */
                let Some(request_id) = message
                    .get("requestId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|request_id| {
                        !request_id.trim().is_empty() && request_id.chars().count() <= 128
                    })
                else {
                    return;
                };
                let ok = message.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
                let can_reveal = message
                    .get("canReveal")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true);
                let path = message
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .filter(|path| !path.trim().is_empty());
                self.pending_export_transcript_reveal_path = (ok && can_reveal)
                    .then(|| path.map(str::to_string))
                    .flatten();
                let mut result = serde_json::json!({
                    "canReveal": can_reveal,
                    "ok": ok,
                    "requestId": request_id,
                    "type": "exportSessionTranscriptResult",
                });
                if let Some(path) = path {
                    result["path"] = serde_json::json!(path);
                }
                if let Some(agent_id) = message.get("agentId").and_then(serde_json::Value::as_str) {
                    result["agentId"] = serde_json::json!(agent_id);
                }
                if let Some(error) = message.get("error").and_then(serde_json::Value::as_str) {
                    result["error"] = serde_json::json!(error);
                }
                self.dispatch_open_gpui_app_modal_message(result, cx);
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
            "closeTitlebarDropdownPanel" => {
                if self.titlebar_popup_menu.is_some() {
                    self.close_gpui_titlebar_popup(None, window, cx);
                }
                if self.titlebar_resources_panel_open {
                    self.set_gpui_titlebar_resources_panel_open(false, window, cx);
                }
                if self.titlebar_tips_panel_open {
                    self.set_gpui_titlebar_tips_panel_open(false, window, cx);
                }
            }
            "toast" => {
                self.receive_gpui_app_toast_bridge_message(&message, cx);
            }
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

    pub(crate) fn receive_gpui_titlebar_native_host_message(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let Some(message_type) = message.get("type").and_then(serde_json::Value::as_str) else {
            return;
        };

        match message_type {
            "sidebarDiagnosticLog" => {
                let Some(scenario_id) = message
                    .get("scenarioId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|scenario_id| !scenario_id.trim().is_empty())
                else {
                    return;
                };
                support_logs::append_for_scenario(
                    if scenario_id == "gpui.sessionChat.drafts" {
                        support_logs::GpuiSupportLog::SessionChat
                    } else {
                        support_logs::GpuiSupportLog::SidebarRefresh
                    },
                    scenario_id,
                    message
                        .get("event")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("gpui.sidebar.diagnostic"),
                    message
                        .get("details")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            navigation_history::NAVIGATION_HISTORY_STATE_MESSAGE_TYPE => {
                self.receive_navigation_history_state_message(&message, cx);
            }
            "runProcess" => {
                self.receive_gpui_titlebar_native_host_run_process(message, cx);
            }
            "titlebarDropdownPanelReady" => {
                self.receive_gpui_titlebar_native_host_dropdown_ready_message(&message, cx);
            }
            "closeTitlebarDropdownPanel" => {
                if self.titlebar_popup_menu.is_some() {
                    self.close_gpui_titlebar_popup(None, window, cx);
                }
                if self.titlebar_resources_panel_open {
                    self.set_gpui_titlebar_resources_panel_open(false, window, cx);
                }
                if self.titlebar_tips_panel_open {
                    self.set_gpui_titlebar_tips_panel_open(false, window, cx);
                }
            }
            "focusResourceSessionFromTitlebar" => {
                self.receive_gpui_titlebar_resources_focus_session_message(&message, window, cx);
            }
            "sleepInactiveSessionsFromTitlebar" => {
                self.receive_gpui_titlebar_resources_sleep_inactive_sessions_message(&message, cx);
            }
            "quitResourcesFromTitlebar" => {
                self.receive_gpui_titlebar_resources_quit_message(&message, window, cx);
            }
            "startGxserverFromTitlebar" => {
                self.show_gpui_gxserver_bootstrap_toast(
                    "info",
                    "Loading sessions",
                    "Starting gxserver and loading projects.",
                    true,
                    cx,
                );
                self.start_gpui_local_gxserver_bootstrap(cx);
                self.dispatch_gpui_titlebar_resources_project_state_update(cx);
            }
            "gxserverPresentationReady" => {
                if !self.sidebar_timer_presentations_replayed_after_ready {
                    /*
                    CDXC:DelayedSend 2026-07-22:
                    Restored timer state is re-armed before the sidebar CEF
                    surface exists. The first renderer presentation hydrate is
                    the earliest authority that its bridge and React runtime
                    can receive timer projections. Discard any pre-ready
                    dispatch snapshots and replay both Agents and Commands
                    summaries exactly once at that boundary so a restored
                    timer cannot remain active without its sidebar chrome.
                    */
                    self.sidebar_command_pane_sessions_snapshot.clear();
                    self.sidebar_agents_delayed_sends_snapshot.clear();
                    let command_timers_replayed =
                        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
                    let agents_timers_replayed =
                        self.refresh_sidebar_agents_delayed_sends_if_changed(cx);
                    self.sidebar_timer_presentations_replayed_after_ready =
                        command_timers_replayed && agents_timers_replayed;
                }
                let loading_toast_visible = self
                    .app_toasts
                    .iter()
                    .any(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID && toast.loading);
                if loading_toast_visible {
                    self.remove_gpui_app_toast(GPUI_GXSERVER_DAEMON_TOAST_ID, cx);
                }
            }
            "stopGxserverFromTitlebar" => {
                self.stop_gpui_local_gxserver_from_titlebar(false, cx);
                self.dispatch_gpui_titlebar_resources_project_state_update(cx);
            }
            "restartGxserverFromTitlebar" => {
                self.stop_gpui_local_gxserver_from_titlebar(true, cx);
                self.dispatch_gpui_titlebar_resources_project_state_update(cx);
            }
            "setGxserverAlwaysStartFromTitlebar" => {
                self.receive_gpui_titlebar_resources_set_gxserver_always_start_message(
                    &message, cx,
                );
            }
            "openExternalUrl" => {
                self.receive_gpui_titlebar_resources_open_external_url_message(&message);
            }
            "resizeTitlebarDropdownPanel" | "titlebarBlankMouseDown" => {}
            _ => {}
        }
    }

    pub(crate) fn receive_gpui_titlebar_native_host_dropdown_ready_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(kind) = message
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .filter(|kind| matches!(*kind, "resources" | "tips"))
        else {
            return;
        };
        if kind != "resources" || !self.titlebar_resources_panel_open {
            return;
        }
        let project_state_update = self.gpui_titlebar_resources_project_state_update(cx);
        self.titlebar_resources_panel_ready = true;
        let Some(panel) = self.titlebar_resources_panel.clone() else {
            cx.notify();
            return;
        };
        let browser = panel.update(cx, |panel, cx| {
            panel.set_visible(true, cx);
            panel.browser(cx)
        });
        gpui_titlebar_resources_dispatch_project_state_update(cx, browser, project_state_update);
        cx.notify();
    }

    pub(crate) fn receive_gpui_titlebar_resources_focus_session_message(
        &mut self,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = message
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
        else {
            return;
        };
        let _ = self.focus_gpui_titlebar_resource_session(session_id, cx);
        self.set_gpui_titlebar_resources_panel_open(false, window, cx);
    }

    pub(crate) fn receive_gpui_titlebar_resources_sleep_inactive_sessions_message(
        &mut self,
        _message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Resources 2026-07-08:
        React sends the exact inactive session ids it derived from the Resources
        rows, but the GPUI sidebar runtime's existing batch path revalidates the
        current inactive set itself. Reuse that owner instead of introducing a
        second explicit-id lifecycle route in this phase.
        */
        let _ = self.dispatch_gpui_workspace_sleep_inactive_sessions(cx);
        self.dispatch_gpui_titlebar_resources_project_state_update(cx);
    }

    pub(crate) fn receive_gpui_titlebar_resources_quit_message(
        &mut self,
        message: &serde_json::Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let session_ids = gpui_titlebar_resources_string_array_field(message, "sessionIds");
        let project_ids = gpui_titlebar_resources_string_array_field(message, "projectIds");
        let mut changed = false;
        let mut seen_sessions = HashSet::new();
        for session_id in session_ids {
            let Some(shell_session_id) = self.gpui_titlebar_resource_shell_session_id(&session_id)
            else {
                /*
                CDXC:Resources 2026-07-26:
                Resources now also lists sessions this window has not mounted,
                so Close cannot stop at the local pane map. Sessions that carry
                a gxserver identity close through the sidebar runtime's existing
                lifecycle route, exactly like a sidebar card close.
                */
                if let Some(key) = gpui_combined_presentation_session_key(&session_id) {
                    changed |=
                        self.dispatch_gpui_workspace_session_key_runtime_action("close", &key, cx);
                }
                continue;
            };
            if !seen_sessions.insert(shell_session_id) {
                continue;
            }
            let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
                continue;
            };
            changed |= self.close_agents_tab(pane_id, shell_session_id, cx);
        }

        if self.gpui_titlebar_resources_project_ids_include_active_project(&project_ids) {
            changed |= self.stop_source_code_server_runtime(cx);
            changed |= self
                .project_editor_shell
                .mark_mode_sleeping(TitlebarMode::Source);
            if changed {
                self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
                self.persist_shell_layout_state();
            }
        }
        if changed {
            cx.notify();
        }
        self.set_gpui_titlebar_resources_panel_open(false, window, cx);
    }

    pub(crate) fn receive_gpui_titlebar_resources_set_gxserver_always_start_message(
        &mut self,
        _message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Resources 2026-07-08:
        GPUI has no shared Settings key for disabling local gxserver startup; the
        app bootstrap already starts/reconciles it. Keep the React action wired
        and refresh daemon status without inventing or faking a persisted
        always-start setting in this phase.
        */
        self.dispatch_gpui_titlebar_resources_project_state_update(cx);
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
        self.active_mode = TitlebarMode::Agents;
        focus_existing_local_workspace_terminal_tab_model(
            &mut self.agents_workspace,
            &mut self.agents_terminal_runtime_sessions,
            pane_id,
            shell_session_id,
        );
        self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
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

    pub(crate) fn gpui_titlebar_resources_project_ids_include_active_project(
        &self,
        project_ids: &[String],
    ) -> bool {
        let Some(active_project_id) = self.gpui_app_modal_active_project_id() else {
            return false;
        };
        project_ids
            .iter()
            .any(|project_id| project_id.as_str() == active_project_id.as_str())
    }

    pub(crate) fn receive_gpui_titlebar_native_host_run_process(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_id = message
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|request_id| !request_id.is_empty())
            .filter(|request_id| {
                request_id.chars().count() <= GPUI_TITLEBAR_NATIVE_PROCESS_REQUEST_ID_MAX_CHARS
            })
            .map(str::to_string);
        let request = match gpui_titlebar_native_process_request_from_message(&message) {
            Ok(request) => request,
            Err(error) => {
                if let Some(request_id) = request_id {
                    self.dispatch_gpui_titlebar_native_process_result(
                        GpuiTitlebarNativeProcessResult::rejected(request_id, error),
                        cx,
                    );
                }
                return;
            }
        };

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_run_titlebar_native_process(request) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_gpui_titlebar_native_process_result(result, cx);
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_titlebar_native_process_result(
        &mut self,
        result: GpuiTitlebarNativeProcessResult,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_titlebar_native_host_event(
            serde_json::json!({
                "exitCode": result.exit_code,
                "requestId": result.request_id,
                "stderr": result.stderr,
                "stdout": result.stdout,
                "type": "processResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_titlebar_native_host_event(
        &mut self,
        event: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(panel) = self.titlebar_resources_panel.clone() {
            panel.update(cx, |panel, cx| {
                panel.dispatch_native_host_event(event.clone(), cx);
            });
            return;
        }
        let Some(panel) = self.titlebar_tips_panel.clone() else {
            return;
        };
        panel.update(cx, |panel, cx| {
            panel.dispatch_native_host_event(event.clone(), cx);
        });
    }
}

/// The string fields an `open` message may carry into the modal host, for the
/// modals whose payload is a flat field set. `None` means "forward the sidebar's
/// message unchanged", which is what the draft-carrying dialogs need.
fn gpui_app_modal_open_message_allowed_fields(
    modal: GpuiAppModalKind,
) -> Option<&'static [&'static str]> {
    match modal {
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
