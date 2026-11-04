//! The two browser opens the sidebar asks for, a cmd+clicked terminal link, and the More menu's
//! Search by Prompt: four paths that only ever left Rust for the old runtime and came straight back.
//!
//! CDXC:Browser 2026-09-25 WHY:
//! A project header's New Browser Tab made its group active, published, and THEN posted
//! `postOpenBrowserUrl`, and the bridge dispatcher flushed the project switch that publish queued
//! before it opened the tab: the tab lands in the new project's browser AFTER that project's
//! remembered view was restored, so the user ends in Browser. The group activation is still the
//! runtime's (`focusGroup`, whose focus authority moves later in the port), so a New Browser Tab
//! on ANOTHER project is held here until that project's context has been applied
//! (`gx_store_land_pending_browser_open`, called from `apply_changed_active_project_snapshot`);
//! one on the project already active opens at once, as it did. The hold has a deadline so a switch
//! the runtime never made cannot open a tab minutes later.
//!
//! SEE-ALSO: packages/gx-core/src/session_create/browser.rs,
//! apps/desktop/src/app/workspace_events.rs (`open_browser_url_from_renderer_command`).

use std::time::Duration;
use web_time::Instant;

use ghostex_gx_core::{
    ActiveGroup, BrowserPaneOpen, DEFAULT_BROWSER_LAUNCH_URL, Event, Intent, MachineId, ProjectKey,
    SessionKey, plan_browser_pane_open,
};
use gpui::Window;
use serde_json::json;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_active_project_id_from_snapshot;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};
#[cfg(not(target_arch = "wasm32"))]
use crate::app::workspace_windows::ProjectWindowRoute;

/// How long a New Browser Tab waits for the project switch it follows.
const PENDING_BROWSER_OPEN_DEADLINE: Duration = Duration::from_secs(5);

pub(crate) struct PendingBrowserOpen {
    project_id: String,
    message: GpuiSidebarOpenBrowserUrlMessage,
    deadline: Instant,
}

impl GhostexGpuiApp {
    /// `openBrowserPaneInGroup(groupId = this.activeGroupId)`.
    pub(super) fn gx_store_open_browser_pane_in_group(
        &mut self,
        group_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_group = self
            .gx_store
            .core
            .focus()
            .active_group
            .as_ref()
            .map(ActiveGroup::to_sidebar_group_id);
        let Some(plan) = plan_browser_pane_open(group_id.or(active_group.as_deref())) else {
            return;
        };
        self.gx_store.create.counters.browser_pane_opens += 1;
        match &plan {
            BrowserPaneOpen::Remote { .. } => {
                // The runtime made the remote group its active group and published it; the store
                // owns the highlight, so it moves there. The ports page is the remote branch's
                // whole open (CDXC:RemoteMachines 2026-07-30).
                let intent = match ActiveGroup::parse_sidebar_group_id(plan.group_id()) {
                    Some(ActiveGroup::Subgroup { project, group_id }) => {
                        Some(Intent::FocusSubgroup { project, group_id })
                    }
                    Some(ActiveGroup::Project(project)) => Some(Intent::FocusProject { project }),
                    None => None,
                };
                if let Some(intent) = intent {
                    let output = self
                        .gx_store
                        .core
                        .handle(Event::Intent(intent), super::super::host::now_ms());
                    if !output.changes.is_empty() {
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                        self.gx_store_update_sidebar_list(cx);
                    }
                }
                if let Some(payload) = plan.remote_native_payload() {
                    self.receive_sidebar_native_project_path_action_payload(
                        &payload.to_string(),
                        cx,
                    );
                }
            }
            BrowserPaneOpen::Local {
                project_id,
                group_id,
            } => {
                // `if (!this.presentation) return;`: this computer's rows have not loaded.
                if self
                    .gx_store
                    .core
                    .presentation()
                    .loaded(&MachineId::Local)
                    .is_none()
                {
                    return;
                }
                self.dispatch_native_sidebar_command(
                    json!({ "type": "focusGroup", "groupId": group_id }),
                    cx,
                );
                let message = GpuiSidebarOpenBrowserUrlMessage {
                    url: DEFAULT_BROWSER_LAUNCH_URL.to_string(),
                    reuse: GpuiBrowserRendererOpenReuse::None,
                    from_quick_header: false,
                    project_id: Some(project_id.clone()),
                };
                let active_project_id = gpui_active_project_id_from_snapshot(
                    self.latest_sidebar_project_snapshot.as_ref(),
                );
                if active_project_id == Some(project_id.as_str()) {
                    self.gx_store_open_browser_url(message, cx);
                    return;
                }
                self.gx_store_hold_browser_open(project_id.clone(), message);
            }
        }
    }

    /// CDXC:SessionLinks 2026-10-25 WHY:
    /// A link (a PR, an issue, a Linear ticket) of a session in another project, from its card's chips, its links list or its Work chips, switches to that project and opens in its Browser, so the page is in the right project and uses the right workspace's Browser profile; it no longer opens in the project on screen. Only the project is focused, on this computer or a remote one: no session is chosen, attached or started (a chip keeps the click from the row); the highlight then moves to the session the project's workspace shows (`gx_store_follow_project_only_focus`). The project comes back on its remembered view and the page opens once it is there, as a header's New Browser Tab does.
    pub(crate) fn gx_store_open_browser_url_in_session_project(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        session_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = session_id else {
            self.gx_store_open_browser_url(message, cx);
            return;
        };
        // A link that names a session it cannot be read from opens nowhere, not on screen.
        let Some(session) = SessionKey::parse_sidebar_session_id(session_id) else {
            return;
        };
        self.gx_store_open_browser_url_in_project(session.project_key(), message, cx);
    }

    /// `message` opens in `project`'s Browser, in the window that shows the project's workspace: at
    /// once when the project is the active one, else once the switch to it has landed.
    #[cfg(not(target_arch = "wasm32"))]
    fn gx_store_open_browser_url_in_project(
        &mut self,
        project: ProjectKey,
        mut message: GpuiSidebarOpenBrowserUrlMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let route = if project.machine.is_local() {
            self.route_local_project_to_its_window(&project.project_id, cx)
        } else {
            ProjectWindowRoute::Here
        };
        if let ProjectWindowRoute::Other(app) = route {
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| {
                    app.gx_store_open_browser_url_in_project(project, message, cx)
                });
            });
            return;
        }
        let project_id = project.to_workspace_project_id();
        message.project_id = Some(project_id.clone());
        let active_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref());
        if active_project_id == Some(project_id.as_str()) {
            self.gx_store_open_browser_url(message, cx);
            return;
        }
        self.gx_store_hold_browser_open(project_id, message);
        self.gx_store_focus_intent(Intent::FocusProject { project }, cx);
        self.gx_store_publish_workspace_focus(cx);
        if matches!(route, ProjectWindowRoute::Switched) {
            self.settle_routed_window_workspace(cx);
        }
    }

    /// The web page has no per-project Browser: a link opens in a tab of the page's own browser.
    #[cfg(target_arch = "wasm32")]
    fn gx_store_open_browser_url_in_project(
        &mut self,
        _project: ProjectKey,
        message: GpuiSidebarOpenBrowserUrlMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_open_browser_url(message, cx);
    }

    /// Holds `message` until `project_id` is the active project (`gx_store_land_pending_browser_open`),
    /// after the opens already waiting: a second link clicked before the first one's switch lands
    /// does not replace it.
    fn gx_store_hold_browser_open(
        &mut self,
        project_id: String,
        message: GpuiSidebarOpenBrowserUrlMessage,
    ) {
        let now = Instant::now();
        let pending = &mut self.gx_store.create.pending_browser_opens;
        pending.retain(|open| now <= open.deadline);
        pending.push(PendingBrowserOpen {
            project_id,
            message,
            deadline: now + PENDING_BROWSER_OPEN_DEADLINE,
        });
    }

    /// Opens, oldest first, the browser opens that were waiting for this project to become the
    /// active one. One owed to another project keeps waiting until its deadline.
    pub(crate) fn gx_store_land_pending_browser_open(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store.create.pending_browser_opens.is_empty() {
            return;
        }
        let now = Instant::now();
        let active_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref());
        let (landing, waiting): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.gx_store.create.pending_browser_opens)
                .into_iter()
                .filter(|open| now <= open.deadline)
                .partition(|open| active_project_id == Some(open.project_id.as_str()));
        self.gx_store.create.pending_browser_opens = waiting;
        for open in landing {
            self.open_browser_url_from_renderer_command(open.message, window, cx);
        }
    }

    /// `openQuickBrowserTab`: the projectless Quick header's browser launch.
    pub(super) fn gx_store_open_quick_browser_tab(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.create.counters.quick_browser_opens += 1;
        self.gx_store_open_browser_url(
            GpuiSidebarOpenBrowserUrlMessage {
                url: DEFAULT_BROWSER_LAUNCH_URL.to_string(),
                reuse: GpuiBrowserRendererOpenReuse::None,
                from_quick_header: true,
                project_id: None,
            },
            cx,
        );
    }

    /// A web link cmd+clicked in a GPUI engine terminal, opened in the embedded browser with
    /// similar-tab reuse. It used to be posted to the runtime's bridge by a script only to come
    /// back as `postOpenBrowserUrl` (ledger R047).
    pub(crate) fn gx_store_open_terminal_link_in_browser(
        &mut self,
        url: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_open_browser_url(
            GpuiSidebarOpenBrowserUrlMessage {
                url: url.to_string(),
                reuse: GpuiBrowserRendererOpenReuse::Similar,
                from_quick_header: false,
                project_id: None,
            },
            cx,
        );
    }

    /// The More menu's Search by Prompt: the Find Prompts shortcut, which is all the runtime did.
    ///
    /// CDXC:PromptSearch 2026-08-20:
    /// Search by Text used to create a terminal and type `gx f` into it. The same search is now a
    /// first-class modal, so this runs the native Find action and both entry points (the More
    /// menu row and the command palette) land on one implementation instead of two.
    pub(super) fn gx_store_open_find_prompts(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.create.counters.find_prompts += 1;
        self.defer_in_main_window(cx, |this, window, cx| {
            this.handle_gpui_app_modal_sidebar_command(
                json!({
                    "message": { "actionId": "openFindPrompts", "type": "runGhostexHotkeyAction" },
                }),
                window,
                cx,
            );
        });
    }

    /// The browser door with the window it needs, landing a queued project switch first exactly as
    /// the bridge dispatcher does for `postOpenBrowserUrl`.
    fn gx_store_open_browser_url(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        self.defer_in_main_window(cx, move |this, window, cx| {
            if !this.project_switch_pending_requests.is_empty() {
                this.flush_coalesced_project_switch_requests(window, cx);
            }
            this.open_browser_url_from_renderer_command(message, window, cx);
        });
    }
}
