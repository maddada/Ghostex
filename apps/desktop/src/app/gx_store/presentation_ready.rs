//! This computer's presentation has (re)loaded: the work that waited for it.
//!
//! CDXC:StateSync 2026-09-25 WHY:
//! The app runtime used to announce this as `gxserverPresentationReady` one frame after every
//! snapshot its own socket applied. The store's socket is the one the app draws from, so the same
//! work runs when the store takes a full snapshot of this computer (a launch, a reconnect, a
//! resync), and no longer depends on the runtime being alive. The one-time replay of the Commands
//! and Agents timer summaries went with it: it existed to hand restored timers to the runtime once
//! it could receive them (CDXC:DelayedSend 2026-07-22), and both summaries are the store's own now,
//! held from the moment they are armed.

use crate::GhostexGpuiApp;
use crate::app::helpers::board_gxserver::gxserver_health_and_daemon::GPUI_GXSERVER_DAEMON_TOAST_ID;

impl GhostexGpuiApp {
    pub(super) fn gx_store_presentation_ready(&mut self, cx: &mut gpui::Context<Self>) {
        self.refresh_gpui_new_thread_picker_agents(cx);
        self.refresh_gpui_new_thread_picker_accounts(cx);
        self.ensure_gpui_new_thread_picker_preloaded(cx);
        let loading_toast_visible = self
            .app_toasts
            .iter()
            .any(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID && toast.loading);
        if loading_toast_visible {
            self.remove_gpui_app_toast(GPUI_GXSERVER_DAEMON_TOAST_ID, cx);
        }
    }
}
