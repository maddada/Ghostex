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

impl GhostexGpuiApp {
    /// Startup daemon bootstrap, mirroring the macOS GxserverClient contract:
    /// reuse a healthy protocol-matched daemon silently, surface protocol and
    /// toolchain problems honestly, and otherwise launch the bundled daemon
    /// (app-independent; quitting Ghostex never stops it) while a persistent
    /// status toast tracks progress. Unlike macOS this does not gate window
    /// creation; the shell shows its normal disconnected state until healthy.
    pub(crate) fn start_gpui_local_gxserver_bootstrap(&mut self, cx: &mut gpui::Context<Self>) {
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
            #[cfg(target_os = "windows")]
            {
                /*
                CDXC:GPUIWindowsWslPersistence 2026-07-15:
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
                    Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell) => {
                        let _ = this.update(cx, |this, cx| {
                            this.replay_sidebar_gxserver_bootstrap(cx);
                        });
                        return;
                    }
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
            let health = cx
                .background_executor()
                .spawn(async { gpui_probe_local_gxserver_health() })
                .await;
            match health {
                GpuiLocalGxserverHealthState::Healthy {
                    tools_available: true,
                } => {
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
                        this.start_gpui_portless_setup_prompt_check(cx);
                        this.start_gpui_first_run_onboarding(cx);
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
                if windows_first_run_setup_active {
                    let _ = this.update(cx, |this, cx| {
                        this.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Failed(
                            "Ghostex started its terminal engine, but Windows could not connect to it. Check that WSL localhost forwarding is enabled, then try again."
                                .to_string(),
                        );
                        cx.notify();
                    });
                    return;
                }
                let _ = this.update(cx, |this, cx| {
                    this.show_gpui_gxserver_bootstrap_toast(
                        "error",
                        "WSL gxserver unavailable",
                        "gxserver started inside WSL2, but Windows could not reach it through localhost:58744. Check that WSL localhost forwarding is enabled, then retry.",
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
                        "Bundled gxserver binary is missing. Run `bun run build` for development, or reinstall Ghostex so Web/gxserver is present.",
                        true,
                        cx,
                    );
                });
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.show_gpui_gxserver_bootstrap_toast(
                    "info",
                    "Loading sessions",
                    "Starting gxserver and loading projects.",
                    true,
                    cx,
                );
            });
            let spawn_result = cx
                .background_executor()
                .spawn(async move { gpui_spawn_local_gxserver_daemon(&binary) })
                .await;
            if let Err(message) = spawn_result {
                let _ = this.update(cx, |this, cx| {
                    this.show_gpui_gxserver_bootstrap_toast(
                        "error",
                        "gxserver failed",
                        &message,
                        true,
                        cx,
                    );
                });
                return;
            }
            for _ in 0..40 {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let health = cx
                    .background_executor()
                    .spawn(async { gpui_probe_local_gxserver_health() })
                    .await;
                match health {
                    GpuiLocalGxserverHealthState::Healthy { tools_available } => {
                        let _ = this.update(cx, |this, cx| {
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
                            this.start_gpui_portless_setup_prompt_check(cx);
                            /*
                            CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
                            A launch that had to respawn the daemon reaches
                            "gxserver healthy" here instead of through the
                            bootstrap branch, so first-run onboarding has to be
                            started from this recovery path too. The in-memory
                            latch plus the persisted markers keep it a no-op
                            when onboarding already ran.
                            */
                            this.start_gpui_first_run_onboarding(cx);
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
            let launch_output = cx
                .background_executor()
                .spawn(async { gpui_recent_gxserver_launch_output() })
                .await;
            let description = launch_output
                .unwrap_or_else(|| "The daemon did not become healthy in time.".to_string());
            let _ = this.update(cx, |this, cx| {
                this.show_gpui_gxserver_bootstrap_toast(
                    "error",
                    "gxserver failed to start",
                    &description,
                    true,
                    cx,
                );
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
