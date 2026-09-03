// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the macOS lid-sleep-prevention sync and its caffeinate-process spawn/finish/cleanup helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "macos")]
    pub(crate) fn sync_gpui_keep_awake_lid_sleep_prevention(
        &mut self,
        settings: shared_settings::SharedKeepAwakeTitlebarSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:KeepAwake 2026-06-26-00:09:
        `keepAwakePreventLidSleep` is active only while GPUI owns a live Keep Awake runtime. Enable through the privileged helper once per runtime with installation allowed, refresh the helper lease by heartbeat, and disable with no installer prompt when Settings hides the control, the setting flips off, the runtime stops, or the app exits.
        */
        let action = {
            let Some(runtime) = self.keep_awake_runtime.as_mut() else {
                return;
            };
            let runtime_id = runtime.runtime_id;
            let should_enable = settings.titlebar_control_visible() && settings.prevent_lid_sleep;
            if !should_enable {
                if runtime.lid_sleep_prevention_enabled
                    && !runtime.lid_sleep_prevention_update_in_flight
                {
                    runtime.lid_sleep_prevention_update_in_flight = true;
                    Some((runtime_id, GpuiKeepAwakeLidSleepAction::Disable))
                } else {
                    None
                }
            } else if runtime.lid_sleep_prevention_enabled {
                let heartbeat_due = runtime
                    .lid_sleep_prevention_last_refresh_at
                    .map(|last_refresh| {
                        Instant::now().saturating_duration_since(last_refresh)
                            >= GPUI_KEEP_AWAKE_LID_SLEEP_HEARTBEAT_INTERVAL
                    })
                    .unwrap_or(true);
                if heartbeat_due && !runtime.lid_sleep_prevention_update_in_flight {
                    runtime.lid_sleep_prevention_update_in_flight = true;
                    runtime.lid_sleep_prevention_last_refresh_at = Some(Instant::now());
                    Some((runtime_id, GpuiKeepAwakeLidSleepAction::Heartbeat))
                } else {
                    None
                }
            } else if !runtime.lid_sleep_prevention_update_in_flight {
                let install_if_needed = !runtime.lid_sleep_prevention_install_attempted;
                runtime.lid_sleep_prevention_install_attempted = true;
                runtime.lid_sleep_prevention_update_in_flight = true;
                Some((
                    runtime_id,
                    GpuiKeepAwakeLidSleepAction::Enable { install_if_needed },
                ))
            } else {
                None
            }
        };

        if let Some((runtime_id, action)) = action {
            self.spawn_gpui_keep_awake_lid_sleep_action(runtime_id, action, cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn sync_gpui_keep_awake_lid_sleep_prevention(
        &mut self,
        _settings: shared_settings::SharedKeepAwakeTitlebarSettings,
        _cx: &mut gpui::Context<Self>,
    ) {
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn spawn_gpui_keep_awake_lid_sleep_action(
        &mut self,
        runtime_id: u64,
        action: GpuiKeepAwakeLidSleepAction,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        let cleanup_background = background.clone();
        cx.spawn(async move |this, cx| {
            let ok = background
                .spawn(async move {
                    match action {
                        GpuiKeepAwakeLidSleepAction::Enable { install_if_needed } => {
                            gpui_set_lid_sleep_prevention_enabled(true, install_if_needed)
                        }
                        GpuiKeepAwakeLidSleepAction::Disable => {
                            gpui_set_lid_sleep_prevention_enabled(false, false)
                        }
                        GpuiKeepAwakeLidSleepAction::Heartbeat => {
                            gpui_heartbeat_lid_sleep_prevention()
                        }
                    }
                })
                .await;
            let applied = this
                .update(cx, |this, cx| {
                    this.finish_gpui_keep_awake_lid_sleep_action(runtime_id, action, ok, cx);
                })
                .is_ok();
            if ok && !applied && matches!(action, GpuiKeepAwakeLidSleepAction::Enable { .. }) {
                let _ = cleanup_background
                    .spawn(async move { gpui_set_lid_sleep_prevention_enabled(false, false) })
                    .await;
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn finish_gpui_keep_awake_lid_sleep_action(
        &mut self,
        runtime_id: u64,
        action: GpuiKeepAwakeLidSleepAction,
        ok: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        let should_enable = settings.titlebar_control_visible() && settings.prevent_lid_sleep;
        let now = Instant::now();
        let mut disable_stale_enabled_lease = false;
        let mut resync_after_stale_disable = false;
        let mut warn_unavailable = false;

        if let Some(runtime) = self.keep_awake_runtime.as_mut() {
            if runtime.runtime_id != runtime_id {
                match action {
                    GpuiKeepAwakeLidSleepAction::Enable { .. } if ok => {
                        if should_enable {
                            runtime.lid_sleep_prevention_enabled = true;
                            runtime.lid_sleep_prevention_last_refresh_at = Some(now);
                        } else {
                            disable_stale_enabled_lease = true;
                        }
                    }
                    GpuiKeepAwakeLidSleepAction::Disable if ok && should_enable => {
                        runtime.lid_sleep_prevention_enabled = false;
                        runtime.lid_sleep_prevention_last_refresh_at = None;
                        resync_after_stale_disable = true;
                    }
                    _ => {}
                }
            } else {
                runtime.lid_sleep_prevention_update_in_flight = false;
                match action {
                    GpuiKeepAwakeLidSleepAction::Enable { .. } => {
                        if ok && should_enable {
                            runtime.lid_sleep_prevention_enabled = true;
                            runtime.lid_sleep_prevention_last_refresh_at = Some(now);
                        } else {
                            runtime.lid_sleep_prevention_enabled = false;
                            runtime.lid_sleep_prevention_last_refresh_at = None;
                            if ok && !should_enable {
                                disable_stale_enabled_lease = true;
                            } else if should_enable && !runtime.lid_sleep_prevention_warning_sent {
                                runtime.lid_sleep_prevention_warning_sent = true;
                                warn_unavailable = true;
                            }
                        }
                    }
                    GpuiKeepAwakeLidSleepAction::Disable => {
                        if ok {
                            runtime.lid_sleep_prevention_enabled = false;
                            runtime.lid_sleep_prevention_last_refresh_at = None;
                        } else if !runtime.lid_sleep_prevention_warning_sent {
                            runtime.lid_sleep_prevention_warning_sent = true;
                            warn_unavailable = true;
                        }
                    }
                    GpuiKeepAwakeLidSleepAction::Heartbeat => {
                        if ok && should_enable {
                            runtime.lid_sleep_prevention_enabled = true;
                            runtime.lid_sleep_prevention_last_refresh_at = Some(now);
                        } else {
                            runtime.lid_sleep_prevention_enabled = false;
                            runtime.lid_sleep_prevention_last_refresh_at = None;
                            if should_enable && !runtime.lid_sleep_prevention_warning_sent {
                                runtime.lid_sleep_prevention_warning_sent = true;
                                warn_unavailable = true;
                            }
                        }
                    }
                }
            }
        } else if ok && matches!(action, GpuiKeepAwakeLidSleepAction::Enable { .. }) {
            disable_stale_enabled_lease = true;
        }

        if disable_stale_enabled_lease {
            self.spawn_gpui_keep_awake_lid_sleep_disable_cleanup(cx);
        }
        if resync_after_stale_disable {
            self.sync_gpui_keep_awake_lid_sleep_prevention(settings, cx);
        }
        if warn_unavailable {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not update closed-lid Keep Awake",
                "GPUI could not update closed-lid Keep Awake.",
                cx,
            );
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn spawn_gpui_keep_awake_lid_sleep_disable_cleanup(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            let _ = background
                .spawn(async move { gpui_set_lid_sleep_prevention_enabled(false, false) })
                .await;
        })
        .detach();
    }
}
