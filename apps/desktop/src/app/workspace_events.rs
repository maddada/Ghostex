// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-workarea + sidebar bridge events, agents chat/find surfaces

use std::rc::Rc;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
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
        CDXC:CefRuntime 2026-06-24-11:03:
        Runtime workarea bridge events are accepted only from the CefSurface that owns the current slot. Manage file events resolve against the explicit in-memory project root from the sidebar snapshot, Kanban/Automate Beads and board events call gxserver's typed Project Board endpoints, and response dispatch stays inside the owning CEF surface without WKWebView/WebKit handlers, shelling out to bd, fallback project detection, logs, persistence, or generic IPC.
        */
        match (slot_key, event) {
            (
                ProjectWorkareaCefSurfaceSlotKey::Manage,
                cef::ProjectWorkareaBridgeEvent::ManageFilesRequest(payload),
            ) => {
                // CDXC:Docs 2026-09-06 SEE-ALSO: The shared Mermaid viewer opens through the same native child-window route as chat (packages/core-ui/mermaid/mermaid-diagram.tsx).
                if let Ok(request) = serde_json::from_str::<serde_json::Value>(&payload)
                    && request.get("action").and_then(serde_json::Value::as_str)
                        == Some("openMermaidDiagram")
                {
                    if let Some(source) = request.get("source").and_then(serde_json::Value::as_str)
                    {
                        self.receive_app_modal_host_bridge_event(
                            cef::AppModalHostBridgeEvent::Message(
                                serde_json::json!({
                                    "type": "open", "modal": "mermaidDiagram", "source": source,
                                })
                                .to_string(),
                            ),
                            window,
                            cx,
                        );
                    }
                    return;
                }
                /*
                CDXC:Docs 2026-07-11:
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
                let chat_docs_authorization = snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.active_project_id.as_ref())
                    .and_then(|project_id| {
                        self.session_chat_docs_file_authorization
                            .lock()
                            .ok()
                            .and_then(|authorization| {
                                authorization
                                    .as_ref()
                                    .filter(|authorization| {
                                        authorization.project_id == project_id.0
                                    })
                                    .cloned()
                            })
                    });
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
                                    chat_docs_authorization
                                        .as_ref()
                                        .map(|authorization| authorization.root.clone()),
                                    chat_docs_authorization
                                        .map(|authorization| authorization.file_name),
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
                                CDXC:ProjectBoard 2026-08-14:
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
        CDXC:Navigation 2026-07-29:
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
                CDXC:Workarea 2026-06-29-00:02:
                Legacy sidebar readiness/proof messages stay accepted as compatibility no-ops. Source, Kanban, Automate, and Manage mounting now follows only the current runtime URL gate plus owned CEF surface map, and first-party Kanban/Automate/Manage CEF requests still flow through the separate project-workarea bridge.
                */
            }
            cef::SidebarBridgeEvent::NativeProjectPathAction(payload) => {
                self.receive_sidebar_native_project_path_action_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::NativeAppShotPrompt(payload) => {
                self.receive_sidebar_native_app_shot_prompt_payload(&payload, cx);
            }
            cef::SidebarBridgeEvent::ResourcesSnapshotRequest(payload) => {
                self.receive_sidebar_resources_snapshot_request_payload(&payload, cx);
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
            cef::SidebarBridgeEvent::RefusedPageNavigation(url) => {
                // Same rule as a clicked transcript link: embedded Browser
                // while "Open links in embedded browser" is on, else the
                // system browser (CDXC:SessionChat 2026-09-09 in
                // cef/shell/request_handling.rs).
                self.open_session_chat_link(&url, false, false, window, cx);
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
        CDXC:Browser 2026-07-12:
        The sidebar lists browser rows for every project (parked local and
        machine-scoped remote models included), so this bridge must reach
        beyond the active browser project: close edits the parked model
        directly, and focus swaps the browser workarea to the owning project
        first — but only when that parked model really contains the tab, so
        stale rows cannot park the live project into an empty default model.

        CDXC:Browser 2026-08-26:
        A parked project's tabs do own live CEF surfaces now, so close and
        sleep reach the parked bundle too: both drop that tab's parked page, the
        same teardown the mounted project gets, instead of leaving an orphaned
        browser behind a row that says it is asleep.
        */
        let is_active_browser_project =
            self.browser_tabs_project_id.as_deref() == Some(message.project_id.as_str());
        if object.get("close").and_then(serde_json::Value::as_bool) == Some(true) {
            if is_active_browser_project {
                self.close_browser_tab(message.tab_id, window, cx);
                return;
            }
            self.close_parked_browser_tab(&message.project_id, message.tab_id, cx);
            return;
        }
        if object.get("sleeping").and_then(serde_json::Value::as_bool) == Some(true) {
            if !is_active_browser_project {
                self.sleep_parked_browser_tab(&message.project_id, message.tab_id, cx);
                return;
            }
            if find_browser_leaf_id_for_tab(&self.browser_tabs.root, message.tab_id).is_none() {
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
        CDXC:Extensions 2026-08-23:
        Close and sleep above are housekeeping the sidebar may still need for
        tabs that already exist, but everything past this point focuses the
        Browser workarea. With Browser turned off in Settings → Customize a
        stale sidebar tab row must not be able to drag the shell back into it.

        CDXC:Browser 2026-08-26:
        Availability is decided by the tab's own project, not by whichever
        project the shell is currently showing. A row of another project is
        exactly the click that has to switch projects, and its active-project
        context is still in flight — the sidebar publishes it first, but the
        project-switch coalescer can hold it for the settle window — so
        answering the arriving payload from the outgoing context dropped those
        clicks silently. Every payload here carries a validated real project
        key, and a real project always has the Browser workarea, so this
        matches `open_browser_url_from_renderer_command`: an explicit project
        target skips the context-scoped predicate and only the
        project-independent Customize refusal still applies.
        */
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Browser) {
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
        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(ShellFocusTarget::BrowserPane(pane_id), cx);
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

        CDXC:Browser 2026-07-12:
        A validated explicit project target swaps the browser workarea to that
        project's tab model synchronously before the open, so sidebar project
        headers (local and machine-scoped remote) never race the async
        active-project context round-trip. Explicit real-project targets always
        have the Browser workarea, so the availability gate only applies to
        untargeted opens.
        */
        /*
        CDXC:Extensions 2026-08-23:
        This is the one door every embedded-browser open goes through — chat
        and terminal links, saved Action links, sidebar project and Quick
        headers, `ghostex browser open`. With Browser turned off in Settings →
        Customize none of them may open a tab, and that includes the two routes
        that bypass the availability gate below (an explicit project target and
        the projectless Quick header), so the Customize refusal is checked
        first and answers with the copied link rather than a dead click.
        */
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Browser) {
            self.copy_path_for_disabled_project_workarea(&message.url, "Browser", cx);
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
            self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
            self.focus_shell_target(
                ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
                cx,
            );
            self.commit_browser_address(url, cx);
            self.sync_active_browser_tab_to_surface(window, cx);
            self.scroll_focused_browser_pane_active_tab();
            return;
        }
        let created_tab_id = self.browser_tabs.add_loaded_popup_tab(
            url,
            self.browser_profiles.active_profile_id(),
            cef::BrowserPopupPlacement::Selected,
        );
        let Some(created_tab_id) = created_tab_id else {
            return;
        };
        self.request_sidebar_browser_tab_reveal(created_tab_id);
        self.change_active_mode_with_pane_state(TitlebarMode::Browser, cx);
        self.mark_project_editor_mode_awake(TitlebarMode::Browser, cx);
        self.focus_shell_target(
            ShellFocusTarget::BrowserPane(self.browser_tabs.focused_pane),
            cx,
        );
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
        CDXC:FocusRouting 2026-06-24-21:07:
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
        CDXC:FocusRouting 2026-06-26-06:08:
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
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": message.project_id,
                "sessionId": message.session_id,
                "settleWindowActive": self
                    .project_switch_settling_until
                    .is_some_and(|until| Instant::now() < until),
            }),
        );
        /*
        CDXC:Navigation 2026-07-29:
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
            support_logs::append_temporary(
                support_logs::GpuiSupportLog::TerminalFocus,
                "TEMP.gpui.sessionSwitchLatency.coalescedDeferred",
                serde_json::json!({
                    "epochMs": support_logs::temporary_epoch_ms(),
                    "projectId": message.project_id,
                    "sessionId": message.session_id,
                }),
            );
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
    CDXC:SessionTitles 2026-07-29:
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
        /*
        CDXC:Navigation 2026-09-04 DECISION:
        User: restart must restore the last active project, the last active view, and the last visible sessions.
        The restored shell state already put this session on its pane as the active tab, `attach_surfaced_local_workspace_terminals` attaches it there, and in a project-editor mode with the companion open the focus-state handler retargets the companion to it.
        When the user quit on Code, Browser, Kanban, Automate, or Docs with no companion, the ordinary focus path below would switch the app to Agents and move keyboard focus to the pane, replacing the restored view with a different one.
        The replay therefore stops here in that case; `local_workspace_latest_focus_key` stays untouched so the pending surfaced-restore attach completes as a silent restore rather than being promoted to a click.
        */
        if message.startup_restore
            && self.active_mode != TitlebarMode::Agents
            && !self.should_keep_project_editor_open_for_local_workspace_terminal_focus(&key)
        {
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.startupRestoreKeptView",
                serde_json::json!({
                    "projectId": key.project_id,
                    "sessionId": key.session_id,
                }),
            );
            return;
        }
        if message.preferred_interface == GpuiPreferredAgentInterface::Chat {
            self.pending_agents_chat_launch_intents
                .insert(GpuiWorkspaceTerminalSessionKey::Local(key.clone()));
        }
        /*
        CDXC:Navigation 2026-09-11 DECISION:
        User: landing on another project keeps that project's remembered view; only a session click inside the active project still opens Agents.
        The project swap that preceded this focus already restored the destination's view, so a keep-view focus must not overwrite it: the tab is selected in the background and attached silently when it has no live terminal yet.
        A remembered Agents view, or a Code/Docs companion that can show the session, takes the ordinary path below.
        */
        if message.keep_view
            && self.active_mode != TitlebarMode::Agents
            && !self.should_keep_project_editor_open_for_local_workspace_terminal_focus(&key)
        {
            self.select_local_workspace_terminal_keeping_view(&key, cx);
            return;
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
        CDXC:SessionFork 2026-07-10:
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
                "epochMs": support_logs::temporary_epoch_ms(),
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
        CDXC:Workarea 2026-09-04 DECISION:
        User: Advanced > Split Right opens the session in a pane to the right of
        the focused agents pane. A session that already has a tab is moved into
        a new right-hand leaf here, then the ordinary focus below selects it; a
        session with no tab yet is attached into a new leaf at completion.
        Splitting the lone tab of the focused pane is a no-op inside the model,
        so that case degrades to a plain focus.
        */
        if message.placement == GpuiWorkspaceTerminalFocusPlacement::SplitRight
            && let Some((shell_session_id, source_pane_id)) =
                mapped_shell_session_id.zip(mapped_pane_id)
            && self.agents_workspace.split_tab_to_pane(
                source_pane_id,
                requested_pane_id,
                shell_session_id,
                WorkspaceDropZone::Right,
            )
        {
            self.persist_shell_layout_state();
            cx.notify();
        }
        /*
        CDXC:CefRuntime 2026-07-12:
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
                    "epochMs": support_logs::temporary_epoch_ms(),
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
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        self.spawn_local_workspace_attach_plan(
            key,
            attach_intent,
            requested_pane_id,
            force_requested_pane_placement,
            message.placement,
            GpuiLocalWorkspaceAttachOrigin::SidebarFocus,
            cx,
        );
    }

    pub(crate) fn receive_sidebar_create_project_agent_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let _profile = crate::profiling::span(crate::profiling::Metric::AgentCreate);
        let Ok(message) = gpui_sidebar_create_project_agent_from_json(payload) else {
            return;
        };
        // Only the Windows arm below drives a workspace agent from this payload.
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (message, cx);
            return;
        }
        #[cfg(target_os = "windows")]
        {
            if message.preferred_interface == GpuiPreferredAgentInterface::Chat {
                self.request_windows_agent_chat_launch(message, cx);
                return;
            }
            /*
            CDXC:PlatformSupport 2026-08-11:
            Project-header agents on Windows use one Rust-owned WSL operation
            from gxserver row creation through provider startup and terminal
            attachment. CEF supplies only the clicked project id, selected
            agent id, and bounded interface preference; gxserver resolves the
            authoritative project path, configured command, launch policy,
            and attach metadata.
            */
            let account_id = message.account_id;
            let agent_id = message.agent_id;
            let preferred_interface = message.preferred_interface;
            let project_id = message.project_id;
            let request_id = message.request_id;
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let result = background
                    .spawn(async move {
                        gpui_create_local_project_workspace_agent(
                            project_id.as_str(),
                            agent_id.as_str(),
                            account_id.as_deref(),
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
                        let opened = this.open_gpui_local_workspace_terminal(
                            key,
                            plan,
                            requested_pane_id,
                            false,
                            cx,
                        );
                        if !opened {
                            this.pending_agents_chat_launch_intents
                                .remove(&workspace_key);
                            this.compensate_unmaterialized_created_workspace_terminal(&cleanup_key);
                        }
                        if let Some(request_id) = request_id.as_deref() {
                            this.dispatch_gpui_first_launch_create_project_session_result(
                                request_id,
                                opened,
                                (!opened)
                                    .then_some("Ghostex could not open the new agent session."),
                                cx,
                            );
                        }
                    }
                    Err(message) => {
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Agent unavailable",
                            message.as_str(),
                            cx,
                        );
                        if let Some(request_id) = request_id.as_deref() {
                            this.dispatch_gpui_first_launch_create_project_session_result(
                                request_id,
                                false,
                                Some(message.as_str()),
                                cx,
                            );
                        }
                    }
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
            CDXC:PlatformSupport 2026-07-26:
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
            let request_id = message.request_id;
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
                        let opened = this.open_gpui_local_workspace_terminal(
                            key,
                            plan,
                            requested_pane_id,
                            false,
                            cx,
                        );
                        if !opened {
                            this.compensate_unmaterialized_created_workspace_terminal(&cleanup_key);
                        }
                        if let Some(request_id) = request_id.as_deref() {
                            this.dispatch_gpui_first_launch_create_project_session_result(
                                request_id,
                                opened,
                                (!opened)
                                    .then_some("Ghostex could not open the new terminal session."),
                                cx,
                            );
                        }
                    }
                    Err(message) => {
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Terminal unavailable",
                            message.as_str(),
                            cx,
                        );
                        if let Some(request_id) = request_id.as_deref() {
                            this.dispatch_gpui_first_launch_create_project_session_result(
                                request_id,
                                false,
                                Some(message.as_str()),
                                cx,
                            );
                        }
                    }
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
        placement: GpuiWorkspaceTerminalFocusPlacement,
        origin: GpuiLocalWorkspaceAttachOrigin,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.local_workspace_attach_pending.insert(key.clone()) {
            return;
        }

        let attach_started_at = Instant::now();
        let open_chat_early = placement == GpuiWorkspaceTerminalFocusPlacement::Tab
            && self
                .pending_agents_chat_launch_intents
                .contains(&GpuiWorkspaceTerminalSessionKey::Local(key.clone()));
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            if open_chat_early {
                let preview_key = key.clone();
                let preview = background
                    .spawn(async move {
                        gpui_gxserver_rpc_result(
                            "/api/attachSessionMetadata",
                            &serde_json::json!({
                                "projectId": preview_key.project_id,
                                "sessionId": preview_key.session_id,
                            }),
                            std::time::Duration::from_secs(15),
                        )
                    })
                    .await;
                if let Ok(metadata) = preview {
                    let _ = this.update(cx, |this, cx| {
                        if this.local_workspace_latest_focus_key.as_ref() == Some(&key) {
                            this.show_pending_agents_chat_launch(
                                GpuiWorkspaceTerminalSessionKey::Local(key.clone()),
                                &metadata,
                                requested_pane_id,
                                cx,
                            );
                        }
                    });
                }
            }
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
                        "epochMs": support_logs::temporary_epoch_ms(),
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
                    GpuiLocalWorkspaceAttachOrigin::BackgroundSelect => {
                        // The view stayed on another mode while the plan was
                        // prepared, so `local_workspace_latest_focus_key` was
                        // never set for this request. The workspace must still
                        // belong to the project and the sidebar must still show
                        // the session as focused, or the tab would land in the
                        // wrong workspace or override a newer selection.
                        if this.agents_workspace_project_id.as_deref()
                            != Some(key.project_id.as_str())
                            || this
                                .sidebar_gxserver_presentation_focus_state
                                .focused_session_id
                                .as_deref()
                                != Some(key.session_id.as_str())
                        {
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
                        GpuiLocalWorkspaceAttachOrigin::BackgroundSelect => {
                            let _ = this.open_gpui_local_workspace_terminal_keeping_view(
                                key,
                                plan,
                                requested_pane_id,
                                cx,
                            );
                        }
                        GpuiLocalWorkspaceAttachOrigin::SidebarFocus
                        | GpuiLocalWorkspaceAttachOrigin::WakeRecovery => {
                            // A Split Right request whose tab already exists was
                            // moved into its right-hand leaf when the focus arrived;
                            // the ordinary open reuses that tab in place. Only a
                            // session without a tab is attached into a new leaf here.
                            if placement == GpuiWorkspaceTerminalFocusPlacement::SplitRight
                                && !this.local_workspace_session_mappings.contains_key(&key)
                            {
                                let _ = this.open_gpui_local_workspace_terminal_in_new_leaf(
                                    key,
                                    plan,
                                    requested_pane_id,
                                    AgentsWorkspaceNewTerminalPlacement::SplitRight,
                                    cx,
                                );
                            } else {
                                let _ = this.open_gpui_local_workspace_terminal(
                                    key,
                                    plan,
                                    requested_pane_id,
                                    force_requested_pane_placement,
                                    cx,
                                );
                            }
                        }
                    },
                    Err(message) => {
                        if matches!(
                            completion_origin,
                            GpuiLocalWorkspaceAttachOrigin::SurfacedRestore
                                | GpuiLocalWorkspaceAttachOrigin::BackgroundSelect
                        ) {
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
}
