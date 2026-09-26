//! A session's own controls (the terminal action bar, the tab menu, the hotkeys, the chat
//! composer, the Resources panel) acting on it by gxserver identity: Close, Sleep, Fork, Full
//! Reload, Note, Switch Account, and the two sleep sweeps. Each one runs the store's own sidebar
//! action for that session, so a terminal's Fork and its sidebar row's Fork are one implementation.
//!
//! CDXC:Resources 2026-09-04 WHY:
//! The titlebar Resources panel lists the project's live sessions even when no pane is mounted for them, so its moon, Sleep Project and Close cannot go through Rust's pane-owned paths for those rows. They arrive here by gxserver identity and take the same path a sidebar row's Sleep and Close use.
//!
//! CDXC:Resources 2026-09-25 WHY:
//! Resources "Quit" on a session with no open pane asked the old runtime for the action `close`, which the runtime never accepted (only `closeSession`), so those sessions were never closed. Both names are the same Close here.
//!
//! SEE-ALSO: apps/desktop/src/app/terminal_sync/workspace_dispatch_and_focus.rs,
//! apps/desktop/src/app/os_integration/gxserver_stop_and_workspace_sleep.rs,
//! docs/2026-09-25/app-runtime-port/LEDGER.md rows C034, R049 to R058, M002.

use ghostex_gx_core::{SessionKey, running_local_session_ids, titlebar_sleep_inactive_ids};
use serde_json::{Value, json};

use super::session_calls::switch_session_agent;
use crate::GhostexGpuiApp;
use crate::app::model::GpuiLocalWorkspaceSessionKey;

/// The store's sidebar command envelope, as the renderer posts it.
fn command(message: Value) -> Value {
    json!({ "type": "command", "message": message })
}

impl GhostexGpuiApp {
    /// Performs a session control's action. Returns whether it was performed; `false` when the
    /// store cannot answer yet (the sidebar list is still loading) or the action is not one of
    /// these, which the caller hands on unchanged.
    pub(crate) fn gx_store_run_workspace_runtime_action(
        &mut self,
        action: &str,
        key: &GpuiLocalWorkspaceSessionKey,
        agent_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<bool> {
        let session_id =
            SessionKey::local(&key.project_id, &key.session_id).to_sidebar_session_id();
        let performed = match action {
            "closeSession" | "close" => self.gx_store_run_sidebar_close(
                &command(json!({ "type": "closeSession", "sessionId": session_id })),
                cx,
            ),
            "sleepSession" => self.gx_store_run_sidebar_lifecycle(
                &command(json!({
                    "type": "setSessionSleeping",
                    "sessionId": session_id,
                    "sleeping": true,
                })),
                cx,
            ),
            "forkSession" => {
                self.gx_store_run_workspace_session_fork(&key.project_id, &key.session_id, cx)
            }
            "fullReloadSession" => self.gx_store_run_sidebar_reload(
                &command(json!({ "type": "fullReloadSession", "sessionId": session_id })),
                cx,
            ),
            "openSessionNote" => self.gx_store_open_workspace_session_note(key, &session_id, cx),
            "switchSessionAgent" => {
                let Some(agent_id) = agent_id.filter(|agent| !agent.trim().is_empty()) else {
                    return Some(false);
                };
                self.gx_store_switch_workspace_session_agent(key, &session_id, agent_id, cx);
                true
            }
            // Export and Handoff open the export dialog, which the caller routes to
            // gx_store/git/export_transcript.rs.
            _ => return None,
        };
        Some(performed)
    }

    /// The note editor for a session's own Note control. Same dialog and seed as a sidebar row's
    /// Note; the title is the session's primary title, else its title, else its terminal title.
    fn gx_store_open_workspace_session_note(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session) = self.gx_store_local_server_session(&key.project_id, &key.session_id)
        else {
            return false;
        };
        let non_empty = |value: Option<&str>| {
            value
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let session_title = non_empty(session.primary_title.as_deref())
            .or_else(|| non_empty(Some(session.title.as_str())))
            .or_else(|| non_empty(session.terminal_title.as_deref()));
        // The note editor replaces whatever modal is open instead of stacking behind it.
        self.close_app_modal_from_bridge(cx);
        let mut open = json!({
            "initialNote": session.session_note.clone().unwrap_or_default(),
            "modal": "sessionNote",
            "sessionId": session_id,
            "type": "open",
        });
        if let Some(title) = session_title {
            open["sessionTitle"] = Value::String(title);
        }
        self.open_app_modal_from_bridge(open, cx);
        true
    }

    /// CDXC:AgentProviders 2026-09-03:
    /// Resume the same conversation under another same-family agent configuration (another account). The owning daemon rewrites the row's launch identity; the provider cycle that follows is Full Reload itself, so the wake resumes through the ordinary restore path with the new agent's command. A refused switch (incompatible agent, draft, old daemon) leaves the row untouched, so nothing is reloaded and the daemon's own sentence is what the user sees.
    fn gx_store_switch_workspace_session_agent(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        session_id: &str,
        agent_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let project_id = key.project_id.clone();
        let raw_session_id = key.session_id.clone();
        let agent_id = agent_id.to_string();
        let session_id = session_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = switch_session_agent(project_id, raw_session_id, agent_id).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => {
                    this.gx_store_run_sidebar_reload(
                        &command(json!({ "type": "fullReloadSession", "sessionId": session_id })),
                        cx,
                    );
                }
                Err(error) => {
                    this.dispatch_gpui_app_modal_toast(
                        "error",
                        "Could not switch account",
                        &error.message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// The titlebar and Resources "Sleep Inactive": every inactive awake session on every
    /// connected machine, slept one at a time through the bulk path.
    pub(crate) fn gx_store_sleep_inactive_sessions_everywhere(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // The titlebar sweep is not a project action, so it still includes app tabs: every awake
        // browser tab that is not showing in a pane.
        self.gx_store_sleep_browser_tabs(false, cx);
        let ids = titlebar_sleep_inactive_ids(&self.gx_store.core);
        self.gx_store_sleep_session_set(ids, cx)
    }

    /// The Running Sessions list's daemon-stop control: every running local session, slept one at
    /// a time. The shared daemon process keeps running.
    pub(crate) fn gx_store_sleep_all_local_daemon_sessions(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        // Every awake browser tab too, as the Running Sessions stop always did.
        self.gx_store_sleep_browser_tabs(true, cx);
        let ids = running_local_session_ids(&self.gx_store.core);
        self.gx_store_sleep_session_set(ids, cx)
    }

    /// Sleeps the loaded browser tabs of every project that still own a page, the visible ones
    /// only when `include_visible`.
    ///
    /// CDXC:SessionSleep 2026-09-21 DECISION:
    /// User: a PROJECT's Sleep, Wake, Sleep Inactive and Close Inactive touch only its sessions; the titlebar sweep and the Running Sessions stop are not project actions and still include app tabs.
    fn gx_store_sleep_browser_tabs(&mut self, include_visible: bool, cx: &mut gpui::Context<Self>) {
        let active_project = self.browser_tabs_project_id.clone();
        let mut targets: Vec<(String, crate::app::model::BrowserTabId, bool)> = Vec::new();
        let mut collect = |project_id: &str,
                           model: &crate::app::model::BrowserTabModel,
                           awake: &std::collections::HashSet<crate::app::model::BrowserTabId>,
                           is_active: bool| {
            let visible: std::collections::HashSet<_> = model
                .rendered_leaf_order()
                .into_iter()
                .filter_map(|pane_id| model.active_tab_id_for_pane(pane_id))
                .collect();
            for tab in &model.tabs {
                if tab.state != crate::app::model::BrowserTabState::Loaded
                    || !awake.contains(&tab.id)
                    || (!include_visible && visible.contains(&tab.id))
                {
                    continue;
                }
                targets.push((project_id.to_string(), tab.id, is_active));
            }
        };
        if let Some(project_id) = active_project.as_deref() {
            let awake: std::collections::HashSet<_> =
                self.browser_surfaces.keys().copied().collect();
            collect(project_id, &self.browser_tabs, &awake, true);
        }
        for (project_id, model) in &self.parked_browser_tabs_by_project {
            if active_project.as_deref() == Some(project_id.as_str()) {
                continue;
            }
            let awake: std::collections::HashSet<_> = self
                .parked_browser_runtimes_by_project
                .get(project_id)
                .map(|runtime| runtime.surface_tab_ids())
                .unwrap_or_default();
            collect(project_id, model, &awake, false);
        }
        for (project_id, tab_id, is_active) in targets {
            if is_active {
                self.sleep_browser_tab(tab_id, cx);
            } else {
                self.sleep_parked_browser_tab(&project_id, tab_id, cx);
            }
        }
    }

    fn gx_store_sleep_session_set(
        &mut self,
        session_ids: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.gx_store_sidebar_list_ready() {
            return false;
        }
        if session_ids.is_empty() {
            return true;
        }
        self.gx_store_run_sidebar_bulk(
            &command(json!({
                "type": "setSessionsSleeping",
                "sessionIds": session_ids,
                "sleeping": true,
            })),
            cx,
        )
    }
}
