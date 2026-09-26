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
            // CDXC:Clipboard 2026-09-15 SEE-ALSO: The Kanban, Automate, and Docs pages have no app-modal host shim, so packages/core-ui/copy-sound.ts posts the copy-sound request through the project-board bridge function every project workarea page receives; it needs no response.
            (_, cef::ProjectWorkareaBridgeEvent::ProjectBoardRequest(payload))
                if serde_json::from_str::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|request| manage_request_string(&request, "action"))
                    .as_deref()
                    == Some("playCopySound") =>
            {
                gpui_copy_feedback(cx);
            }
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
                // Native Docs' browser area: a link in an HTML file opens that file in Docs.
                if let Ok(request) = serde_json::from_str::<serde_json::Value>(&payload)
                    && manage_request_string(&request, "action").as_deref() == Some("openDocsFile")
                {
                    if let Some(path) = manage_request_string(&request, "path") {
                        self.native_docs_open_external(path, cx);
                    }
                    return;
                }
                // Annotation feedback never touches the file system: the target
                // session and the delivery are app state, so both requests are
                // answered here instead of through the git-backed file bridge.
                if let Ok(request) = serde_json::from_str::<serde_json::Value>(&payload)
                    && let Some(action) = manage_request_string(&request, "action")
                    && matches!(
                        action.as_str(),
                        "annotationSendTarget" | "sendAnnotationFeedback"
                    )
                {
                    let request_id =
                        manage_request_string(&request, "requestId").unwrap_or_default();
                    if action == "annotationSendTarget" {
                        let response =
                            self.docs_annotation_send_target_response(&action, &request_id);
                        self.dispatch_project_workarea_json_event(
                            slot_key,
                            "ghostex-manage-files-response",
                            &response.to_string(),
                            cx,
                        );
                    } else {
                        let content =
                            manage_request_string(&request, "content").unwrap_or_default();
                        self.send_docs_annotation_feedback(action, request_id, content, cx);
                    }
                    return;
                }
                self.run_docs_files_request(payload, cx, move |this, response, cx| {
                    this.dispatch_project_workarea_json_event(
                        slot_key,
                        "ghostex-manage-files-response",
                        &response.to_string(),
                        cx,
                    );
                });
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
                    startWork, links, jumps, toasts) live in the Rust store
                    (gx_store/create/board.rs; the sidebar runtime until
                    2026-09-25), which owns agents, presentation state, focus
                    routing, and the gxserver client. Rust bounds the first-party
                    page request and later routes the store's response back to
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

    pub(crate) fn receive_sidebar_project_board_conversation_response_payload(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The store (gx_store/create/board.rs) answers board conversation requests
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
        if self.route_native_automate_board_response(response, cx) {
            return;
        }
        if self.native_kanban_receive_conversation_response(response, cx) {
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

    pub(crate) fn open_browser_url_from_renderer_command(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_machine_id = match message.project_id.as_deref() {
            Some(project_id) => gpui_remote_project_reference_from_project_id(project_id)
                .map(|reference| reference.remote_machine_id),
            None if !message.from_quick_header => self.browser_project_remote_machine_id(),
            None => None,
        };
        self.open_browser_url_from_renderer_command_with_machine(
            message,
            remote_machine_id,
            window,
            cx,
        );
    }

    pub(crate) fn open_browser_url_from_renderer_command_with_machine(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        remote_machine_id: Option<String>,
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
        if let Some((pane_id, tab_id)) = self.browser_tabs.find_renderer_open_reuse_tab(
            &url,
            message.reuse,
            remote_machine_id.as_deref(),
        ) {
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
        if let Some(tab) = self
            .browser_tabs
            .tabs
            .iter_mut()
            .find(|tab| tab.id == created_tab_id)
        {
            tab.remote_machine_id = remote_machine_id;
        }
        self.reveal_new_browser_tab(created_tab_id);
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
        The restored shell state already put this session on its pane as the active tab, and `attach_surfaced_local_workspace_terminals` attaches it there.
        The replay stops here while a view panel is open; `local_workspace_latest_focus_key` stays untouched so the pending surfaced-restore attach completes as a silent restore rather than being promoted to a click.

        CDXC:Workarea 2026-09-20 WHY:
        The clause about the ordinary focus path switching the app to Agents no longer applies: opening
        a session cannot close the view panel any more. The early return is kept for the other half of
        the rule, which is that a restore must not be promoted to a click.
        */
        if message.startup_restore
            && self.view_panel_open()
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
        /*
        CDXC:SessionChat 2026-09-20 WHY:
        Every sidebar focus now carries the session's effective Default Agent View, not only the creation flows, so a
        restore can open its chat surface while the wake and attach run behind it instead of showing the terminal until
        the runtime is live. A session that already has a tab carries its own recorded view in
        `agents_chat_mode_sessions`, and arming the launch intent for it would pull a tab the user deliberately put back
        in Terminal into Chat on the next click, so only a session with no tab yet takes the intent.
        */
        let session_has_tab = self
            .local_workspace_session_mappings
            .get(&key)
            .copied()
            .is_some_and(|shell_session_id| {
                self.agents_workspace
                    .pane_id_for_session(shell_session_id)
                    .is_some()
            });
        if message.preferred_interface == GpuiPreferredAgentInterface::Chat && !session_has_tab {
            self.pending_agents_chat_launch_intents
                .insert(GpuiWorkspaceTerminalSessionKey::Local(key.clone()));
        }
        /*
        CDXC:Navigation 2026-09-11 DECISION:
        User: landing on another project keeps that project's remembered view; only a session click inside the active project still opens Agents.
        The project swap that preceded this focus already restored the destination's view, so a keep-view focus must not overwrite it: the tab is selected in the background and attached silently when it has no live terminal yet.
        A project with no view panel open takes the ordinary path below.

        CDXC:Workarea 2026-09-20 WHY:
        The decision's second clause has no object any more: a session click inside the active project
        never opens or closes the view panel, because the sessions are on screen beside it. What
        survives is its first clause, which this branch still implements — landing on another project
        selects the session in the background and leaves that project's remembered view alone.
        */
        let keeps_view = message.keep_view
            && self.view_panel_open()
            && !self.should_keep_project_editor_open_for_local_workspace_terminal_focus(&key);
        if message.keep_sleeping
            && !message.force_remount
            && self.select_sleeping_local_workspace_tab(&key, keeps_view, cx)
        {
            return;
        }
        if keeps_view {
            self.select_local_workspace_terminal_keeping_view(&key, message.wake_sleeping, cx);
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
            if let Some(shell_session_id) = self.local_workspace_session_mappings.get(&key).copied()
            {
                self.agents_gpui_terminal_viewer_recipes
                    .remove(&shell_session_id);
                self.agents_terminal_chat_claims.remove(&shell_session_id);
                self.agents_gpui_engine_terminals.remove(&shell_session_id);
                cx.notify();
            }
        /*
        CDXC:FocusRouting 2026-09-20 WHY:
        A session the sidebar reports asleep never reuses its mapped tab in place: the daemon behind that tab is dead,
        so reusing it shows a terminal nothing is attached to. It goes to the attach plan below, whose Wake intent
        revives the provider and rebuilds the attach payload for the same tab. This is what the sidebar's own wake used
        to guarantee by running before this message was ever posted.
        */
        } else if !message.wake_sleeping
            && self.focus_existing_gpui_local_workspace_terminal(&key, cx)
        {
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
        let mapped_attach_intent = self.local_workspace_attach_intent_for_key(&key);
        // A session the sidebar knows is asleep wakes through this one plan (`wake_sleeping` on the message), whether
        // or not it still has a mapped tab to read a Sleeping presentation state from.
        let attach_intent = if message.wake_sleeping {
            GpuiLocalWorkspaceAttachIntent::Wake
        } else {
            mapped_attach_intent
        };
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.attachPlanRequired",
            serde_json::json!({
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        // A mapped tab with nothing live behind it is re-attached where it is, so it is brought to
        // the focused pane first, as the focus-existing path above does (session_pane_placement.rs).
        if message.placement == GpuiWorkspaceTerminalFocusPlacement::Tab
            && !force_requested_pane_placement
            && let Some(shell_session_id) = self.local_workspace_session_mappings.get(&key).copied()
            && let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id)
        {
            self.pull_workspace_session_into_focused_pane(pane_id, shell_session_id);
        }
        let created_here = self.gx_store_is_created_attach(&key);
        self.spawn_local_workspace_attach_plan(
            key,
            attach_intent,
            requested_pane_id,
            force_requested_pane_placement,
            message.placement,
            match message.placement_target_session_id {
                Some(_) => GpuiLocalWorkspaceAttachOrigin::Fork,
                None if created_here => GpuiLocalWorkspaceAttachOrigin::Fork,
                None => GpuiLocalWorkspaceAttachOrigin::SidebarFocus,
            },
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
                        // CDXC:Workarea 2026-09-20 WHY:
                        // A newly created agent lands in the Agents column, which is beside whatever
                        // the view panel shows, so there is no longer a keep-view variant to choose.
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
        let focused_at_request = self.gx_store_focused_session();
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
                                origin == GpuiLocalWorkspaceAttachOrigin::BackgroundSelect
                                    || this.should_keep_project_editor_open_for_local_workspace_terminal_focus(&key),
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
                // The store's newest selection, not the workspace's focus state copy, which lags
                // it while a project switch is coalesced (gx_store_selection_names_local_session).
                let selected = this.gx_store_selection_names_local_session(&key);
                let completion_origin = if origin == GpuiLocalWorkspaceAttachOrigin::SurfacedRestore
                    && this.local_workspace_latest_focus_key.as_ref() == Some(&key)
                    && selected
                {
                    GpuiLocalWorkspaceAttachOrigin::SidebarFocus
                } else {
                    origin
                };
                match completion_origin {
                    GpuiLocalWorkspaceAttachOrigin::SidebarFocus => {
                        if this.local_workspace_latest_focus_key.as_ref() != Some(&key) || !selected {
                            return;
                        }
                    }
                    GpuiLocalWorkspaceAttachOrigin::Fork => {
                        // A newer focus request, a project switch, or a selection the store took
                        // since the fork was requested wins over it. The runtime's focus copy is
                        // not consulted: it still names the source session until it is told.
                        let focused = this.gx_store_focused_session();
                        if this.local_workspace_latest_focus_key.as_ref() != Some(&key)
                            || this.agents_workspace_project_id.as_deref()
                                != Some(key.project_id.as_str())
                            || (focused != focused_at_request
                                && focused.as_ref().is_none_or(|focused| {
                                    !focused.machine.is_local()
                                        || focused.project_id != key.project_id
                                        || focused.session_id != key.session_id
                                }))
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
                            || !selected && !this.gx_store_is_created_attach(&key)
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
                        | GpuiLocalWorkspaceAttachOrigin::Fork
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
