//! Settings > Integrations' permission rows for SpaceO and Fast Computer & Browser Use: the
//! Grant button, SpaceO's restart, and the quick re-check Settings polls (helpers in
//! helpers/os_cli/tool_permissions.rs).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::app::helpers::*;
use crate::*;

/// One quick re-check at a time; Settings' polls that arrive while one runs are dropped.
static PERMISSION_REFRESH_RUNNING: AtomicBool = AtomicBool::new(false);
/// How long after a Grant click its Privacy pane opens.
const GRANT_PANE_DELAY: std::time::Duration = std::time::Duration::from_millis(1500);

impl GhostexGpuiApp {
    pub(crate) fn refresh_gpui_tool_permission_status(&mut self, cx: &mut gpui::Context<Self>) {
        if PERMISSION_REFRESH_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload = background
                .spawn(async move { gpui_tool_permission_status_message() })
                .await;
            PERMISSION_REFRESH_RUNNING.store(false, Ordering::SeqCst);
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload, cx);
            });
        })
        .detach();
    }

    /// Grant: the tool asks macOS for its own grants, so it appears in System Settings, then
    /// the Privacy pane opens, for SpaceO with the floating permission bar beside it
    /// (permission_guide.rs). macOS shows its own request only once, so without the pane a second
    /// click would show nothing; a click while CuaDriver's Grant still runs only opens the pane.
    pub(crate) fn grant_gpui_tool_permission(
        &mut self,
        tool: &str,
        permission: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let pane_url = match permission {
            "accessibility" => GPUI_MACOS_ACCESSIBILITY_PREFERENCES_URL,
            "screenRecording" => GPUI_MACOS_SCREEN_RECORDING_PREFERENCES_URL,
            _ => return,
        };
        let background = cx.background_executor().clone();
        match tool {
            "spaceo" if !cfg!(target_os = "macos") => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "SpaceO can't run here",
                    "SpaceO is only available on macOS.",
                    cx,
                );
            }
            "spaceo" => {
                let Some(path) = gpui_spaceo_executable_path() else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "SpaceO is not installed",
                        "Install SpaceO first, then grant its permissions.",
                        cx,
                    );
                    return;
                };
                let permission = permission.to_string();
                let guide_path = path.clone();
                cx.spawn(async move |this, cx| {
                    let requested = background
                        .spawn(async move { gpui_spaceo_request_permissions(&path) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if let Err(message) = requested {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "SpaceO could not ask macOS for permission",
                                &message,
                                cx,
                            );
                        }
                        this.open_gpui_macos_system_settings_url(
                            pane_url,
                            "grantToolPermission",
                            cx,
                        );
                        this.open_gpui_permission_guide(guide_path, &permission, cx);
                        this.refresh_gpui_tool_permission_status(cx);
                    });
                })
                .detach();
            }
            "cuaDriver" => {
                let Some(path) = gpui_cua_driver_executable_path() else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Fast Computer & Browser Use is not installed",
                        "Install it first, then grant its permissions.",
                        cx,
                    );
                    return;
                };
                let grant =
                    background.spawn(async move { gpui_cua_driver_request_permissions(&path) });
                cx.spawn(async move |this, cx| {
                    // The request comes first, so the pane opens with CuaDriver listed.
                    background.timer(GRANT_PANE_DELAY).await;
                    let _ = this.update(cx, |this, cx| {
                        this.open_gpui_macos_system_settings_url(
                            pane_url,
                            "grantToolPermission",
                            cx,
                        );
                        this.refresh_gpui_tool_permission_status(cx);
                    });
                    let result = grant.await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(Err(message)) = result {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Fast Computer & Browser Use did not finish granting",
                                &message,
                                cx,
                            );
                        }
                        this.refresh_gpui_tool_permission_status(cx);
                    });
                })
                .detach();
            }
            _ => {}
        }
    }

    /// Restart CuaDriver's daemon so it picks up what macOS allows now: the row's button, and
    /// automatically during a Grant.
    pub(crate) fn restart_gpui_cua_driver(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = gpui_cua_driver_executable_path() else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let restart = background.spawn(async move { gpui_cua_driver_restart_daemon(&path) });
            let _ = this.update(cx, |this, cx| this.refresh_gpui_tool_permission_status(cx));
            let result = restart.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Err(message)) = result {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Fast Computer & Browser Use did not restart",
                        if message.is_empty() {
                            "Try the Restart button again."
                        } else {
                            &message
                        },
                        cx,
                    );
                }
                this.refresh_gpui_tool_permission_status(cx);
            });
        })
        .detach();
    }

    /// Restart SpaceO once its agents finish so its daemon picks up grants made after it started.
    pub(crate) fn restart_gpui_spaceo(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = gpui_spaceo_executable_path() else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let restart = background.spawn(async move { gpui_spaceo_restart_daemon(&path) });
            // Show "Restarting" while it waits for agents' sessions to end.
            let _ = this.update(cx, |this, cx| this.refresh_gpui_tool_permission_status(cx));
            let result = restart.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(())) => this.dispatch_gpui_app_modal_toast(
                        "success",
                        "SpaceO restarted",
                        "SpaceO now runs with the permissions you granted.",
                        cx,
                    ),
                    Some(Err(message)) => this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "SpaceO did not restart",
                        if message.is_empty() {
                            "Try Restart SpaceO again."
                        } else {
                            &message
                        },
                        cx,
                    ),
                    None => {}
                }
                this.refresh_gpui_tool_permission_status(cx);
            });
        })
        .detach();
    }
}
