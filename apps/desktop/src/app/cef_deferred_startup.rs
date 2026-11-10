// Launch-time CEF start: an installed runtime is warmed up a few seconds after launch, and a web
// view about to be shown starts it sooner (app/helpers/web_runtime.rs).

use futures::StreamExt as _;

use crate::app::helpers::web_runtime::*;
use crate::app::model::*;
use crate::*;

/// How long after the first window's first frame an installed web runtime is started, so its
/// start (which holds the app while Chromium initializes) lands after launch has settled.
const WEB_RUNTIME_WARM_UP_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

/// An app-modal open that arrived before CEF was ready.
pub(crate) struct GpuiAppModalOpenDeferredForCef {
    modal: GpuiAppModalKind,
    open_message: serde_json::Value,
}

impl GhostexGpuiApp {
    /// CEF starts on the first browser-creation signal, which only a web view about to be shown
    /// sends (a Browser tab, a web workarea view, an extension popup or modal, Files' browser
    /// area, the chat's page window), including one restored at launch, and, when it is
    /// installed, a few seconds after launch (`warm_up_web_runtime_after_launch`).
    pub(crate) fn begin_deferred_cef_startup(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_load_remote_recent_projects(cx);
        if let Some(mut demand) = cef::take_runtime_demand_receiver() {
            // Every open window answers the signal, and the listener outlives the window that
            // took it (app/workspace_windows/).
            let app: &gpui::App = cx;
            app.spawn(async move |cx| {
                while demand.next().await.is_some() {
                    cx.update(crate::app::workspace_windows::request_cef_runtime_in_every_window);
                }
            })
            .detach();
            // The demand receiver is taken once per process, so this runs for the first window.
            // A Windows Reinstall asked for while the runtime ran removed it at launch
            // (app/helpers/web_runtime_pending.rs); it is downloaded again now.
            if crate::app::helpers::web_runtime_pending::take_reinstall_after_launch() {
                self.install_web_runtime(false, cx);
            } else {
                self.update_older_web_runtime_after_launch(cx);
            }
            Self::warm_up_web_runtime_after_launch(cx);
        }
    }

    /// CDXC:CefRuntime 2026-11-10 WHY:
    /// GitHub #530: when an app update sealed a new CEF version, the web views showed the first-install prompt ("an optional install (one-time)") to a user who had installed the runtime. Having installed it is the user's yes, the same as the warm-up below treats an installed runtime: the new version downloads in the background right after launch, and the web views show its progress as "Updating web runtime". A computer that never installed it is left alone.
    fn update_older_web_runtime_after_launch(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            if web_runtime_state() != WebRuntimeState::NotInstalled {
                return;
            }
            let older = cx
                .background_executor()
                .spawn(async { cef_component_window::older_cef_component_installed() });
            cx.spawn(async move |this, cx| {
                if !older.await {
                    return;
                }
                let _ = this.update(cx, |this, cx| {
                    if web_runtime_state() == WebRuntimeState::NotInstalled {
                        set_web_runtime_install_is_update(true);
                        this.install_web_runtime(false, cx);
                    }
                });
            })
            .detach();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let _ = cx;
    }

    /// CDXC:CefRuntime 2026-10-23 DECISION:
    /// User: asked whether to start the web runtime in the background at launch so the first chat page opens fast (the first page waited about 1.5 s for the runtime), "Yes, warm it up at launch". An installed runtime is started `WEB_RUNTIME_WARM_UP_AFTER` the first window's first frame, so it costs launch nothing; a runtime that is not installed is left alone (no prompt, no download), and web views still start it on demand when they come first. This supersedes the 2026-09-28 rule that only a web view about to be shown starts it (app/helpers/web_runtime.rs).
    fn warm_up_web_runtime_after_launch(cx: &mut gpui::Context<Self>) {
        if web_runtime_state() != WebRuntimeState::Installed {
            return;
        }
        let app: &gpui::App = cx;
        app.spawn(async move |cx| {
            cx.background_executor()
                .timer(WEB_RUNTIME_WARM_UP_AFTER)
                .await;
            if web_runtime_state() != WebRuntimeState::Installed {
                return;
            }
            support_logs::append(
                support_logs::GpuiSupportLog::HostLifecycle,
                "gpui.host.webRuntimeWarmUp",
                serde_json::json!({
                    "pid": std::process::id(),
                    "sinceLaunchMs": support_logs::ms_since_launch(),
                }),
            );
            cx.update(crate::app::workspace_windows::request_cef_runtime_in_every_window);
        })
        .detach();
    }

    /// A web view wants CEF. Starts it when it is installed and not started yet; while it is not
    /// installed, being installed or failed, the web views show the prompt instead and nothing
    /// is downloaded.
    pub(crate) fn request_cef_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        if self.cef_runtime_requested || web_runtime_state() != WebRuntimeState::Installed {
            return;
        }
        self.cef_runtime_requested = true;
        self.start_web_runtime(cx);
    }

    /// The modal host creates its page once and does not retry, so an open that lands before CEF is ready is held and replayed instead of showing an empty window. The newest request wins, matching the one-app-modal rule.
    /// While the runtime is not installed the open is still held, so the modal opens once the user installs it from the prompt (`open_web_runtime_modal_prompt`).
    pub(crate) fn defer_gpui_app_modal_open_for_cef(
        &mut self,
        modal: GpuiAppModalKind,
        open_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        self.app_modal_open_deferred_for_cef = Some(GpuiAppModalOpenDeferredForCef {
            modal,
            open_message,
        });
        if web_runtime_available() {
            self.request_cef_runtime(cx);
        } else {
            self.open_web_runtime_modal_prompt(modal, cx);
        }
    }

    /// The modal waiting for the web runtime, if any.
    pub(crate) fn gpui_app_modal_deferred_for_cef_kind(&self) -> Option<GpuiAppModalKind> {
        self.app_modal_open_deferred_for_cef
            .as_ref()
            .map(|deferred| deferred.modal)
    }

    pub(crate) fn forget_gpui_app_modal_deferred_for_cef(&mut self) {
        self.app_modal_open_deferred_for_cef = None;
    }

    pub(crate) fn open_gpui_app_modal_deferred_for_cef(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(deferred) = self.app_modal_open_deferred_for_cef.take() else {
            return;
        };
        // A floating chat page is a native window with its own CEF page, not the extension host.
        if deferred.modal == GpuiAppModalKind::VisualPage {
            self.open_gpui_visual_page_modal(&deferred.open_message, cx);
            return;
        }
        self.open_gpui_app_modal_window_inner(deferred.modal, deferred.open_message, None, cx);
    }
}
