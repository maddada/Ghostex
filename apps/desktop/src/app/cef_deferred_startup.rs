// Launch-time CEF deferral: the runtime starts on the first real need for a
// web view or once launch has settled, whichever comes first.

use std::time::Duration;

use futures::StreamExt as _;

use crate::app::model::*;
use crate::*;

/// How long after the first frame CEF is warmed when nothing asked for it sooner.
const CEF_LAUNCH_WARMUP_DELAY: Duration = Duration::from_secs(4);

/// An app-modal open that arrived before CEF was ready.
pub(crate) struct GpuiAppModalOpenDeferredForCef {
    modal: GpuiAppModalKind,
    open_message: serde_json::Value,
    sidebar_state_message: serde_json::Value,
    reset_ready_retry: bool,
}

impl GhostexGpuiApp {
    /// CDXC:CefRuntime 2026-09-19 DECISION:
    /// User: make startup as fast as possible by deferring CEF until it is actually needed, and warm it once launch has settled so Settings and quick access still open instantly.
    /// Launch starts only the QuickJS sidebar and chat service. CEF starts on the first browser-creation signal (a woken web pane, a modal, React chat when GPUI chat is off) or after the warm-up delay, so Chromium's framework load and helper processes no longer compete with session restore.
    pub(crate) fn begin_deferred_cef_startup(&mut self, cx: &mut gpui::Context<Self>) {
        self.ensure_native_service(cx);
        if let Some(mut demand) = cef::take_runtime_demand_receiver() {
            cx.spawn(async move |this, cx| {
                while demand.next().await.is_some() {
                    if this
                        .update(cx, |this, cx| this.request_cef_runtime(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(CEF_LAUNCH_WARMUP_DELAY)
                .await;
            let _ = this.update(cx, |this, cx| this.request_cef_runtime(cx));
        })
        .detach();
    }

    pub(crate) fn request_cef_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        if self.cef_runtime_requested {
            return;
        }
        self.cef_runtime_requested = true;
        self.begin_cef_startup(cx);
    }

    /// The modal host creates its page once and does not retry, so an open that lands before CEF is ready is held and replayed instead of showing an empty window. The newest request wins, matching the one-app-modal rule.
    pub(crate) fn defer_gpui_app_modal_open_for_cef(
        &mut self,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        sidebar_state_message: serde_json::Value,
        reset_ready_retry: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_modal_open_deferred_for_cef = Some(GpuiAppModalOpenDeferredForCef {
            modal,
            open_message,
            sidebar_state_message,
            reset_ready_retry,
        });
        self.request_cef_runtime(cx);
    }

    pub(crate) fn open_gpui_app_modal_deferred_for_cef(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(deferred) = self.app_modal_open_deferred_for_cef.take() else {
            return;
        };
        self.open_gpui_app_modal_window_inner(
            deferred.modal,
            deferred.open_message,
            deferred.sidebar_state_message,
            None,
            deferred.reset_ready_retry,
            cx,
        );
    }
}
