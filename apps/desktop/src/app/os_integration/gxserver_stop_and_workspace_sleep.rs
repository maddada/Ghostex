// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the titlebar gxserver stop/restart action, workspace sleep dispatch, and the macOS sidebar pointer/context-menu dispatch helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// Explicit user gxserver Stop/Restart (macOS `stopGxserverFromUserAction`
    /// / `restartGxserverFromUserAction` parity) through the daemon's
    /// `/api/control/stop` control-plane API. The stop API deliberately leaves
    /// zmx provider sessions running; restart reuses the normal bootstrap.
    pub(crate) fn stop_gpui_local_gxserver_from_titlebar(
        &mut self,
        restart_after_stop: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let previous_pid = cx
                .background_executor()
                .spawn(async { gpui_local_gxserver_health_pid() })
                .await;
            let stop_result = cx
                .background_executor()
                .spawn(async {
                    gxserver_post_typed_operation(
                        "/api/control/stop",
                        &serde_json::json!({}),
                        Duration::from_secs(5),
                    )
                })
                .await;
            let mut stop_error = match &stop_result {
                Ok((status_code, _)) if (200..300).contains(status_code) => None,
                Ok((status_code, _)) => {
                    Some(format!("gxserver stop failed with HTTP {status_code}."))
                }
                Err(message) => Some(message.clone()),
            };
            if stop_error.is_none() {
                // Bounded wait for the control plane to actually drop, like
                // macOS stopRunningGxserverControlPlane's 5s health poll.
                for _ in 0..20 {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let health = cx
                        .background_executor()
                        .spawn(async { gpui_probe_local_gxserver_health() })
                        .await;
                    if matches!(health, GpuiLocalGxserverHealthState::Unreachable) {
                        break;
                    }
                }
                stop_error = cx
                    .background_executor()
                    .spawn(async move {
                        gpui_wait_for_local_gxserver_process_exit(
                            previous_pid,
                            Duration::from_secs(15),
                        )
                    })
                    .await
                    .err();
            }
            let _ = this.update(cx, |this, cx| {
                let should_restart = restart_after_stop && stop_error.is_none();
                match stop_error {
                    None => {
                        if !restart_after_stop {
                            this.show_gpui_gxserver_bootstrap_toast(
                                "info",
                                "gxserver stopped",
                                "The local gxserver control plane was stopped. zmx sessions keep running.",
                                false,
                                cx,
                            );
                        }
                    }
                    Some(message) => {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "error",
                            "gxserver stop failed",
                            &message,
                            true,
                            cx,
                        );
                    }
                }
                let _ = this.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
                if should_restart {
                    this.start_gpui_local_gxserver_bootstrap(true, cx);
                }
            });
        })
        .detach();
    }

    /// The titlebar batch sleep reuses the store's inactive-session filter (the one a project's
    /// Sleep Inactive uses) across every connected machine, so working, attention, background-work
    /// and already-sleeping sessions stay untouched like macOS.
    pub(crate) fn dispatch_gpui_workspace_sleep_inactive_sessions(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.gx_store_sleep_inactive_sessions_everywhere(cx)
    }

    /// macOS `killTerminalDaemon` parity: since the gxserver cutover the Running Sessions
    /// daemon-stop control never stops the shared gxserver process. Every awake local session goes
    /// through the shared paced sleep path; the modal refresh converges as sessions go to sleep.
    pub(crate) fn dispatch_gpui_workspace_sleep_all_daemon_sessions(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.gx_store_sleep_all_local_daemon_sessions(cx)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_sidebar_pointer_inside(
        &mut self,
        inside: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.native_sidebar.pointer_inside != inside {
            self.native_sidebar.pointer_inside = inside;
            cx.notify();
        }
        if !inside {
            self.native_sidebar.hovered_collection = None;
            self.native_sidebar.hovered_group = None;
            self.native_sidebar.hovered_session = None;
            self.native_sidebar.hovered_section = None;
            cx.notify();
        }
        true
    }

    /// CDXC:Sidebar 2026-09-17 WHY:
    /// Embedded browser panes receive clicks outside GPUI, so the AppKit observer dismisses the native sidebar menu through this callback.
    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_sidebar_dismiss_context_menus(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let open = self.native_sidebar.menu.take().is_some();
        if open {
            cx.notify();
        }
        open
    }
}
