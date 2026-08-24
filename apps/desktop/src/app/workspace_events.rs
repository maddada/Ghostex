// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-workarea + sidebar bridge events, agents chat/find surfaces

use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Entity;
use gpui::Window;
use gpui::rgb;

use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn project_workarea_bridge_event_handler(
        &self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        cx: &mut gpui::Context<Self>,
    ) -> cef::ProjectWorkareaBridgeEventHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();

        Rc::new(move |event: cef::ProjectWorkareaBridgeEvent| {
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        this.receive_project_workarea_bridge_event(slot_key, event, window, cx);
                    });
                })
                .detach();
        })
    }

    pub(crate) fn receive_project_workarea_bridge_event(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        event: cef::ProjectWorkareaBridgeEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
        Runtime workarea bridge events are accepted only from the CefSurface that owns the current slot. Manage file events resolve against the explicit in-memory project root from the sidebar snapshot, Kanban/Automate Beads and board events call gxserver's typed Project Board endpoints, and response dispatch stays inside the owning CEF surface without WKWebView/WebKit handlers, shelling out to bd, fallback project detection, logs, persistence, or generic IPC.
        */
        match (slot_key, event) {
            (
                ProjectWorkareaCefSurfaceSlotKey::Manage,
                cef::ProjectWorkareaBridgeEvent::ManageFilesRequest(payload),
            ) => {
                /*
                CDXC:GPUIManageFilesBridge 2026-07-11:
                This arm previously ran synchronously inside the bridge event
                handler, but manage_files_bridge_result shells out to `git`
                (rev-parse/check-ignore/cat-file, up to six calls, no timeout)
                and reads files/directories — all on the main thread. A stuck
                git (index.lock, network filesystem, slow hook) beach-balled
                the app. Run it on the background executor like the Beads and
                automation-board arms, then dispatch the response from the
                follow-up update.
                */
                let snapshot = self.latest_sidebar_project_snapshot.clone();
                let additional_docs_folders_text = gpui_manage_additional_docs_folders_text(
                    &self.sidebar_runtime_settings_snapshot,
                );
                let global_docs_directory_text =
                    gpui_global_docs_directory_text(&self.sidebar_runtime_settings_snapshot);
                let remote_context = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.active_project_id.as_ref())
                    .and_then(|project_id| {
                        gpui_remote_project_reference_from_project_id(project_id.0.as_str())
                    })
                    .map(|reference| {
                        let target = self.gpui_remote_gxserver_request_target(
                            reference.remote_machine_id.as_str(),
                        );
                        (reference, target)
                    });
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let outcome = background
                        .spawn(async move {
                            match remote_context {
                                Some((reference, target)) => {
                                    run_remote_manage_files_bridge_request_for_project_snapshot(
                                        &payload,
                                        snapshot.as_ref(),
                                        &additional_docs_folders_text,
                                        &reference,
                                        target.as_ref(),
                                    )
                                }
                                None => run_manage_files_bridge_request_for_project_snapshot(
                                    &payload,
                                    snapshot.as_ref(),
                                    &additional_docs_folders_text,
                                    &global_docs_directory_text,
                                ),
                            }
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        let ManageFilesBridgeOutcome {
                            action,
                            request_id,
                            mut response,
                            side_effect,
                        } = outcome;
                        if let Some(side_effect) = side_effect
                            && let Err(error) =
                                this.perform_manage_files_bridge_side_effect(side_effect, cx)
                        {
                            response =
                                manage_files_bridge_error_response(&action, &request_id, &error);
                        }
                        this.dispatch_project_workarea_json_event(
                            slot_key,
                            "ghostex-manage-files-response",
                            &response.to_string(),
                            cx,
                        );
                    });
                })
                .detach();
            }
            (
                ProjectWorkareaCefSurfaceSlotKey::Kanban
                | ProjectWorkareaCefSurfaceSlotKey::Automate,
                cef::ProjectWorkareaBridgeEvent::ProjectBoardRequest(payload),
            ) => {
                let request =
                    serde_json::from_str::<serde_json::Value>(&payload).unwrap_or_default();
                let action = manage_request_string(&request, "action").unwrap_or_default();
                if matches!(
                    action.as_str(),
                    GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION
                        | GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION
                        | GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION
                ) {
                    let request_id =
                        manage_request_string(&request, "requestId").unwrap_or_default();
                    let context = project_board_bridge_runtime_context_from_snapshot(
                        self.latest_sidebar_project_snapshot.as_ref(),
                    );
                    let response =
                        match gpui_project_board_command_request(&request, context.as_ref()) {
                            Ok(intent) => {
                                /*
                                CDXC:ProjectBoardBeadsCommands 2026-08-14:
                                The Kanban CEF surface sends only fixed setup/migration selectors.
                                Rust owns every literal command and the active-project cwd, then uses
                                the existing command-Action lifecycle so completion comes from the
                                terminal status file instead of renderer shell text, a timer, or a
                                hidden subprocess.
                                */
                                self.open_gpui_command_action_terminal(
                                    intent.command_id().to_string(),
                                    intent.title().to_string(),
                                    intent.command().to_string(),
                                    false,
                                    false,
                                    window,
                                    cx,
                                );
                                serde_json::json!({
                                    "ok": true,
                                    "payload": { "started": true },
                                    "requestId": request_id,
                                })
                            }
                            Err(error) => gpui_project_board_error_response(&request_id, &error),
                        };
                    self.dispatch_project_workarea_json_event(
                        slot_key,
                        "ghostex-project-board-response",
                        &response.to_string(),
                        cx,
                    );
                    return;
                }
                if action.starts_with("automation") {
                    /*
                    macOS `handleGxserverProjectAutomationRequest` parity: automation
                    board actions are thin translations onto the gxserver automation
                    endpoints, so they run on the background executor like the Beads
                    bridge. Run-session/worktree rows additionally navigate through
                    the existing reviewed focus bridges after the response posts.
                    */
                    let mut context = project_board_bridge_runtime_context_from_snapshot(
                        self.latest_sidebar_project_snapshot.as_ref(),
                    );
                    if let Some(context) = context.as_mut()
                        && let Some(remote_machine_id) = context.remote_machine_id.as_deref()
                    {
                        context.remote_target =
                            self.gpui_remote_gxserver_request_target(remote_machine_id);
                    }
                    let background = cx.background_executor().clone();
                    cx.spawn(async move |this, cx| {
                        let (response, navigation) = background
                            .spawn(async move {
                                run_gpui_project_board_automation_request(
                                    &request,
                                    context.as_ref(),
                                )
                            })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_project_workarea_json_event(
                                slot_key,
                                "ghostex-project-board-response",
                                &response.to_string(),
                                cx,
                            );
                            match navigation {
                                Some(GpuiAutomationBoardNavigation::FocusSession(focus_id)) => {
                                    let _ = this
                                        .dispatch_gpui_command_palette_session_focus(&focus_id, cx);
                                }
                                Some(GpuiAutomationBoardNavigation::FocusProject(project_id)) => {
                                    let _ = this
                                        .dispatch_gpui_menu_bar_project_activation(&project_id, cx);
                                }
                                Some(GpuiAutomationBoardNavigation::RevealWorktreePath(path)) => {
                                    let _ = gpui_spawn_os_open(std::ffi::OsStr::new(&path));
                                }
                                None => {}
                            }
                        });
                    })
                    .detach();
                    return;
                }
                if gpui_project_board_conversation_action_forwarded(&action) {
                    /*
                    macOS parity ownership: board conversation actions (state,
                    startWork, links, jumps, toasts) live in the sidebar
                    runtime — the GPUI equivalent of `native-sidebar.tsx` —
                    which owns agents, presentation state, focus routing, and
                    the gxserver client. Rust forwards the first-party page
                    request and later routes the runtime's response back to
                    the originating tasks CEF page.
                    */
                    if !self.dispatch_gpui_project_board_conversation_request(&request, cx) {
                        let request_id =
                            manage_request_string(&request, "requestId").unwrap_or_default();
                        let response = gpui_project_board_error_response(
                            &request_id,
                            "The Ghostex sidebar runtime is not available.",
                        );
                        self.dispatch_project_workarea_json_event(
                            slot_key,
                            "ghostex-project-board-response",
                            &response.to_string(),
                            cx,
                        );
                    }
                    return;
                }
                let response = project_board_bridge_response_for_request_payload(
                    &payload,
                    self.latest_sidebar_project_snapshot.as_ref(),
                );
                self.dispatch_project_workarea_json_event(
                    slot_key,
                    "ghostex-project-board-response",
                    &response.to_string(),
                    cx,
                );
            }
            (
                ProjectWorkareaCefSurfaceSlotKey::Kanban
                | ProjectWorkareaCefSurfaceSlotKey::Automate,
                cef::ProjectWorkareaBridgeEvent::ProjectBeadsRequest(payload),
            ) => {
                let mut context = project_board_bridge_runtime_context_from_snapshot(
                    self.latest_sidebar_project_snapshot.as_ref(),
                );
                if let Some(context) = context.as_mut()
                    && let Some(remote_machine_id) = context.remote_machine_id.as_deref()
                {
                    context.remote_target =
                        self.gpui_remote_gxserver_request_target(remote_machine_id);
                }
                let background = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let response = background
                        .spawn(async move {
                            run_project_beads_bridge_request_for_context(&payload, context.as_ref())
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_project_workarea_json_event(
                            slot_key,
                            "ghostex-project-beads-response",
                            &response.to_string(),
                            cx,
                        );
                    });
                })
                .detach();
            }
            (
                ProjectWorkareaCefSurfaceSlotKey::Kanban
                | ProjectWorkareaCefSurfaceSlotKey::Automate,
                cef::ProjectWorkareaBridgeEvent::ProjectBoardImageRequest(payload),
            ) => {
                let clipboard_item = if project_board_image_request_needs_clipboard(&payload) {
                    cx.read_from_clipboard()
                } else {
                    None
                };
                let response =
                    project_board_image_bridge_response_for_payload(&payload, clipboard_item);
                self.dispatch_project_workarea_json_event(
                    slot_key,
                    "ghostex-project-board-image-response",
                    &response.to_string(),
                    cx,
                );
            }
            _ => {}
        }
    }

    pub(crate) fn dispatch_project_workarea_json_event(
        &mut self,
        slot_key: ProjectWorkareaCefSurfaceSlotKey,
        event_name: &str,
        json: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(runtime_url) = self.project_workarea_runtime_url_for_slot(slot_key) else {
            return;
        };
        let Some(owned_surface) = self.project_workarea_runtime_cef_surfaces.get(&slot_key) else {
            return;
        };
        if !owned_surface.matches_runtime_url(&runtime_url) {
            return;
        }
        let surface = owned_surface.surface.clone();
        let script = format!(
            "window.dispatchEvent(new CustomEvent('{}', {{ detail: {} }})); undefined;",
            event_name, json
        );
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn dispatch_gpui_project_board_command_completed(
        &mut self,
        action: &str,
        exit_code: i32,
        cx: &mut gpui::Context<Self>,
    ) {
        let payload = serde_json::json!({
            "action": action,
            "exitCode": exit_code,
        });
        self.dispatch_project_workarea_json_event(
            ProjectWorkareaCefSurfaceSlotKey::Kanban,
            GPUI_PROJECT_BOARD_COMMAND_COMPLETED_EVENT,
            &payload.to_string(),
            cx,
        );
    }

    pub(crate) fn receive_sidebar_bridge_event(
        &mut self,
        event: cef::SidebarBridgeEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIProjectSwitchCoalescing 2026-07-29:
        Backstop for the coalescer: a project-scoped sidebar command must never
        overtake a project switch that is still queued behind the settle
        window, or it would act on the outgoing project's runtime. Land the
        trailing switch first. Project-agnostic and high-frequency telemetry
        events pass through so they cannot defeat the debounce.
        */
        if !self.project_switch_pending_requests.is_empty()
            && gpui_sidebar_bridge_event_must_follow_pending_project_switch(&event)
        {
            self.flush_coalesced_project_switch_requests(window, cx);
        }
        match event {
            cef::SidebarBridgeEvent::ActiveProjectContext(payload) => {
                self.receive_sidebar_project_context_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::GxserverPresentationFocusState(payload) => {
                self.receive_sidebar_gxserver_presentation_focus_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::CreateProjectAgent(payload) => {
                self.receive_sidebar_create_project_agent_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::CreateProjectTerminal(payload) => {
                self.receive_sidebar_create_project_terminal_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalFocus(payload) => {
                self.receive_sidebar_workspace_terminal_focus_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalRenameCommand(payload) => {
                self.receive_sidebar_workspace_terminal_rename_command_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalEnter(payload) => {
                self.receive_sidebar_workspace_terminal_enter_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::WorkspaceTerminalLifecycleResult(payload) => {
                self.receive_sidebar_workspace_terminal_lifecycle_result_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::SourceWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::BrowserWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ProjectWorkareaReadiness(_)
            | cef::SidebarBridgeEvent::ManageFileWorkareaOperationRequest(_) => {
                /*
                CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
                Legacy sidebar readiness/proof messages stay accepted as compatibility no-ops. Source, Kanban, Automate, and Manage mounting now follows only the current runtime URL gate plus owned CEF surface map, and first-party Kanban/Automate/Manage CEF requests still flow through the separate project-workarea bridge.
                */
            }
            cef::SidebarBridgeEvent::NativeProjectPathAction(payload) => {
                self.receive_sidebar_native_project_path_action_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::NativeAppShotPrompt(payload) => {
                self.receive_sidebar_native_app_shot_prompt_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::SidebarCommandAction(payload) => {
                self.receive_sidebar_command_action_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::SidebarCommandRunEnd(payload) => {
                self.receive_sidebar_command_run_end_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::GhostexHotkeyAction(payload) => {
                self.receive_sidebar_ghostex_hotkey_action_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::SessionCompletionSound(payload) => {
                self.receive_sidebar_session_completion_sound_payload(&payload);
            }
            cef::SidebarBridgeEvent::SessionStatusIndicators(payload) => {
                self.receive_sidebar_session_status_indicators_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::PetOverlayState(payload) => {
                self.receive_sidebar_pet_overlay_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::GlobalActions(payload) => {
                self.receive_sidebar_global_actions_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::TitlebarGitMenuState(payload) => {
                self.receive_sidebar_titlebar_git_menu_state_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::OpenBrowserUrl(payload) => {
                self.receive_sidebar_open_browser_url_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::BrowserTabFocus(payload) => {
                self.receive_sidebar_browser_tab_focus_payload(&payload, window, cx);
            }
            cef::SidebarBridgeEvent::ProjectBoardConversationResponse(payload) => {
                self.receive_sidebar_project_board_conversation_response_payload(&payload, cx);
            }
        }
    }

    pub(crate) fn receive_sidebar_project_board_conversation_response_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The sidebar runtime answers forwarded board conversation requests
        here; the validated response object travels back to any tasks CEF
        workarea as the standard `ghostex-project-board-response` event,
        matched by the page on its own requestId.
        */
        if payload.chars().count() > GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_PAYLOAD_MAX_CHARS {
            return;
        }
        let Ok(message) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        if message.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE)
            || message.get("version").and_then(serde_json::Value::as_u64)
                != Some(GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION)
        {
            return;
        }
        let Some(response) = message.get("response").filter(|value| value.is_object()) else {
            return;
        };
        let request_id_valid = response
            .get("requestId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .is_some_and(|value| {
                !value.is_empty() && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            });
        if !request_id_valid {
            return;
        }
        let response_json = response.to_string();
        for slot_key in [
            ProjectWorkareaCefSurfaceSlotKey::Kanban,
            ProjectWorkareaCefSurfaceSlotKey::Automate,
        ] {
            self.dispatch_project_workarea_json_event(
                slot_key,
                "ghostex-project-board-response",
                &response_json,
                cx,
            );
        }
    }

    pub(crate) fn receive_sidebar_open_browser_url_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = gpui_sidebar_open_browser_url_from_json(payload) else {
            return;
        };
        self.open_browser_url_from_renderer_command(message, window, cx);
    }

    pub(crate) fn receive_sidebar_browser_tab_focus_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
            return;
        };
        let Some(object) = value.as_object() else {
            return;
        };
        if json_string_field(object, "type") != Some(GPUI_SIDEBAR_BROWSER_TAB_FOCUS_MESSAGE_TYPE)
            || object.get("version").and_then(serde_json::Value::as_u64) != Some(1)
        {
            return;
        }
        let Some(project_id) = json_string_field(object, "projectId")
            .map(str::trim)
            .filter(|project_id| gpui_browser_tabs_project_key_allowed(project_id))
        else {
            return;
        };
        let Some(tab_id) = object.get("tabId").and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|value| value.parse::<u64>().ok()))
        }) else {
            return;
        };
        let message = GpuiSidebarBrowserTabFocusMessage {
            project_id: project_id.to_string(),
            tab_id: BrowserTabId(tab_id),
        };
        /*
        CDXC:GPUIRemoteBrowserTabs 2026-07-12:
        The sidebar lists browser rows for every project (parked local and
        machine-scoped remote models included), so this bridge must reach
        beyond the active browser project: close edits the parked model
        directly (parked tabs own no CEF surfaces), and focus swaps the
        browser workarea to the owning project first — but only when that
        parked model really contains the tab, so stale rows cannot park the
        live project into an empty default model.
        */
        let is_active_browser_project =
            self.browser_tabs_project_id.as_deref() == Some(message.project_id.as_str());
        if object.get("close").and_then(serde_json::Value::as_bool) == Some(true) {
            if is_active_browser_project {
                self.close_browser_tab(message.tab_id, window, cx);
                return;
            }
            let active_profile_id = self.browser_profiles.active_profile_id();
            if let Some(parked_tabs) = self
                .parked_browser_tabs_by_project
                .get_mut(&message.project_id)
            {
                if parked_tabs.close_tab(message.tab_id, active_profile_id) {
                    self.persist_shell_layout_state();
                    cx.notify();
                }
            }
            return;
        }
        if object.get("sleeping").and_then(serde_json::Value::as_bool) == Some(true) {
            if !is_active_browser_project
                || find_browser_leaf_id_for_tab(&self.browser_tabs.root, message.tab_id).is_none()
            {
                return;
            }
            self.remove_browser_surface(message.tab_id, cx);
            self.browser_find_states.remove(&message.tab_id);
            self.browser_find_inputs.remove(&message.tab_id);
            self.browser_find_input_subscriptions
                .remove(&message.tab_id);
            if self.pending_browser_find_focus == Some(message.tab_id) {
                self.pending_browser_find_focus = None;
            }
            self.update_active_mode_cef_child_visibility(cx);
            cx.notify();
            return;
        }
        /*
        CDXC:DisabledPluginRouting 2026-08-23:
        Close and sleep above are housekeeping the sidebar may still need for
        tabs that already exist, but everything past this point focuses the
        Browser workarea. With Browser turned off in Settings → Customize a
        stale sidebar tab row must not be able to drag the shell back into it.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if !is_active_browser_project {
            let parked_model_has_tab = self
                .parked_browser_tabs_by_project
                .get(&message.project_id)
                .is_some_and(|parked_tabs| {
                    find_browser_leaf_id_for_tab(&parked_tabs.root, message.tab_id).is_some()
                });
            if !parked_model_has_tab {
                return;
            }
            self.swap_browser_tabs_to_project_id(Some(message.project_id.clone()), cx);
        }
        let Some(pane_id) = find_browser_leaf_id_for_tab(&self.browser_tabs.root, message.tab_id)
        else {
            return;
        };
        if !self
            .browser_tabs
            .select_tab_in_pane(pane_id, message.tab_id)
        {
            return;
        }
        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(pane_id));
        self.sync_active_browser_tab_to_surface(window, cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn open_browser_url_from_renderer_command(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        macOS `openNativeBrowserPaneFromCli` parity for `ghostex browser open` /
        `openBrowser(Pane)` renderer commands: reuse an exact or same-origin tab
        by navigating it, otherwise create a new loaded
        tab in the focused pane (the reviewed popup-tab path). The URL goes
        through the same toolbar normalization as typed addresses.

        CDXC:GPUIRemoteBrowserTabs 2026-07-12:
        A validated explicit project target swaps the browser workarea to that
        project's tab model synchronously before the open, so sidebar project
        headers (local and machine-scoped remote) never race the async
        active-project context round-trip. Explicit real-project targets always
        have the Browser workarea, so the availability gate only applies to
        untargeted opens.
        */
        /*
        CDXC:DisabledPluginRouting 2026-08-23:
        This is the one door every embedded-browser open goes through — chat
        and terminal links, saved Action links, sidebar project and Quick
        headers, `ghostex browser open`. With Browser turned off in Settings →
        Customize none of them may open a tab, and that includes the two routes
        that bypass the availability gate below (an explicit project target and
        the projectless Quick header), so the Customize refusal is checked
        first and answers with the copied link rather than a dead click.
        */
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Browser) {
            self.copy_path_for_disabled_project_workarea(
                &message.url,
                "Browser",
                cx,
            );
            return;
        }
        if let Some(project_id) = message.project_id.as_deref() {
            if self.browser_tabs_project_id.as_deref() != Some(project_id) {
                self.swap_browser_tabs_to_project_id(Some(project_id.to_string()), cx);
            }
        } else if !message.from_quick_header && !self.titlebar_mode_available(TitlebarMode::Browser)
        {
            return;
        }
        let Some(url) = normalize_address(&message.url) else {
            return;
        };
        if let Some((pane_id, tab_id)) = self
            .browser_tabs
            .find_renderer_open_reuse_tab(&url, message.reuse)
        {
            self.browser_tabs.select_tab_in_pane(pane_id, tab_id);
            self.active_mode = TitlebarMode::Browser;
            self.set_shell_focus(ShellFocusTarget::BrowserPane(
                self.browser_tabs.focused_pane,
            ));
            self.commit_browser_address(url, cx);
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_focused_browser_pane_active_tab();
            return;
        }
        let created_tab_id = self
            .browser_tabs
            .add_loaded_popup_tab(
                url,
                self.browser_profiles.active_profile_id(),
                cef::BrowserPopupPlacement::Selected,
            );
        let Some(created_tab_id) = created_tab_id else {
            return;
        };
        self.request_sidebar_browser_tab_reveal(created_tab_id);
        self.active_mode = TitlebarMode::Browser;
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.set_shell_focus(ShellFocusTarget::BrowserPane(
            self.browser_tabs.focused_pane,
        ));
        self.sync_active_browser_tab_to_surface(window, cx);
        self.scroll_focused_browser_pane_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn receive_sidebar_titlebar_git_menu_state_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(state) = gpui_titlebar_git_menu_state_from_payload(payload) else {
            return;
        };
        if self.titlebar_git_menu_state.as_ref() == Some(&state) {
            return;
        }
        self.titlebar_git_menu_state = Some(state);
        cx.notify();
    }

    pub(crate) fn receive_sidebar_gxserver_presentation_focus_state_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
        React may return only the gxserver presentation session ids it already owns from daemon create/focus/fork/restore flows. Store the parsed focus state in runtime memory, refresh only the sidebar bootstrap bridge on changes, and ignore malformed payloads without logging raw renderer JSON or deriving ids from terminal tabs, labels, paths, project names, or command text.
        */
        let Ok(next_state) =
            gpui_gxserver_presentation_focus_state_from_sidebar_contract_json(payload)
        else {
            return;
        };
        self.set_sidebar_gxserver_presentation_focus_state(next_state, cx);
    }

    pub(crate) fn receive_sidebar_workspace_terminal_focus_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
        A local SidebarApp session click is a real workspace selection request, not only a sidebar highlight. Parse the fixed project/session payload, select an existing mapped Agents tab when possible, or ask gxserver for attach metadata before creating an awake Running tab through the exact mount-slot launch source. Renderer labels, commands, paths, titles, daemon responses, and terminal content are not accepted by this bridge.
        */
        let Ok(message) = gpui_sidebar_workspace_terminal_focus_from_json(payload) else {
            return;
        };
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.bridgeReceived",
            serde_json::json!({
                "activeProjectId": self.agents_workspace_project_id,
                "projectId": message.project_id,
                "sessionId": message.session_id,
                "settleWindowActive": self
                    .project_switch_settling_until
                    .is_some_and(|until| Instant::now() < until),
            }),
        );
        /*
        CDXC:GPUIProjectSwitchCoalescing 2026-07-29:
        The sidebar posts the presentation snapshot before this imperative
        focus request, so when the snapshot is collapsed into the trailing
        switch this request must ride with it. Running it now would attach the
        clicked session into the outgoing project's workspace, which the
        trailing swap would then tear down.
        */
        if self.project_switch_request_is_coalesced(
            Some(message.project_id.as_str()),
            GpuiProjectSwitchRequestKind::WorkspaceTerminalFocus,
        ) {
            self.enqueue_coalesced_project_switch_request(
                Some(message.project_id.clone()),
                GpuiPendingProjectSwitchPayload::WorkspaceTerminalFocus(message),
                cx,
            );
            return;
        }
        self.focus_local_workspace_terminal_from_message(&message, cx);
    }

    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-07-29:
    Rename-command delivery shares this exact focus/attach pipeline with
    sidebar session clicks: selecting a tab alone never mounts its Ghostty
    surface (mount slots consume one-shot attach payloads), so any flow that
    must type into a session's terminal first routes through the same
    focus-existing / gxserver-attach owner as a real selection.
    */
    pub(crate) fn focus_local_workspace_terminal_from_message(
        &mut self,
        message: &GpuiSidebarWorkspaceTerminalFocusMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = GpuiLocalWorkspaceSessionKey::from(message);
        if message.preferred_interface == GpuiPreferredAgentInterface::Chat {
            self.pending_agents_chat_launch_intents
                .insert(GpuiWorkspaceTerminalSessionKey::Local(key.clone()));
        }
        // macOS TerminalFocusDebugLog parity (scenario native.terminal.focus):
        // bounded gxserver ids only.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.workspaceFocusRequested",
            serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        /*
        CDXC:GPUIForkParity 2026-07-10:
        Ordinary sidebar focus keeps targeting the currently focused Agents
        pane. Fork may additionally name the clicked source session; resolve
        that bounded gxserver id through the process-local map so the returned
        session is appended to the source tab group even if another pane was
        focused while gxserver was preparing it.
        */
        let placement_target_pane_id = message
            .placement_target_session_id
            .as_ref()
            .and_then(|session_id| {
                self.local_workspace_session_mappings
                    .get(&GpuiLocalWorkspaceSessionKey {
                        project_id: message.project_id.clone(),
                        session_id: session_id.clone(),
                    })
                    .copied()
            })
            .and_then(|session_id| self.agents_workspace.pane_id_for_session(session_id));
        let requested_pane_id =
            placement_target_pane_id.unwrap_or(self.agents_workspace.focused_pane);
        let force_requested_pane_placement = placement_target_pane_id.is_some();
        let mapped_shell_session_id = self.local_workspace_session_mappings.get(&key).copied();
        let mapped_pane_id = mapped_shell_session_id.and_then(|shell_session_id| {
            self.agents_workspace.pane_id_for_session(shell_session_id)
        });
        let mapped_slot_id =
            mapped_shell_session_id
                .zip(mapped_pane_id)
                .map(
                    |(shell_session_id, pane_id)| AgentsTerminalBodyMountSlotId {
                        pane_id,
                        session_id: shell_session_id,
                    },
                );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.focusDecision",
            serde_json::json!({
                "alreadyActiveInMappedPane": mapped_shell_session_id.zip(mapped_pane_id).is_some_and(
                    |(shell_session_id, pane_id)| self
                        .agents_workspace
                        .active_session_in_pane(pane_id)
                        == Some(shell_session_id),
                ),
                "attachAlreadyPending": self.local_workspace_attach_pending.contains(&key),
                "liveTerminalOwner": mapped_slot_id.is_some_and(|slot_id| {
                    self.local_workspace_terminal_has_live_terminal_owner(slot_id)
                }),
                "mappedPanePresent": mapped_pane_id.is_some(),
                "mappedSessionPresent": mapped_shell_session_id.is_some(),
                "pendingAttachPayload": mapped_slot_id.is_some_and(|slot_id| {
                    self.local_workspace_terminal_has_pending_attach_payload(slot_id)
                }),
                "presentationRunning": mapped_shell_session_id.is_some_and(|shell_session_id| {
                    self.agents_workspace.session(shell_session_id).is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                    })
                }),
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        self.begin_sidebar_focus_border_handoff(cx);
        self.local_workspace_latest_focus_key = Some(key.clone());
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        /*
        CDXC:GPUIFullReload 2026-07-12:
        Full reload kills the zmx daemon before this focus arrives, so the
        mounted terminal owner is a dead attach client that map-presence
        liveness would happily re-select. `forceRemount` drops the stale engine
        record synchronously (keeping the tab mapping for in-place reuse) and
        skips the focus-existing short-circuit so the ordinary attach pipeline
        re-attaches the reused tab to the freshly respawned provider.
        */
        if message.force_remount {
            if self
                .local_workspace_session_mappings
                .get(&key)
                .copied()
                .and_then(|shell_session_id| {
                    self.agents_gpui_engine_terminals.remove(&shell_session_id)
                })
                .is_some()
            {
                cx.notify();
            }
        } else if self.focus_existing_gpui_local_workspace_terminal(&key, cx) {
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.sessionSwitchLatency.focusExistingCompleted",
                serde_json::json!({
                    "projectId": key.project_id,
                    "sessionId": key.session_id,
                }),
            );
            self.reconcile_preferred_agents_chat_launch_intents(cx);
            return;
        }
        let attach_intent = self.local_workspace_attach_intent_for_key(&key);
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.attachPlanRequired",
            serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        self.spawn_local_workspace_attach_plan(
            key,
            attach_intent,
            requested_pane_id,
            force_requested_pane_placement,
            GpuiLocalWorkspaceAttachOrigin::SidebarFocus,
            cx,
        );
    }

    pub(crate) fn receive_sidebar_create_project_agent_payload(
        &mut self,
        payload: &str,
        _cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = gpui_sidebar_create_project_agent_from_json(payload) else {
            return;
        };
        #[cfg(not(target_os = "windows"))]
        {
            let _ = message;
            return;
        }
        #[cfg(target_os = "windows")]
        {
            /*
            CDXC:GPUIWindowsProjectAgent 2026-08-11:
            Project-header agents on Windows use one Rust-owned WSL operation
            from gxserver row creation through provider startup and terminal
            attachment. CEF supplies only the clicked project id, selected
            agent id, and bounded interface preference; gxserver resolves the
            authoritative project path, configured command, launch policy,
            and attach metadata.
            */
            let agent_id = message.agent_id;
            let preferred_interface = message.preferred_interface;
            let project_id = message.project_id;
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move {
                        gpui_create_local_project_workspace_agent(
                            project_id.as_str(),
                            agent_id.as_str(),
                        )
                    })
                    .await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok((key, plan)) => {
                        this.swap_agents_workspace_to_project_id(Some(key.project_id.clone()), cx);
                        let requested_pane_id = this.agents_workspace.focused_pane;
                        this.local_workspace_latest_focus_key = Some(key.clone());
                        let cleanup_key = key.clone();
                        let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key.clone());
                        if preferred_interface == GpuiPreferredAgentInterface::Chat {
                            this.pending_agents_chat_launch_intents
                                .insert(workspace_key.clone());
                        }
                        if !this.open_gpui_local_workspace_terminal(
                            key,
                            plan,
                            requested_pane_id,
                            false,
                            cx,
                        ) {
                            this.pending_agents_chat_launch_intents
                                .remove(&workspace_key);
                            this.compensate_unmaterialized_created_workspace_terminal(&cleanup_key);
                        }
                    }
                    Err(message) => this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Agent unavailable",
                        message.as_str(),
                        cx,
                    ),
                });
            })
            .detach();
        }
    }

    pub(crate) fn receive_sidebar_create_project_terminal_payload(
        &mut self,
        payload: &str,
        _cx: &mut gpui::Context<Self>,
    ) {
        let Ok(message) = gpui_sidebar_create_project_terminal_from_json(payload) else {
            return;
        };
        #[cfg(not(target_os = "windows"))]
        {
            let _ = message;
            return;
        }
        #[cfg(target_os = "windows")]
        {
            /*
            CDXC:GPUIWindowsProjectTerminal 2026-07-26:
            A Windows project-heading terminal uses the same host-owned
            gxserver create-plus-attach operation as New Terminal. The renderer
            supplies only the bounded clicked project id; gxserver resolves the
            project's authoritative WSL cwd and attach command, and the selected
            WSL backend launches it. Capability negotiation inside that
            host-owned operation selects the atomic endpoint when the installed
            daemon supports it. Do not translate a renderer path, start a host
            PowerShell process, or route creation back through CEF.
            */
            let project_id = message.project_id;
            let background = _cx.background_executor().clone();
            _cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move {
                        gpui_create_local_project_workspace_terminal(project_id.as_str())
                    })
                    .await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok((key, plan)) => {
                        this.swap_agents_workspace_to_project_id(Some(key.project_id.clone()), cx);
                        let requested_pane_id = this.agents_workspace.focused_pane;
                        this.local_workspace_latest_focus_key = Some(key.clone());
                        let cleanup_key = key.clone();
                        if !this.open_gpui_local_workspace_terminal(
                            key,
                            plan,
                            requested_pane_id,
                            false,
                            cx,
                        ) {
                            this.compensate_unmaterialized_created_workspace_terminal(&cleanup_key);
                        }
                    }
                    Err(message) => this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Terminal unavailable",
                        message.as_str(),
                        cx,
                    ),
                });
            })
            .detach();
        }
    }

    pub(crate) fn spawn_local_workspace_attach_plan(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
        attach_intent: GpuiLocalWorkspaceAttachIntent,
        requested_pane_id: WorkspacePaneId,
        force_requested_pane_placement: bool,
        origin: GpuiLocalWorkspaceAttachOrigin,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.local_workspace_attach_pending.insert(key.clone()) {
            return;
        }

        let attach_started_at = Instant::now();
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
                support_logs::append_temporary(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "TEMP.gpui.sessionSwitchLatency.attachPlanCompleted",
                    serde_json::json!({
                        "elapsedMs": attach_started_at.elapsed().as_millis() as u64,
                        "planReady": result.is_ok(),
                        "projectId": key.project_id,
                        "sessionId": key.session_id,
                    }),
                );
                let completion_origin = if origin == GpuiLocalWorkspaceAttachOrigin::SurfacedRestore
                    && this.local_workspace_latest_focus_key.as_ref() == Some(&key)
                    && this
                        .sidebar_gxserver_presentation_focus_state
                        .focused_session_id
                        .as_deref()
                        == Some(key.session_id.as_str())
                {
                    GpuiLocalWorkspaceAttachOrigin::SidebarFocus
                } else {
                    origin
                };
                match completion_origin {
                    GpuiLocalWorkspaceAttachOrigin::SidebarFocus => {
                        if this.local_workspace_latest_focus_key.as_ref() != Some(&key) {
                            return;
                        }
                        if this
                            .sidebar_gxserver_presentation_focus_state
                            .focused_session_id
                            .as_deref()
                            != Some(key.session_id.as_str())
                        {
                            return;
                        }
                    }
                    GpuiLocalWorkspaceAttachOrigin::SurfacedRestore => {
                        let Some(shell_session_id) =
                            this.local_workspace_session_mappings.get(&key).copied()
                        else {
                            return;
                        };
                        if this.agents_workspace.pane_id_for_session(shell_session_id)
                            != Some(requested_pane_id)
                            || this
                                .agents_workspace
                                .active_session_in_pane(requested_pane_id)
                                != Some(shell_session_id)
                            || !this.agents_tab_selected_local_runtime_missing(
                                requested_pane_id,
                                shell_session_id,
                            )
                        {
                            return;
                        }
                    }
                    GpuiLocalWorkspaceAttachOrigin::WakeRecovery => {
                        // A wake-origin attach revives an already-selected
                        // mapped tab; the sidebar highlight is irrelevant, but
                        // the tab must still exist so a close during the RPC
                        // cannot resurrect it as a fresh tab.
                        if !this.local_workspace_session_mappings.contains_key(&key) {
                            return;
                        }
                    }
                }
                match result {
                    Ok(plan) => match completion_origin {
                        GpuiLocalWorkspaceAttachOrigin::SurfacedRestore => {
                            if attach_gpui_surfaced_local_workspace_terminal(
                                &mut this.agents_workspace,
                                &mut this.agents_terminal_runtime_sessions,
                                &mut this.agents_terminal_launch_payload_source,
                                &this.local_workspace_session_mappings,
                                &mut this.local_app_shot_session_mappings,
                                requested_pane_id,
                                &key,
                                plan,
                            )
                            .is_ok()
                            {
                                this.persist_shell_layout_state();
                                cx.notify();
                            }
                        }
                        GpuiLocalWorkspaceAttachOrigin::SidebarFocus
                        | GpuiLocalWorkspaceAttachOrigin::WakeRecovery => {
                            let _ = this.open_gpui_local_workspace_terminal(
                                key,
                                plan,
                                requested_pane_id,
                                force_requested_pane_placement,
                                cx,
                            );
                        }
                    },
                    Err(message) => {
                        if completion_origin == GpuiLocalWorkspaceAttachOrigin::SurfacedRestore {
                            return;
                        }
                        this.cancel_sidebar_focus_border_handoff();
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Session attach unavailable",
                            message.as_str(),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn agents_session_chat_transcript_agent(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&'static str> {
        let session = self.agents_workspace.session(session_id)?;
        match session.agent_icon {
            Some("claude") => Some("claude"),
            Some("codex") => Some("codex"),
            Some("grok-build") => Some("grok"),
            Some("pi") => Some("pi"),
            Some("omp") => Some("omp"),
            _ => None,
        }
    }

    pub(crate) fn agents_chat_local_key_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiLocalWorkspaceSessionKey> {
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))?;
        // Remote attach sessions use machine-scoped workspace keys and their
        // own tunneled gxserver bootstrap rather than the local daemon.
        if key.project_id.starts_with("remote:") || key.session_id.starts_with("remote:") {
            return None;
        }
        Some(key)
    }

    pub(crate) fn agents_chat_remote_key_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiRemoteAttachSessionKey> {
        let scoped_project_id = self.agents_workspace_project_id.as_deref()?;
        let remote_project = gpui_remote_project_reference_from_project_id(scoped_project_id)?;
        self.remote_attach_sessions
            .iter()
            .find_map(|(key, mapped)| {
                (*mapped == session_id
                    && key.remote_machine_id == remote_project.remote_machine_id
                    && key.project_id == remote_project.project_id)
                    .then(|| key.clone())
            })
    }

    pub(crate) fn agents_session_chat_eligible(&self, session_id: TerminalSessionId) -> bool {
        if self
            .agents_session_chat_transcript_agent(session_id)
            .is_none()
        {
            return false;
        }
        if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            return self
                .sidebar_gxserver_presentation_focus_state
                .active_project_tab_sessions
                .as_deref()
                .and_then(|sessions| {
                    sessions.iter().find(|session| {
                        session.key == GpuiWorkspaceTerminalSessionKey::Local(key.clone())
                    })
                })
                .and_then(|session| session.agent_session_id.as_deref())
                .is_some_and(|agent_session_id| !agent_session_id.trim().is_empty());
        }
        let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
            return false;
        };
        let has_projected_agent_session_id = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_deref()
            .and_then(|sessions| {
                sessions.iter().find(|session| {
                    session.key == GpuiWorkspaceTerminalSessionKey::Remote(key.clone())
                })
            })
            .and_then(|session| session.agent_session_id.as_deref())
            .is_some_and(|agent_session_id| !agent_session_id.trim().is_empty());
        has_projected_agent_session_id
            && self
                .gpui_remote_gxserver_request_target(key.remote_machine_id.as_str())
                .is_some()
    }

    pub(crate) fn toggle_agents_session_chat_mode_for_focused_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return;
        };
        self.handoff_agents_session_chat_mode(session_id, cx);
    }

    pub(crate) fn handoff_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            self.request_session_chat_handoff_to_terminal(session_id, cx);
        } else {
            self.request_terminal_handoff_to_session_chat(session_id, cx);
        }
    }

    pub(crate) fn toggle_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // Toggling back to the terminal must always work, even if eligibility
        // inputs (agent icon, session mapping) changed while chat was showing.
        if self.agents_chat_mode_sessions.remove(&session_id) {
            if self.pending_session_chat_composer_focus == Some(session_id) {
                self.pending_session_chat_composer_focus = None;
            }
            // Chat's CEF child owns keyboard focus while visible. Queue the
            // canonical terminal focus handoff for the exact shell-focused
            // slot so the terminal reclaims first responder as it remounts.
            match self.focused_terminal_text_mount_target() {
                Some(FocusedTerminalTextMountTarget::Agents(slot_id))
                    if slot_id.session_id == session_id =>
                {
                    self.request_agents_terminal_text_focus_handoff(slot_id);
                }
                Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                    if slot_id.session_id == session_id =>
                {
                    self.request_project_editor_companion_terminal_text_focus_handoff(slot_id);
                }
                _ => {}
            }
            self.reconcile_agents_pane_surfaces(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }
        let _ = self.show_agents_session_chat_mode(session_id, cx);
    }

    pub(crate) fn request_agents_session_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&slot_id.session_id) {
            self.pending_agents_terminal_text_focus_slot = None;
            self.pending_session_chat_composer_focus = Some(slot_id.session_id);
            self.reconcile_agents_pane_surfaces(cx);
        } else {
            self.pending_session_chat_composer_focus = None;
            self.request_agents_terminal_text_focus_handoff(slot_id);
        }
    }

    pub(crate) fn request_project_editor_companion_session_text_focus_handoff(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&slot_id.session_id) {
            self.pending_project_editor_companion_terminal_text_focus_slot = None;
            self.pending_session_chat_composer_focus = Some(slot_id.session_id);
            self.reconcile_agents_pane_surfaces(cx);
        } else {
            self.pending_session_chat_composer_focus = None;
            self.request_project_editor_companion_terminal_text_focus_handoff(slot_id);
        }
    }

    /*
    CDXC:SessionChatDraftHandoff 2026-08-18:
    Background terminal → chat draft transfer for every view switch. Automatic,
    manual, local, and remote switches all show Chat first; draft capture must
    never keep the user trapped on a terminal startup/permission prompt or on
    an agent version that cannot answer the Ctrl+G handshake.

    Chat is shown immediately and the captured draft lands in the composer when
    the daemon's Ctrl+G handshake answers, so a slow or unanswerable capture
    costs the user nothing but the text staying where they typed it. That is
    also why failures are silent here: the user did not ask for a transfer, so
    a warning toast would be noise about an operation they never requested.
    */
    pub(crate) fn request_session_chat_draft_transfer(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // Grok Build binds Ctrl+G to its Tasks pane, so it can never answer the
        // handshake. The daemon rejects it outright; skip the round trip.
        if self.agents_session_chat_transcript_agent(session_id) == Some("grok") {
            return;
        }
        let request = if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_gxserver_rpc_result(
                    "/api/handoffSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        } else {
            let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
                return;
            };
            let Some(target) = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)
            else {
                return;
            };
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_remote_gxserver_rpc_result(
                    &target,
                    "/api/handoffSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        };
        cx.spawn(async move |this, cx| {
            let Ok(result) = request.await else {
                return;
            };
            let content = result
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if content.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.deliver_session_chat_composer_insert(session_id, content, cx);
            });
        })
        .detach();
    }

    pub(crate) fn start_session_chat_queued_count_polling(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_SESSION_CHAT_QUEUE_COUNT_POLL_INTERVAL)
                    .await;

                if this
                    .update(cx, |this, cx| {
                        this.refresh_session_chat_queued_counts(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Sessions whose queued-prompt count the terminal chrome needs right now,
    /// each with the daemon that owns it (`None` is this Mac's local daemon)
    /// and that daemon's own project/session ids.
    pub(crate) fn session_chat_queued_count_requests(
        &self,
    ) -> Vec<(
        TerminalSessionId,
        Option<GpuiRemoteGxserverRequestTarget>,
        String,
        String,
    )> {
        if self.active_mode != TitlebarMode::Agents {
            return Vec::new();
        }
        self.agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
            .filter(|session_id| {
                !self.agents_chat_mode_sessions.contains(session_id)
            })
            .filter(|session_id| {
                self.agents_session_chat_transcript_agent(*session_id)
                    .is_some()
            })
            .filter_map(|session_id| {
                if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
                    return Some((session_id, None, key.project_id, key.session_id));
                }
                let key = self.agents_chat_remote_key_for_session(session_id)?;
                let target = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)?;
                Some((session_id, Some(target), key.project_id, key.session_id))
            })
            .collect()
    }

    pub(crate) fn refresh_session_chat_queued_counts(&mut self, cx: &mut gpui::Context<Self>) {
        if self.session_chat_queued_count_refresh_in_flight {
            return;
        }
        let requests = self.session_chat_queued_count_requests();
        if requests.is_empty() {
            if !self.session_chat_queued_counts.is_empty() {
                self.session_chat_queued_counts.clear();
                cx.notify();
            }
            return;
        }
        self.session_chat_queued_count_refresh_in_flight = true;
        cx.spawn(async move |this, cx| {
            let reads = cx
                .background_executor()
                .spawn(async move {
                    requests
                        .into_iter()
                        .map(|(session_id, target, project_id, gxserver_session_id)| {
                            let params = serde_json::json!({
                                "projectId": project_id,
                                "sessionId": gxserver_session_id,
                            });
                            let result = match target.as_ref() {
                                Some(target) => gpui_remote_gxserver_rpc_result(
                                    target,
                                    "/api/readSessionChatQueue",
                                    &params,
                                    GPUI_SESSION_CHAT_QUEUE_COUNT_TIMEOUT,
                                ),
                                None => gpui_gxserver_rpc_result(
                                    "/api/readSessionChatQueue",
                                    &params,
                                    GPUI_SESSION_CHAT_QUEUE_COUNT_TIMEOUT,
                                ),
                            };
                            (
                                session_id,
                                result.ok().map(|value| {
                                    gpui_session_chat_queued_counts_from_result(&value)
                                }),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.session_chat_queued_count_refresh_in_flight = false;
                this.apply_session_chat_queued_counts(reads, cx);
            });
        })
        .detach();
    }

    /// A read that failed (dead tunnel, daemon restart, a daemon that predates
    /// the queue) keeps the previous count instead of blanking the chip, so a
    /// single lost round trip cannot make a pane's queue look emptied.
    pub(crate) fn apply_session_chat_queued_counts(
        &mut self,
        reads: Vec<(TerminalSessionId, Option<GpuiSessionChatQueuedCounts>)>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut counts = HashMap::new();
        for (session_id, read) in reads {
            let read = match read {
                Some(read) => read,
                None => self
                    .session_chat_queued_counts
                    .get(&session_id)
                    .copied()
                    .unwrap_or_default(),
            };
            if read.total > 0 {
                counts.insert(session_id, read);
            }
        }
        if self.session_chat_queued_counts != counts {
            self.session_chat_queued_counts = counts;
            cx.notify();
        }
    }

    /// Puts transferred draft text in the chat composer, or parks it until the
    /// composer reports itself ready. The park is not an edge case: an
    /// automatic switch starts the transfer and the surface load in the same
    /// tick, so either can win.
    pub(crate) fn deliver_session_chat_composer_insert(
        &mut self,
        session_id: TerminalSessionId,
        content: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        if self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
            && self.insert_prompt_into_session_chat(session_id, &content, cx)
        {
            return;
        }
        self.pending_session_chat_composer_insert
            .insert(session_id, content);
    }

    pub(crate) fn show_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return true;
        }
        if !self.agents_session_chat_eligible(session_id) {
            return false;
        }
        self.agents_chat_mode_sessions.insert(session_id);
        self.pending_session_chat_composer_focus = Some(session_id);
        self.reconcile_agents_pane_surfaces(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn agents_terminal_runtime_is_live_for_chat_launch(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        if self.agents_gpui_engine_terminals.contains_key(&session_id) {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            return self
                .agents_terminal_ghostty_surfaces
                .keys()
                .any(|slot_id| slot_id.session_id == session_id);
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    pub(crate) fn activate_preferred_agents_chat_launch_intent(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) else {
            return false;
        };
        if !self.pending_agents_chat_launch_intents.contains(&key) {
            return false;
        }
        // The projected agent icon is the compatibility authority available
        // before the hidden terminal runtime starts. Unsupported terminals
        // keep their normal terminal body and focus behavior.
        if self
            .agents_session_chat_transcript_agent(session_id)
            .is_none()
        {
            self.pending_agents_chat_launch_intents.remove(&key);
            return false;
        }

        self.pending_agents_chat_launch_intents.remove(&key);
        self.agents_chat_mode_sessions.insert(session_id);
        self.pending_agents_terminal_text_focus_slot = None;
        self.pending_project_editor_companion_terminal_text_focus_slot = None;
        self.pending_session_chat_composer_focus = Some(session_id);
        self.reconcile_agents_pane_surfaces(cx);
        true
    }

    pub(crate) fn reconcile_preferred_agents_chat_launch_intents(&mut self, cx: &mut gpui::Context<Self>) {
        let intents = self
            .pending_agents_chat_launch_intents
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in intents {
            let shell_session_id = match &key {
                GpuiWorkspaceTerminalSessionKey::Local(local_key) => self
                    .local_workspace_session_mappings
                    .get(local_key)
                    .copied(),
                GpuiWorkspaceTerminalSessionKey::Remote(remote_key) => {
                    self.remote_attach_sessions.get(remote_key).copied()
                }
            };
            let Some(shell_session_id) = shell_session_id else {
                continue;
            };
            // The icon is the existing chat capability authority. Once the
            // terminal exists, an unsupported launcher keeps Terminal view.
            if self
                .agents_session_chat_transcript_agent(shell_session_id)
                .is_none()
            {
                self.pending_agents_chat_launch_intents.remove(&key);
                continue;
            }
            if !self.agents_terminal_runtime_is_live_for_chat_launch(shell_session_id)
                || !self.agents_session_chat_eligible(shell_session_id)
            {
                continue;
            }
            self.pending_agents_chat_launch_intents.remove(&key);
            if self.show_agents_session_chat_mode(shell_session_id, cx) {
                /*
                CDXC:SessionChatDraftHandoff 2026-08-18:
                This intent waits for the agent to become chat-eligible, which
                can take the whole of its boot. The terminal is live and
                focused that entire time, so a user who started typing before
                the switch landed must not lose what they wrote.
                */
                self.request_session_chat_draft_transfer(shell_session_id, cx);
            }
        }
    }

    pub(crate) fn agents_session_chat_runtime_url(&self, session_id: TerminalSessionId) -> Option<String> {
        let agent = self.agents_session_chat_transcript_agent(session_id)?;
        let (project_id, gxserver_session_id, remote) =
            if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
                (key.project_id, key.session_id, false)
            } else {
                let key = self.agents_chat_remote_key_for_session(session_id)?;
                (key.project_id, key.session_id, true)
            };
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_CHAT_URL", "chat.html").ok()?;
        let mut params = vec![
            ("projectId", project_id),
            ("sessionId", gxserver_session_id),
            ("agentId", agent.to_string()),
            (
                "theme",
                gpui_session_chat_theme_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "fontFamily",
                gpui_session_chat_font_family_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                ),
            ),
            (
                "transcriptWidthPercent",
                gpui_session_chat_transcript_width_percent_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "verboseMode",
                gpui_session_chat_verbose_mode_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "hotkeys",
                shared_settings::shared_sidebar_settings_snapshot()
                    .object()
                    .get("hotkeys")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Object(Default::default()))
                    .to_string(),
            ),
        ];
        if remote {
            params.push(("remote", "true".to_string()));
        }
        Some(append_url_query_params(base_url, &params))
    }

    pub(crate) fn agents_session_chat_gxserver_bootstrap(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<cef::SidebarGxserverBootstrap> {
        if self.agents_chat_local_key_for_session(session_id).is_some() {
            return self.sidebar_gxserver_bootstrap.clone();
        }
        let key = self.agents_chat_remote_key_for_session(session_id)?;
        let target = self.gpui_remote_gxserver_request_target(key.remote_machine_id.as_str())?;
        Some(cef::SidebarGxserverBootstrap {
            base_url: format!("http://127.0.0.1:{}", target.local_port),
            auth_token: target.token,
            protocol_version: GPUI_GXSERVER_PROTOCOL_VERSION as i32,
            client_id: format!("{GPUI_SIDEBAR_GXSERVER_CLIENT_ID}-chat-{}", session_id.0),
            initial_active_project_id: Some(key.project_id),
            focused_session_id: Some(key.session_id.clone()),
            visible_session_ids: vec![key.session_id],
        })
    }

    /*
    CDXC:AgentHistorySearchModal 2026-08-23:
    Search by Prompt is a native child-window page, matching the Settings
    ownership model instead of replacing a pane body. Prompt history is
    machine-wide, so the page URL carries only the current visual theme while
    the child surface receives the local gxserver bootstrap separately.
    */
    pub(crate) fn agents_find_runtime_url(&self) -> Option<String> {
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_FIND_URL", "find.html").ok()?;
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        Some(append_url_query_params(
            base_url,
            &[
                (
                    "theme",
                    gpui_session_chat_theme_from_settings(settings.object()).to_string(),
                ),
                (
                    "fontFamily",
                    gpui_session_chat_font_family_from_settings(settings.object()),
                ),
            ],
        ))
    }

    pub(crate) fn receive_find_prompts_modal_host_action(
        &mut self,
        message: &serde_json::Value,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = message.get("action").and_then(serde_json::Value::as_str) else {
            return;
        };
        match action {
            "close" => {
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "focusSession" => {
                let project_id = message
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let session_id = message
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if project_id.is_empty() || session_id.is_empty() {
                    return;
                }
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                self.focus_local_workspace_terminal_from_message(
                    &GpuiSidebarWorkspaceTerminalFocusMessage {
                        force_remount: false,
                        placement_target_session_id: None,
                        preferred_interface: GpuiPreferredAgentInterface::Terminal,
                        project_id,
                        session_id,
                    },
                    cx,
                );
            }
            "launchSession" => {
                let command = message
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let cwd = message
                    .get("cwd")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if command.is_empty() || cwd.is_empty() {
                    return;
                }
                let title = message
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                self.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "createQuickTerminal",
                        "command": command,
                        "cwd": cwd,
                        "title": title,
                    }),
                    cx,
                );
            }
            _ => {}
        }
    }

    /// Reconcile the per-session Chat surfaces that can occupy a workspace pane.
    pub(crate) fn reconcile_agents_pane_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        self.reconcile_agents_chat_surfaces(cx);
    }

    /// The Chat CEF surface currently occupying a session's pane.
    pub(crate) fn agents_pane_cef_surface(&self, session_id: TerminalSessionId) -> Option<&Entity<CefSurface>> {
        self.agents_chat_surfaces.get(&session_id)
    }

    pub(crate) fn ensure_agents_chat_surface(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        if let Some(surface) = self.agents_chat_surfaces.get(&session_id) {
            return Some(surface.clone());
        }
        // The chat page cannot do anything without its owning gxserver
        // bootstrap; later local bootstrap or remote reconnect availability
        // retries through the normal visibility reconciliation path.
        let bootstrap = self.agents_session_chat_gxserver_bootstrap(session_id)?;
        let url = self.agents_session_chat_runtime_url(session_id)?;
        let host_action_handler = self.session_chat_host_bridge_event_handler(session_id, cx);
        let chat_theme = gpui_session_chat_theme_from_settings(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let prepaint_background = if chat_theme == "light" {
            CEF_LIGHT_PREPAINT_BACKGROUND_COLOR
        } else {
            CEF_SESSION_CHAT_DARK_PREPAINT_BACKGROUND_COLOR
        };
        let background = if chat_theme == "light" {
            rgb(0xfdfdfd).into()
        } else {
            rgb(0x0a0a0a).into()
        };
        /*
        CDXC:GPUISessionChatContextMenu 2026-08-21:
        The first-party chat composer owns a shadcn context menu instead of
        exposing Chromium's page/developer menu. Its explicit Paste action
        reads the clipboard during the user's menu gesture, so grant only this
        bundled chat origin the same bounded clipboard capability that the
        app-owned Source surface receives.
        */
        let trusted_clipboard_origin = Some(url.clone());
        let surface = match CefSurface::try_new(
            format!("ghostex-gpui-session-chat-{}", session_id.0),
            self.parent_ns_view,
            url,
            "session-chat".to_string(),
            prepaint_background,
            false,
            background,
            trusted_clipboard_origin,
            true,
            None,
            None,
            None,
            None,
            Some(bootstrap),
            None,
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::SessionChat),
            Some(host_action_handler),
            None,
            cx,
        ) {
            Ok(surface) => surface,
            Err(error) => {
                // Ensure-style reconcile: skip this pass, retried on the next
                // visibility sync (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "sessionChat", "error": error }),
                );
                return None;
            }
        };
        self.agents_chat_surfaces
            .insert(session_id, surface.clone());
        Some(surface)
    }

    pub(crate) fn reconcile_agents_chat_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        // Drop chat state for sessions that no longer exist in the shell.
        let live_session_ids = self
            .agents_workspace
            .terminal_session_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        self.agents_chat_mode_sessions
            .retain(|session_id| live_session_ids.contains(session_id));
        let stale_surface_ids = self
            .agents_chat_surfaces
            .keys()
            .copied()
            .filter(|session_id| {
                !self.agents_chat_mode_sessions.contains(session_id)
                    && !self
                        .pending_session_chat_draft_handoffs
                        .contains(session_id)
            })
            .collect::<Vec<_>>();
        for session_id in stale_surface_ids {
            self.session_chat_composer_ready_sessions
                .remove(&session_id);
            if let Some(surface) = self.agents_chat_surfaces.remove(&session_id) {
                surface.update(cx, |surface, _| surface.set_visible(false));
            }
        }

        let drag_active = self.workspace_tab_drag_active
            || self.browser_tab_drag_active
            || self.command_tab_drag_active;
        let visible_session_ids = if drag_active {
            HashSet::new()
        } else if self.active_mode == TitlebarMode::Agents {
            self.agents_workspace
                .rendered_leaf_order()
                .into_iter()
                .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
                .filter(|session_id| self.agents_chat_mode_sessions.contains(session_id))
                .collect::<HashSet<_>>()
        } else if self.active_mode.is_project_editor_mode() {
            // CDXC:GPUISessionChatSurface 2026-08-02: the companion side pane
            // shows chat-mode sessions in Code/Browser/Kanban/Automate/Docs
            // too. The mount-slot enumeration already gates on companion
            // visibility, mode wakefulness, and slot eligibility.
            self.current_project_editor_companion_terminal_body_mount_slots()
                .into_iter()
                .map(|slot_id| slot_id.session_id)
                .filter(|session_id| self.agents_chat_mode_sessions.contains(session_id))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        for session_id in &visible_session_ids {
            let _ = self.ensure_agents_chat_surface(*session_id, cx);
        }
        for (session_id, surface) in &self.agents_chat_surfaces {
            let visible = visible_session_ids.contains(session_id);
            surface.update(cx, |surface, _| surface.set_visible(visible));
        }
    }

    pub(crate) fn remove_agents_chat_surface_for_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_chat_mode_sessions.remove(&session_id);
        self.session_chat_composer_ready_sessions
            .remove(&session_id);
        if self.pending_session_chat_composer_focus == Some(session_id) {
            self.pending_session_chat_composer_focus = None;
        }
        self.pending_session_chat_composer_insert
            .remove(&session_id);
        /*
        CDXC:SessionChatDraftHandoff 2026-08-24:
        A handed-off draft that never reached its terminal is dropped here with
        no way to hand it anywhere else — the chat surface that owned it is
        going away in the same call. That is survivable only because the record
        is not the text's only home: the transient Saved Prompts row created
        before the composer was cleared is deleted solely on a confirmed paste,
        so this drop leaves the draft recoverable from Prompts.
        */
        self.pending_session_terminal_composer_insert
            .remove(&session_id);
        self.pending_session_chat_draft_handoffs.remove(&session_id);
        if let Some(surface) = self.agents_chat_surfaces.remove(&session_id) {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
    }

    pub(crate) fn remove_all_agents_chat_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        self.agents_chat_mode_sessions.clear();
        self.agents_chat_auto_switch_observed_sessions.clear();
        self.pending_agents_chat_launch_intents.clear();
        self.session_chat_composer_ready_sessions.clear();
        self.pending_session_chat_composer_focus = None;
        self.pending_session_chat_composer_insert.clear();
        // Same contract as the per-session teardown above: every dropped
        // handoff still has its Saved Prompts row, which only a confirmed
        // terminal paste deletes.
        self.pending_session_terminal_composer_insert.clear();
        self.pending_session_chat_draft_handoffs.clear();
        for surface in self.agents_chat_surfaces.values() {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
        self.agents_chat_surfaces.clear();
    }
}
