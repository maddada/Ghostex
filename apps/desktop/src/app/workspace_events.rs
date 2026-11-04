// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-workarea + sidebar bridge events, agents chat/find surfaces

use crate::app::browser_parked_runtime::{TargetedBrowserOpenRoute, targeted_browser_open_route};
use crate::app::helpers::web_bridge_types::AppModalHostBridgeEvent;
use std::rc::Rc;

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
                // CDXC:Docs 2026-09-06 SEE-ALSO: The shared Mermaid viewer opens through the same native child-window route as chat (packages/core-ui/mermaid/mermaid-diagram.tsx (deleted 2026-10-01)).
                if let Ok(request) = serde_json::from_str::<serde_json::Value>(&payload)
                    && request.get("action").and_then(serde_json::Value::as_str)
                        == Some("openMermaidDiagram")
                {
                    if let Some(source) = request.get("source").and_then(serde_json::Value::as_str)
                    {
                        self.receive_app_modal_host_bridge_event(
                            AppModalHostBridgeEvent::Message(
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
                        // A link inside an outside file names a file beside it on the resource
                        // origin; it opens by its real path, through a grant of its own.
                        let outside = self.native_docs.project.as_ref().and_then(|project| {
                            manage_chat_file_real_path(&project.project_id, &path)
                        });
                        match outside {
                            Some(real) => {
                                self.native_docs_open_absolute_path(
                                    std::path::Path::new(&real),
                                    cx,
                                );
                            }
                            None => self.native_docs_open_external(path, cx),
                        }
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
                let page_save = serde_json::from_str::<serde_json::Value>(&payload)
                    .ok()
                    .filter(|request| {
                        manage_request_string(request, "action").as_deref() == Some("save")
                    })
                    .and_then(|request| manage_request_string(&request, "path"));
                if let Some(path) = page_save.as_deref() {
                    self.native_docs_begin_page_save(path);
                }
                self.run_docs_files_request(payload, cx, move |this, response, cx| {
                    if let Some(path) = page_save.as_deref() {
                        this.native_docs_finish_page_save(path, &response);
                    }
                    this.dispatch_project_workarea_json_event(
                        slot_key,
                        "ghostex-manage-files-response",
                        &response.to_string(),
                        cx,
                    );
                });
            }
            // The Work page's requests (app/work_view/bridge.rs).
            (
                ProjectWorkareaCefSurfaceSlotKey::Work,
                cef::ProjectWorkareaBridgeEvent::ProjectBoardRequest(payload),
            ) => {
                self.receive_work_view_request(payload, cx);
            }
            // Same rule as a clicked chat link (CDXC:SessionChat 2026-09-09 in
            // cef/shell/request_handling.rs).
            (_, cef::ProjectWorkareaBridgeEvent::RefusedPageNavigation(url)) => {
                self.open_session_chat_link(&url, None, false, false, window, cx);
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
        // A page an agent published may be showing in the Files slot (native_docs/published_page.rs);
        // a Files answer still on its way to the embed page it replaced must not reach it.
        if slot_key == ProjectWorkareaCefSurfaceSlotKey::Manage
            && self.native_docs_active_page().is_some()
        {
            return;
        }
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

    /// CDXC:Browser 2026-10-31 WHY:
    /// An explicit project target that is not on screen gets the page in its own Browser in the background (`open_browser_url_in_parked_project`). Swapping the live tab model to the target, the 2026-07-12 rule this supersedes, showed that project's pages in the panel of the project on screen, and `ghostex browser open` targets the agent's own project by default.
    /// Any project switch still settling lands first, whichever project it goes to, so the open is routed against the project that will be on screen: routed against the one being left, an agent's page opened in its Browser on screen and vanished when the switch landed (Discord report, 2026-10-27); a sidebar project header still opens its page on screen. Supersedes the 2026-10-25 rule that landed only a switch to the target. Explicit real-project targets always have the Browser workarea, so the availability gate only applies to untargeted opens.
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
        /*
        CDXC:CefRuntime 2026-09-28 WHY:
        Without the optional web runtime a link opened here would land on the Browser's install prompt, so it opens in the system browser instead; the Open links in setting keeps its value and applies again once the runtime is installed. A remote computer's own localhost is the exception: the system browser cannot reach it, so that link still opens the Browser, which offers the install.
        */
        if remote_machine_id.is_none() && !crate::app::helpers::web_runtime::web_runtime_available()
        {
            if let Some(url) = normalize_address(&message.url) {
                let _ = gpui_open_external_http_url(&url);
            }
            return;
        }
        if let Some(project_id) = message.project_id.as_deref() {
            if !self.project_switch_pending_requests.is_empty() {
                self.flush_coalesced_project_switch_requests(window, cx);
            }
            let route = targeted_browser_open_route(
                project_id,
                self.agents_workspace_project_id.as_deref(),
                self.browser_tabs_project_id.as_deref(),
            );
            if let TargetedBrowserOpenRoute::Background {
                realign_browser_model,
            } = route
            {
                if realign_browser_model {
                    self.swap_browser_tabs_to_project_id(
                        self.agents_workspace_project_id.clone(),
                        cx,
                    );
                }
                if let Some(url) = normalize_address(&message.url) {
                    self.open_browser_url_in_parked_project(
                        project_id,
                        url,
                        message.reuse,
                        remote_machine_id,
                        cx,
                    );
                }
                return;
            }
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
        /*
        CDXC:Browser 2026-10-10 DECISION:
        User: "when i click on this button to open in a new github tab or on the linear button to open on linear tab in the embedded browser we're opening a useless empty new browser tab pls fix", then "pls fix this for all similar cases in the app". Every project's Browser carries an empty "New Tab" placeholder even if it never opened the Browser, and appending here left it beside the page once the Browser came on screen. Every open through this door therefore loads the link into that sole placeholder, as page popups already do (`open_loaded_popup_tab`).
        */
        let created_tab_id = self.browser_tabs.open_loaded_popup_tab(
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
        self.begin_sidebar_focus_border_handoff(cx);
        self.local_workspace_latest_focus_key = Some(key.clone());
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
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
        // A mapped tab with nothing live behind it is re-attached where it is, so it is brought to
        // the focused pane first, as the focus-existing path above does (session_pane_placement.rs).
        if !force_requested_pane_placement
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
                        this.gx_store_take_created_session(&key, cx);
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
                        this.gx_store_take_created_session(&key, cx);
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

    /// CDXC:AppModal 2026-10-24 WHY:
    /// The restore pass (`attach_surfaced_local_workspace_terminals`) runs on every sidebar snapshot and re-asked gxserver to attach the shown tab of a session whose account was removed; its refusal came back as the sidebar focus it had turned into and raised "Session attach unavailable" again each time, a stream of the same toast. gxserver's `accountUnavailable` refusal cannot change until the session's account does, so the pass waits for that: the account the row names now differs from the one refused, or the user clicks, forks or wakes the session. Other refusals (a restart, a slow wake) still retry on their own.
    pub(crate) fn local_workspace_attach_refused_still(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) -> bool {
        let Some(refused_account) = self.local_workspace_attach_refused.get(key) else {
            return false;
        };
        let account_now = self
            .gx_store_local_server_session(&key.project_id, &key.session_id)
            .map(|session| session.account_id);
        if !account_refusal_still_applies(refused_account, account_now.as_ref()) {
            self.local_workspace_attach_refused.remove(key);
            return false;
        }
        true
    }

    /// gxserver's refusal names the session, the agent and the account; the toast adds Open
    /// Accounts on that agent's page (CDXC:AgentProviders 2026-10-24 in server/src/accounts/launch.rs).
    fn show_local_workspace_account_refusal_toast(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let provider = self
            .gx_store_local_server_session(&key.project_id, &key.session_id)
            .and_then(|session| session.agent_icon.or(session.agent_id))
            .filter(|agent| agent == "claude" || agent == "codex");
        self.app_toast_id_counter = self.app_toast_id_counter.wrapping_add(1);
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: format!("gpui-app-toast-{}", self.app_toast_id_counter),
                level: GpuiAppToastLevel::Warning,
                title: "Account unavailable".to_string(),
                description: Some(message.to_string()),
                copy_text: None,
                action: Some(GpuiAppToastAction::OpenAccounts { provider }),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
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
        // A restore pass runs on every sidebar snapshot; it does not ask again for a session whose
        // account gxserver refused, while that account is unchanged. A click, fork or wake asks.
        if origin == GpuiLocalWorkspaceAttachOrigin::SurfacedRestore {
            if self.local_workspace_attach_refused_still(&key) {
                return;
            }
        } else {
            self.local_workspace_attach_refused.remove(&key);
        }
        if !self.local_workspace_attach_pending.insert(key.clone()) {
            return;
        }

        let focused_at_request = self.gx_store_focused_session();
        let open_chat_early = self
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
                    gpui_prepare_local_workspace_attach_terminal_plan_with_code(
                        &prepare_key,
                        None,
                        attach_intent,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.local_workspace_attach_pending.remove(&key);
                match &result {
                    Err(error) if error.code.as_deref() == Some("accountUnavailable") => {
                        let account_id = this
                            .gx_store_local_server_session(&key.project_id, &key.session_id)
                            .and_then(|session| session.account_id);
                        this.local_workspace_attach_refused
                            .insert(key.clone(), account_id);
                    }
                    Ok(_) => {
                        this.local_workspace_attach_refused.remove(&key);
                    }
                    Err(_) => {}
                }
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
                            let _ = this.open_gpui_local_workspace_terminal(
                                key,
                                plan,
                                requested_pane_id,
                                force_requested_pane_placement,
                                cx,
                            );
                        }
                    },
                    Err(error) => {
                        if matches!(
                            completion_origin,
                            GpuiLocalWorkspaceAttachOrigin::SurfacedRestore
                                | GpuiLocalWorkspaceAttachOrigin::BackgroundSelect
                        ) {
                            return;
                        }
                        this.cancel_sidebar_focus_border_handoff();
                        if error.code.as_deref() == Some("accountUnavailable") {
                            this.show_local_workspace_account_refusal_toast(&key, &error.message, cx);
                            return;
                        }
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Session attach unavailable",
                            error.message.as_str(),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }
}

/// Whether an attach refused on `refused_account` would be refused again: the session's row
/// (`account_now`, `None` when the row is gone) still names that account.
pub(crate) fn account_refusal_still_applies(
    refused_account: &Option<String>,
    account_now: Option<&Option<String>>,
) -> bool {
    account_now == Some(refused_account)
}

#[cfg(test)]
mod account_refusal_tests {
    use super::account_refusal_still_applies;

    #[test]
    fn a_refused_attach_waits_until_the_account_changes() {
        let refused = Some("acct-removed".to_string());
        // Every restore pass while the row still names the removed account stays quiet.
        for _ in 0..5 {
            assert!(account_refusal_still_applies(&refused, Some(&refused)));
        }
        // The user picked another account: the next pass attaches again.
        assert!(!account_refusal_still_applies(
            &refused,
            Some(&Some("acct-new".to_string()))
        ));
        // The row lost its account or is gone: try again.
        assert!(!account_refusal_still_applies(&refused, Some(&None)));
        assert!(!account_refusal_still_applies(&refused, None));
    }
}
