// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: command terminal runtime clipboard drain and ghostty surface focus handoff, remaining layout-bounds recorders, and CEF init/visibility sync.

use std::collections::HashSet;
use std::time::Duration;

use gpui::Bounds;
use gpui::Pixels;

use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "macos")]
    pub(crate) fn drain_command_terminal_runtime_clipboard_requests(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUITerminalClipboard 2026-06-23-19:07:
        Command runtime clipboard handoff mirrors Agents ownership rules: only exact command mount keys from the current surface map can authorize app-thread standard clipboard access for queued owner-local Ghostty requests. Focus is never requester identity, reads stay explicit-string-only, writes forward only runtime-provided text, and stale/missing surfaces naturally keep their queued operations unreachable.

        CDXC:GPUITerminalImagePaste 2026-06-27-10:28:
        Command-pane runtime clipboard reads use the same runtime previewable-image setting and normalization as Agents terminals, keeping image Markdown parity per mounted owner without focused-surface requester fallback.
        */
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = terminal_runtime_clipboard_authorized_mounted_slot_ids(
            self.command_terminal_ghostty_surfaces
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &self.command_terminal_ghostty_surfaces,
        );

        let mut runtime_osc_state_changed = false;
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get(&slot_id) else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if !action_events.is_empty() {
                let runtime_session_id = surface.runtime_session_id();
                terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                    let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } =
                        event
                    else {
                        return None;
                    };
                    Some((runtime_session_id, url.clone()))
                }));
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                    )
                }) {
                    self.terminal_search_focus_pending = Some(surface.runtime_session_id());
                }
                runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                    &mut self.command_terminal_runtime_osc_states,
                    runtime_session_id,
                    action_events,
                );
            }
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .command_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        if !self.command_terminal_runtime_osc_states.is_empty() {
            let live_runtime_session_ids = self
                .command_terminal_ghostty_surfaces
                .values()
                .map(|surface| surface.runtime_session_id())
                .chain(
                    self.command_gpui_engine_terminals
                        .values()
                        .map(|record| record.runtime_session_id),
                )
                .collect::<HashSet<_>>();
            self.command_terminal_runtime_osc_states
                .retain(|runtime_session_id, _| {
                    live_runtime_session_ids.contains(runtime_session_id)
                });
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surface_focus(&mut self) {
        self.sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(false);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        let mounted_slot_ids = self
            .command_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focus_states = command_terminal_surface_focus_states_for_slots(
            self.shell_focus,
            &self.command_pane,
            &mounted_slot_ids,
        );
        let app_has_focused_terminal_surface =
            focus_states.iter().any(|(_slot_id, focused)| *focused);
        let focused_mounted_slot_id = focus_states
            .iter()
            .find_map(|(slot_id, focused)| (*focused).then_some(*slot_id));

        for (slot_id, focused) in focus_states {
            if let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) {
                surface.set_focus(focused);
            }
        }

        if let Some(app) = self.command_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_mounted_slot_id.and_then(|slot_id| {
            if !self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.command_terminal_host_native_views.get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.command_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.command_terminal_appkit_focused_host =
                next_appkit_focus_identity.and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.command_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.command_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }

    pub(crate) fn record_command_group_layout_bounds(
        &mut self,
        group_id: CommandPaneGroupId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.command_group_layout_bounds.insert(group_id, bounds);
        }
    }

    pub(crate) fn record_command_pane_layout_bounds(&mut self, child_bounds: &[Bounds<Pixels>]) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.command_pane_layout_bounds = Some(bounds);
        }
    }

    pub(crate) fn record_browser_leaf_layout_bounds(
        &mut self,
        pane_id: BrowserPaneId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.browser_leaf_layout_bounds.insert(pane_id, bounds);
        }
    }

    pub(crate) fn record_project_editor_surface_layout_bounds(
        &mut self,
        mode: TitlebarMode,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.project_editor_surface_layout_bounds =
                Some(ProjectEditorFocusBounds { mode, bounds });
        }
    }

    pub(crate) fn record_project_editor_companion_layout_bounds(
        &mut self,
        mode: TitlebarMode,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.project_editor_companion_layout_bounds =
                Some(ProjectEditorFocusBounds { mode, bounds });
        }
    }

    pub(crate) fn initialize_cef(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar.is_some() {
            return;
        }

        cef::initialize(cx).expect("failed to initialize CEF");
        if !cef::context_initialized() {
            if self.cef_context_initialization_waiting {
                return;
            }
            self.cef_context_initialization_waiting = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(25))
                        .await;
                    if cef::context_initialized() {
                        let _ = this.update(cx, |this, cx| {
                            this.cef_context_initialization_waiting = false;
                            this.initialize_cef(cx);
                        });
                        break;
                    }
                }
            })
            .detach();
            return;
        }
        let parent_ns_view = self.parent_ns_view;
        let sidebar_url = self.sidebar_url.clone();
        let sidebar_bridge_event_handler = self.sidebar_bridge_event_handler(cx);
        let app_modal_host_bridge_event_handler = self.app_modal_host_bridge_event_handler(cx);
        let sidebar_runtime_settings = self.sidebar_runtime_settings_snapshot.clone();
        let sidebar_gxserver_bootstrap = self.sidebar_gxserver_bootstrap.clone();
        let sidebar_visible = gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        match CefSurface::try_new(
            "gpui-sidebar".to_string(),
            parent_ns_view,
            sidebar_url,
            "gpui-sidebar".to_string(),
            sidebar_cef_prepaint_background_color(),
            false,
            titlebar_background(),
            None,
            sidebar_visible,
            None,
            None,
            None,
            Some(sidebar_runtime_settings),
            sidebar_gxserver_bootstrap,
            Some(sidebar_bridge_event_handler),
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::Sidebar),
            Some(app_modal_host_bridge_event_handler),
            None,
            cx,
        ) {
            Ok(sidebar) => {
                /*
                CDXC:GPUISidebarPointerTracking 2026-08-02:
                Hand the sidebar's CEF child view to the AppKit sendEvent
                observer so pointer crossings of its frame, and mouse-downs
                outside it, become the page's hover-suppression and
                context-menu-dismissal signals.
                */
                #[cfg(target_os = "macos")]
                if let Some(native_view) =
                    sidebar.read(cx).native_view_for_sidebar_pointer_tracking()
                {
                    cef::set_sidebar_pointer_tracking_view(native_view);
                }
                self.sidebar = Some(sidebar);
            }
            Err(error) => {
                // The sidebar profile uses the pre-initialized global app-ui
                // context, so a creation failure here is unexpected. Retry
                // once after CEF has had time to settle; on a second failure
                // keep the app alive without the sidebar instead of the
                // previous process abort
                // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({
                        "surface": "sidebar",
                        "retryScheduled": !self.cef_sidebar_creation_retried,
                        "error": error,
                    }),
                );
                if !self.cef_sidebar_creation_retried {
                    self.cef_sidebar_creation_retried = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(750))
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.sidebar.is_none() {
                                this.initialize_cef(cx);
                            }
                        });
                    })
                    .detach();
                }
                return;
            }
        }
        self.ensure_active_browser_surface(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.update_active_mode_cef_child_visibility(cx);
        // First-run onboarding may open the CEF app-modal host. Start it only
        // after the required runtime and initial sidebar surface are ready;
        // macOS release first launch can spend time in the native component
        // window before CEF is available.
        self.start_gpui_first_run_onboarding(cx);
        cx.notify();
    }

    pub(crate) fn update_active_mode_cef_child_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Browser tab CEF surfaces may show only in Browser mode. Kanban, Automate, and Manage use the separate project-workarea CEF surface map and visibility gate, so Browser CEF child views must still hide under Source, Kanban, Automate, Manage, and Agents instead of sitting underneath non-browser editor panes.

        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Per-tab Browser CEF entities must never visually overlap each other or other project-editor modes. Visibility is derived from Browser mode plus rendered Browser leaves' active loaded tab ids, so inactive tab views and all Browser views outside Browser mode are hidden at the native child-view boundary.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-07:30:
        Browser project-editor sleep is shell-level for this slice: sleeping hides all Browser CEF child views but does not delete Browser tab metadata or CEF surface entities. Waking Browser mode uses the existing selected-tab sync path plus rendered-leaf visibility so existing active loaded surfaces can show again.

        CDXC:GPUIBrowserDragDrop 2026-06-22-07:41:
        Browser tab drags must hide native CEF child surfaces for the whole drag, not only while hovering a valid drop target. GPUI keeps the drag feedback in normal tab-strip layout, while eligible rendered Browser leaf surfaces are restored through this same visibility gate when the drag drops or is canceled.

        CDXC:GPUICommandPaneDrag 2026-06-22-08:01:
        Command-pane tab drags can run while Browser mode remains active underneath the command panel, so any active command tab drag must hide every Browser CEF child surface through the same visibility gate used by Browser tab drags. Drop and root mouse-up cancellation restore eligible rendered Browser leaf surfaces by clearing the command drag flag, without overlays, hit-test rerouting, or changing command-only drag/drop semantics.

        CDXC:GPUIBrowserSplits 2026-06-22-09:55:
        Browser split parity requires all rendered Browser leaves to show their own active loaded CEF surface when that surface already exists. Visibility is a set of BrowserTabIds derived from rendered leaves; this keeps Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hiding every surface while avoiding CEF creation for restored or inactive tabs that have not been materialized yet.

        CDXC:GPUIBrowserLifecycle 2026-06-23-11:32:
        Runtime visibility now routes through BrowserRuntimeSurfacePolicy so the hide/hold/restored-placeholder decision is centralized and reviewable. The sync loop only toggles existing tab-owned CEF entities; it never creates restored loaded tab surfaces or tears down hidden ones.

        CDXC:GPUIBrowserLifecycle 2026-06-23-14:30:
        Keep this loop limited to set_visible on existing Browser surfaces. Browser sleep, non-Browser mode, Browser tab drags, and command-tab drags hide-and-hold; deeper CEF suspend/teardown and restored-surface recreation remain deferred decisions, not fallback behavior in the visibility path.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        The same active-mode visibility pass also hides or shows already-owned Source/Kanban/Automate/Manage runtime CEF child views. It does not create project-workarea surfaces, issue URLs, use temporary pages, use WKWebView/WebKit, or allow hidden CEF views to sit under placeholders.
        */
        let visible_tab_ids = browser_runtime_visible_surface_tab_ids(
            self.browser_runtime_lifecycle_input(),
            &self.browser_tabs,
            self.browser_surfaces.keys().copied().collect::<Vec<_>>(),
        );

        for (tab_id, surface) in &self.browser_surfaces {
            let surface_visible = visible_tab_ids.contains(tab_id);
            surface.update(cx, |surface, _| {
                surface.set_visible(surface_visible);
            });
        }
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        /*
        CDXC:GPUISessionChatSurface 2026-08-19:
        Session Chat is a CEF child view gated on exactly the same inputs as
        the Browser and project-workarea surfaces: active mode, mode
        wakefulness, companion visibility, and the tab-drag flags. Every mode
        switch, drag, sleep, and companion mutation already re-runs this pass,
        so the chat gate belongs here rather than being re-added by hand at
        each call site. Sites that set `active_mode` and then only synced
        Browser surfaces (terminal link opens through
        `open_browser_url_from_renderer_command`, for example) used to leave
        the Agents chat child painted over the new workarea.
        */
        self.reconcile_agents_pane_surfaces(cx);
    }
}
