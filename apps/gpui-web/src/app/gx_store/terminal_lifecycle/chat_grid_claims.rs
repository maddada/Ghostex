//! The page's half of the chat grid claim: the chat the work area shows, reported to gxserver so
//! it holds the session's chat grid claim (`server/src/session_chat_grid_claim.rs`). The desktop's
//! twin (`apps/desktop/src/app/gx_store/terminal_lifecycle/chat_grid_claims.rs`) reports every
//! pane in Chat View; a page shows one session at a time, and a session shown as its terminal is
//! never reported.
//!
//! CDXC:Zmx 2026-10-23 WHY: the page never reported its chat, so a session read in a browser kept
//! whatever grid its last terminal left (a phone's 40 columns, a narrow split) instead of the
//! 200-column resting width every other client's Chat View claims. A page opened straight into
//! Chat View attaches no terminal, so this lease is its only claim.

use std::time::Duration;

use ghostex_gx_core::MachineId;

use super::shown_sessions_report::{
    CHAT_GRID_RENEW_MS, HoldCall, ShownSessions, ShownSessionsReport, send_chat_grid_call,
};
use crate::GhostexGpuiApp;

/// The report and the lease holder this page uses.
#[derive(Default)]
pub(crate) struct WebChatGridClaims {
    report: ShownSessionsReport,
    holder_id: Option<String>,
    renewing: bool,
}

impl GhostexGpuiApp {
    /// Reports the chat on screen when it changed, and starts the renewal the first time. Cheap
    /// when nothing changed; called every render.
    pub(crate) fn web_report_chat_grid_claims(&mut self, cx: &mut gpui::Context<Self>) {
        let mut shown = ShownSessions::new();
        if let Some(session) = self.open_session.as_ref().filter(|session| {
            session.machine == MachineId::Local
                && !self.web_shows_terminal(session)
                && self.native_chats.contains_key(*session)
        }) {
            shown
                .entry(None)
                .or_default()
                .insert((session.project_id.clone(), session.session_id.clone()));
        }
        // The side chat beside it is a chat on screen too (`web_host/side_chat.rs`).
        if let Some(session) = self
            .web_side_chat_shown()
            .filter(|session| session.machine == MachineId::Local)
        {
            shown
                .entry(None)
                .or_default()
                .insert((session.project_id.clone(), session.session_id.clone()));
        }
        let calls = self.chat_grid_claims.report.changes(shown);
        self.web_send_chat_grid_calls(calls, cx);
        if !self.chat_grid_claims.renewing {
            self.chat_grid_claims.renewing = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(CHAT_GRID_RENEW_MS))
                        .await;
                    let alive = this.update(cx, |this, cx| {
                        let calls = this.chat_grid_claims.report.renewal();
                        this.web_send_chat_grid_calls(calls, cx);
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    fn web_send_chat_grid_calls(&mut self, calls: Vec<HoldCall>, cx: &mut gpui::Context<Self>) {
        if calls.is_empty() {
            return;
        }
        let holder_id = self
            .chat_grid_claims
            .holder_id
            .get_or_insert_with(|| {
                format!(
                    "gpui-web-chat-{}",
                    crate::app::helpers::gpui_random_uuid_string().unwrap_or_default()
                )
            })
            .clone();
        for call in calls {
            let holder_id = holder_id.clone();
            cx.spawn(async move |_, _| send_chat_grid_call(None, holder_id, call).await)
                .detach();
        }
    }
}
