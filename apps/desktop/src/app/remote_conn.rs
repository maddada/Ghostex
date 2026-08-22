// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: app-modal host bridge events and remote gxserver connect/clone/attach

use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui::ClipboardItem;
use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::hotkeys::*;
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
                CDXC:GPUIGitCommitInlineDiff 2026-07-26:
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
                CDXC:ExportTranscript 2026-08-20:
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
                let mut open_message = if modal == GpuiAppModalKind::RecentProjects {
                    let mut open_message = modal.open_message();
                    for field in ["machineId", "machineName"] {
                        if let Some(value) = message.get(field).and_then(serde_json::Value::as_str)
                        {
                            open_message[field] = serde_json::json!(value);
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
            "close" => {
                if !self.remote_repository_clone_requests.is_empty() {
                    /*
                    CDXC:RemoteClone 2026-06-24-19:35:
                    The shared Clone Repository modal clears its React dialog immediately after submit. While a GPUI remote clone is pending, keep the native app-modal host alive so the real daemon job can show cancel/final toasts; close the host only after the final toast dismisses instead of dropping visible progress.
                    */
                    return;
                }
                let closing_modal_id = self.app_modal_window.clone().and_then(|handle| {
                    handle
                        .update(cx, |host, _window, _cx| host.current_modal.modal_id())
                        .ok()
                });
                support_logs::append(
                    support_logs::GpuiSupportLog::AppModal,
                    "gpui.appModal.lifecycle",
                    serde_json::json!({ "action": "close", "modal": closing_modal_id }),
                );
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                if closing_modal_id.as_deref() == Some("firstLaunchSetup") {
                    self.complete_first_launch_setup();
                }
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
            CDXC:SettingsModalBlankUnnormalizedHydrate 2026-07-29:
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
                    support_logs::GpuiSupportLog::SidebarRefresh,
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
                    CDXC:GPUIAgentsDelayedSendPersistence 2026-07-22:
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
        CDXC:GPUIResourcesTitlebar 2026-07-08:
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
                CDXC:GPUIResourcesDevServers 2026-07-26:
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
        CDXC:GPUIResourcesTitlebar 2026-07-08:
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

    pub(crate) fn handle_gpui_app_modal_update_settings_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsPersistence 2026-06-24-11:14:
        The shared React Settings modal posts normalized `updateSettings` objects through the app-modal host. GPUI must persist only object-shaped payloads through the shared settings service, reject malformed or non-object messages without panicking, then use the returned in-memory snapshot to refresh runtime settings and rehydrate the open modal host so the saved revision/object cannot drift.
        */
        let Some(settings_object) = message
            .get("settings")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        let previous_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let previous_agent_settings = previous_settings_snapshot.gxserver_agent_settings();
        let previous_settings_object = previous_settings_snapshot.object().clone();
        #[cfg(target_os = "windows")]
        let previous_windows_terminal_backend =
            windows_terminal_backend::WindowsTerminalBackendPreference::from_settings_value(
                previous_settings_object
                    .get("windowsTerminalBackend")
                    .and_then(serde_json::Value::as_str),
            );
        #[cfg(target_os = "windows")]
        let previous_windows_wsl_distribution = previous_settings_object
            .get("windowsWslDistribution")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut settings_object = settings_object.clone();
        // Only explicit remote-machine UI and sidebar ordering saves may replace the
        // saved machine list; broad Settings saves keep the stored value. Mirrors
        // canSettingsUpdateSourceChangeRemoteMachines in packages/shared/ghostex-settings.ts.
        let source_can_change_remote_machines = matches!(
            message.get("source").and_then(serde_json::Value::as_str),
            Some("settings:remoteMachines") | Some("sidebar:remoteMachineOrder")
        );
        if !source_can_change_remote_machines {
            match previous_settings_object.get("remoteMachines") {
                Some(previous_remote_machines) => {
                    settings_object.insert(
                        "remoteMachines".to_string(),
                        previous_remote_machines.clone(),
                    );
                }
                None => {
                    settings_object.remove("remoteMachines");
                }
            }
        }
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };

        let next_agent_settings = write_result.snapshot.gxserver_agent_settings();
        let ghostty_config_backed_setting_keys_changed =
            shared_settings::ghostty_terminal_config_backed_setting_keys_changed(
                &previous_settings_object,
                write_result.snapshot.object(),
            );
        cx.bind_keys(gpui_configured_hotkey_unbinds_from_settings(
            &previous_settings_snapshot,
        ));
        self.sync_gpui_ghostty_config_file_after_settings_save(
            &ghostty_config_backed_setting_keys_changed,
            &write_result.snapshot,
            cx,
        );
        // Config-backed terminal settings must reach Ghostty's managed file
        // before live GPUI terminals reload it. The old order reloaded the
        // previous theme and only wrote the new theme afterwards.
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.sync_gpui_gxserver_agent_settings_after_save(
            previous_agent_settings,
            next_agent_settings,
            cx,
        );
        self.sync_gpui_portless_settings_after_save(
            &previous_settings_object,
            write_result.snapshot.object(),
            cx,
        );
        self.sync_gpui_removed_remote_machine_passwords_after_settings_save(
            &previous_settings_object,
            write_result.snapshot.object(),
            cx,
        );
        let previous_app_icon_source_id =
            app_icon::source_id_from_settings(&previous_settings_object);
        let next_app_icon_source_id =
            app_icon::source_id_from_settings(write_result.snapshot.object());
        if previous_app_icon_source_id != next_app_icon_source_id {
            let _ = app_icon::apply_persisted_source_id(&next_app_icon_source_id);
        }
        #[cfg(target_os = "windows")]
        {
            let next_windows_terminal_backend =
                windows_terminal_backend::WindowsTerminalBackendPreference::from_settings_value(
                    write_result
                        .snapshot
                        .object()
                        .get("windowsTerminalBackend")
                        .and_then(serde_json::Value::as_str),
                );
            let next_windows_wsl_distribution = write_result
                .snapshot
                .object()
                .get("windowsWslDistribution")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if previous_windows_terminal_backend != next_windows_terminal_backend
                || previous_windows_wsl_distribution != next_windows_wsl_distribution
            {
                /*
                WSL distribution changes apply to newly spawned ConPTY
                terminals. Drop only the process-memory distro selection/token
                and bootstrap the new distro; existing terminal processes are
                not killed or silently migrated.
                */
                windows_terminal_backend::reset();
                self.replay_sidebar_gxserver_bootstrap(cx);
                self.start_gpui_local_gxserver_bootstrap(cx);
            }
        }
    }

    /// Granular Settings controls save through `updateSettingsPatch` (the modal's
    /// `onPatch` path), not bulk `updateSettings`. Merging the patch onto the
    /// current stored snapshot — not the modal's `baseRevision` view — is what
    /// makes concurrent saves safe, matching macOS `saveSidebarSettingsPatch`.
    pub(crate) fn handle_gpui_app_modal_update_settings_patch_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(patch_object) = message.get("patch").and_then(serde_json::Value::as_object) else {
            return;
        };
        let mut merged_settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        for (key, value) in patch_object {
            merged_settings.insert(key.clone(), value.clone());
        }
        let merged_message = serde_json::json!({
            "settings": merged_settings,
            "source": message.get("source").cloned().unwrap_or(serde_json::Value::Null),
        });
        self.handle_gpui_app_modal_update_settings_message(&merged_message, cx);
    }

    pub(crate) fn handle_gpui_save_remote_machine_password_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachinesSettings 2026-06-24-13:36:
        Remote Machine password saves are explicit one-shot Settings commands. GPUI may accept the transient password only from this command, validate the bounded remote machine id, write or delete macOS Keychain first, and persist only the `sshPasswordSaved` marker after Keychain success.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "GPUI could not save the SSH password for this remote machine.",
                cx,
            );
            return;
        };
        let Some(password) = command
            .get("password")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "GPUI could not save the SSH password for this remote machine.",
                cx,
            );
            return;
        };

        if !gpui_settings_object_has_remote_machine_id(
            shared_settings::shared_sidebar_settings_snapshot().object(),
            &remote_machine_id,
        ) {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "SSH password not saved",
                "The remote machine changed before GPUI could update its saved password.",
                cx,
            );
            return;
        }

        let has_password = !password.is_empty();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let keychain_result = background
                .spawn({
                    let remote_machine_id = remote_machine_id.clone();
                    async move {
                        let result = gpui_save_remote_machine_password_to_keychain(
                            &remote_machine_id,
                            password.as_str(),
                        );
                        drop(password);
                        result
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_remote_machine_password_save(
                    remote_machine_id,
                    has_password,
                    keychain_result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn finish_gpui_remote_machine_password_save(
        &mut self,
        remote_machine_id: String,
        has_password: bool,
        keychain_result: GpuiRemoteSshPasswordKeychainResult,
        cx: &mut gpui::Context<Self>,
    ) {
        match keychain_result {
            GpuiRemoteSshPasswordKeychainResult::Success => {}
            GpuiRemoteSshPasswordKeychainResult::Unsupported => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "Remote SSH password storage is only available on macOS.",
                    cx,
                );
                return;
            }
            GpuiRemoteSshPasswordKeychainResult::Failed => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    gpui_remote_machine_password_failure_title(has_password),
                    "macOS Keychain could not update the SSH password.",
                    cx,
                );
                return;
            }
        }

        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        if !gpui_set_remote_machine_password_marker(
            &mut settings_object,
            &remote_machine_id,
            has_password,
        ) {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                gpui_remote_machine_password_failure_title(has_password),
                "The remote machine changed before GPUI could update its saved password.",
                cx,
            );
            return;
        }

        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                gpui_remote_machine_password_failure_title(has_password),
                "GPUI could not update Settings after the Keychain operation.",
                cx,
            );
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.dispatch_gpui_app_modal_toast(
            "success",
            if has_password {
                "SSH password saved"
            } else {
                "SSH password removed"
            },
            if has_password {
                "The password is stored in macOS Keychain."
            } else {
                "The Keychain password was removed."
            },
            cx,
        );
    }

    pub(crate) fn handle_gpui_probe_remote_gxserver_install_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-19:
        Settings asks whether a saved remote machine already runs gxserver so
        its action can read Install or Update and show the installed version.
        The command carries only the bounded machine id; SSH details come from
        the shared Settings snapshot exactly like reconnect.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        self.probe_gpui_remote_gxserver_install(remote_machine_id, cx);
    }

    pub(crate) fn probe_gpui_remote_gxserver_install(
        &mut self,
        remote_machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.app_modal_window.is_none() {
            /*
            Nothing can render the answer while no app modal is open, so skip
            the SSH round trip instead of probing every saved machine on the
            automatic startup connect pass.
            */
            return;
        }
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            &remote_machine_id,
        ) else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let probe = background
                .spawn(async move { gpui_probe_remote_gxserver_install(config) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut message = serde_json::json!({
                    "installed": probe.installed,
                    "remoteMachineId": remote_machine_id,
                    "type": "remoteGxserverInstallState",
                });
                if let Some(version) = probe.version
                    && let Some(object) = message.as_object_mut()
                {
                    object.insert("version".to_string(), serde_json::Value::String(version));
                }
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_reconnect_remote_machine_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachinesSettings 2026-06-24-14:34:
        Settings `reconnectRemoteMachine` must mirror the macOS app's Remote gxserver connect button: read the saved remote machine from shared Settings, start/read the remote daemon token over SSH, store only the token in Keychain, then open a checked localhost tunnel. The command may carry only the bounded machine id, install approval flag, and automatic-attempt flag; it must not carry host/user/path/token/password/command/output data from React.

        CDXC:GPUIRemoteMachines 2026-06-24-20:08:
        Approved install retries should surface the existing `installing` remote-machine state while Rust uploads/installs the bundled package, but React still provides no SSH details, package paths, commands, tokens, stdout/stderr, or daemon response authority.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote connect failed",
                "GPUI could not identify the remote machine to connect.",
                cx,
            );
            return;
        };
        let install_approved = command
            .get("installApproved")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let automatic = command
            .get("automatic")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let connect_generation =
            self.next_gpui_remote_gxserver_connect_generation(&remote_machine_id);
        if self
            .source_code_server_runtime
            .target
            .as_ref()
            .is_some_and(|target| {
                matches!(
                    &target.endpoint,
                    SourceCodeServerRuntimeEndpoint::Remote {
                        remote_machine_id: runtime_machine_id,
                        ..
                    } if runtime_machine_id == &remote_machine_id
                )
            })
        {
            self.stop_source_code_server_runtime(cx);
        }
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            &remote_machine_id,
        ) else {
            self.dispatch_gpui_remote_machine_status(remote_machine_id.as_str(), "invalid", cx);
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote connect failed",
                "The saved remote machine is missing or incomplete.",
                cx,
            );
            return;
        };

        self.stop_gpui_remote_gxserver_connection(&remote_machine_id);
        let status_state = if install_approved {
            "installing"
        } else {
            GpuiRemoteGxserverConnectState::Connecting.wire_status_state()
        };
        self.dispatch_gpui_remote_machine_status(remote_machine_id.as_str(), status_state, cx);
        if !automatic {
            self.dispatch_gpui_app_modal_toast(
                "info",
                if install_approved {
                    "Installing remote gxserver"
                } else {
                    "Connecting remote gxserver"
                },
                if install_approved {
                    "GPUI is installing the remote gxserver package on the saved remote machine."
                } else {
                    "GPUI is connecting to the saved remote machine over SSH."
                },
                cx,
            );
        }
        let (progress_tx, mut progress_rx) = mpsc::unbounded::<GpuiRemoteGxserverConnectProgress>();
        let progress_remote_machine_id = remote_machine_id.clone();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = progress_rx.next().await {
                let should_continue = this
                    .update(cx, |this, cx| {
                        if !this.gpui_remote_gxserver_connect_generation_is_current(
                            progress_remote_machine_id.as_str(),
                            connect_generation,
                        ) {
                            return false;
                        }
                        if !this
                            .remote_machine_connect_states
                            .get(progress_remote_machine_id.as_str())
                            .is_some_and(|state| {
                                gpui_remote_gxserver_status_state_is_connect_progress(
                                    state.as_str(),
                                )
                            })
                        {
                            return false;
                        }
                        this.dispatch_gpui_remote_machine_status(
                            progress_remote_machine_id.as_str(),
                            progress.state.wire_status_state(),
                            cx,
                        );
                        true
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_connect_remote_gxserver(config, install_approved, Some(progress_tx))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_gpui_reconnect_remote_machine(
                    remote_machine_id,
                    connect_generation,
                    automatic,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn finish_gpui_reconnect_remote_machine(
        &mut self,
        remote_machine_id: String,
        connect_generation: u64,
        automatic: bool,
        mut result: GpuiRemoteGxserverConnectResult,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gpui_remote_gxserver_connect_generation_is_current(
            remote_machine_id.as_str(),
            connect_generation,
        ) {
            result.terminate_connection();
            return;
        }
        match result.state {
            GpuiRemoteGxserverConnectState::Connected => {
                if let Some(connection) = result.connection {
                    /*
                    Reconnects replace the machine's connection entry. The
                    outgoing connection owns an `ssh -N` tunnel child that
                    nothing kills on drop, so terminate it explicitly or every
                    reconnect leaks a live tunnel process.
                    */
                    if let Some(mut replaced) = self
                        .remote_gxserver_connections
                        .insert(remote_machine_id.clone(), connection)
                    {
                        replaced.terminate();
                    }
                    self.restart_gpui_remote_gxserver_presentation_stream(
                        remote_machine_id.clone(),
                        gpui_remote_gxserver_presentation_client_id(remote_machine_id.as_str()),
                        None,
                        cx,
                    );
                    self.clear_project_editor_companion_remote_attach_states_for_machine(
                        remote_machine_id.as_str(),
                    );
                    self.reattach_project_editor_companion_remote_terminal_after_reconnect(
                        remote_machine_id.as_str(),
                        connect_generation,
                        cx,
                    );
                    let reconnects_active_remote_docs = self
                        .latest_sidebar_project_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.active_project_id.as_ref())
                        .and_then(|project_id| {
                            gpui_remote_project_reference_from_project_id(project_id.0.as_str())
                        })
                        .is_some_and(|reference| reference.remote_machine_id == remote_machine_id);
                    if reconnects_active_remote_docs {
                        /*
                        The synthetic Docs resource handler captures the exact
                        tunnel target that owns it. Recreate only the active
                        remote Docs surface after reconnect so resource reads
                        use the replacement tunnel instead of a dead port/token.
                        */
                        self.remove_project_workarea_runtime_cef_surface(
                            ProjectWorkareaCefSurfaceSlotKey::Manage,
                            cx,
                        );
                        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
                    }
                    self.ensure_source_code_server_runtime_for_current_context(cx);
                }
                self.dispatch_gpui_remote_machine_status(
                    remote_machine_id.as_str(),
                    "connected",
                    cx,
                );
                /*
                A connect may have installed or upgraded the remote package, so
                refresh the version Settings shows next to its Install/Update
                action instead of leaving the pre-connect answer on screen.
                */
                self.probe_gpui_remote_gxserver_install(remote_machine_id.clone(), cx);
                if !automatic {
                    self.dispatch_gpui_app_modal_toast(
                        "success",
                        "Remote gxserver connected",
                        "The remote gxserver tunnel is ready.",
                        cx,
                    );
                }
            }
            GpuiRemoteGxserverConnectState::InstallApprovalRequired => {
                self.dispatch_gpui_remote_machine_status(
                    remote_machine_id.as_str(),
                    "installApprovalRequired",
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Install approval required",
                    "gxserver is not installed on that machine. Approve the install to continue.",
                    cx,
                );
                self.open_gpui_remote_gxserver_install_modal(remote_machine_id, cx);
            }
            _ => {
                self.dispatch_gpui_remote_machine_status_with_message(
                    remote_machine_id.as_str(),
                    result.state.wire_status_state(),
                    Some(result.message.as_str()),
                    cx,
                );
                if !automatic {
                    self.dispatch_gpui_app_modal_toast(
                        result.state.toast_level(),
                        result.state.toast_title(),
                        result.message.as_str(),
                        cx,
                    );
                }
            }
        }
    }

    pub(crate) fn reattach_project_editor_companion_remote_terminal_after_reconnect(
        &mut self,
        remote_machine_id: &str,
        connection_generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.active_mode.is_project_editor_mode()
            || !self.project_editor_shell.left_companion_visible
        {
            return;
        }
        let Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) =
            self.project_editor_companion_active_terminal_key()
        else {
            return;
        };
        if key.remote_machine_id != remote_machine_id {
            return;
        }
        if !self.gpui_remote_gxserver_connect_generation_is_current(
            remote_machine_id,
            connection_generation,
        ) {
            return;
        }
        let Some(shell_session_id) = self.remote_attach_sessions.get(&key).copied() else {
            return;
        };
        let Some(slot_id) = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| slot_id.session_id == shell_session_id)
        else {
            return;
        };
        let attempt = GpuiProjectEditorCompanionRemoteAttachAttempt {
            connection_generation,
            remote_key: key.clone(),
        };
        if !self.project_editor_companion_remote_attach_attempt_is_current(slot_id, &attempt) {
            return;
        }
        self.project_editor_companion_remote_attach_states.insert(
            slot_id,
            GpuiProjectEditorCompanionRemoteAttachState::Preparing(attempt.clone()),
        );
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            self.record_project_editor_companion_remote_attach_unavailable(
                slot_id,
                attempt,
                "Reconnect the remote machine to show this terminal.".to_string(),
            );
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings_snapshot.object(), remote_machine_id)
        else {
            self.record_project_editor_companion_remote_attach_unavailable(
                slot_id,
                attempt,
                "The saved remote machine is missing required SSH settings.".to_string(),
            );
            return;
        };
        let reference = GpuiRemoteAttachSessionReference {
            remote_machine_id: key.remote_machine_id,
            project_id: key.project_id,
            session_id: key.session_id,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let prepare_reference = reference.clone();
            let result = background
                .spawn(async move {
                    gpui_prepare_remote_attach_terminal_plan(
                        &config,
                        &target,
                        &prepare_reference,
                        true,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this.gpui_remote_gxserver_connect_generation_is_current(
                    reference.remote_machine_id.as_str(),
                    connection_generation,
                ) || !this
                    .project_editor_companion_remote_attach_states
                    .get(&slot_id)
                    .is_some_and(|state| state.attempt() == &attempt)
                    || !this.project_editor_companion_remote_attach_attempt_is_current(
                        slot_id, &attempt,
                    )
                {
                    if this
                        .project_editor_companion_remote_attach_states
                        .get(&slot_id)
                        .is_some_and(|state| state.attempt() == &attempt)
                    {
                        this.project_editor_companion_remote_attach_states
                            .remove(&slot_id);
                    }
                    return;
                }
                let expected_key = GpuiRemoteAttachSessionKey::from(&reference);
                if this.project_editor_companion_active_terminal_key()
                    != Some(GpuiWorkspaceTerminalSessionKey::Remote(expected_key))
                {
                    this.project_editor_companion_remote_attach_states
                        .remove(&slot_id);
                    return;
                }
                let plan = match result {
                    Ok(plan) => plan,
                    Err(message) => {
                        this.record_project_editor_companion_remote_attach_unavailable(
                            slot_id, attempt, message,
                        );
                        cx.notify();
                        return;
                    }
                };
                this.project_editor_companion_remote_attach_states
                    .remove(&slot_id);
                this.open_gpui_remote_attach_terminal(
                    reference,
                    plan,
                    None,
                    AgentsWorkspaceNewTerminalPlacement::Tab,
                    GpuiRemoteAttachOpenIntent::AttachExistingSession,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_remote_gxserver_subscribe_presentation_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        let Some(client_id) = gpui_remote_presentation_client_id_from_command(command) else {
            return;
        };
        let last_revision = command
            .get("lastRevision")
            .and_then(serde_json::Value::as_u64);
        self.restart_gpui_remote_gxserver_presentation_stream(
            remote_machine_id,
            client_id,
            last_revision,
            cx,
        );
    }

    pub(crate) fn open_gpui_remote_gxserver_install_modal(
        &mut self,
        remote_machine_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machine_name =
            gpui_remote_machine_name_from_settings(remote_machine_id.as_str())
                .unwrap_or_else(|| "Remote".to_string());
        let open_message = serde_json::json!({
            "modal": "remoteGxserverInstall",
            "remoteMachineId": remote_machine_id,
            "remoteMachineName": remote_machine_name,
            "type": "open",
        });
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(
            GpuiAppModalKind::RemoteGxserverInstall,
            cx,
        );
        self.open_gpui_app_modal_window(
            GpuiAppModalKind::RemoteGxserverInstall,
            open_message,
            sidebar_state_message,
            None,
            cx,
        );
    }

    pub(crate) fn handle_gpui_browse_remote_project_directories_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote machine is unavailable."),
                cx,
            );
            return;
        };
        let Some(partial_path) =
            gpui_remote_path_like_string_from_command(command, "partialPath", true)
        else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote path is invalid."),
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            self.dispatch_gpui_remote_project_directory_browse_result(
                request_id,
                false,
                None,
                Some("Remote machine is not connected."),
                cx,
            );
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/browseProjectDirectories",
                        &serde_json::json!({ "partialPath": partial_path }),
                        Duration::from_secs(15),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => this.dispatch_gpui_remote_project_directory_browse_result(
                    request_id,
                    true,
                    Some(result),
                    None,
                    cx,
                ),
                Err(_) => this.dispatch_gpui_remote_project_directory_browse_result(
                    request_id,
                    false,
                    None,
                    Some("Remote directory browse failed."),
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_add_remote_project_path_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote machine is unavailable."),
                cx,
            );
            return;
        };
        let Some(path) = gpui_remote_path_like_string_from_command(command, "path", false) else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote path is invalid."),
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            self.dispatch_gpui_remote_project_add_result(
                request_id,
                false,
                None,
                Some("Remote machine is not connected."),
                cx,
            );
            return;
        };
        let fallback_project_path = path.clone();
        let project_name = gpui_remote_project_name_from_path(path.as_str());
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/addProjectPath",
                        &serde_json::json!({
                            "name": project_name,
                            "path": path,
                        }),
                        GPUI_ADD_PROJECT_DIALOG_ADD_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        let project_path = result
                            .get("project")
                            .and_then(|project| project.get("path"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                            .unwrap_or(fallback_project_path);
                        this.dispatch_gpui_remote_project_add_result(
                            request_id,
                            true,
                            Some(project_path),
                            None,
                            cx,
                        );
                    }
                    Err(_) => this.dispatch_gpui_remote_project_add_result(
                        request_id,
                        false,
                        None,
                        Some("Remote project add failed."),
                        cx,
                    ),
                }
                /*
                CDXC:AddProject 2026-07-30:
                Refresh the machine's presentation on BOTH arms. A remote add
                that lands after our request gives up (slow reconnect, dropped
                answer) still registered the project on that machine, and the
                machine's presentation stream is frequently the thing that was
                broken in the first place — so a failure answer is exactly when
                a snapshot pull is needed for the project to become visible.
                */
                this.refresh_gpui_remote_gxserver_presentation_in_background(
                    remote_machine_id,
                    false,
                    cx,
                );
            });
        })
        .detach();
    }

    /*
    CDXC:AddProject 2026-07-30:
    The shared add-project dialog runs in the app-modal child window and reaches
    gxserver only through this request/response pair. `machineId` is the whole
    routing vocabulary: the local machine id goes to the local daemon, a saved
    remote machine id goes through that machine's live tunnel, and an id with no
    live tunnel is answered with an explicit "not connected" error instead of
    silently falling back to the local filesystem.
    */
    pub(crate) fn handle_gpui_add_project_dialog_request_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(operation) = command
            .get("operation")
            .and_then(serde_json::Value::as_str)
            .and_then(GpuiAddProjectDialogOperation::from_wire)
        else {
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                false,
                None,
                Some("The add-project request was invalid."),
                cx,
            );
            return;
        };
        support_logs::append(
            support_logs::GpuiSupportLog::AppModal,
            "gpui.addProject.request",
            serde_json::json!({ "operation": operation.as_wire() }),
        );
        if operation == GpuiAddProjectDialogOperation::ListMachines {
            let machines = self.gpui_add_project_dialog_machine_options();
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                true,
                Some(serde_json::json!({ "machines": machines })),
                None,
                cx,
            );
            return;
        }
        let requested_machine_id = command
            .get("machineId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|machine_id| !machine_id.is_empty())
            .unwrap_or(GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID);
        let remote_machine_id = if requested_machine_id == GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID
        {
            None
        } else {
            match gpui_normalize_remote_machine_id(requested_machine_id) {
                Some(remote_machine_id) => Some(remote_machine_id),
                None => {
                    self.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some("That machine is unavailable."),
                        cx,
                    );
                    return;
                }
            }
        };
        let empty_params = serde_json::Map::new();
        let raw_params = command
            .get("params")
            .and_then(serde_json::Value::as_object)
            .unwrap_or(&empty_params);
        let Some(params) = gpui_add_project_dialog_params(operation, raw_params) else {
            self.dispatch_gpui_add_project_dialog_result(
                request_id,
                false,
                None,
                Some("The add-project request was invalid."),
                cx,
            );
            return;
        };
        #[cfg(target_os = "windows")]
        let params = if remote_machine_id.is_none() {
            match gpui_add_project_dialog_translate_local_windows_paths(operation, params) {
                Ok(params) => params,
                Err(error) => {
                    self.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some(error.as_str()),
                        cx,
                    );
                    return;
                }
            }
        } else {
            params
        };
        let target = match remote_machine_id.as_deref() {
            Some(remote_machine_id) => {
                match self.gpui_remote_gxserver_request_target(remote_machine_id) {
                    Some(target) => Some(target),
                    None => {
                        self.dispatch_gpui_add_project_dialog_result(
                            request_id,
                            false,
                            None,
                            Some("That machine is not connected."),
                            cx,
                        );
                        return;
                    }
                }
            }
            None => None,
        };
        let Some(endpoint) = operation.endpoint() else {
            return;
        };
        let timeout = operation.timeout();
        /*
        CDXC:AddProject 2026-07-30:
        A remote clone job outlives this request: gxserver runs it on the machine
        and registers the project itself when git finishes. Keep the job id so a
        poll whose answer never comes back can be followed natively instead of
        leaving the finished project invisible in the sidebar.
        */
        let clone_watch_job_id = if operation == GpuiAddProjectDialogOperation::ReadCloneJob {
            params
                .get("jobId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        } else {
            None
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    let result = gpui_add_project_dialog_rpc_result(
                        target.as_ref(),
                        endpoint,
                        &params,
                        timeout,
                    )?;
                    if operation == GpuiAddProjectDialogOperation::Add {
                        return gpui_add_project_dialog_restore_recent_project(
                            target.as_ref(),
                            result,
                            timeout,
                        );
                    }
                    Ok(result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let added_project_path = match &result {
                    Ok(value) if operation == GpuiAddProjectDialogOperation::Add => value
                        .get("project")
                        .and_then(|project| project.get("path"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                    _ => None,
                };
                let clone_completed = matches!(&result, Ok(value)
                    if operation == GpuiAddProjectDialogOperation::ReadCloneJob
                        && value
                            .get("job")
                            .and_then(|job| job.get("state"))
                            .and_then(serde_json::Value::as_str)
                            == Some("completed"));
                /*
                A failed startClone/readCloneJob answer does NOT mean the clone
                failed: the request can time out on the tunnel while the job
                keeps running and registers the project on the machine. Treat it
                like a possibly-landed mutation.
                */
                let clone_answer_lost = result.is_err()
                    && matches!(
                        operation,
                        GpuiAddProjectDialogOperation::ReadCloneJob
                            | GpuiAddProjectDialogOperation::StartClone
                    );
                match result {
                    Ok(value) => this.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        true,
                        Some(value),
                        None,
                        cx,
                    ),
                    Err(error) => this.dispatch_gpui_add_project_dialog_result(
                        request_id,
                        false,
                        None,
                        Some(error.as_str()),
                        cx,
                    ),
                }
                if operation != GpuiAddProjectDialogOperation::Add
                    && !clone_completed
                    && !clone_answer_lost
                {
                    return;
                }
                match remote_machine_id {
                    /*
                    CDXC:AddProject 2026-07-30:
                    A remote add, a finished remote clone, and a clone request
                    whose answer was lost all refresh that machine's presentation
                    on BOTH arms, because a request that times out can still have
                    registered the project and the machine's presentation stream
                    is often the broken part. A lost readCloneJob answer also
                    hands the job to a native watcher, because that clone can
                    still be running and will register its project minutes after
                    the dialog gave up.
                    */
                    Some(remote_machine_id) => {
                        this.refresh_gpui_remote_gxserver_presentation_in_background(
                            remote_machine_id.clone(),
                            false,
                            cx,
                        );
                        if clone_answer_lost {
                            if let Some(job_id) = clone_watch_job_id {
                                this.watch_gpui_remote_add_project_clone_job(
                                    remote_machine_id,
                                    job_id,
                                    cx,
                                );
                            }
                        }
                    }
                    /*
                    The local sidebar runtime owns local project state and does
                    not receive daemon pushes, so a successful local add is
                    reported through the same workspace-folder bridge the OS
                    folder picker used. The runtime re-registers idempotently,
                    focuses the project, and pulls a fresh local presentation.
                    */
                    None => {
                        if let Some(project_path) = added_project_path {
                            this.dispatch_gpui_workspace_folder_picked_message(
                                serde_json::json!({
                                    "path": project_path,
                                    "type": "workspaceFolderPicked",
                                }),
                                cx,
                            );
                        }
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn watch_gpui_remote_add_project_clone_job(
        &mut self,
        remote_machine_id: String,
        job_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AddProject 2026-07-30:
        The dialog stops polling as soon as one readCloneJob answer is lost, but
        the job itself is server-side: git keeps running on the machine and
        gxserver registers the cloned project when it finishes. Follow the job
        natively so the project appears when it lands. Bounded on both axes: a
        maximum number of polls, and a short run of consecutive transport
        failures ends the watch because a dead tunnel cannot deliver a refresh
        either. Only the machine id and the job id are used; nothing about this
        watch is reported to CEF.
        */
        if self
            .gpui_remote_gxserver_request_target(&remote_machine_id)
            .is_none()
        {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let mut consecutive_errors = 0_u32;
            for _ in 0..GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_POLLS {
                cx.background_executor()
                    .timer(GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_INTERVAL)
                    .await;
                let Ok(Some(target)) = this.update(cx, |this, _cx| {
                    this.gpui_remote_gxserver_request_target(remote_machine_id.as_str())
                }) else {
                    return;
                };
                let job_id = job_id.clone();
                let result = background
                    .spawn(async move {
                        gpui_add_project_dialog_rpc_result(
                            Some(&target),
                            "/api/readRepositoryCloneJob",
                            &serde_json::json!({ "jobId": job_id }),
                            GPUI_ADD_PROJECT_DIALOG_JOB_TIMEOUT,
                        )
                    })
                    .await;
                match result {
                    Ok(value) => {
                        let running = value
                            .get("job")
                            .and_then(|job| job.get("state"))
                            .and_then(serde_json::Value::as_str)
                            == Some("running");
                        if running {
                            consecutive_errors = 0;
                            continue;
                        }
                        let _ = this.update(cx, |this, cx| {
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                remote_machine_id.clone(),
                                false,
                                cx,
                            );
                        });
                        return;
                    }
                    Err(_) => {
                        consecutive_errors += 1;
                        if consecutive_errors
                            >= GPUI_ADD_PROJECT_DIALOG_CLONE_WATCH_MAX_CONSECUTIVE_ERRORS
                        {
                            return;
                        }
                    }
                }
            }
        })
        .detach();
    }

    pub(crate) fn gpui_add_project_dialog_machine_options(&self) -> serde_json::Value {
        /*
        CDXC:AddProject 2026-07-30:
        The dialog's machine step is built from the same saved machine list the
        sidebar renders, plus this computer. Only display labels and bounded ids
        cross the boundary: SSH hosts, users, identity files, and tunnel ports
        stay native. Machines with no live tunnel are still listed (a remote
        entry point must be able to preselect one) and carry an explicit
        not-connected line instead of quietly disappearing.
        */
        let local_label = if cfg!(target_os = "macos") {
            "This Mac"
        } else if cfg!(target_os = "windows") {
            "This PC"
        } else {
            "This Computer"
        };
        let mut machines = vec![serde_json::json!({
            "label": local_label,
            "machineId": GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID,
            "platform": gpui_add_project_dialog_local_platform(),
        })];
        if let Some(saved_machines) = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
        {
            for machine in saved_machines {
                let Some(machine_id) = gpui_remote_machine_id_from_value(machine) else {
                    continue;
                };
                let label = machine
                    .as_object()
                    .and_then(|machine| gpui_remote_machine_string_field(machine, "name"))
                    .unwrap_or_else(|| "Remote".to_string());
                let mut option = serde_json::json!({
                    "label": label,
                    "machineId": machine_id,
                });
                if self
                    .gpui_remote_gxserver_request_target(&machine_id)
                    .is_none()
                {
                    option["description"] = serde_json::json!("Not connected");
                }
                machines.push(option);
            }
        }
        serde_json::Value::Array(machines)
    }

    pub(crate) fn dispatch_gpui_add_project_dialog_result(
        &mut self,
        request_id: String,
        ok: bool,
        result: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "requestId": request_id,
                "result": result,
                "type": "addProjectDialogResult",
            }),
            cx,
        );
    }

    pub(crate) fn handle_gpui_preview_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteClone 2026-06-24-19:35:
        GPUI remote Clone Repository preview accepts only the selected saved machine id plus explicit modal input. Rust forwards those bounded fields through the live tunnel and returns only the preview payload needed by the shared modal; raw failures remain generic and are not logged.
        */
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let remote_machine_id = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id);
        let Some(params) = gpui_remote_repository_clone_preview_params_from_command(command) else {
            self.dispatch_gpui_repository_clone_preview_result(
                request_id,
                false,
                None,
                Some("Repository clone preview failed."),
                cx,
            );
            return;
        };
        let target = match remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_repository_clone_preview_result(
                        request_id,
                        false,
                        None,
                        Some("Repository clone preview failed."),
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/previewRepositoryClone",
                        &params,
                        GPUI_REMOTE_REPOSITORY_CLONE_PREVIEW_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => {
                    if let Some(preview) = result.get("preview").cloned() {
                        this.dispatch_gpui_repository_clone_preview_result(
                            request_id,
                            true,
                            Some(preview),
                            None,
                            cx,
                        );
                    } else {
                        this.dispatch_gpui_repository_clone_preview_result(
                            request_id,
                            false,
                            None,
                            Some("Repository clone preview failed."),
                            cx,
                        );
                    }
                }
                Err(_) => this.dispatch_gpui_repository_clone_preview_result(
                    request_id,
                    false,
                    None,
                    Some("Repository clone preview failed."),
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_start_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteClone 2026-06-24-19:35:
        Starting a remote clone is a Rust-owned gxserver job request, not renderer shell execution. CEF may submit repository URL text, target parent text, optional folder/branch text, clone flags, and the selected machine id; the remote daemon validates, derives the target path, runs Git, registers the project, and publishes presentation.
        */
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let remote_machine_id = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id);
        let Some(params) = gpui_remote_repository_clone_start_params_from_command(command) else {
            self.dispatch_gpui_repository_clone_result(
                request_id,
                false,
                None,
                Some("Repository clone failed."),
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "error",
                "Repository clone failed",
                "The clone request was invalid.",
                cx,
            );
            return;
        };
        let target = match remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.dispatch_gpui_repository_clone_result(
                        request_id,
                        false,
                        None,
                        Some("Repository clone failed."),
                        cx,
                    );
                    self.dispatch_gpui_app_modal_toast(
                        "error",
                        "Repository clone failed",
                        "Reconnect the remote machine before cloning a repository.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let running_description = if remote_machine_id.is_some() {
            "The remote clone is running."
        } else {
            "The clone is running."
        };
        let toast_id = gpui_remote_repository_clone_toast_id(&request_id);
        self.remote_repository_clone_requests.insert(
            request_id.clone(),
            GpuiRemoteRepositoryCloneRequest {
                job_id: String::new(),
                remote_machine_id: remote_machine_id.clone(),
                toast_id: toast_id.clone(),
            },
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/startRepositoryClone",
                        &params,
                        GPUI_REMOTE_REPOSITORY_CLONE_START_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(result) => {
                    let Some(job) = result.get("job").cloned() else {
                        this.fail_gpui_remote_repository_clone_request(
                            request_id,
                            Some(toast_id),
                            "Repository clone failed.",
                            cx,
                        );
                        return;
                    };
                    let Some(job_id) = job
                        .get("jobId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::trim)
                        .filter(|value| gpui_remote_repository_clone_job_id_allowed(value))
                        .map(str::to_string)
                    else {
                        this.fail_gpui_remote_repository_clone_request(
                            request_id,
                            Some(toast_id),
                            "Repository clone failed.",
                            cx,
                        );
                        return;
                    };
                    this.remote_repository_clone_requests.insert(
                        request_id.clone(),
                        GpuiRemoteRepositoryCloneRequest {
                            job_id,
                            remote_machine_id,
                            toast_id: toast_id.clone(),
                        },
                    );
                    this.dispatch_gpui_repository_clone_toast(
                        "info",
                        "Cloning repository",
                        running_description,
                        Some(toast_id),
                        true,
                        Some(request_id.clone()),
                        cx,
                    );
                    this.handle_gpui_remote_repository_clone_job_status(request_id, job, cx);
                }
                Err(_) => this.fail_gpui_remote_repository_clone_request(
                    request_id,
                    Some(toast_id),
                    "Repository clone failed.",
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_cancel_remote_repository_clone_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Repository clone already finished",
                "There is no active remote clone for that request.",
                cx,
            );
            return;
        };
        let target = match active_clone.remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let job_id = active_clone.job_id.clone();
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/cancelRepositoryCloneJob",
                        &serde_json::json!({ "jobId": job_id }),
                        GPUI_REMOTE_REPOSITORY_CLONE_JOB_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_active = this
                    .remote_repository_clone_requests
                    .get(&request_id)
                    .is_some_and(|current| {
                        current.job_id == active_clone.job_id
                            && current.remote_machine_id == active_clone.remote_machine_id
                    });
                if !still_active {
                    return;
                }
                match result {
                    Ok(result) => match result.get("job").cloned() {
                        Some(job) => {
                            this.handle_gpui_remote_repository_clone_job_status(
                                request_id, job, cx,
                            );
                        }
                        None => {
                            this.fail_gpui_remote_repository_clone_request(
                                request_id,
                                Some(active_clone.toast_id),
                                "Repository clone failed.",
                                cx,
                            );
                        }
                    },
                    Err(_) => this.dispatch_gpui_repository_clone_toast(
                        "error",
                        "Repository clone cancel failed",
                        "gxserver did not cancel the clone.",
                        Some(active_clone.toast_id),
                        false,
                        None,
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_remote_repository_clone_job_status(
        &mut self,
        request_id: String,
        job: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            return;
        };
        match job
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("failed")
        {
            "running" => {
                self.schedule_gpui_remote_repository_clone_poll(request_id, cx);
            }
            "completed" => {
                self.remote_repository_clone_requests.remove(&request_id);
                let completed_description = if active_clone.remote_machine_id.is_some() {
                    "The remote project was added."
                } else {
                    "The project was added."
                };
                if let Some(machine_id) = active_clone.remote_machine_id {
                    self.refresh_gpui_remote_gxserver_presentation_in_background(
                        machine_id, false, cx,
                    );
                }
                self.dispatch_gpui_repository_clone_toast(
                    "success",
                    "Repository cloned",
                    completed_description,
                    Some(active_clone.toast_id),
                    false,
                    None,
                    cx,
                );
                self.dispatch_gpui_repository_clone_result(request_id, true, None, None, cx);
            }
            "canceled" => {
                self.remote_repository_clone_requests.remove(&request_id);
                let canceled_description = if active_clone.remote_machine_id.is_some() {
                    "The partial remote folder may still exist."
                } else {
                    "The partial folder may still exist."
                };
                self.dispatch_gpui_repository_clone_toast(
                    "warning",
                    "Repository clone canceled",
                    canceled_description,
                    Some(active_clone.toast_id),
                    false,
                    None,
                    cx,
                );
                self.dispatch_gpui_repository_clone_result(
                    request_id,
                    false,
                    None,
                    Some("Repository clone canceled."),
                    cx,
                );
            }
            _ => {
                self.fail_gpui_remote_repository_clone_request(
                    request_id,
                    Some(active_clone.toast_id),
                    "Repository clone failed.",
                    cx,
                );
            }
        }
    }

    pub(crate) fn schedule_gpui_remote_repository_clone_poll(
        &mut self,
        request_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(active_clone) = self
            .remote_repository_clone_requests
            .get(&request_id)
            .cloned()
        else {
            return;
        };
        let target = match active_clone.remote_machine_id.as_deref() {
            Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                Some(target) => Some(target),
                None => {
                    self.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    );
                    return;
                }
            },
            None => None,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(GPUI_REMOTE_REPOSITORY_CLONE_POLL_INTERVAL)
                .await;
            let job_id = active_clone.job_id.clone();
            let result = background
                .spawn(async move {
                    gpui_repository_clone_rpc_result(
                        target.as_ref(),
                        "/api/readRepositoryCloneJob",
                        &serde_json::json!({ "jobId": job_id }),
                        GPUI_REMOTE_REPOSITORY_CLONE_JOB_TIMEOUT,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_active = this
                    .remote_repository_clone_requests
                    .get(&request_id)
                    .is_some_and(|current| {
                        current.job_id == active_clone.job_id
                            && current.remote_machine_id == active_clone.remote_machine_id
                    });
                if !still_active {
                    return;
                }
                match result {
                    Ok(result) => {
                        if let Some(job) = result.get("job").cloned() {
                            this.handle_gpui_remote_repository_clone_job_status(
                                request_id, job, cx,
                            );
                        } else {
                            this.fail_gpui_remote_repository_clone_request(
                                request_id,
                                Some(active_clone.toast_id),
                                "Repository clone failed.",
                                cx,
                            );
                        }
                    }
                    Err(_) => this.fail_gpui_remote_repository_clone_request(
                        request_id,
                        Some(active_clone.toast_id),
                        "Repository clone failed.",
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn fail_gpui_remote_repository_clone_request(
        &mut self,
        request_id: String,
        toast_id: Option<String>,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let toast_id = toast_id.or_else(|| {
            self.remote_repository_clone_requests
                .remove(&request_id)
                .map(|active| active.toast_id)
        });
        self.remote_repository_clone_requests.remove(&request_id);
        self.dispatch_gpui_repository_clone_toast(
            "error",
            "Repository clone failed",
            "The repository clone did not complete.",
            toast_id,
            false,
            None,
            cx,
        );
        self.dispatch_gpui_repository_clone_result(request_id, false, None, Some(message), cx);
    }

    pub(crate) fn handle_gpui_remote_gxserver_sidebar_request_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-16:48:
        Sidebar-origin remote gxserver actions are allowlisted Rust-owned RPCs through the selected live SSH tunnel. Renderer commands may identify only a saved remote machine id, an allowed endpoint, and endpoint params; Rust must not accept tokens, hosts, SSH users, key paths, command text, URLs, or raw response handling authority from CEF.

        CDXC:GPUIRemoteGit 2026-06-24-17:47:
        Remote Git/GitHub/worktree parity expands this bridge to gxserver-owned project actions. Responses must be shaped at this boundary before CEF sees them: no command summaries, no PR URL launch authority, no raw delete-project bodies, and no remote tokens, hostnames, SSH details, stdout/stderr logging, or daemon body persistence.
        */
        let Some(remote_machine_id) = command
            .get("remoteMachineId")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_normalize_remote_machine_id)
        else {
            return;
        };
        let response_request_id = gpui_remote_request_id_from_command(command);
        let Some(path) = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| gpui_remote_sidebar_request_path_allowed(path))
            .map(str::to_string)
        else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            }
            return;
        };
        let Some(params) = command.get("params").cloned().filter(|params| {
            params.is_object() && params.to_string().len() <= GPUI_REMOTE_GXSERVER_PARAMS_MAX_BYTES
        }) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "GPUI rejected the remote gxserver request.",
                    cx,
                );
            }
            return;
        };
        let Some(params) = gpui_remote_sidebar_request_params(path.as_str(), params) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "GPUI rejected the remote gxserver request.",
                    cx,
                );
            }
            return;
        };
        let timeout = gpui_remote_sidebar_request_timeout(command);
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            if let Some(request_id) = response_request_id.as_deref() {
                self.dispatch_gpui_remote_gxserver_request_error(
                    remote_machine_id.as_str(),
                    request_id,
                    cx,
                );
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Remote action unavailable",
                    "Reconnect the remote machine before using its sessions.",
                    cx,
                );
            }
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let response_path = path.clone();
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(&target, path.as_str(), &params, timeout)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(result) => {
                        if let Some(request_id) = response_request_id.as_deref() {
                            this.dispatch_gpui_sidebar_remote_event(
                                serde_json::json!({
                                    "ok": true,
                                    "remoteMachineId": remote_machine_id.as_str(),
                                    "requestId": request_id,
                                    "result": gpui_remote_sidebar_response_payload(
                                        response_path.as_str(),
                                        result,
                                    ),
                                    "type": "remoteGxserverResponse",
                                }),
                                cx,
                            );
                        }
                    }
                    Err(_) => {
                        if let Some(request_id) = response_request_id.as_deref() {
                            this.dispatch_gpui_sidebar_remote_event(
                                serde_json::json!({
                                    "error": "Remote gxserver request failed.",
                                    "ok": false,
                                    "remoteMachineId": remote_machine_id.as_str(),
                                    "requestId": request_id,
                                    "type": "remoteGxserverResponse",
                                }),
                                cx,
                            );
                        } else {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Remote action failed",
                                "The remote gxserver action did not complete.",
                                cx,
                            );
                        }
                    }
                }
                if gpui_remote_sidebar_request_refreshes_presentation(response_path.as_str()) {
                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                        remote_machine_id,
                        false,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_remote_gxserver_request_error(
        &mut self,
        remote_machine_id: &str,
        request_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_sidebar_remote_event(
            serde_json::json!({
                "error": "Remote gxserver request failed.",
                "ok": false,
                "remoteMachineId": remote_machine_id,
                "requestId": request_id,
                "type": "remoteGxserverResponse",
            }),
            cx,
        );
    }

    pub(crate) fn gpui_remote_gxserver_request_target(
        &self,
        remote_machine_id: &str,
    ) -> Option<GpuiRemoteGxserverRequestTarget> {
        self.remote_gxserver_connections
            .get(remote_machine_id)
            .map(GpuiRemoteGxserverConnection::request_target)
    }

    pub(crate) fn handle_gpui_app_modal_recent_project_path_action(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project_id) = command
            .get("projectId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|project_id| !project_id.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        if gpui_remote_project_reference_from_project_id(project_id.as_str()).is_some() {
            let action = match command_type {
                "copyRecentProjectPath" => {
                    GpuiSidebarNativeProjectPathAction::CopyRemoteProjectPath
                }
                "openRecentProjectInFinder" => {
                    GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal
                }
                "openRecentProjectTerminal" => {
                    GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal
                }
                _ => return,
            };
            self.handle_gpui_remote_project_native_action(
                GpuiSidebarNativeProjectPathActionMessage {
                    action,
                    file_path: None,
                    preferred_interface: GpuiPreferredAgentInterface::Terminal,
                    project_id,
                },
                cx,
            );
            return;
        }
        if !gpui_remote_sidebar_project_id_allowed(project_id.as_str()) {
            return;
        }
        let action = match command_type {
            "copyRecentProjectPath" => GpuiSidebarNativeProjectPathAction::CopyRecentProjectPath,
            "openRecentProjectInFinder" => {
                GpuiSidebarNativeProjectPathAction::OpenRecentProjectInFinder
            }
            _ => return,
        };
        let message = GpuiSidebarNativeProjectPathActionMessage {
            action,
            file_path: None,
            preferred_interface: GpuiPreferredAgentInterface::Terminal,
            project_id,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { execute_gpui_sidebar_native_project_path_action(message) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(GpuiSidebarNativeProjectPathActionResult::Copied(path)) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(path));
                }
                Ok(GpuiSidebarNativeProjectPathActionResult::Opened) => {}
                Err(message) => this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Native action unavailable",
                    &message,
                    cx,
                ),
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_app_modal_recent_project_mutation(
        &mut self,
        mutation: GpuiRecentProjectMutation,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(scoped_project_id) = command
            .get("projectId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|project_id| !project_id.is_empty())
        else {
            return;
        };
        let (machine_id, machine_name, project_id, remote_target) = if let Some(reference) =
            gpui_remote_project_reference_from_project_id(scoped_project_id)
        {
            let machine_name =
                gpui_remote_machine_name_from_settings(reference.remote_machine_id.as_str());
            let remote_target =
                self.gpui_remote_gxserver_request_target(reference.remote_machine_id.as_str());
            (
                Some(reference.remote_machine_id),
                machine_name,
                reference.project_id,
                remote_target,
            )
        } else {
            if !gpui_remote_sidebar_project_id_allowed(scoped_project_id) {
                return;
            }
            (None, None, scoped_project_id.to_string(), None)
        };
        let request = GpuiRecentProjectsRequest {
            machine_id: machine_id.clone(),
            machine_name,
            remote_target,
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (mutated, result_message) = background
                .spawn(async move {
                    gpui_recent_project_mutation_and_result(mutation, project_id, request)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(result_message, cx);
                if mutated {
                    if mutation == GpuiRecentProjectMutation::Restore
                        && let Some(machine_id) = machine_id
                    {
                        this.refresh_gpui_remote_gxserver_presentation_in_background(
                            machine_id, false, cx,
                        );
                    }
                    return;
                }
                this.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Recent Projects unavailable",
                    "The project could not be updated through gxserver.",
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn connected_gpui_remote_previous_session_sources(
        &self,
    ) -> Vec<GpuiRemotePreviousSessionSource> {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(machines) = settings_snapshot
            .object()
            .get("remoteMachines")
            .and_then(serde_json::Value::as_array)
        else {
            return Vec::new();
        };
        machines
            .iter()
            .filter_map(|machine| {
                let remote_machine_id = gpui_remote_machine_id_from_value(machine)?;
                let target =
                    self.gpui_remote_gxserver_request_target(remote_machine_id.as_str())?;
                let machine_name = machine
                    .as_object()
                    .and_then(|machine| gpui_remote_machine_string_field(machine, "name"));
                Some(GpuiRemotePreviousSessionSource {
                    machine_name,
                    remote_machine_id,
                    target,
                })
            })
            .collect()
    }

    pub(crate) fn handle_gpui_remote_session_native_action(
        &mut self,
        message: GpuiSidebarNativeProjectPathActionMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(reference) =
            gpui_remote_attach_session_reference_from_project_id(message.project_id.as_str())
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "GPUI could not identify the remote session.",
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            reference.remote_machine_id.as_str(),
        ) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return;
        };

        match message.action {
            GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal => {
                if message.preferred_interface == GpuiPreferredAgentInterface::Chat {
                    self.pending_agents_chat_launch_intents.insert(
                        GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey::from(
                            &reference,
                        )),
                    );
                }
                self.begin_gpui_remote_attach_terminal_open(
                    reference,
                    config,
                    target,
                    None,
                    AgentsWorkspaceNewTerminalPlacement::Tab,
                    cx,
                );
            }
            GpuiSidebarNativeProjectPathAction::CopyRemoteAttachCommand => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_prepare_remote_attach_terminal_plan(
                                &config, &target, &reference, false,
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(plan) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                plan.clipboard_command,
                            ));
                            this.dispatch_gpui_app_modal_toast(
                                "info",
                                "Remote attach command copied",
                                "SSH attach command copied to the clipboard.",
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Remote attach unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::CopyRemoteResumeCommand => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_prepare_remote_resume_clipboard_command(
                                &config, &target, &reference,
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(command) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(command));
                            this.dispatch_gpui_app_modal_toast(
                                "info",
                                "Remote resume command copied",
                                "SSH resume command copied to the clipboard.",
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Remote resume unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            _ => {}
        }
    }

    pub(crate) fn handle_gpui_remote_project_native_action(
        &mut self,
        message: GpuiSidebarNativeProjectPathActionMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(reference) =
            gpui_remote_project_reference_from_project_id(message.project_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Remote action unavailable",
                "GPUI could not identify the remote project.",
                cx,
            );
            return;
        };
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using that project.",
                cx,
            );
            return;
        };

        match message.action {
            GpuiSidebarNativeProjectPathAction::CopyRemoteProjectPath => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_remote_gxserver_project_path_by_id(
                                &target,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok(path) => {
                            cx.write_to_clipboard(ClipboardItem::new_string(path));
                            this.dispatch_gpui_workspace_action_toast(
                                "info",
                                "Remote project path copied",
                                "Remote path copied to the clipboard.",
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote path unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteProjectTerminal => {
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote terminal unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let remote_machine_id = reference.remote_machine_id.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_remote_gxserver_rpc_result(
                                &target,
                                "/api/restoreRecentProject",
                                &serde_json::json!({ "projectId": reference.project_id.as_str() }),
                                Duration::from_secs(10),
                            )?;
                            gpui_create_remote_project_workspace_terminal(
                                &config, &target, &reference,
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| match result {
                        Ok((reference, plan)) => {
                            let key = GpuiRemoteAttachSessionKey::from(&reference);
                            this.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
                            this.open_gpui_remote_attach_terminal(
                                reference,
                                plan,
                                None,
                                AgentsWorkspaceNewTerminalPlacement::Tab,
                                GpuiRemoteAttachOpenIntent::CreatedByThisAction,
                                cx,
                            );
                            this.refresh_gpui_remote_gxserver_presentation_in_background(
                                remote_machine_id,
                                false,
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote terminal unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInIde
            | GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInVscode
            | GpuiSidebarNativeProjectPathAction::OpenRemoteWorkspaceProjectInZed => {
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote IDE open unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let action = message.action;
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_project_in_ide(
                                &config,
                                &target,
                                action,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote IDE open unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteExistingPullRequestInBrowser => {
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_existing_project_pull_request_in_browser(
                                &target,
                                reference.project_id.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote pull request unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteSidebarGitChangedFileInIde => {
                let Some(file_path) = message.file_path else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote file open unavailable",
                        "Choose a changed file from the current remote Git review.",
                        cx,
                    );
                    return;
                };
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote file open unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_open_remote_sidebar_git_changed_file_in_ide(
                                &config,
                                &target,
                                reference.project_id.as_str(),
                                file_path.as_str(),
                            )
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = result {
                            this.dispatch_gpui_workspace_action_toast(
                                "warning",
                                "Remote file open unavailable",
                                message.as_str(),
                                cx,
                            );
                        }
                    });
                })
                .detach();
            }
            GpuiSidebarNativeProjectPathAction::OpenRemoteProjectPortsBrowser => {
                /*
                CDXC:GPUIRemotePortsBrowser 2026-07-30:
                A remote project's Browser pane opens on the machine's
                listening-ports page: Rust lists the sockets over the saved
                SSH configuration, renders a local HTML page whose rows link
                to `http://<ssh host>:<port>`, and opens it in the remote
                project's own browser tab model. The renderer contributed only
                the fixed action plus the machine-scoped project id; hosts,
                ports, and process names never cross back through CEF.
                */
                let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                let Some(config) = gpui_remote_machine_config_from_settings(
                    settings_snapshot.object(),
                    reference.remote_machine_id.as_str(),
                ) else {
                    self.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Remote browser unavailable",
                        "The saved remote machine is missing required SSH settings.",
                        cx,
                    );
                    return;
                };
                let scoped_project_id = gpui_remote_scoped_project_id(
                    reference.remote_machine_id.as_str(),
                    reference.project_id.as_str(),
                );
                let execution_target = target.execution_target.clone();
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = background
                        .spawn(async move {
                            gpui_prepare_remote_ports_browser_page(&config, &execution_target)
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(page_url) => {
                            this.open_browser_url_from_renderer_command(
                                GpuiSidebarOpenBrowserUrlMessage {
                                    url: page_url,
                                    reuse: GpuiBrowserRendererOpenReuse::Similar,
                                    from_quick_header: false,
                                    project_id: Some(scoped_project_id),
                                },
                                window,
                                cx,
                            );
                        }
                        Err(message) => this.dispatch_gpui_workspace_action_toast(
                            "warning",
                            "Remote ports unavailable",
                            message.as_str(),
                            cx,
                        ),
                    });
                })
                .detach();
            }
            _ => {}
        }
    }

    pub(crate) fn request_gpui_remote_attach_terminal_open(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self
            .remote_machine_connect_states
            .get(&reference.remote_machine_id)
            .map(String::as_str)
            != Some(GpuiRemoteGxserverConnectState::Connected.wire_status_state())
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return false;
        }
        let Some(target) = self.gpui_remote_gxserver_request_target(&reference.remote_machine_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "Reconnect the remote machine before using its sessions.",
                cx,
            );
            return false;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) = gpui_remote_machine_config_from_settings(
            settings_snapshot.object(),
            reference.remote_machine_id.as_str(),
        ) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return false;
        };
        self.begin_gpui_remote_attach_terminal_open(
            reference,
            config,
            target,
            requested_pane_id,
            placement,
            cx,
        );
        true
    }

    pub(crate) fn begin_gpui_remote_attach_terminal_open(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        config: GpuiRemoteMachineConfig,
        target: GpuiRemoteGxserverRequestTarget,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        /*
        A remote sidebar click or restored native placeholder activates the
        owning machine-scoped workspace before lookup. The same path then
        focuses a live SSH attachment or re-arms the existing canonical tab.
        */
        self.swap_agents_workspace_to_project_id(
            Some(gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            )),
            cx,
        );
        self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.openRequested",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        if self.focus_existing_gpui_remote_attach_terminal(&key, cx) {
            return;
        }
        let prepare_reference = reference.clone();
        let update_reference = reference;
        let remote_machine_id = key.remote_machine_id;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_prepare_remote_attach_terminal_plan(
                        &config,
                        &target,
                        &prepare_reference,
                        true,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(plan) => {
                    this.open_gpui_remote_attach_terminal(
                        update_reference,
                        plan,
                        requested_pane_id,
                        placement,
                        GpuiRemoteAttachOpenIntent::AttachExistingSession,
                        cx,
                    );
                    this.refresh_gpui_remote_gxserver_presentation_in_background(
                        remote_machine_id,
                        false,
                        cx,
                    );
                }
                Err(message) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.remoteAttach.planFailed",
                        serde_json::json!({ "machineId": remote_machine_id }),
                    );
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Remote attach unavailable",
                        message.as_str(),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn focus_existing_gpui_remote_attach_terminal(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.remote_attach_sessions.get(key).copied() else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
            /*
            CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
            The mapped tab lives in the remote project's own workspace model.
            While another project's model is live its absence proves nothing;
            forgetting the mapping here would strand the parked tab and stack a
            duplicate attach tab on every click.
            */
            if self.agents_workspace_project_id.as_deref()
                == Some(
                    gpui_remote_scoped_project_id(
                        key.remote_machine_id.as_str(),
                        key.project_id.as_str(),
                    )
                    .as_str(),
                )
            {
                self.remote_attach_sessions.remove(key);
            }
            return false;
        };
        if self.agents_tab_selected_local_runtime_missing(pane_id, session_id) {
            /*
            Parking a workspace drops local attach clients, so a restored
            remote tab keeps Running presentation with no live SSH client.
            Report "not existing" so the open path re-arms this same tab with a
            freshly prepared SSH attach payload instead of focusing dead
            content.
            */
            return false;
        }
        let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
        let keep_editor_mode =
            self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key);
        let project_editor_mode = self.active_mode;
        self.agents_workspace.select_tab(pane_id, session_id);
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        if keep_editor_mode {
            self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                project_editor_mode,
                pane_id,
                session_id,
                &workspace_key,
            );
            self.retarget_project_editor_companion_to_workspace_terminal(
                project_editor_mode,
                session_id,
                &workspace_key,
                true,
                cx,
            );
        } else {
            self.active_mode = TitlebarMode::Agents;
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.request_agents_session_text_focus_handoff(
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                },
                cx,
            );
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        self.set_sidebar_gxserver_remote_attach_focus_state(key, cx);
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        cx.notify();
        true
    }

    pub(crate) fn open_gpui_remote_attach_terminal(
        &mut self,
        reference: GpuiRemoteAttachSessionReference,
        plan: GpuiRemoteAttachTerminalPlan,
        requested_pane_id: Option<WorkspacePaneId>,
        placement: AgentsWorkspaceNewTerminalPlacement,
        intent: GpuiRemoteAttachOpenIntent,
        cx: &mut gpui::Context<Self>,
    ) {
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.remoteNewTerminal.tabMaterializeStarted",
            serde_json::json!({}),
        );
        let key = GpuiRemoteAttachSessionKey::from(&reference);
        /*
        SSH plan preparation runs in the background, so by the time it
        completes the user may have focused something else. Mirror the local
        attach completion guard: only materialize the terminal while this
        remote session still owns the published presentation focus, instead
        of yanking the workspace back to the remote project.
        */
        let scoped_session_id = gpui_remote_scoped_session_id(
            key.remote_machine_id.as_str(),
            key.project_id.as_str(),
            key.session_id.as_str(),
        );
        if intent == GpuiRemoteAttachOpenIntent::AttachExistingSession
            && self
                .sidebar_gxserver_presentation_focus_state
                .focused_session_id
                .as_deref()
                != Some(scoped_session_id.as_str())
        {
            return;
        }
        /*
        CDXC:GPUIRemoteWorkspaceProjectKey 2026-07-30:
        Activate the remote project's own Agents workspace model before any
        tab lookup or creation. The per-project layout swap (2026-07-23)
        parks the outgoing model and clears the one-shot startup launch
        payload source, so mounting the SSH attach tab into whichever project
        happened to be live let a trailing project switch destroy the tab
        before it mounted. Swapping first makes the mount deterministic and
        turns the follow-up focus-state swap into a same-project no-op.
        */
        self.swap_agents_workspace_to_project_id(
            Some(gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            )),
            cx,
        );
        /*
        The sidebar projection is the display-title authority for both local
        and remote workspaces. Attach metadata is still needed before the
        sidebar has published a row, but it must not overwrite a projected
        display/primary/terminal/alias title when re-arming a restored tab.
        */
        let projected_tab_session = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_deref()
            .and_then(|sessions| {
                sessions.iter().find(|session| {
                    session.key == GpuiWorkspaceTerminalSessionKey::Remote(key.clone())
                })
            });
        let tab_title = projected_tab_session
            .map(|session| session.title.clone())
            .unwrap_or_else(|| plan.title.clone());
        let tab_agent_icon = projected_tab_session
            .and_then(|session| session.agent_icon)
            .or(plan.agent_icon);
        if self.focus_existing_gpui_remote_attach_terminal(&key, cx) {
            return;
        }
        #[cfg(target_os = "macos")]
        let env_vars = plan
            .askpass
            .as_ref()
            .map(|askpass| {
                vec![
                    (
                        "DISPLAY".to_string(),
                        env::var("DISPLAY").unwrap_or_else(|_| "localhost:0".to_string()),
                    ),
                    (
                        "SSH_ASKPASS".to_string(),
                        gpui_path_string(askpass.script.as_path()),
                    ),
                    ("SSH_ASKPASS_REQUIRE".to_string(), "force".to_string()),
                ]
            })
            .unwrap_or_default();
        #[cfg(not(target_os = "macos"))]
        let env_vars = Vec::new();
        let payload = AgentsTerminalExplicitLaunchPayload {
            working_directory: None,
            command: Some(plan.terminal_command),
            env_vars,
            initial_input: None,
            wait_after_command: false,
        };
        if payload.to_ghostty_launch_payload().is_err() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote attach unavailable",
                "GPUI could not prepare the remote attach terminal command.",
                cx,
            );
            return;
        }
        let resolved_requested_pane_id =
            requested_pane_id.unwrap_or(self.agents_workspace.focused_pane);
        if let Some(existing_session_id) = self.remote_attach_sessions.get(&key).copied() {
            if let Some(existing_pane_id) = self
                .agents_workspace
                .pane_id_for_session(existing_session_id)
            {
                /*
                The live-tab case already returned through
                focus_existing_gpui_remote_attach_terminal above, so this
                mapped tab is a restored one whose SSH attach client was
                dropped when its workspace was parked. Re-arm the same tab
                with the freshly prepared attach payload — the identical
                reuse path local sessions take after a restart — instead of
                stacking a duplicate attach tab.
                */
                let Some(placed_pane_id) = self
                    .agents_workspace
                    .place_existing_session_for_new_terminal(
                        existing_pane_id,
                        requested_pane_id.unwrap_or(existing_pane_id),
                        existing_session_id,
                        placement,
                    )
                else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Remote attach unavailable",
                        "GPUI could not place the remote terminal in the requested pane.",
                        cx,
                    );
                    return;
                };
                let runtime_session_id = self
                    .agents_terminal_runtime_sessions
                    .ensure_runtime_session_id(existing_session_id);
                let mount_slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id: placed_pane_id,
                    session_id: existing_session_id,
                };
                self.agents_terminal_launch_payload_source
                    .insert_explicit_payload_for_mount_slot(
                        runtime_session_id,
                        mount_slot_id,
                        payload.clone(),
                    );
                if let Some(session) = self
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == existing_session_id)
                {
                    session.title = tab_title.clone();
                    session.agent_icon = tab_agent_icon;
                }
                #[cfg(target_os = "macos")]
                if let Some(askpass) = plan.askpass {
                    self.remote_attach_askpass_scripts
                        .insert(key.clone(), askpass);
                }
                self.agents_workspace
                    .select_tab(placed_pane_id, existing_session_id);
                let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
                self.activate_preferred_agents_chat_launch_intent(existing_session_id, cx);
                if self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key)
                {
                    let project_editor_mode = self.active_mode;
                    self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                        project_editor_mode,
                        placed_pane_id,
                        existing_session_id,
                        &workspace_key,
                    );
                    self.retarget_project_editor_companion_to_workspace_terminal(
                        project_editor_mode,
                        existing_session_id,
                        &workspace_key,
                        true,
                        cx,
                    );
                } else {
                    self.active_mode = TitlebarMode::Agents;
                    self.set_shell_focus_with_terminal_handoff(
                        ShellFocusTarget::AgentsPane(placed_pane_id),
                        true,
                    );
                    self.request_agents_session_text_focus_handoff(mount_slot_id, cx);
                }
                self.scroll_workspace_pane_active_tab(placed_pane_id);
                self.persist_shell_layout_state();
                self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.remoteAttach.terminalOpened",
                    serde_json::json!({
                        "machineId": key.remote_machine_id,
                        "mode": "rearmedRestoredTab",
                        "sessionId": key.session_id,
                    }),
                );
                cx.notify();
                return;
            }
            self.remote_attach_sessions.remove(&key);
        }
        let created = match placement {
            AgentsWorkspaceNewTerminalPlacement::Tab => {
                self.agents_workspace.add_running_session_to_pane(
                    resolved_requested_pane_id,
                    tab_title.clone(),
                    tab_agent_icon,
                )
            }
            AgentsWorkspaceNewTerminalPlacement::SplitRight => self
                .agents_workspace
                .split_mounting_session_to_right_of_pane(resolved_requested_pane_id),
            AgentsWorkspaceNewTerminalPlacement::SplitBelow => self
                .agents_workspace
                .split_mounting_session_below_pane(resolved_requested_pane_id),
            AgentsWorkspaceNewTerminalPlacement::BottomRow => self
                .agents_workspace
                .resolve_action_pane_id(resolved_requested_pane_id)
                .map(|resolved_pane_id| {
                    self.agents_workspace.focus_pane(resolved_pane_id);
                    self.agents_workspace.append_mounting_session_bottom_row()
                }),
        };
        let Some((pane_id, session_id)) = created else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote attach unavailable",
                "GPUI could not create a terminal pane for the remote session.",
                cx,
            );
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = tab_title;
            session.agent_icon = tab_agent_icon;
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Running,
                false,
            );
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        let mount_slot_id = AgentsTerminalBodyMountSlotId {
            pane_id,
            session_id,
        };
        self.agents_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(runtime_session_id, mount_slot_id, payload);
        #[cfg(target_os = "macos")]
        if let Some(askpass) = plan.askpass {
            self.remote_attach_askpass_scripts
                .insert(key.clone(), askpass);
        }
        self.remote_attach_sessions.insert(key.clone(), session_id);
        let workspace_key = GpuiWorkspaceTerminalSessionKey::Remote(key.clone());
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        if self.should_keep_project_editor_open_for_workspace_terminal_focus(&workspace_key) {
            let project_editor_mode = self.active_mode;
            self.seed_project_editor_companion_terminal_attach_payload_from_agents_slot(
                project_editor_mode,
                pane_id,
                session_id,
                &workspace_key,
            );
            self.retarget_project_editor_companion_to_workspace_terminal(
                project_editor_mode,
                session_id,
                &workspace_key,
                true,
                cx,
            );
        } else {
            self.active_mode = TitlebarMode::Agents;
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.request_agents_session_text_focus_handoff(mount_slot_id, cx);
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        self.set_sidebar_gxserver_remote_attach_focus_state(&key, cx);
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.terminalOpened",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "mode": "createdRunningAttachTab",
                "sessionId": key.session_id,
            }),
        );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.remoteNewTerminal.tabMaterialized",
            serde_json::json!({}),
        );
        cx.notify();
    }

    pub(crate) fn dispatch_gpui_remote_project_directory_browse_result(
        &mut self,
        request_id: String,
        ok: bool,
        result: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "requestId": request_id,
                "result": result,
                "type": "remoteProjectDirectoryBrowseResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_remote_project_add_result(
        &mut self,
        request_id: String,
        ok: bool,
        project_path: Option<String>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "projectPath": project_path,
                "requestId": request_id,
                "type": "remoteProjectAddResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_preview_result(
        &mut self,
        request_id: String,
        ok: bool,
        preview: Option<serde_json::Value>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "preview": preview,
                "requestId": request_id,
                "type": "repositoryClonePreviewResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_result(
        &mut self,
        request_id: String,
        ok: bool,
        project_path: Option<String>,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "error": error,
                "ok": ok,
                "projectPath": project_path,
                "requestId": request_id,
                "type": "repositoryCloneResult",
            }),
            cx,
        );
    }

    pub(crate) fn dispatch_gpui_repository_clone_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        toast_id: Option<String>,
        persistent: bool,
        cancel_request_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let action = cancel_request_id.map(|request_id| {
            serde_json::json!({
                "label": "Cancel",
                "sidebarMessage": {
                    "requestId": request_id,
                    "type": "cancelRepositoryClone",
                },
            })
        });
        self.dispatch_open_gpui_app_modal_message(
            serde_json::json!({
                "action": action,
                "description": description,
                "level": level,
                "persistent": persistent,
                "title": title,
                "toastId": toast_id,
                "type": "toast",
            }),
            cx,
        );
    }

    pub(crate) fn refresh_gpui_remote_gxserver_presentation_in_background(
        &mut self,
        remote_machine_id: String,
        mark_failed_on_error: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            return;
        };
        let refresh_stream_generation = self
            .remote_gxserver_connections
            .get(&remote_machine_id)
            .and_then(|connection| connection.presentation_stream_generation);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/readPresentationSnapshot",
                        &serde_json::json!({}),
                        Duration::from_secs(15),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !refresh_stream_generation.is_some_and(|generation| {
                    !this.gpui_remote_gxserver_presentation_stream_is_current(
                        remote_machine_id.as_str(),
                        generation,
                    )
                }) {
                    match result {
                        Ok(result) => {
                            if let Some(snapshot) = result.get("snapshot").cloned() {
                                this.dispatch_gpui_sidebar_remote_event(
                                    serde_json::json!({
                                        "payload": {
                                            "snapshot": snapshot,
                                            "type": "presentationSnapshot",
                                        },
                                        "remoteMachineId": remote_machine_id.as_str(),
                                        "type": "remoteGxserverPresentation",
                                    }),
                                    cx,
                                );
                            } else if mark_failed_on_error {
                                this.dispatch_gpui_remote_machine_status(
                                    remote_machine_id.as_str(),
                                    "failed",
                                    cx,
                                );
                            }
                        }
                        Err(_) if mark_failed_on_error => {
                            this.dispatch_gpui_remote_machine_status(
                                remote_machine_id.as_str(),
                                "failed",
                                cx,
                            );
                        }
                        Err(_) => {}
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn next_gpui_remote_gxserver_connect_generation(&mut self, remote_machine_id: &str) -> u64 {
        let generation = self
            .remote_gxserver_connect_generations
            .entry(remote_machine_id.to_string())
            .or_insert(0);
        *generation = generation.wrapping_add(1);
        if *generation == 0 {
            *generation = 1;
        }
        *generation
    }

    pub(crate) fn gpui_remote_gxserver_connect_generation_is_current(
        &self,
        remote_machine_id: &str,
        generation: u64,
    ) -> bool {
        self.remote_gxserver_connect_generations
            .get(remote_machine_id)
            .copied()
            == Some(generation)
    }

    pub(crate) fn next_gpui_remote_gxserver_presentation_stream_generation(&mut self) -> u64 {
        self.remote_gxserver_presentation_stream_generation = self
            .remote_gxserver_presentation_stream_generation
            .wrapping_add(1);
        if self.remote_gxserver_presentation_stream_generation == 0 {
            self.remote_gxserver_presentation_stream_generation = 1;
        }
        self.remote_gxserver_presentation_stream_generation
    }

    pub(crate) fn gpui_remote_gxserver_presentation_stream_is_current(
        &self,
        remote_machine_id: &str,
        generation: u64,
    ) -> bool {
        self.remote_gxserver_connections
            .get(remote_machine_id)
            .and_then(|connection| connection.presentation_stream_generation)
            == Some(generation)
    }

    pub(crate) fn restart_gpui_remote_gxserver_presentation_stream(
        &mut self,
        remote_machine_id: String,
        client_id: String,
        last_revision: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .remote_gxserver_connections
            .contains_key(remote_machine_id.as_str())
        {
            return false;
        }
        let generation = self.next_gpui_remote_gxserver_presentation_stream_generation();
        let cancel = Arc::new(AtomicBool::new(false));
        let target = {
            let Some(connection) = self
                .remote_gxserver_connections
                .get_mut(remote_machine_id.as_str())
            else {
                return false;
            };
            if let Some(previous_cancel) = connection.presentation_stream_cancel.as_ref() {
                previous_cancel.store(true, Ordering::SeqCst);
            }
            let target = connection.request_target();
            connection.presentation_stream_cancel = Some(cancel.clone());
            connection.presentation_stream_generation = Some(generation);
            target
        };
        self.start_gpui_remote_gxserver_presentation_stream(
            remote_machine_id,
            target,
            generation,
            cancel,
            client_id,
            last_revision,
            cx,
        );
        true
    }

    pub(crate) fn start_gpui_remote_gxserver_presentation_stream(
        &mut self,
        remote_machine_id: String,
        target: GpuiRemoteGxserverRequestTarget,
        generation: u64,
        cancel: Arc<AtomicBool>,
        client_id: String,
        last_revision: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemotePresentationStreaming 2026-06-24-19:54:
        A connected remote machine needs the same live gxserver presentation contract as local GPUI, but CEF must not receive remote base URLs or bearer tokens. Rust opens `/api/events` through the localhost SSH tunnel, subscribes with the shared sidebar client id, and forwards only sanitized snapshot/delta payloads. A terminal stream failure enters the shared broken-status funnel, which tears down the stale tunnel before the sidebar schedules a full reconnect.
        */
        let (tx, mut rx) = mpsc::unbounded::<GpuiRemoteGxserverPresentationStreamMessage>();
        let background = cx.background_executor().clone();
        background
            .spawn(async move {
                gpui_remote_gxserver_presentation_stream_loop(
                    target,
                    cancel,
                    tx,
                    client_id,
                    last_revision,
                );
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Some(message) = rx.next().await {
                let should_continue = this
                    .update(cx, |this, cx| {
                        if !this.gpui_remote_gxserver_presentation_stream_is_current(
                            remote_machine_id.as_str(),
                            generation,
                        ) {
                            return false;
                        }
                        match message {
                            GpuiRemoteGxserverPresentationStreamMessage::Event(payload) => {
                                this.dispatch_gpui_sidebar_remote_event(
                                    serde_json::json!({
                                        "payload": payload,
                                        "remoteMachineId": remote_machine_id.as_str(),
                                        "type": "remoteGxserverPresentation",
                                    }),
                                    cx,
                                );
                                true
                            }
                            GpuiRemoteGxserverPresentationStreamMessage::Failed => {
                                this.dispatch_gpui_remote_machine_status(
                                    remote_machine_id.as_str(),
                                    GpuiRemoteGxserverConnectState::PresentationStreamFailed
                                        .wire_status_state(),
                                    cx,
                                );
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn start_gpui_remote_gxserver_watchdog(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_REMOTE_GXSERVER_WATCHDOG_INTERVAL)
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.validate_gpui_remote_gxserver_connections(false, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn validate_gpui_remote_gxserver_connections(
        &mut self,
        wake_validation: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let exited_machine_ids = self
            .remote_gxserver_connections
            .iter_mut()
            .filter_map(|(machine_id, connection)| {
                (!matches!(connection.child.try_wait(), Ok(None))).then(|| machine_id.clone())
            })
            .collect::<Vec<_>>();
        for machine_id in exited_machine_ids {
            self.stop_gpui_remote_gxserver_connection(machine_id.as_str());
            self.dispatch_gpui_remote_machine_status_with_message(
                machine_id.as_str(),
                "disconnected",
                Some("The remote SSH tunnel disconnected."),
                cx,
            );
        }

        if !wake_validation && self.remote_gxserver_watchdog_probe_in_flight {
            return;
        }
        let probes = self
            .remote_gxserver_connections
            .iter()
            .filter_map(|(machine_id, connection)| {
                let generation = self
                    .remote_gxserver_connect_generations
                    .get(machine_id)
                    .copied()?;
                Some((machine_id.clone(), generation, connection.request_target()))
            })
            .collect::<Vec<_>>();
        if probes.is_empty() {
            return;
        }
        if !wake_validation {
            self.remote_gxserver_watchdog_probe_in_flight = true;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let results = background
                .spawn(async move {
                    probes
                        .into_iter()
                        .map(|(machine_id, generation, target)| {
                            let healthy = gpui_remote_authenticated_health(
                                target.local_port,
                                target.token.as_str(),
                            )
                            .is_some();
                            (machine_id, generation, target.local_port, healthy)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !wake_validation {
                    this.remote_gxserver_watchdog_probe_in_flight = false;
                }
                let mut failed_machine_ids = Vec::new();
                for (machine_id, generation, local_port, healthy) in results {
                    if !this.gpui_remote_gxserver_connect_generation_is_current(
                        machine_id.as_str(),
                        generation,
                    ) {
                        continue;
                    }
                    let Some(connection) = this
                        .remote_gxserver_connections
                        .get_mut(machine_id.as_str())
                    else {
                        continue;
                    };
                    if connection.local_port != local_port {
                        continue;
                    }
                    if healthy {
                        connection.health_check_failures = 0;
                        continue;
                    }
                    connection.health_check_failures =
                        connection.health_check_failures.saturating_add(1);
                    if wake_validation
                        || connection.health_check_failures
                            >= GPUI_REMOTE_GXSERVER_WATCHDOG_FAILURE_THRESHOLD
                    {
                        failed_machine_ids.push(machine_id);
                    }
                }
                for machine_id in failed_machine_ids {
                    this.stop_gpui_remote_gxserver_connection(machine_id.as_str());
                    this.dispatch_gpui_remote_machine_status_with_message(
                        machine_id.as_str(),
                        "disconnected",
                        Some("The remote gxserver tunnel stopped responding."),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn stop_gpui_remote_gxserver_connection(&mut self, remote_machine_id: &str) {
        if let Some(mut connection) = self.remote_gxserver_connections.remove(remote_machine_id) {
            connection.terminate();
        }
    }

    pub(crate) fn stop_all_gpui_remote_gxserver_connections(&mut self) {
        for (_, mut connection) in self.remote_gxserver_connections.drain() {
            connection.terminate();
        }
    }
}
