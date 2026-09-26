//! The desktop half of the shown-sessions report: what this window shows, and the renewal timer.
//!
//! What counts as shown is what the old runtime's Auto Sleep sweep protected: the session in each
//! rendered pane (chat-mode sessions included, because the tab still owns its pane), the focused
//! session, the sessions with one of this app's own Delayed Send timers or send-when-stopped
//! watchers (CDXC:DelayedSend 2026-08-20: sleeping one kills the provider the send would type
//! into), and the owner of each visible command pane split in the active project
//! (CDXC:SessionSleep 2026-06-27).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/terminal_lifecycle/shown_sessions_report.rs.

use std::time::Duration;

use ghostex_gx_core::MachineId;

use super::shown_sessions_report::{
    HoldCall, SHOWN_SESSIONS_RENEW_MS, ShownSessions, ShownSessionsReport, send_hold_call,
};
use crate::GhostexGpuiApp;
use crate::app::helpers::GpuiWorkspaceTerminalSessionKey;
use crate::app::model::gpui_command_session_external_id;

/// The report and the lease holder this app run uses.
#[derive(Default)]
pub(crate) struct ShownSessionsHost {
    report: ShownSessionsReport,
    holder_id: Option<String>,
    renewing: bool,
}

impl GhostexGpuiApp {
    /// Reports the shown sessions when they changed. Cheap when nothing did; called every frame the
    /// selection is not moving.
    pub(crate) fn gx_store_report_shown_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        let shown = self.gx_store_shown_sessions();
        let calls = self.gx_store.shown_sessions.report.changes(shown);
        self.gx_store_send_hold_calls(calls, cx);
        if !self.gx_store.shown_sessions.renewing {
            self.gx_store.shown_sessions.renewing = true;
            // Once per run, when the app is up: the old runtime's Close After Done set moves to
            // the daemons (terminal_lifecycle/close_after_done.rs).
            self.gx_store_migrate_close_after_done_storage(cx);
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(SHOWN_SESSIONS_RENEW_MS))
                        .await;
                    let alive = this.update(cx, |this, cx| {
                        let calls = this.gx_store.shown_sessions.report.renewal();
                        this.gx_store_send_hold_calls(calls, cx);
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    fn gx_store_send_hold_calls(&mut self, calls: Vec<HoldCall>, cx: &mut gpui::Context<Self>) {
        if calls.is_empty() {
            return;
        }
        let holder_id = self
            .gx_store
            .shown_sessions
            .holder_id
            .get_or_insert_with(|| {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or_default();
                format!("desktop-{:x}{:x}", std::process::id(), nanos)
            })
            .clone();
        for call in calls {
            let remote = match call.machine.as_deref() {
                None => None,
                Some(machine_id) => match self.gpui_remote_gxserver_request_target(machine_id) {
                    Some(target) => Some(target),
                    // A machine without a tunnel has nothing to hold on; its daemon's leases run out.
                    None => continue,
                },
            };
            cx.background_executor()
                .spawn(send_hold_call(remote, holder_id.clone(), call))
                .detach();
        }
    }

    fn gx_store_shown_sessions(&self) -> ShownSessions {
        let mut shown = ShownSessions::new();
        let mut add = |machine: Option<String>, project_id: &str, session_id: &str| {
            shown
                .entry(machine)
                .or_default()
                .insert((project_id.to_string(), session_id.to_string()));
        };
        for pane_id in self.agents_workspace.rendered_leaf_order() {
            let Some(shell_session_id) = self.agents_workspace.active_session_in_pane(pane_id)
            else {
                continue;
            };
            match self.workspace_terminal_key_for_shell_session(shell_session_id) {
                Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                    add(None, &key.project_id, &key.session_id)
                }
                Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => add(
                    Some(key.remote_machine_id.clone()),
                    &key.project_id,
                    &key.session_id,
                ),
                None => {}
            }
        }
        let focus = self.gx_store.core.focus();
        if let Some(session) = focus.focused_session.as_ref() {
            let machine = match &session.machine {
                MachineId::Local => None,
                MachineId::Remote(machine_id) => Some(machine_id.clone()),
            };
            add(machine, &session.project_id, &session.session_id);
        }
        for (key, shell_session_id) in &self.local_workspace_session_mappings {
            if self
                .agents_delayed_send_timers
                .contains_key(shell_session_id)
                || self
                    .agents_send_when_stopped_watchers
                    .contains_key(shell_session_id)
            {
                add(None, &key.project_id, &key.session_id);
            }
        }
        if let Some(project) = focus
            .active_project
            .as_ref()
            .filter(|project| project.machine.is_local())
        {
            for (_, command_session_id) in self.command_pane.pane_owner_session_ids() {
                add(
                    None,
                    &project.project_id,
                    &gpui_command_session_external_id(command_session_id),
                );
            }
        }
        shown
    }
}
