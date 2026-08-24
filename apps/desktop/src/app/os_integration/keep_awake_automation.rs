// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds keep-awake automation-from-settings sync, the working-session grace refresh, the power ticker, and the power-rules background check/snapshot apply.
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
    pub(crate) fn sync_gpui_keep_awake_automation_from_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
        GPUI Keep Awake automation matches the available macOS runtime behavior from shared Settings: launch and external-display rules start the default-duration hold, battery and Low Power Mode stop the GPUI-owned hold with generic user-visible failure boundaries, and active Delayed Send timers start an automatic until-turned-off hold only while no manual/launch/display hold exists.

        CDXC:GPUITitlebarKeepAwake 2026-06-26-00:09:
        `keepAwakePreventLidSleep` now mirrors macOS through the privileged lid-sleep helper only. GPUI enables the helper lease only for a live Keep Awake runtime, refreshes it while active, disables it on runtime/settings/app shutdown paths without prompting, and never runs `pmset disablesleep` directly from the main app.

        CDXC:GPUITitlebarKeepAwake 2026-06-26-00:29:
        `keepAwakeWhileWorkingSessions` now mirrors the native automatic-hold rule from safe GPUI session state: count Running Agents marked Working and live awake command tabs marked Working, start an automatic until-turned-off hold while the count is positive, and keep only that automatic hold alive for the 20-minute grace after the count drops to zero. `keepAwakeDeactivateOnUserSwitch` remains parsed/no-op because no concrete macOS titlebar runtime behavior has been identified.

        CDXC:GPUITitlebarKeepAwake 2026-06-27-00:40:
        Native titlebar Keep Awake also holds for non-sleeping terminal sessions with projected Delayed Send timers. GPUI derives the same automatic input from live command-tab model state instead of the raw timer map so sleeping tabs and orphan persisted command rows cannot keep the power hold alive.
        */
        let settings = settings_snapshot.keep_awake_titlebar_settings();
        let mut changed = self.refresh_gpui_keep_awake_runtime();
        if self.refresh_gpui_keep_awake_working_session_grace(settings) {
            changed = true;
        }
        if !settings.titlebar_control_visible() {
            if self.stop_gpui_keep_awake_if_hidden_by_settings(settings_snapshot, cx) {
                changed = true;
            }
            if changed {
                cx.notify();
            }
            return;
        }

        if settings.activate_on_launch
            && self.keep_awake_runtime.is_none()
            && !self.keep_awake_auto_start_suppressed
        {
            if self
                .start_gpui_keep_awake_runtime(
                    settings.default_duration_minutes,
                    GpuiKeepAwakeRuntimeSource::Manual,
                    settings,
                    cx,
                )
                .is_ok()
            {
                changed = true;
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not start Keep Awake",
                    "GPUI could not start Keep Awake.",
                    cx,
                );
            }
        }

        if self.sync_gpui_keep_awake_automatic_hold(settings, cx) {
            changed = true;
        }
        self.sync_gpui_keep_awake_lid_sleep_prevention(settings, cx);
        self.check_gpui_keep_awake_power_rules_in_background(cx);
        self.ensure_gpui_keep_awake_power_ticker(cx);
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn sync_gpui_keep_awake_automation_from_current_settings(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        self.sync_gpui_keep_awake_automation_from_settings(&settings_snapshot, cx);
    }

    pub(crate) fn refresh_gpui_keep_awake_working_session_grace(
        &mut self,
        settings: shared_settings::SharedKeepAwakeTitlebarSettings,
    ) -> bool {
        let now = Instant::now();
        let current_working_session_count =
            gpui_keep_awake_working_session_count(&self.agents_workspace, &self.command_pane);
        let next_state = gpui_keep_awake_refresh_working_session_grace_state(
            settings.while_working_sessions && settings.titlebar_control_visible(),
            self.keep_awake_previous_working_session_count,
            current_working_session_count,
            self.keep_awake_working_session_grace_until,
            now,
        );
        let changed = self.keep_awake_previous_working_session_count
            != next_state.previous_working_session_count
            || self.keep_awake_working_session_grace_until != next_state.grace_until;
        self.keep_awake_previous_working_session_count = next_state.previous_working_session_count;
        self.keep_awake_working_session_grace_until = next_state.grace_until;
        changed
    }

    pub(crate) fn sync_gpui_keep_awake_automatic_hold(
        &mut self,
        settings: shared_settings::SharedKeepAwakeTitlebarSettings,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let delayed_send_hold_active =
            gpui_keep_awake_command_delayed_send_session_count(&self.command_pane) > 0
                || !self.agents_delayed_send_timers.is_empty()
                || !self.agents_send_when_stopped_watchers.is_empty();
        let working_session_hold_active = gpui_keep_awake_working_session_hold_active(
            settings,
            self.keep_awake_previous_working_session_count,
            self.keep_awake_working_session_grace_until,
            Instant::now(),
        );
        let should_run_automatic_keep_awake = settings.titlebar_control_visible()
            && !self.keep_awake_auto_start_suppressed
            && (delayed_send_hold_active || working_session_hold_active);
        if !should_run_automatic_keep_awake {
            if self
                .keep_awake_runtime
                .as_ref()
                .is_some_and(|runtime| runtime.source == GpuiKeepAwakeRuntimeSource::Automatic)
            {
                return self.stop_gpui_keep_awake_runtime_with_options(false);
            }
            return false;
        }
        if self.keep_awake_runtime.is_some() {
            return false;
        }
        if self
            .start_gpui_keep_awake_runtime(
                shared_settings::SharedKeepAwakeDurationMinutes::UntilTurnedOff,
                GpuiKeepAwakeRuntimeSource::Automatic,
                settings,
                cx,
            )
            .is_ok()
        {
            self.ensure_gpui_keep_awake_power_ticker(cx);
            return true;
        }
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Could not start Keep Awake",
            "GPUI could not start Keep Awake.",
            cx,
        );
        false
    }

    pub(crate) fn gpui_keep_awake_power_ticker_needed(&self) -> bool {
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        self.keep_awake_runtime.is_some()
            || (settings.titlebar_control_visible()
                && !self.keep_awake_auto_start_suppressed
                && settings.activate_on_external_display)
    }

    pub(crate) fn ensure_gpui_keep_awake_power_ticker(&mut self, cx: &mut gpui::Context<Self>) {
        if self.keep_awake_power_ticker_active || !self.gpui_keep_awake_power_ticker_needed() {
            return;
        }
        self.keep_awake_power_ticker_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_KEEP_AWAKE_POWER_CHECK_INTERVAL)
                    .await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
                        this.sync_gpui_keep_awake_automation_from_settings(&settings_snapshot, cx);
                        if this.gpui_keep_awake_power_ticker_needed() {
                            true
                        } else {
                            this.keep_awake_power_ticker_active = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn check_gpui_keep_awake_power_rules_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            let Some(options) = self.gpui_keep_awake_power_probe_options() else {
                return;
            };
            let background = cx.background_executor().clone();
            cx.spawn(async move |this, cx| {
                let snapshot = background
                    .spawn(async move { gpui_read_keep_awake_power_snapshot(options) })
                    .await;
                let Ok(snapshot) = snapshot else {
                    return;
                };
                let _ = this.update(cx, |this, cx| {
                    this.apply_gpui_keep_awake_power_snapshot(snapshot, cx);
                });
            })
            .detach();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = cx;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn gpui_keep_awake_power_probe_options(
        &self,
    ) -> Option<GpuiKeepAwakePowerProbeOptions> {
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        let include_external_display = settings.titlebar_control_visible()
            && self.keep_awake_runtime.is_none()
            && !self.keep_awake_auto_start_suppressed
            && settings.activate_on_external_display;
        let include_battery =
            self.keep_awake_runtime.is_some() && settings.deactivate_below_battery_threshold;
        let include_low_power_mode =
            self.keep_awake_runtime.is_some() && settings.deactivate_on_low_power_mode;
        if !include_external_display && !include_battery && !include_low_power_mode {
            return None;
        }
        Some(GpuiKeepAwakePowerProbeOptions {
            include_battery,
            include_external_display,
            include_low_power_mode,
        })
    }

    pub(crate) fn apply_gpui_keep_awake_power_snapshot(
        &mut self,
        snapshot: GpuiKeepAwakePowerSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        if !settings.titlebar_control_visible() {
            return;
        }
        if self.keep_awake_runtime.is_some()
            && settings.deactivate_below_battery_threshold
            && snapshot
                .battery_percent
                .is_some_and(|percent| percent <= settings.battery_threshold_percent)
        {
            if self.stop_gpui_keep_awake_runtime_with_options(true) {
                cx.notify();
            }
            return;
        }
        if self.keep_awake_runtime.is_some()
            && settings.deactivate_on_low_power_mode
            && snapshot.low_power_mode == Some(true)
        {
            if self.stop_gpui_keep_awake_runtime_with_options(true) {
                cx.notify();
            }
            return;
        }
        if self.keep_awake_runtime.is_none()
            && !self.keep_awake_auto_start_suppressed
            && settings.activate_on_external_display
            && snapshot.external_display_connected
        {
            if self
                .start_gpui_keep_awake_runtime(
                    settings.default_duration_minutes,
                    GpuiKeepAwakeRuntimeSource::Manual,
                    settings,
                    cx,
                )
                .is_ok()
            {
                self.ensure_gpui_keep_awake_power_ticker(cx);
                cx.notify();
            } else {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not start Keep Awake",
                    "GPUI could not start Keep Awake.",
                    cx,
                );
            }
        }
    }

    pub(crate) fn stop_gpui_keep_awake_if_hidden_by_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
        Keep Awake is beta-gated and titlebar-control-gated at runtime, not just at render time. If Settings disables beta or hides the control while this GPUI process owns a caffeinate child, stop only that child so hidden chrome cannot leave an invisible process-local hold running.

        CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
        A beta/control visibility disable also suppresses launch/display/delayed-send autostarts for the current GPUI run once it stops an active hold, matching the macOS titlebar runtime boundary without killing non-GPUI caffeinate processes.
        */
        if settings_snapshot
            .keep_awake_titlebar_settings()
            .titlebar_control_visible()
        {
            return false;
        }
        let stopped = self.stop_gpui_keep_awake_runtime_with_options(true);
        if stopped {
            cx.notify();
        }
        stopped
    }
}
