// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds the support-log maintenance startup task and the Sparkle/Windows/no-op updater check + download + progress + apply methods.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;

impl GhostexGpuiApp {
    /// Startup lifecycle breadcrumb plus the one-minute-delayed support-log
    /// retention pass (macOS `scheduleSupportLogLineRetentionAfterStartup`
    /// parity, scoped to GPUI's own `gpui-*` files).
    pub(crate) fn start_gpui_support_log_maintenance(&mut self, cx: &mut gpui::Context<Self>) {
        support_logs::append(
            support_logs::GpuiSupportLog::HostLifecycle,
            "gpui.host.didFinishLaunching",
            serde_json::json!({ "pid": std::process::id() }),
        );
        let executor = cx.background_executor().clone();
        let timer_executor = executor.clone();
        executor
            .spawn(async move {
                timer_executor
                    .timer(support_logs::RETENTION_STARTUP_DELAY)
                    .await;
                support_logs::prune_gpui_support_logs();
            })
            .detach();
    }

    /// Starts the packaged platform updater. Unpackaged builds simply run
    /// without an updater; packaged macOS keeps Sparkle and packaged Windows
    /// uses the Velopack manifest staged beside the executable.
    #[cfg(target_os = "macos")]
    pub(crate) fn start_gpui_updater(&mut self, cx: &mut gpui::Context<Self>) {
        let start_result = unsafe { GhostexGpuiSparkleUpdaterStart() };
        if start_result == 0 {
            return;
        }
        if start_result != 1 {
            cx.spawn(async move |this, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            id: "gpui-sparkle-updater-start-failed".to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                            title: "Updates unavailable".to_string(),
                            description: Some(
                                "Sparkle could not start the updater for this build.".to_string(),
                            ),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            epoch: 0,
                        },
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        self.updater_started = true;
        // Sparkle intentionally defers its automatic scheduler by one main-loop
        // cycle so clients can initiate their own check immediately after
        // start. Match the legacy AppKit host by using that window for the
        // quiet availability probe; deferring this call lets Sparkle's
        // scheduler win and leaves both manual and titlebar checks blocked by
        // its in-progress session.
        unsafe { GhostexGpuiSparkleProbeForUpdateInformation() };
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_UPDATE_AVAILABILITY_PROBE_INTERVAL)
                    .await;
                let probed = this
                    .update(cx, |this, _cx| {
                        if !this.updater_started {
                            return false;
                        }
                        unsafe { GhostexGpuiSparkleProbeForUpdateInformation() };
                        true
                    })
                    .unwrap_or(false);
                if !probed {
                    return;
                }
            }
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn start_gpui_updater(&mut self, cx: &mut gpui::Context<Self>) {
        let Ok(updater) = windows_updater::WindowsUpdater::new() else {
            // Cargo/dev builds intentionally have no Velopack manifest.
            return;
        };
        self.windows_updater = Some(updater);
        self.updater_started = true;
        self.begin_windows_update_check(false, cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_UPDATE_AVAILABILITY_PROBE_INTERVAL)
                    .await;
                let keep_running = this
                    .update(cx, |this, cx| {
                        if !this.updater_started {
                            return false;
                        }
                        this.begin_windows_update_check(false, cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    return;
                }
            }
        })
        .detach();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(crate) fn start_gpui_updater(&mut self, _cx: &mut gpui::Context<Self>) {}

    /// Titlebar update-button click and app-menu Check for Updates (macOS
    /// `showUpdateDialogFromTitlebar` / `checkForUpdates` parity): the click is
    /// the consent boundary; hand off to Sparkle's standard flow so release
    /// notes, signature validation, download, and install stay on the
    /// supported path. Ignored while a download runs, matching the disabled
    /// React titlebar button.
    // The window stays in the signature for parity with the Windows updater,
    // which opens its own update modal; Sparkle owns its own panels here.
    #[cfg(target_os = "macos")]
    pub(crate) fn check_for_gpui_updates(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.update_downloading {
            return;
        }
        if !self.updater_started {
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: "gpui-sparkle-updater-unavailable".to_string(),
                    level: GpuiAppToastLevel::from_raw(Some("warning")),
                    title: "Updates unavailable".to_string(),
                    description: Some(
                        "This build was packaged without the Sparkle updater.".to_string(),
                    ),
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    epoch: 0,
                },
                cx,
            );
            return;
        }
        unsafe { GhostexGpuiSparkleCheckForUpdates() };
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn check_for_gpui_updates(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.update_downloading {
            return;
        }
        if self.windows_update.is_some() || self.windows_ready_update.is_some() {
            self.open_windows_update_modal(Some(window), cx);
            return;
        }
        if !self.updater_started {
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: "gpui-windows-updater-unavailable".to_string(),
                    level: GpuiAppToastLevel::from_raw(Some("warning")),
                    title: "Updates unavailable".to_string(),
                    description: Some(
                        "This build was not installed or packaged with Velopack.".to_string(),
                    ),
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    epoch: 0,
                },
                cx,
            );
            return;
        }
        self.begin_windows_update_check(true, cx);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub(crate) fn check_for_gpui_updates(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) {}

    pub(crate) fn set_gpui_update_available(&mut self, available: bool, cx: &mut gpui::Context<Self>) {
        if self.update_available == available {
            return;
        }
        self.update_available = available;
        cx.notify();
    }

    pub(crate) fn set_gpui_update_downloading(&mut self, downloading: bool, cx: &mut gpui::Context<Self>) {
        let progress_cleared = !downloading && self.update_download_progress.is_some();
        if progress_cleared {
            self.update_download_progress = None;
        }
        if self.update_downloading == downloading && !progress_cleared {
            return;
        }
        self.update_downloading = downloading;
        cx.notify();
    }

    pub(crate) fn set_gpui_update_download_progress(
        &mut self,
        progress: Option<f64>,
        cx: &mut gpui::Context<Self>,
    ) {
        let normalized =
            progress.and_then(|value| value.is_finite().then(|| value.clamp(0.0, 1.0)));
        if self.update_download_progress == normalized {
            return;
        }
        self.update_download_progress = normalized;
        if self.update_downloading {
            cx.notify();
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn begin_windows_update_check(&mut self, interactive: bool, cx: &mut gpui::Context<Self>) {
        if self.update_checking || self.update_downloading {
            return;
        }
        let Some(updater) = self.windows_updater.clone() else {
            return;
        };
        self.update_checking = true;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    updater
                        .check_for_updates()
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.update_checking = false;
                match result {
                    Ok(windows_updater::WindowsUpdateCheck::UpdateAvailable(update)) => {
                        let ready_update = this
                            .windows_updater
                            .as_ref()
                            .and_then(windows_updater::WindowsUpdater::pending_restart)
                            .filter(|asset| asset.Version == update.version());
                        this.windows_update = Some(update);
                        this.windows_ready_update = ready_update;
                        this.set_gpui_update_available(true, cx);
                        if interactive {
                            this.open_windows_update_modal(Some(window), cx);
                        }
                    }
                    Ok(windows_updater::WindowsUpdateCheck::NoUpdateAvailable) => {
                        this.windows_update = None;
                        this.windows_ready_update = None;
                        this.set_gpui_update_available(false, cx);
                        if interactive {
                            this.upsert_gpui_app_toast(
                                GpuiAppToast {
                                    id: "gpui-windows-updater-current".to_string(),
                                    level: GpuiAppToastLevel::from_raw(Some("success")),
                                    title: "Ghostex is up to date".to_string(),
                                    description: None,
                                    loading: false,
                                    persistent: false,
                                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                                    epoch: 0,
                                },
                                cx,
                            );
                        }
                    }
                    Err(_) if !interactive => {}
                    Err(_) => {
                        this.upsert_gpui_app_toast(
                            GpuiAppToast {
                                id: "gpui-windows-updater-check-failed".to_string(),
                                level: GpuiAppToastLevel::from_raw(Some("warning")),
                                title: "Could not check for updates".to_string(),
                                description: Some(
                                    "Check your connection and try again.".to_string(),
                                ),
                                loading: false,
                                persistent: false,
                                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                                epoch: 0,
                            },
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn open_windows_update_modal(
        &mut self,
        source_window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(update) = self.windows_update.as_ref() else {
            return;
        };
        let ready = self.windows_ready_update.is_some();
        let portable = self
            .windows_updater
            .as_ref()
            .is_some_and(windows_updater::WindowsUpdater::is_portable);
        let open_message = serde_json::json!({
            "modal": GpuiAppModalKind::UpdateAvailable.modal_id(),
            "notesMarkdown": update.notes_markdown(),
            "portable": portable,
            "state": if ready { "ready" } else { "available" },
            "type": "open",
            "version": update.version(),
        });
        self.open_gpui_app_modal_window(
            GpuiAppModalKind::UpdateAvailable,
            open_message,
            serde_json::Value::Null,
            source_window,
            cx,
        );
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn download_windows_update(&mut self, cx: &mut gpui::Context<Self>) {
        if self.update_downloading || self.windows_ready_update.is_some() {
            return;
        }
        let (Some(updater), Some(update)) =
            (self.windows_updater.clone(), self.windows_update.clone())
        else {
            return;
        };
        self.set_gpui_update_downloading(true, cx);
        self.set_gpui_update_download_progress(Some(0.0), cx);

        let (progress_sender, progress_receiver) = std::sync::mpsc::channel::<i16>();
        let (result_sender, result_receiver) = std::sync::mpsc::channel();
        cx.background_executor()
            .spawn(async move {
                let result = updater
                    .download(&update, progress_sender)
                    .map_err(|error| error.to_string());
                let _ = result_sender.send(result);
            })
            .detach();

        cx.spawn(async move |this, cx| {
            loop {
                let mut latest_progress = None;
                for progress in progress_receiver.try_iter() {
                    latest_progress = Some(f64::from(progress) / 100.0);
                }
                if let Some(progress) = latest_progress {
                    let _ = this.update(cx, |this, cx| {
                        this.set_gpui_update_download_progress(Some(progress), cx);
                    });
                }
                match result_receiver.try_recv() {
                    Ok(result) => {
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.set_gpui_update_downloading(false, cx);
                            match result {
                                Ok(asset) => {
                                    this.windows_ready_update = Some(asset);
                                    this.set_gpui_update_available(true, cx);
                                    this.open_windows_update_modal(Some(window), cx);
                                }
                                Err(_) => {
                                    this.upsert_gpui_app_toast(
                                        GpuiAppToast {
                                            id: "gpui-windows-updater-download-failed".to_string(),
                                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                                            title: "Update download failed".to_string(),
                                            description: Some(
                                                "The update was not installed. Try again from the titlebar."
                                                    .to_string(),
                                            ),
                                            loading: false,
                                            persistent: false,
                                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                                            epoch: 0,
                                        },
                                        cx,
                                    );
                                }
                            }
                        });
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
            }
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn restart_and_apply_windows_update(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(updater), Some(asset)) = (
            self.windows_updater.as_ref(),
            self.windows_ready_update.as_ref(),
        ) else {
            return;
        };
        if updater.apply_after_exit(asset).is_err() {
            self.upsert_gpui_app_toast(
                GpuiAppToast {
                    id: "gpui-windows-updater-apply-failed".to_string(),
                    level: GpuiAppToastLevel::from_raw(Some("warning")),
                    title: "Could not start the updater".to_string(),
                    description: Some("Ghostex was not changed. Try again.".to_string()),
                    loading: false,
                    persistent: false,
                    duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                    epoch: 0,
                },
                cx,
            );
            return;
        }
        GPUI_APP_QUIT_IN_PROGRESS.store(true, Ordering::Release);
        cx.quit();
    }

}
