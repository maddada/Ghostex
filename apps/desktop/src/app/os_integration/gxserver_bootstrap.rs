// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the local gxserver bootstrap flow and the workspace open-target availability scan.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "windows")]
use futures::channel::mpsc;

#[cfg(any(target_os = "windows", test))]
mod message;

impl GhostexGpuiApp {
    /// Startup daemon bootstrap, mirroring the macOS GxserverClient contract:
    /// reuse a healthy protocol-matched daemon silently, surface protocol and
    /// toolchain problems honestly, and otherwise launch the bundled daemon
    /// (app-independent; quitting Ghostex never stops it), optionally showing
    /// a persistent status toast while it tracks progress. Unlike macOS this
    /// does not gate window creation; the shell shows its normal disconnected
    /// state until healthy.
    pub(crate) fn start_gpui_local_gxserver_bootstrap(
        &mut self,
        show_loading_toast: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "windows")]
        let windows_first_run_setup_active =
            self.windows_first_run_setup_state != GpuiWindowsFirstRunSetupState::Ready;
        #[cfg(target_os = "windows")]
        let windows_setup_progress_tx = if windows_first_run_setup_active {
            let (progress_tx, mut progress_rx) = mpsc::unbounded();
            cx.spawn(async move |this, cx| {
                while let Some(phase) = progress_rx.next().await {
                    let _ = this.update(cx, |this, cx| {
                        this.windows_first_run_setup_state =
                            GpuiWindowsFirstRunSetupState::SettingUp(phase);
                        cx.notify();
                    });
                }
            })
            .detach();
            Some(progress_tx)
        } else {
            None
        };
        cx.spawn(async move |this, cx| {
            let startup_started = std::time::Instant::now();
            let mut startup_diagnostics = Vec::new();
            #[cfg(target_os = "windows")]
            {
                /*
                CDXC:PlatformSupport 2026-07-15:
                Windows resolves an initialized WSL2 distribution before it
                probes localhost. The optional exact-name setting overrides
                automatic discovery. A missing or invalid distro surfaces setup
                guidance instead of falling back to PowerShell; Ghostex never
                invokes `wsl --install`, requests elevation, or schedules a
                reboot.
                */
                if windows_first_run_setup_active {
                    let readiness = cx
                        .background_executor()
                        .spawn(async { windows_terminal_backend::wsl_readiness() })
                        .await;
                    if readiness != windows_terminal_backend::WindowsWslReadiness::Ready {
                        let state = match readiness {
                            windows_terminal_backend::WindowsWslReadiness::MissingWsl => {
                                GpuiWindowsFirstRunSetupState::MissingWsl
                            }
                            windows_terminal_backend::WindowsWslReadiness::MissingDistribution => {
                                GpuiWindowsFirstRunSetupState::MissingDistribution
                            }
                            windows_terminal_backend::WindowsWslReadiness::ChooseDistribution(
                                distributions,
                            ) => GpuiWindowsFirstRunSetupState::ChooseDistribution(distributions),
                            windows_terminal_backend::WindowsWslReadiness::ConfiguredDistributionUnavailable(
                                distribution,
                            ) => GpuiWindowsFirstRunSetupState::ConfiguredDistributionUnavailable(
                                distribution,
                            ),
                            windows_terminal_backend::WindowsWslReadiness::Ready => unreachable!(),
                        };
                        let _ = this.update(cx, |this, cx| {
                            this.windows_first_run_setup_state = state;
                            cx.notify();
                        });
                        return;
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.windows_first_run_setup_state =
                            GpuiWindowsFirstRunSetupState::SettingUp(
                                windows_terminal_backend::WindowsWslSetupPhase::Checking,
                            );
                        cx.notify();
                    });
                }
                let progress_tx = windows_setup_progress_tx.clone();
                let preparation = cx
                    .background_executor()
                    .spawn(async move {
                        if let Some(progress_tx) = progress_tx {
                            windows_terminal_backend::prepare_gxserver_for_current_settings_with_progress(
                                &mut |phase| {
                                    let _ = progress_tx.unbounded_send(phase);
                                },
                            )
                        } else {
                            windows_terminal_backend::prepare_gxserver_for_current_settings()
                        }
                    })
                    .await;
                match preparation {
                    Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell) => {}
                    Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. }) => {}
                    Err(message) => {
                        if windows_first_run_setup_active {
                            let _ = this.update(cx, |this, cx| {
                                this.windows_first_run_setup_state =
                                    GpuiWindowsFirstRunSetupState::Failed(message.clone());
                                cx.notify();
                            });
                            return;
                        }
                        let _ = this.update(cx, |this, cx| {
                            this.show_gpui_gxserver_bootstrap_toast(
                                "warning",
                                "WSL runtime setup needed",
                                &message,
                                true,
                                cx,
                            );
                            this.replay_sidebar_gxserver_bootstrap(cx);
                        });
                        return;
                    }
                }
            }
            let (health, detail) = cx
                .background_executor()
                .spawn(async { gpui_probe_local_gxserver_health_with_diagnostics() })
                .await;
            startup_diagnostics.push(format!("+{}ms initial probe: {detail}", startup_started.elapsed().as_millis()));
            match health {
                GpuiLocalGxserverHealthState::Healthy {
                    tools_available: true,
                } => {
                    // A gxserver Windows started outside the desktop is replaced before anything loads from it.
                    #[cfg(target_os = "windows")]
                    if let Some(limited) = cx
                        .background_executor()
                        .spawn(async {
                            crate::app::os_integration::gxserver_launch_context::gpui_local_gxserver_limited_launch()
                        })
                        .await
                        && this
                            .update(cx, |this, cx| this.replace_gpui_limited_gxserver(limited, cx))
                            .unwrap_or(false)
                    {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| {
                        #[cfg(target_os = "windows")]
                        if windows_first_run_setup_active {
                            this.windows_first_run_setup_state =
                                GpuiWindowsFirstRunSetupState::Ready;
                            let mut state = load_gpui_first_run_onboarding_state();
                            state.windows_terminal_setup_complete = true;
                            persist_gpui_first_run_onboarding_state(&state);
                        }
                        this.replay_sidebar_gxserver_bootstrap(cx);
                        this.refresh_titlebar_accounts(cx);
                        this.start_gpui_portless_setup_prompt_check(cx);
                        this.start_gpui_first_run_onboarding(cx);
                        /*
                        CDXC:Telemetry 2026-08-26:
                        `app.launched` is a loopback ping to the daemon we just
                        confirmed healthy, so it can only fire once the token
                        and listener exist. The helper latches per process, so
                        firing it from both healthy branches still yields one
                        event per launch.
                        */
                        record_gpui_app_launched_telemetry(cx.background_executor());
                        cx.notify();
                    });
                    return;
                }
                GpuiLocalGxserverHealthState::Healthy {
                    tools_available: false,
                } => {
                    #[cfg(target_os = "windows")]
                    windows_terminal_backend::mark_package_update_required();
                    let _ = this.update(cx, |this, cx| {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "info",
                            "Restarting gxserver",
                            "The running gxserver does not match the tools bundled with this Ghostex build.",
                            true,
                            cx,
                        );
                        this.stop_gpui_local_gxserver_from_titlebar(true, cx);
                    });
                    return;
                }
                GpuiLocalGxserverHealthState::BuildMismatch => {
                    #[cfg(target_os = "windows")]
                    windows_terminal_backend::mark_package_update_required();
                    let _ = this.update(cx, |this, cx| {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "info",
                            "Updating gxserver",
                            "The running gxserver belongs to a different Ghostex build. Ghostex is restarting it before loading migrated storage.",
                            true,
                            cx,
                        );
                        this.stop_gpui_local_gxserver_from_titlebar(true, cx);
                    });
                    return;
                }
                GpuiLocalGxserverHealthState::ProtocolMismatch { reported } => {
                    #[cfg(target_os = "windows")]
                    {
                        windows_terminal_backend::mark_package_update_required();
                        let _ = this.update(cx, |this, cx| {
                            this.show_gpui_gxserver_bootstrap_toast(
                                "info",
                                "Updating WSL gxserver",
                                "The running WSL gxserver belongs to an older Ghostex build. Ghostex is activating the bundled runtime and restarting it.",
                                true,
                                cx,
                            );
                            this.stop_gpui_local_gxserver_from_titlebar(true, cx);
                        });
                        return;
                    }
                    #[cfg(not(target_os = "windows"))]
                    let message = gpui_gxserver_protocol_mismatch_message(reported);
                    #[cfg(not(target_os = "windows"))]
                    let _ = this.update(cx, |this, cx| {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "error",
                            "gxserver protocol mismatch",
                            &message,
                            true,
                            cx,
                        );
                    });
                    #[cfg(not(target_os = "windows"))]
                    return;
                }
                GpuiLocalGxserverHealthState::Unreachable => {}
            }

            #[cfg(target_os = "windows")]
            if matches!(
                windows_terminal_backend::resolve_current(),
                Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::Wsl { .. })
            ) {
                let detail = message::bounded_health_detail(&detail);
                let message = format!(
                    "gxserver started inside WSL2, but its health check from Windows failed: {detail}. Check the server status and WSL localhost connectivity, then retry."
                );
                if windows_first_run_setup_active {
                    let _ = this.update(cx, |this, cx| {
                        this.windows_first_run_setup_state =
                            GpuiWindowsFirstRunSetupState::Failed(message);
                        cx.notify();
                    });
                    return;
                }
                let _ = this.update(cx, |this, cx| {
                    this.show_gpui_gxserver_bootstrap_toast(
                        "error",
                        "WSL gxserver unavailable",
                        &message,
                        true,
                        cx,
                    );
                });
                return;
            }

            let Some(binary) = gpui_resolve_local_gxserver_binary() else {
                let _ = this.update(cx, |this, cx| {
                    this.show_gpui_gxserver_bootstrap_toast(
                        "error",
                        "gxserver unavailable",
                        "Bundled gxserver binary is missing. Run `cargo xtask build` for development, or reinstall Ghostex so Web/gxserver is present.",
                        true,
                        cx,
                    );
                });
                return;
            };
            // The first-run setup screen already shows progress; a toast behind it is noise.
            #[cfg(target_os = "windows")]
            let show_loading_toast = show_loading_toast && !windows_first_run_setup_active;
            if show_loading_toast {
                let _ = this.update(cx, |this, cx| {
                    this.show_gpui_gxserver_bootstrap_toast(
                        "info",
                        "Loading sessions",
                        "Starting gxserver and loading projects.",
                        true,
                        cx,
                    );
                });
            }
            let spawn_result = cx
                .background_executor()
                .spawn(async move { gpui_spawn_local_gxserver_daemon(&binary) })
                .await;
            let launch_steps = match spawn_result {
                Ok(steps) => steps,
                Err(message) => {
                    // The launcher's own step log is the evidence for this
                    // failure, so it goes into the same copyable report as a
                    // health timeout.
                    startup_diagnostics.push(format!("+{}ms launcher failed: {message}", startup_started.elapsed().as_millis()));
                    let report = cx
                        .background_executor()
                        .spawn(async move { gpui_gxserver_startup_failure_report(&startup_diagnostics) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "error",
                            "gxserver failed",
                            message.lines().next().unwrap_or(&message),
                            true,
                            cx,
                        );
                        if let Some(toast) = this.app_toasts.iter_mut().find(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID) {
                            toast.copy_text = Some(report);
                        }
                        this.sync_gpui_app_toast_window(cx);
                    });
                    return;
                }
            };
            startup_diagnostics.extend(launch_steps);
            startup_diagnostics.push(format!("+{}ms launcher accepted the start request.", startup_started.elapsed().as_millis()));
            #[cfg(target_os = "windows")]
            if windows_first_run_setup_active {
                let _ = this.update(cx, |this, cx| {
                    this.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::SettingUp(
                        windows_terminal_backend::WindowsWslSetupPhase::Starting,
                    );
                    cx.notify();
                });
            }
            /*
            CDXC:ServerDaemon 2026-09-28 WHY:
            On a freshly installed Windows the first gxserver start waited almost a minute before its own code ran (Windows scanning the new executable), while this loop gave up after about 20 seconds: the app showed "gxserver failed to start" and left the first-run setup frozen on "Checking the bundled terminal engine…" even after gxserver came up. Probe quickly for the first seconds, then keep probing less often until the startup patience runs out.
            */
            let spawned_at = std::time::Instant::now();
            let mut attempt = 0;
            while spawned_at.elapsed() < GPUI_GXSERVER_START_PATIENCE {
                attempt += 1;
                let interval = if spawned_at.elapsed() < Duration::from_secs(20) {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(2)
                };
                cx.background_executor().timer(interval).await;
                let (health, detail) = cx
                    .background_executor()
                    .spawn(async { gpui_probe_local_gxserver_health_with_diagnostics() })
                    .await;
                startup_diagnostics.push(format!("+{}ms probe {attempt}: {detail}", startup_started.elapsed().as_millis()));
                match health {
                    GpuiLocalGxserverHealthState::Healthy { tools_available } => {
                        let _ = this.update(cx, |this, cx| {
                            #[cfg(windows)]
                            if windows_first_run_setup_active && tools_available {
                                this.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Ready;
                                let mut state = load_gpui_first_run_onboarding_state();
                                state.windows_terminal_setup_complete = true;
                                persist_gpui_first_run_onboarding_state(&state);
                            }
                            if !tools_available {
                                this.show_gpui_gxserver_bootstrap_toast(
                                    "error",
                                    "gxserver toolchain unavailable",
                                    "The newly started gxserver did not expose the tools bundled with this Ghostex build.",
                                    true,
                                    cx,
                                );
                            }
                            this.replay_sidebar_gxserver_bootstrap(cx);
                            this.refresh_titlebar_accounts(cx);
                            this.start_gpui_portless_setup_prompt_check(cx);
                            /*
                            CDXC:Onboarding 2026-08-18:
                            A launch that had to respawn the daemon reaches
                            "gxserver healthy" here instead of through the
                            bootstrap branch, so first-run onboarding has to be
                            started from this recovery path too. The in-memory
                            latch plus the persisted markers keep it a no-op
                            when onboarding already ran.
                            */
                            this.start_gpui_first_run_onboarding(cx);
                            record_gpui_app_launched_telemetry(cx.background_executor());
                        });
                        return;
                    }
                    GpuiLocalGxserverHealthState::ProtocolMismatch { reported } => {
                        let message = gpui_gxserver_protocol_mismatch_message(reported);
                        let _ = this.update(cx, |this, cx| {
                            this.show_gpui_gxserver_bootstrap_toast(
                                "error",
                                "gxserver protocol mismatch",
                                &message,
                                true,
                                cx,
                            );
                        });
                        return;
                    }
                    GpuiLocalGxserverHealthState::BuildMismatch => {}
                    GpuiLocalGxserverHealthState::Unreachable => {}
                }
            }
            let report = cx
                .background_executor()
                .spawn(async move { gpui_gxserver_startup_failure_report(&startup_diagnostics) })
                .await;
            // During first-run setup the failure belongs on the setup screen, which offers Retry.
            #[cfg(target_os = "windows")]
            if windows_first_run_setup_active {
                let _ = this.update(cx, |this, cx| {
                    this.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Failed(
                        "The Ghostex background service did not start. Try again; if it keeps failing, restart Windows and open Ghostex again."
                            .to_string(),
                    );
                    cx.notify();
                });
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.show_gpui_gxserver_bootstrap_toast(
                    "error",
                    "gxserver failed to start",
                    "The daemon did not become healthy in time. Copy diagnostics to share the failure details.",
                    true,
                    cx,
                );
                if let Some(toast) = this.app_toasts.iter_mut().find(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID) {
                    toast.copy_text = Some(report);
                }
                this.sync_gpui_app_toast_window(cx);
            });
        })
        .detach();
    }

    pub(crate) fn start_gpui_workspace_open_target_availability_scan(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        // macOS detects installed Open In targets once when the native sidebar
        // starts (refreshWorkspaceOpenTargetAvailabilityAtStartup) and persists
        // the result into workspaceOpenTargetAvailability; the manual titlebar
        // refresh command exists in the host protocol but has no live sender in
        // shipped macOS, so the startup scan is the whole parity surface.
        cx.spawn(async move |this, cx| {
            let detected = cx
                .background_executor()
                .spawn(async { gpui_detect_workspace_open_target_availability() })
                .await;
            let Some(detected) = detected else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_gpui_workspace_open_target_availability_scan(detected, cx);
            });
        })
        .detach();
    }

    pub(crate) fn apply_gpui_workspace_open_target_availability_scan(
        &mut self,
        detected: GpuiDetectedOpenTargetAvailability,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        let stored =
            gpui_open_target_availability(settings_object.get("workspaceOpenTargetAvailability"));
        if gpui_detected_open_target_availability_matches_stored(&detected, &stored) {
            return;
        }
        settings_object.insert(
            "workspaceOpenTargetAvailability".to_string(),
            gpui_workspace_open_target_availability_settings_value(&detected),
        );
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
    }
}
