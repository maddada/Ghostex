//! The page's half of the shown-sessions report: the session it has open, and the renewal timer.
//! The desktop's twin (`apps/desktop/src/app/gx_store/terminal_lifecycle/shown_sessions.rs`)
//! reports every pane it renders; a page shows one session at a time.
//!
//! CDXC:SessionSleep 2026-09-26 WHY: a page never reported what it showed, so Agent Auto Sleep put
//! the session open in it to sleep ten minutes after its last activity, and did it again the moment
//! the session was woken, because a wake does not count as activity.

use std::time::Duration;

use ghostex_gx_core::MachineId;

use super::shown_sessions_report::{
    HoldCall, SHOWN_SESSIONS_RENEW_MS, ShownSessions, ShownSessionsReport, send_hold_call,
};
use crate::GhostexGpuiApp;

/// The report and the lease holder this page uses.
#[derive(Default)]
pub(crate) struct WebShownSessions {
    report: ShownSessionsReport,
    holder_id: Option<String>,
    renewing: bool,
}

impl GhostexGpuiApp {
    /// Reports the open session when it changed, and starts the renewal the first time.
    pub(crate) fn web_report_shown_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        let mut shown = ShownSessions::new();
        if let Some(session) = self
            .open_session
            .as_ref()
            .filter(|session| session.machine == MachineId::Local)
        {
            shown
                .entry(None)
                .or_default()
                .insert((session.project_id.clone(), session.session_id.clone()));
        }
        let calls = self.shown_sessions.report.changes(shown);
        self.web_send_hold_calls(calls, cx);
        if !self.shown_sessions.renewing {
            self.shown_sessions.renewing = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(SHOWN_SESSIONS_RENEW_MS))
                        .await;
                    let alive = this.update(cx, |this, cx| {
                        let calls = this.shown_sessions.report.renewal();
                        this.web_send_hold_calls(calls, cx);
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    fn web_send_hold_calls(&mut self, calls: Vec<HoldCall>, cx: &mut gpui::Context<Self>) {
        if calls.is_empty() {
            return;
        }
        let holder_id = self
            .shown_sessions
            .holder_id
            .get_or_insert_with(|| {
                format!(
                    "gpui-web-{}",
                    crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()
                )
            })
            .clone();
        for call in calls {
            let holder_id = holder_id.clone();
            cx.spawn(async move |_, _| send_hold_call(None, holder_id, call).await)
                .detach();
        }
    }
}
