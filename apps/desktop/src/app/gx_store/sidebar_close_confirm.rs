//! Asks before a close that would stop a coordinator or a busy agent, then runs the same close.
//! Which closes ask and what they say is gx-core's `close_confirmation`; this file holds the close
//! while the question is open. The desktop and the GPUI web build compile it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_actions/close_confirm.rs (the DECISION and the rule),
//! apps/desktop/src/app/window/close_sessions_confirm_modal.rs (the dialog),
//! apps/desktop/src/app/focus/agents_tab_actions.rs (the tab closes and Cmd+W that ask through it).

use ghostex_gx_core::{SessionKey, close_confirmation, sidebar_close_targets};
use gpui::AppContext;
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::model::GpuiAppModalKind;
use crate::app::window::{
    CLOSE_SESSIONS_CONFIRM_MODAL_INITIAL_HEIGHT, CLOSE_SESSIONS_CONFIRM_MODAL_WIDTH,
    CloseSessionsConfirmModalCommand, GpuiCloseSessionsConfirmModalWindow,
};

/// The close a confirmation holds until it is answered.
pub(crate) type PendingSessionClose =
    Box<dyn FnOnce(&mut GhostexGpuiApp, &mut gpui::Context<GhostexGpuiApp>)>;

impl GhostexGpuiApp {
    /// Asks first when closing `sessions` would stop a coordinator or a busy agent: opens the
    /// question, keeps `close` for its Close button, and answers true, so the caller stops. Answers
    /// false (run the close now) when nothing is busy, or when this is the confirmed close running.
    pub(crate) fn gx_store_ask_before_close(
        &mut self,
        sessions: &[SessionKey],
        close: PendingSessionClose,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.gx_store.session_close_confirmed {
            return false;
        }
        let Some(confirmation) = close_confirmation(&self.gx_store.core, sessions) else {
            return false;
        };
        self.gx_store.pending_session_close = Some(close);
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.gx_store_answer_close_confirmation(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CloseSessionsConfirm,
            CLOSE_SESSIONS_CONFIRM_MODAL_WIDTH,
            CLOSE_SESSIONS_CONFIRM_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiCloseSessionsConfirmModalWindow::new(
                        palette,
                        host,
                        confirmation,
                        window,
                        cx,
                    )
                })
            },
            cx,
        );
        if self.native_app_modal_kind() != Some(GpuiAppModalKind::CloseSessionsConfirm) {
            // The dialog could not open: nothing closes rather than closing without asking.
            self.gx_store.pending_session_close = None;
        }
        true
    }

    /// The sidebar's doors: a row's Close button, middle click and menu item, a multi-selection's
    /// Close, a project's Close Inactive, and a menu batch of them. True when it asked.
    pub(crate) fn gx_store_ask_before_sidebar_close(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let sessions = sidebar_close_targets(&self.gx_store.core, command);
        if sessions.is_empty() {
            return false;
        }
        let command = command.clone();
        self.gx_store_ask_before_close(
            &sessions,
            Box::new(move |app, cx| app.dispatch_native_sidebar_ui(command, cx)),
            cx,
        )
    }

    fn gx_store_answer_close_confirmation(
        &mut self,
        command: CloseSessionsConfirmModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        self.release_native_app_modal_window(GpuiAppModalKind::CloseSessionsConfirm, cx);
        let pending = self.gx_store.pending_session_close.take();
        if let (CloseSessionsConfirmModalCommand::Close, Some(close)) = (command, pending) {
            self.gx_store.session_close_confirmed = true;
            close(self, cx);
            self.gx_store.session_close_confirmed = false;
        }
        cx.notify();
    }
}
