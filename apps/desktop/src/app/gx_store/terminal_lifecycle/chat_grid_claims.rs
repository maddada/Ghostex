//! The chats this window shows, reported to the gxserver that runs each session so it holds the
//! session's chat grid claim (`server/src/session_chat_grid_claim.rs`). Only a session whose pane
//! shows Chat View counts: a session shown as a terminal, or not shown, is never reported, so a
//! Terminal-default user's sessions keep exactly the grid their terminals claim.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/shown_sessions.rs (the same lease
//! shape for keep-awake), apps/desktop/src/app/terminal_sync/gpui_engine_terminal_viewers.rs,
//! apps/gpui-web/src/app/gx_store/terminal_lifecycle/chat_grid_claims.rs (the web page's twin).

use std::time::Duration;

use super::shown_sessions_report::{
    CHAT_GRID_RENEW_MS, HoldCall, ShownSessions, ShownSessionsReport, send_chat_grid_call,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::GpuiWorkspaceTerminalSessionKey;

#[derive(Default)]
pub(crate) struct ChatGridClaimsHost {
    report: ShownSessionsReport,
    holder_id: Option<String>,
    renewing: bool,
}

impl GhostexGpuiApp {
    /// Reports the chats on screen when they changed; called with the shown-sessions report.
    pub(crate) fn gx_store_report_chat_grid_claims(&mut self, cx: &mut gpui::Context<Self>) {
        let shown = self.gx_store_chat_grid_sessions();
        let calls = self.gx_store.chat_grid_claims.report.changes(shown);
        self.gx_store_send_chat_grid_calls(calls, cx);
        if !self.gx_store.chat_grid_claims.renewing {
            self.gx_store.chat_grid_claims.renewing = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(CHAT_GRID_RENEW_MS))
                        .await;
                    let alive = this.update(cx, |this, cx| {
                        let calls = this.gx_store.chat_grid_claims.report.renewal();
                        this.gx_store_send_chat_grid_calls(calls, cx);
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    fn gx_store_send_chat_grid_calls(
        &mut self,
        calls: Vec<HoldCall>,
        cx: &mut gpui::Context<Self>,
    ) {
        if calls.is_empty() {
            return;
        }
        let holder_id = self
            .gx_store
            .chat_grid_claims
            .holder_id
            .get_or_insert_with(|| {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default();
                format!("desktop-chat-{:x}{:x}", std::process::id(), nanos)
            })
            .clone();
        for call in calls {
            let remote = match call.machine.as_deref() {
                None => None,
                Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                    Some(target) => Some(target),
                    None => continue,
                },
            };
            cx.background_executor()
                .spawn(send_chat_grid_call(remote, holder_id.clone(), call))
                .detach();
        }
    }

    fn gx_store_chat_grid_sessions(&self) -> ShownSessions {
        let mut shown = ShownSessions::new();
        // A side chat in the view panel is a chat on screen too (app/side_chat_panel/).
        let side_chat = self.side_panel_chat_shown_key().map(|key| match key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => (None, key.project_id, key.session_id),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                (Some(key.remote_machine_id), key.project_id, key.session_id)
            }
        });
        if let Some((machine, project_id, session_id)) = side_chat {
            shown
                .entry(machine)
                .or_default()
                .insert((project_id, session_id));
        }
        if !self.agents_workspace_visible() {
            return shown;
        }
        for pane_id in self.agents_workspace.rendered_leaf_order() {
            let Some(shell_session_id) = self.agents_workspace.active_session_in_pane(pane_id)
            else {
                continue;
            };
            if !self.agents_chat_mode_sessions.contains(&shell_session_id) {
                continue;
            }
            let (machine, project_id, session_id) =
                match self.workspace_terminal_key_for_shell_session(shell_session_id) {
                    Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                        (None, key.project_id, key.session_id)
                    }
                    Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => (
                        Some(key.remote_machine_id.clone()),
                        key.project_id,
                        key.session_id,
                    ),
                    None => continue,
                };
            shown
                .entry(machine)
                .or_default()
                .insert((project_id, session_id));
        }
        shown
    }
}
