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
    ActiveGroup, BrowserPaneOpen, DEFAULT_BROWSER_LAUNCH_URL, Event, Intent, MachineId,
    plan_browser_pane_open,
};
use gpui::Window;
use serde_json::json;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_active_project_id_from_snapshot;
use crate::app::model::{GpuiBrowserRendererOpenReuse, GpuiSidebarOpenBrowserUrlMessage};

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
                    Some(ActiveGroup::Chats(_)) | None => None,
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
                self.gx_store.create.pending_browser_open = Some(PendingBrowserOpen {
                    project_id: project_id.clone(),
                    message,
                    deadline: Instant::now() + PENDING_BROWSER_OPEN_DEADLINE,
                });
            }
        }
    }

    /// Opens the New Browser Tab that was waiting for this project to become the active one.
    pub(crate) fn gx_store_land_pending_browser_open(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(pending) = self.gx_store.create.pending_browser_open.take() else {
            return;
        };
        if Instant::now() > pending.deadline {
            return;
        }
        let active_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref());
        if active_project_id != Some(pending.project_id.as_str()) {
            // Another project landed first: the tab is still owed to this one.
            self.gx_store.create.pending_browser_open = Some(pending);
            return;
        }
        self.open_browser_url_from_renderer_command(pending.message, window, cx);
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
