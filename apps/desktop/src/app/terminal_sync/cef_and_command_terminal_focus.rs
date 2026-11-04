// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: remaining layout-bounds recorders, and CEF init/visibility sync.

use std::time::Duration;

use gpui::Bounds;
use gpui::Pixels;

use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
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

    /// Starts CEF (`request_cef_runtime` decides when) and, once its context is up, creates the
    /// web views that were waiting for it. A runtime that cannot start is reported on those views
    /// (CDXC:CefRuntime 2026-09-28 in app/helpers/web_runtime.rs).
    pub(crate) fn initialize_cef(&mut self, cx: &mut gpui::Context<Self>) {
        if let Err(error) = cef::initialize(cx) {
            self.fail_web_runtime_start(format!("The web runtime could not start: {error:#}"), cx);
            return;
        }
        crate::app::helpers::web_runtime::set_web_runtime_state(
            crate::app::helpers::web_runtime::WebRuntimeState::Running,
        );
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
                    // macOS starts CEF on a later run-loop turn (cef/macos.rs `start_cef`).
                    if let Some(error) = cef::take_runtime_start_error() {
                        let _ = this.update(cx, |this, cx| {
                            this.cef_context_initialization_waiting = false;
                            this.fail_web_runtime_start(
                                format!("The web runtime could not start: {error}"),
                                cx,
                            );
                        });
                        break;
                    }
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
        // A Browser that is still asleep from launch gets its page on the click that wakes it (CDXC:Browser 2026-09-19).
        if self
            .project_editor_shell
            .is_mode_awake(TitlebarMode::Browser)
        {
            self.ensure_active_browser_surface(cx);
        }
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.start_waiting_parked_browser_pages(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.retry_titlebar_extension_popup_after_cef_ready(cx);
        self.open_gpui_app_modal_deferred_for_cef(cx);
        self.refresh_web_runtime_views(cx);
        cx.notify();
    }

    pub(crate) fn update_active_mode_cef_child_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:CodeEditor 2026-06-22-05:49:
        Browser tab CEF surfaces may show only in Browser mode. Kanban, Automate, and Manage use the separate project-workarea CEF surface map and visibility gate, so Browser CEF child views must still hide under Source, Kanban, Automate, Manage, and Agents instead of sitting underneath non-browser editor panes.

        CDXC:Browser 2026-06-22-06:59:
        Per-tab Browser CEF entities must never visually overlap each other or other project-editor modes. Visibility is derived from Browser mode plus rendered Browser leaves' active loaded tab ids, so inactive tab views and all Browser views outside Browser mode are hidden at the native child-view boundary.

        CDXC:CodeEditor 2026-06-22-07:30:
        Browser project-editor sleep is shell-level for this slice: sleeping hides all Browser CEF child views but does not delete Browser tab metadata or CEF surface entities. Waking Browser mode uses the existing selected-tab sync path plus rendered-leaf visibility so existing active loaded surfaces can show again.

        CDXC:Browser 2026-06-22-07:41:
        Browser tab drags must hide native CEF child surfaces for the whole drag, not only while hovering a valid drop target. GPUI keeps the drag feedback in normal tab-strip layout, while eligible rendered Browser leaf surfaces are restored through this same visibility gate when the drag drops or is canceled.

        CDXC:CommandPane 2026-06-22-08:01:
        Command-pane tab drags can run while Browser mode remains active underneath the command panel, so any active command tab drag must hide every Browser CEF child surface through the same visibility gate used by Browser tab drags. Drop and root mouse-up cancellation restore eligible rendered Browser leaf surfaces by clearing the command drag flag, without overlays, hit-test rerouting, or changing command-only drag/drop semantics.

        CDXC:Browser 2026-06-22-09:55:
        Browser split parity requires all rendered Browser leaves to show their own active loaded CEF surface when that surface already exists. Visibility is a set of BrowserTabIds derived from rendered leaves; this keeps Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hiding every surface while avoiding CEF creation for restored or inactive tabs that have not been materialized yet.

        CDXC:Browser 2026-06-23-11:32:
        Runtime visibility now routes through BrowserRuntimeSurfacePolicy so the hide/hold/restored-placeholder decision is centralized and reviewable. The sync loop only toggles existing tab-owned CEF entities; it never creates restored loaded tab surfaces or tears down hidden ones.

        CDXC:Browser 2026-06-23-14:30:
        Keep this loop limited to set_visible on existing Browser surfaces. Browser sleep, non-Browser mode, Browser tab drags, and command-tab drags hide-and-hold; deeper CEF suspend/teardown and restored-surface recreation remain deferred decisions, not fallback behavior in the visibility path.

        CDXC:Workarea 2026-06-24-10:12:
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
        CDXC:SessionChat 2026-08-19:
        Session Chat is a CEF child view reconciled at the same boundary as the Browser and
        project-workarea surfaces, so every mode switch, drag and sleep re-runs one pass rather than
        each call site re-adding the chat gate by hand. Sites that set `active_mode` and then only
        synced Browser surfaces (terminal link opens through
        `open_browser_url_from_renderer_command`, for example) used to leave the Agents chat child
        painted over the new workarea.

        CDXC:SessionChat 2026-09-20 WHY:
        Its own gate no longer reads the active mode at all: the Agents column is on screen in every
        view, so a chat page is visible exactly while its pane is rendered.
        */
        self.reconcile_agents_pane_surfaces(cx);
    }
}
