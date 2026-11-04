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
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, keep-awake

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The newest startup bootstrap; an older one that is still retrying stops when a new one begins.
static GPUI_GXSERVER_BOOTSTRAP_GENERATION: AtomicU64 = AtomicU64::new(0);

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::*;

#[cfg(target_os = "windows")]
use futures::channel::mpsc;

#[cfg(target_os = "windows")]
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
        let bootstrap_generation =
            GPUI_GXSERVER_BOOTSTRAP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        // A new attempt (Try again in the sidebar, a restart): the sidebar shows it loading again
        // and the last attempt's failure toast goes, until this attempt answers or fails.
        if self.gpui_gxserver_start_failure_shown() {
            self.set_gpui_gxserver_start_failure(None, cx);
            self.remove_gpui_app_toast(GPUI_GXSERVER_DAEMON_TOAST_ID, cx);
        }
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
                        let title = if windows_terminal_backend::current_preference()
                            == windows_terminal_backend::WindowsTerminalBackendPreference::PowerShell
                        {
                            "Ghostex could not prepare its background service"
                        } else {
                            "WSL runtime setup needed"
                        };
                        let _ = this.update(cx, |this, cx| {
                            this.show_gpui_gxserver_bootstrap_toast(
                                "warning",
                                title,
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
                        this.set_gpui_gxserver_start_failure(None, cx);
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
                            "Restarting gxserver…",
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
                            "Updating gxserver…",
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
                                "Updating WSL gxserver…",
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
                /*
                CDXC:ServerDaemon 2026-10-22 WHY:
                A gxserver that says it is starting (server/src/server/startup_gate.rs) is waited for, with its step on the loading notice, and once it is ready this bootstrap runs again from the top so every ready-state check applies. Starting another one beside it would do nothing. Only one that is still starting after the startup patience falls through to the launch below, whose new gxserver replaces a hung start (stale_server_takeover.rs).
                */
                GpuiLocalGxserverHealthState::Starting { .. } => {
                    #[cfg(target_os = "windows")]
                    let show_starting_toast = show_loading_toast && !windows_first_run_setup_active;
                    #[cfg(not(target_os = "windows"))]
                    let show_starting_toast = show_loading_toast;
                    loop {
                        if GPUI_GXSERVER_BOOTSTRAP_GENERATION.load(Ordering::SeqCst) != bootstrap_generation {
                            return;
                        }
                        let (health, detail) = cx
                            .background_executor()
                            .spawn(async { gpui_probe_local_gxserver_health_with_diagnostics() })
                            .await;
                        match health {
                            GpuiLocalGxserverHealthState::Starting { starting_for, .. }
                                if starting_for
                                    < GPUI_GXSERVER_START_PATIENCE + Duration::from_secs(20) =>
                            {
                                if show_starting_toast {
                                    let _ = this.update(cx, |this, cx| {
                                        this.show_gpui_gxserver_bootstrap_toast(
                                            "info",
                                            "Loading sessions…",
                                            &detail,
                                            true,
                                            cx,
                                        );
                                    });
                                }
                                cx.background_executor().timer(Duration::from_secs(1)).await;
                            }
                            GpuiLocalGxserverHealthState::Starting { .. } => {
                                startup_diagnostics.push(format!("+{}ms gave up waiting: {detail}", startup_started.elapsed().as_millis()));
                                break;
                            }
                            _ => {
                                let _ = this.update(cx, |this, cx| {
                                    this.start_gpui_local_gxserver_bootstrap(show_loading_toast, cx);
                                });
                                return;
                            }
                        }
                    }
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
                        "Loading sessions…",
                        "Starting gxserver and loading projects.",
                        true,
                        cx,
                    );
                });
            }
            let spawn_binary = binary.clone();
            let spawn_result = cx
                .background_executor()
                .spawn(async move { gpui_spawn_local_gxserver_daemon(&spawn_binary) })
                .await;
            let launch_steps = match spawn_result {
                Ok(steps) => Some(steps),
                Err(message) => {
                    // The launcher's own step log is the evidence for this
                    // failure, so it goes into the same copyable report as a
                    // health timeout.
                    startup_diagnostics.push(format!("+{}ms launcher failed: {message}", startup_started.elapsed().as_millis()));
                    // A gxserver another client started can hold the port by now (CDXC:ServerDaemon 2026-10-25 on gpui_gxserver_already_answers_for_launch); the loop below decides then.
                    let (health, detail) = cx
                        .background_executor()
                        .spawn(async { gpui_probe_local_gxserver_health_with_diagnostics() })
                        .await;
                    startup_diagnostics.push(format!("+{}ms probe after the launcher error: {detail}", startup_started.elapsed().as_millis()));
                    if gpui_gxserver_launch_error_is_final(&health) {
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
                    None
                }
            };
            if let Some(launch_steps) = launch_steps {
                startup_diagnostics.extend(launch_steps);
                startup_diagnostics.push(format!("+{}ms launcher accepted the start request.", startup_started.elapsed().as_millis()));
            }
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
            /*
            CDXC:ServerDaemon 2026-10-12 WHY:
            After the 2026-10-12 restart the port answered for minutes with an older gxserver marked `superseded` (one an SSH-side `gxserver start` had launched from the unrefreshed package). This loop only waited for it to leave, showed the startup diagnostics after its patience although the app was already running on that old build, and never retried. Now another build answering is taken over (asked to stop, ended if it keeps answering, the bundled daemon started) and counts as progress, a port that stays silent gets the bundled daemon started again every 30 s (which takes the port from a stale holder), the diagnostics appear only after the patience passes with no progress, and the loop keeps going behind them and clears them the moment the bundled build answers.
            */
            let spawned_at = std::time::Instant::now();
            let mut progress_at = spawned_at;
            let mut last_launch_at = spawned_at;
            let mut failure_shown = false;
            let mut takeovers = 0u32;
            let mut attempt = 0;
            loop {
                if GPUI_GXSERVER_BOOTSTRAP_GENERATION.load(Ordering::SeqCst) != bootstrap_generation {
                    // A newer bootstrap (Retry, a restart) owns startup now.
                    return;
                }
                attempt += 1;
                let interval = if failure_shown {
                    Duration::from_secs(5)
                } else if progress_at.elapsed() < Duration::from_secs(20) {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(2)
                };
                cx.background_executor().timer(interval).await;
                let (health, detail) = cx
                    .background_executor()
                    .spawn(async { gpui_probe_local_gxserver_health_with_diagnostics() })
                    .await;
                if !failure_shown {
                    startup_diagnostics.push(format!("+{}ms probe {attempt}: {detail}", startup_started.elapsed().as_millis()));
                }
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                if !failure_shown {
                    startup_diagnostics.extend(gpui_take_gxserver_exit_notes(startup_started));
                }
                // The gxserver this attempt started has already stopped with an error: say why now
                // instead of after the start patience (gxserver_start_failure.rs). Ghostex keeps
                // starting it every 30 s below and clears this the moment one answers.
                #[cfg(not(target_os = "windows"))]
                if !failure_shown
                    && !matches!(health, GpuiLocalGxserverHealthState::Healthy { .. })
                    && let Some(failure) = cx
                        .background_executor()
                        .spawn(async { gpui_gxserver_start_failure() })
                        .await
                {
                    failure_shown = true;
                    startup_diagnostics.push(format!(
                        "+{}ms {}",
                        startup_started.elapsed().as_millis(),
                        failure.exit_note
                    ));
                    let report_diagnostics = startup_diagnostics.clone();
                    let report = cx
                        .background_executor()
                        .spawn(async move { gpui_gxserver_startup_failure_report(&report_diagnostics) })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.show_gpui_gxserver_bootstrap_toast(
                            "error",
                            ghostex_gx_core::SERVICE_START_FAILED_TITLE,
                            &failure.reason,
                            true,
                            cx,
                        );
                        if let Some(toast) = this.app_toasts.iter_mut().find(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID) {
                            toast.copy_text = Some(report);
                        }
                        this.sync_gpui_app_toast_window(cx);
                        this.set_gpui_gxserver_start_failure(Some(failure.reason.clone()), cx);
                    });
                    continue;
                }
                // A start that is moving counts as progress until it has run for the startup patience; the launch every 30 s below is what lets a hung one be replaced.
                if let GpuiLocalGxserverHealthState::Starting { starting_for, .. } = &health
                    && *starting_for < GPUI_GXSERVER_START_PATIENCE
                {
                    progress_at = std::time::Instant::now();
                }
                match health {
                    GpuiLocalGxserverHealthState::Healthy { tools_available } => {
                        let _ = this.update(cx, |this, cx| {
                            if failure_shown {
                                this.remove_gpui_app_toast(GPUI_GXSERVER_DAEMON_TOAST_ID, cx);
                            }
                            this.set_gpui_gxserver_start_failure(None, cx);
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
                    GpuiLocalGxserverHealthState::BuildMismatch
                        if last_launch_at.elapsed() >= Duration::from_secs(10)
                            || takeovers == 0 =>
                    {
                        takeovers += 1;
                        let takeover_binary = binary.clone();
                        let steps = cx
                            .background_executor()
                            .spawn(async move { gpui_take_over_local_gxserver_from_other_build(&takeover_binary) })
                            .await;
                        if !failure_shown {
                            startup_diagnostics.extend(steps);
                        }
                        last_launch_at = std::time::Instant::now();
                        // A build that keeps coming back is not progress, so the diagnostics still appear.
                        if takeovers <= 3 {
                            progress_at = last_launch_at;
                        }
                        continue;
                    }
                    GpuiLocalGxserverHealthState::BuildMismatch => {}
                    GpuiLocalGxserverHealthState::Unreachable
                    | GpuiLocalGxserverHealthState::Starting { .. } => {
                        if last_launch_at.elapsed() >= Duration::from_secs(30) {
                            last_launch_at = std::time::Instant::now();
                            let launch_binary = binary.clone();
                            let launched = cx
                                .background_executor()
                                .spawn(async move { gpui_spawn_local_gxserver_daemon(&launch_binary) })
                                .await;
                            if !failure_shown {
                                startup_diagnostics.push(format!(
                                    "+{}ms started the bundled gxserver again: {}",
                                    startup_started.elapsed().as_millis(),
                                    match &launched {
                                        Ok(_) => "accepted".to_string(),
                                        Err(error) => error.clone(),
                                    }
                                ));
                            }
                        }
                    }
                }
                if failure_shown || progress_at.elapsed() < GPUI_GXSERVER_START_PATIENCE {
                    continue;
                }
                failure_shown = true;
                let report_diagnostics = startup_diagnostics.clone();
                let report = cx
                    .background_executor()
                    .spawn(async move { gpui_gxserver_startup_failure_report(&report_diagnostics) })
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
                        "The daemon did not become healthy in time. Ghostex keeps trying; copy diagnostics to share the failure details.",
                        true,
                        cx,
                    );
                    if let Some(toast) = this.app_toasts.iter_mut().find(|toast| toast.id == GPUI_GXSERVER_DAEMON_TOAST_ID) {
                        toast.copy_text = Some(report);
                        // The exit status says why it did not become healthy.
                        #[cfg(target_os = "windows")]
                        if let Some(detail) = gpui_windows_gxserver_failure_toast_detail() {
                            toast.description = Some(detail);
                        }
                    }
                    this.sync_gpui_app_toast_window(cx);
                    #[cfg(not(target_os = "windows"))]
                    this.set_gpui_gxserver_start_failure(
                        Some("It has not answered for three minutes. Ghostex keeps trying; the failure toast can copy the details.".to_string()),
                        cx,
                    );
                });
            }
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
