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
            let stop_error = match &stop_result {
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
                    this.start_gpui_local_gxserver_bootstrap(cx);
                }
            });
        })
        .detach();
    }

    /// The titlebar batch sleep reuses the sidebar runtime's inactive-session
    /// revalidation (same filter as per-project bulk sleep) so working,
    /// attention, and already-sleeping sessions stay untouched like macOS.
    pub(crate) fn dispatch_gpui_workspace_sleep_inactive_sessions(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": "sleepInactiveSessions",
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_runtime_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /// macOS `killTerminalDaemon` parity: since the gxserver cutover the
    /// Running Sessions daemon-stop control never stops the shared gxserver
    /// process — macOS routes every awake gxserver-presented terminal through
    /// the shared sleep path and refreshes the modal. GPUI forwards the same
    /// bulk request to the sidebar runtime, which owns the paced sleep
    /// transitions; the modal refresh converges as sessions go to sleep.
    pub(crate) fn dispatch_gpui_workspace_sleep_all_daemon_sessions(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": "sleepAllDaemonSessions",
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_runtime_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /*
    CDXC:GPUISidebarPointerTracking 2026-08-02:
    Report an observed pointer crossing of the sidebar's native frame into the
    page. The sidebar CEF surface is a native sibling of GPUI chrome, Ghostty
    terminal hosts, and the other CEF panes, so Chromium never sees the pointer
    leave and can hold the last hovered row's `:hover` state indefinitely. The
    page turns this into the shared `data-native-pointer-inside` contract that
    the sidebar stylesheet already declares suppressors against.
    */
    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_sidebar_pointer_inside(
        &mut self,
        inside: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_native_pointer_inside_script(inside);
        let wrote_flag = sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        /*
        CDXC:GPUISidebarPointerTracking 2026-08-20:
        The CSS flag can only neutralize hover-derived *styling*. A tooltip is
        page state opened on pointer-enter and closed on pointer-leave, and the
        leave never reaches the renderer when the pointer crosses into a native
        sibling, so a session row's tooltip stayed on screen over a terminal
        pane. Dismissing it needs page code, so it goes through the sidebar
        bridge like context-menu dismissal does; if the bridge is not installed
        the page cannot have an open tooltip either.
        */
        if !inside {
            let dismissed = sidebar.update(cx, |surface, _| {
                surface.execute_app_owned_script(GPUI_SIDEBAR_DISMISS_TOOLTIPS_SCRIPT)
            });
            return wrote_flag || dismissed;
        }
        wrote_flag
    }

    /*
    CDXC:GPUISidebarPointerTracking 2026-08-02:
    A mouse-down landed outside the sidebar's native frame, so any open sidebar
    context menu must close. The page's own backdrop only covers the sidebar
    document, and its window-blur dismissal never fires here: the sidebar
    surface is mouse-focus passive, so clicking a terminal pane or a titlebar
    button does not blur a browsing context that never held focus.
    */
    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_sidebar_dismiss_context_menus(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(GPUI_SIDEBAR_DISMISS_CONTEXT_MENUS_SCRIPT)
        })
    }
}
