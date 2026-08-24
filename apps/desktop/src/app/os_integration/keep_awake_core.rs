// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the keep-awake menu, power settings modal launch, and the keep-awake period start/stop/refresh runtime.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::Pixels;
use gpui::Window;
use gpui_component::WindowExt;
use gpui_component::native_menu::NativeMenu;
use gpui_component::notification::Notification;

use crate::app::actions::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn show_gpui_keep_awake_menu(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
        The GPUI Keep Awake titlebar control is a menu launcher on both left-click and right-click, matching macOS titlebar semantics. The OS-owned NativeMenu exposes only the shared duration choices, the running-only stop action, and Power Settings; it must not become a direct toggle, React overlay, hidden hit region, persistent runtime store, or broad process killer.
        */
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        if !settings.titlebar_control_visible() {
            window.push_notification(
                Notification::warning("Keep Awake is hidden by current Settings."),
                cx,
            );
            cx.notify();
            return;
        }

        self.refresh_gpui_keep_awake_runtime();
        let active_duration = self
            .keep_awake_runtime
            .as_ref()
            .map(|runtime| runtime.duration_minutes);
        let mut menu = NativeMenu::new().menu_with_disabled(
            "Keep awake period",
            true,
            Box::new(GpuiKeepAwakeMenuLabel),
        );
        for duration in shared_settings::KEEP_AWAKE_DURATION_OPTIONS {
            menu = menu.menu_with_check(
                duration.menu_label(),
                active_duration == Some(*duration),
                Box::new(StartGpuiKeepAwakePeriod {
                    duration_minutes: duration.minutes(),
                }),
            );
        }
        if self.keep_awake_runtime.is_some() {
            menu = menu.menu("Don't keep awake", Box::new(StopGpuiKeepAwake));
        }
        menu.separator()
            .menu("Power Settings...", Box::new(OpenGpuiPowerSettingsModal))
            .show(position, window, cx);
    }

    pub(crate) fn open_gpui_power_settings_modal_from_titlebar(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
        Power Settings from the Keep Awake menu must deep-link into the shared Settings modal with `{ modal: "settings", initialSection: "power" }`. Reuse the GPUI app-modal CEF host and sidebar hydrate instead of adding a GPUI-local settings surface or placeholder route.
        */
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialSection": "power",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn start_gpui_keep_awake_period(
        &mut self,
        duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
        Manual Keep Awake starts from the titlebar menu with the same duration choices and allow-display-sleep flag as macOS. The runtime owner below still starts only fixed `/usr/bin/caffeinate` argv and never accepts shell text, command output, paths, or private Settings payloads from React.

        CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
        Manual starts clear autostart suppression and replace any previous GPUI-owned runtime. Delayed Send may start an automatic hold only when no manual/launch/display hold is running, so user-chosen titlebar periods take precedence.
        */
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().keep_awake_titlebar_settings();
        if !settings.feature_enabled {
            self.stop_gpui_keep_awake_runtime_with_options(true);
            window.push_notification(
                Notification::warning("Enable Show Beta Features before using Keep Awake."),
                cx,
            );
            cx.notify();
            return;
        }
        if settings.hide_titlebar_control {
            self.stop_gpui_keep_awake_runtime_with_options(true);
            window.push_notification(
                Notification::warning("Keep Awake is hidden by current Settings."),
                cx,
            );
            cx.notify();
            return;
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = duration_minutes;
            let _ = settings;
            window.push_notification(
                Notification::warning(
                    "Keep Awake requires macOS caffeinate and is not supported on this platform.",
                ),
                cx,
            );
            cx.notify();
        }

        #[cfg(target_os = "macos")]
        {
            if self
                .start_gpui_keep_awake_runtime(
                    duration_minutes,
                    GpuiKeepAwakeRuntimeSource::Manual,
                    settings,
                    cx,
                )
                .is_err()
            {
                window.push_notification(Notification::warning("Could not start Keep Awake."), cx);
            }
            self.ensure_gpui_keep_awake_power_ticker(cx);
            cx.notify();
        }
    }

    pub(crate) fn stop_gpui_keep_awake_from_titlebar(&mut self, cx: &mut gpui::Context<Self>) {
        if self.stop_gpui_keep_awake_runtime_with_options(true) {
            cx.notify();
        }
    }

    pub(crate) fn stop_gpui_keep_awake_runtime(&mut self) -> bool {
        self.stop_gpui_keep_awake_runtime_with_options(false)
    }

    pub(crate) fn stop_gpui_keep_awake_runtime_with_options(
        &mut self,
        suppress_auto_start: bool,
    ) -> bool {
        if suppress_auto_start && self.keep_awake_runtime.is_some() {
            self.keep_awake_auto_start_suppressed = true;
        }
        #[cfg(target_os = "macos")]
        {
            let Some(mut runtime) = self.keep_awake_runtime.take() else {
                return false;
            };
            /*
            CDXC:GPUIKeepAwakeStop 2026-07-11:
            Teardown must not run on the main thread: the lid-sleep disable
            goes through the privileged XPC helper with a 20-second semaphore
            wait (GpuiLidSleepHelperClient.m), so a hung or uninstalled
            helper beach-balled the app for 20s per stop while this ran
            inside an entity update. The enable/heartbeat side already runs
            detached on the background executor and the lease/heartbeat
            resync in finish_gpui_keep_awake_lid_sleep_action reconciles
            ordering, so a detached disable matches the existing model.
            */
            let _ = std::thread::Builder::new()
                .name("gpui-keep-awake-stop".to_string())
                .spawn(move || {
                    if runtime.lid_sleep_prevention_enabled {
                        let _ = gpui_set_lid_sleep_prevention_enabled(false, false);
                    }
                    let _ = runtime.child.kill();
                    let _ = runtime.child.wait();
                });
            true
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.keep_awake_runtime.take().is_some()
        }
    }

    pub(crate) fn refresh_gpui_keep_awake_runtime(&mut self) -> bool {
        if self
            .keep_awake_runtime
            .as_ref()
            .and_then(|runtime| runtime.fire_at)
            .is_some_and(|fire_at| Instant::now() >= fire_at)
        {
            return self.stop_gpui_keep_awake_runtime_with_options(true);
        }

        #[cfg(target_os = "macos")]
        {
            let child_finished = self
                .keep_awake_runtime
                .as_mut()
                .is_some_and(|runtime| !matches!(runtime.child.try_wait(), Ok(None)));
            if child_finished {
                if let Some(mut runtime) = self.keep_awake_runtime.take() {
                    // The child already exited (try_wait above), so this wait
                    // only reaps. The lid-sleep disable is the 20s-capable
                    // XPC call and runs detached — see
                    // CDXC:GPUIKeepAwakeStop 2026-07-11 in the stop path.
                    let _ = runtime.child.wait();
                    if runtime.lid_sleep_prevention_enabled {
                        let _ = std::thread::Builder::new()
                            .name("gpui-keep-awake-stop".to_string())
                            .spawn(move || {
                                let _ = gpui_set_lid_sleep_prevention_enabled(false, false);
                            });
                    }
                }
                self.keep_awake_auto_start_suppressed = false;
                return true;
            }
        }
        false
    }

    pub(crate) fn start_gpui_keep_awake_runtime(
        &mut self,
        duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
        source: GpuiKeepAwakeRuntimeSource,
        settings: shared_settings::SharedKeepAwakeTitlebarSettings,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        if !settings.titlebar_control_visible() {
            return Err("keep awake disabled".to_string());
        }
        if source == GpuiKeepAwakeRuntimeSource::Automatic && self.keep_awake_runtime.is_some() {
            return Ok(());
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = duration_minutes;
            let _ = source;
            let _ = settings;
            let _ = cx;
            Err("unsupported platform".to_string())
        }

        #[cfg(target_os = "macos")]
        {
            self.refresh_gpui_keep_awake_runtime();
            if source == GpuiKeepAwakeRuntimeSource::Automatic && self.keep_awake_runtime.is_some()
            {
                return Ok(());
            }
            self.stop_gpui_keep_awake_runtime_with_options(false);
            let child =
                gpui_spawn_keep_awake_caffeinate(duration_minutes, settings.allow_display_sleep)?;
            let started_at = Instant::now();
            self.keep_awake_runtime_generation = self.keep_awake_runtime_generation.wrapping_add(1);
            if self.keep_awake_runtime_generation == 0 {
                self.keep_awake_runtime_generation = 1;
            }
            let runtime_id = self.keep_awake_runtime_generation;
            self.keep_awake_runtime = Some(GpuiKeepAwakeRuntime {
                runtime_id,
                duration_minutes,
                source,
                started_at,
                fire_at: gpui_keep_awake_fire_at(started_at, duration_minutes),
                child,
                lid_sleep_prevention_enabled: false,
                lid_sleep_prevention_install_attempted: false,
                lid_sleep_prevention_update_in_flight: false,
                lid_sleep_prevention_warning_sent: false,
                lid_sleep_prevention_last_refresh_at: None,
            });
            self.keep_awake_auto_start_suppressed = false;
            self.sync_gpui_keep_awake_lid_sleep_prevention(settings, cx);
            Ok(())
        }
    }
}
