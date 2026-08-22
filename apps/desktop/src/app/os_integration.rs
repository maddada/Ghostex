// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

use std::collections::HashSet;
use std::fs;
use std::time::Duration;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::AppContext as _;
use gpui::Bounds;
use gpui::Pixels;
use gpui::Window;
use gpui::WindowBackgroundAppearance;
use gpui::WindowBounds;
use gpui::WindowKind;
use gpui::WindowOptions;
use gpui::point;
use gpui::px;
use gpui::size;
use gpui_component::WindowExt;
use gpui_component::native_menu::NativeMenu;
use gpui_component::notification::Notification;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;
#[cfg(target_os = "windows")]
use futures::channel::mpsc;
#[cfg(target_os = "windows")]
use gpui::AnyElement;
#[cfg(target_os = "windows")]
use gpui::FontWeight;
#[cfg(target_os = "windows")]
use gpui::MouseButton;
#[cfg(target_os = "windows")]
use gpui::MouseUpEvent;
#[cfg(target_os = "windows")]
use gpui::div;
#[cfg(target_os = "windows")]
use gpui::relative;
#[cfg(target_os = "windows")]
use gpui::rgba;
#[cfg(target_os = "windows")]
use gpui_component::h_flex;
#[cfg(target_os = "windows")]
use gpui_component::v_flex;
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

    /// GPUI port of the macOS sidebar-mount first-run block
    /// (`openTipsAndTricksOnFirstLaunch` + `showOSIntegrationOnboardingOnFirstLaunch`,
    /// native-sidebar.tsx): current shipped macOS consumes the legacy tips and
    /// Highlighted Features markers WITHOUT opening those surfaces (the
    /// Discover→firstLaunchSetup auto chain was superseded 2026-06-18), opens
    /// the tutorial video modal once per first-launch-setup revision, and shows
    /// a once-forever OS Integration toast.
    /// CDXC:GPUIFirstRunOnboardingOnce 2026-08-18: writes one first-run marker
    /// off the foreground thread, re-reading the file first so a concurrent
    /// writer of a different field is not clobbered by this pass's snapshot.
    pub(crate) fn persist_gpui_first_run_onboarding_marker(
        &self,
        marker: GpuiFirstRunOnboardingMarker,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.background_executor()
            .spawn(async move {
                let mut state = load_gpui_first_run_onboarding_state();
                match marker {
                    GpuiFirstRunOnboardingMarker::FirstLaunchSetupSeen => {
                        if state.first_launch_setup_seen_revision.as_deref()
                            == Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION)
                        {
                            return;
                        }
                        state.first_launch_setup_seen_revision =
                            Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION.to_string());
                    }
                    GpuiFirstRunOnboardingMarker::OsIntegrationOnboardingSeen => {
                        if state.os_integration_onboarding_seen {
                            return;
                        }
                        state.os_integration_onboarding_seen = true;
                    }
                }
                persist_gpui_first_run_onboarding_state(&state);
            })
            .detach();
    }

    pub(crate) fn start_gpui_first_run_onboarding(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar.is_none() {
            return;
        }
        #[cfg(target_os = "windows")]
        if self.windows_first_run_setup_state != GpuiWindowsFirstRunSetupState::Ready {
            return;
        }
        /*
        CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
        The first-launch-setup and OS-integration markers are persisted only
        after their surface is really shown (below), so the persisted flags no
        longer dedupe the callers that start onboarding. Latch the pass in
        memory instead, after every early-return guard, so the gxserver
        bootstrap, CEF init, and daemon-respawn entry points can each try to
        start onboarding while only the first one that gets past the guards
        runs it.
        */
        if self.first_run_onboarding_started {
            return;
        }
        self.first_run_onboarding_started = true;
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (show_os_integration_toast, first_launch_setup_sidebar_state) = background
                .spawn(async move {
                    let mut state = load_gpui_first_run_onboarding_state();
                    let mut changed = false;
                    if !state.tips_and_tricks_seen {
                        state.tips_and_tricks_seen = true;
                        changed = true;
                    }
                    if state.highlighted_features_seen_revision.as_deref()
                        != Some(GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION)
                    {
                        state.highlighted_features_seen_revision =
                            Some(GPUI_HIGHLIGHTED_FEATURES_SEEN_REVISION.to_string());
                        changed = true;
                    }
                    /*
                    CDXC:GPUIFirstRunOnboardingOnce 2026-08-18:
                    The first-launch-setup and OS-integration markers describe
                    surfaces the user is supposed to see, so they are persisted
                    from the foreground block below once the toast is really
                    posted and the modal window really exists. Marking them here
                    burned first-run onboarding whenever the pass reached this
                    point and the UI never appeared (no sidebar-owned window,
                    a failed modal open, or an app exit in between). The two
                    legacy markers above show nothing at all, so they stay
                    consumed on this pass.

                    CDXC:GPUIFirstLaunchTutorialVideo 2026-08-19:
                    First run raises exactly ONE modal. The tutorial video used
                    to be its own window opened before (Windows) or instead of
                    (everywhere else) the setup modal; it is the setup modal's
                    first page now. Two markers still gate it, and they mean
                    different things: the revision marker says "this revision's
                    setup was presented", written when the window exists, so a
                    future revision bump shows it again; `complete` says "the
                    user closed it", written on close, so quitting mid-setup
                    brings it back on the next launch.
                    */
                    let show_os_integration_toast = !state.os_integration_onboarding_seen;
                    if changed {
                        persist_gpui_first_run_onboarding_state(&state);
                    }
                    let needs_first_launch_setup = state.first_launch_setup_seen_revision.as_deref()
                        != Some(GPUI_FIRST_LAUNCH_SETUP_SEEN_REVISION)
                        || !state.first_launch_setup_complete;
                    let first_launch_setup_sidebar_state = needs_first_launch_setup.then(|| {
                        gpui_app_modal_sidebar_state_message_for_active_project_id(
                            active_project_id.as_deref(),
                        )
                    });
                    (show_os_integration_toast, first_launch_setup_sidebar_state)
                })
                .await;
            if !show_os_integration_toast && first_launch_setup_sidebar_state.is_none() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                if show_os_integration_toast {
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            id: "gpui-os-integration-onboarding".to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("info")),
                            title: "OS Integration available".to_string(),
                            description: Some(
                                "Open Settings > OS Integration to set Ghostex as your editor or terminal target."
                                    .to_string(),
                            ),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            epoch: 0,
                        },
                        cx,
                    );
                    this.persist_gpui_first_run_onboarding_marker(
                        GpuiFirstRunOnboardingMarker::OsIntegrationOnboardingSeen,
                        cx,
                    );
                }
                if let Some(base_sidebar_state) = first_launch_setup_sidebar_state {
                    this.open_gpui_first_launch_setup_with_sidebar_state(base_sidebar_state, cx);
                    // Only a window that actually exists counts as "this
                    // revision's setup was presented".
                    if this.app_modal_window.is_some() {
                        this.persist_gpui_first_run_onboarding_marker(
                            GpuiFirstRunOnboardingMarker::FirstLaunchSetupSeen,
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn retry_windows_first_run_setup(&mut self, cx: &mut gpui::Context<Self>) {
        windows_terminal_backend::reset();
        self.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Checking;
        self.start_gpui_local_gxserver_bootstrap(cx);
        cx.notify();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn save_windows_first_run_distribution(
        &mut self,
        distribution: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut settings = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings.insert(
            "windowsWslDistribution".to_string(),
            serde_json::Value::String(distribution),
        );
        match shared_settings::write_shared_sidebar_settings_object(settings) {
            Ok(_) => self.retry_windows_first_run_setup(cx),
            Err(_) => {
                self.windows_first_run_setup_state = GpuiWindowsFirstRunSetupState::Failed(
                    "Ghostex could not save the selected Linux environment. Check that its settings folder is writable, then try again."
                        .to_string(),
                );
                cx.notify();
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn handle_windows_first_run_setup_action(
        &mut self,
        action: GpuiWindowsFirstRunSetupAction,
        cx: &mut gpui::Context<Self>,
    ) {
        match action {
            GpuiWindowsFirstRunSetupAction::Retry => self.retry_windows_first_run_setup(cx),
            GpuiWindowsFirstRunSetupAction::OpenWslGuide => {
                let _ =
                    gpui_open_external_http_url("https://learn.microsoft.com/windows/wsl/install");
            }
            GpuiWindowsFirstRunSetupAction::ChooseDistribution(distribution) => {
                self.save_windows_first_run_distribution(distribution, cx);
            }
            GpuiWindowsFirstRunSetupAction::ClearDistribution => {
                self.save_windows_first_run_distribution(String::new(), cx);
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn render_windows_first_run_setup_button(
        &self,
        label: impl Into<gpui::SharedString>,
        primary: bool,
        action: GpuiWindowsFirstRunSetupAction,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .h(px(36.0))
            .px(px(16.0))
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .border_1()
            .border_color(if primary {
                rgba(0x5a9bf0ff)
            } else {
                rgba(0xffffff2b)
            })
            .bg(if primary {
                rgba(0x377bd8ff)
            } else {
                rgba(0xffffff0d)
            })
            .hover(|this| {
                this.bg(if primary {
                    rgba(0x4389e7ff)
                } else {
                    rgba(0xffffff17)
                })
            })
            .cursor_pointer()
            .text_size(px(13.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(0xffffffff))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.handle_windows_first_run_setup_action(action.clone(), cx);
                }),
            )
            .child(label.into())
            .into_any_element()
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn render_windows_first_run_setup(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let state = self.windows_first_run_setup_state.clone();
        let (title, description, detail, progress) = match &state {
            GpuiWindowsFirstRunSetupState::Checking => (
                "Checking Windows",
                "Ghostex uses WSL2 for fast terminals, agents, and persistent sessions.",
                "Looking for an initialized Linux environment…".to_string(),
                0.12,
            ),
            GpuiWindowsFirstRunSetupState::MissingWsl => (
                "Ghostex requires WSL2",
                "Install Windows Subsystem for Linux, then return here. Ghostex will never install it or request administrator access without you choosing to do so.",
                "WSL is not available on this PC.".to_string(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::MissingDistribution => (
                "Finish setting up Linux",
                "Install a Linux distribution such as Ubuntu and open it once to create your Linux account. Then Ghostex can finish terminal setup.",
                "No initialized WSL2 Linux environment was found.".to_string(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::ChooseDistribution(_) => (
                "Choose your Linux environment",
                "Ghostex found more than one initialized WSL2 distribution. Choose where terminals, agents, and sessions should run.",
                "You can change this later in Settings → Terminal.".to_string(),
                0.18,
            ),
            GpuiWindowsFirstRunSetupState::ConfiguredDistributionUnavailable(distribution) => (
                "Your Linux environment is unavailable",
                "The WSL distribution saved in Ghostex was removed, renamed, or has not finished initialization.",
                format!("Saved environment: {distribution}"),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::SettingUp(phase) => {
                let (detail, progress) = match phase {
                    windows_terminal_backend::WindowsWslSetupPhase::Checking => {
                        ("Checking the bundled terminal engine…", 0.28)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Installing => {
                        ("Installing the Ghostex terminal engine in WSL2…", 0.52)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Starting => {
                        ("Starting the Ghostex terminal engine…", 0.74)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Connecting => {
                        ("Connecting Ghostex to WSL2…", 0.9)
                    }
                    windows_terminal_backend::WindowsWslSetupPhase::Ready => {
                        ("Ghostex terminals are ready.", 1.0)
                    }
                };
                (
                    "Setting up Ghostex terminals",
                    "This is a one-time setup for the selected Linux environment.",
                    detail.to_string(),
                    progress,
                )
            }
            GpuiWindowsFirstRunSetupState::Failed(message) => (
                "Terminal setup didn’t finish",
                "Ghostex could not finish its WSL2 terminal setup. Nothing was removed or replaced outside Ghostex’s managed files.",
                message.clone(),
                0.0,
            ),
            GpuiWindowsFirstRunSetupState::Ready => unreachable!(),
        };

        let mut content = v_flex()
            .w(px(620.0))
            .p(px(32.0))
            .gap(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(0xffffff1f))
            .bg(rgba(0x17191dff))
            .child(
                div()
                    .text_size(px(24.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(0xfffffff2))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(21.0))
                    .text_color(rgba(0xffffffad))
                    .child(description),
            )
            .child(
                div()
                    .w_full()
                    .h(px(7.0))
                    .rounded(px(4.0))
                    .bg(rgba(0xffffff12))
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress))
                            .rounded(px(4.0))
                            .bg(rgba(0x4f8fe8ff)),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(
                        if matches!(state, GpuiWindowsFirstRunSetupState::Failed(_)) {
                            rgba(0xffa2aaff)
                        } else {
                            rgba(0xffffff85)
                        },
                    )
                    .child(detail),
            );

        if let GpuiWindowsFirstRunSetupState::ChooseDistribution(distributions) = &state {
            content = content.child(v_flex().gap(px(8.0)).children(
                distributions.iter().cloned().map(|distribution| {
                    self.render_windows_first_run_setup_button(
                        distribution.clone(),
                        true,
                        GpuiWindowsFirstRunSetupAction::ChooseDistribution(distribution),
                        cx,
                    )
                }),
            ));
        }

        let actions = match state {
            GpuiWindowsFirstRunSetupState::MissingWsl
            | GpuiWindowsFirstRunSetupState::MissingDistribution => Some(
                h_flex()
                    .gap(px(10.0))
                    .child(self.render_windows_first_run_setup_button(
                        "View WSL setup guide",
                        false,
                        GpuiWindowsFirstRunSetupAction::OpenWslGuide,
                        cx,
                    ))
                    .child(self.render_windows_first_run_setup_button(
                        "Check again",
                        true,
                        GpuiWindowsFirstRunSetupAction::Retry,
                        cx,
                    )),
            ),
            GpuiWindowsFirstRunSetupState::ConfiguredDistributionUnavailable(_) => Some(
                h_flex()
                    .gap(px(10.0))
                    .child(self.render_windows_first_run_setup_button(
                        "Choose automatically",
                        false,
                        GpuiWindowsFirstRunSetupAction::ClearDistribution,
                        cx,
                    ))
                    .child(self.render_windows_first_run_setup_button(
                        "Check again",
                        true,
                        GpuiWindowsFirstRunSetupAction::Retry,
                        cx,
                    )),
            ),
            GpuiWindowsFirstRunSetupState::Failed(_) => {
                Some(h_flex().child(self.render_windows_first_run_setup_button(
                    "Try again",
                    true,
                    GpuiWindowsFirstRunSetupAction::Retry,
                    cx,
                )))
            }
            _ => None,
        };
        if let Some(actions) = actions {
            content = content.child(actions);
        }

        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x0d0f12ff))
            .child(content)
            .into_any_element()
    }

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

    pub(crate) fn start_gpui_workspace_open_target_availability_scan(&mut self, cx: &mut gpui::Context<Self>) {
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

    pub(crate) fn schedule_gpui_app_toast_auto_dismiss(
        &mut self,
        toast_id: String,
        epoch: u64,
        duration_ms: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(duration_ms))
                .await;
            let _ = this.update(cx, |this, cx| {
                // The epoch guard keeps an update-in-place (same toastId) from
                // being dismissed by the superseded toast's timer.
                if this
                    .app_toasts
                    .iter()
                    .any(|toast| toast.id == toast_id && toast.epoch == epoch)
                {
                    this.remove_gpui_app_toast(&toast_id, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn remove_gpui_app_toast(&mut self, toast_id: &str, cx: &mut gpui::Context<Self>) {
        let previous_len = self.app_toasts.len();
        self.app_toasts.retain(|toast| toast.id != toast_id);
        if self.app_toasts.len() != previous_len {
            self.sync_gpui_app_toast_window(cx);
        }
    }

    pub(crate) fn sync_gpui_app_toast_window(&mut self, cx: &mut gpui::Context<Self>) {
        if self.app_toasts.is_empty() {
            self.app_toast_window_height = px(0.0);
            if let Some(handle) = self.app_toast_window.take() {
                let _ = handle.update(cx, |_, toast_window, _| {
                    toast_window.remove_window();
                });
            }
            return;
        }
        let Some(anchor) = self.app_toast_anchor else {
            return;
        };
        let stack_height = px(gpui_app_toast_stack_height(&self.app_toasts));
        let toasts = self.app_toasts.clone();
        if let Some(handle) = self.app_toast_window.clone() {
            if self.app_toast_window_height == stack_height {
                let update_result = handle.update(cx, |toast_window_entity, toast_window, cx| {
                    toast_window_entity.set_toasts(toasts.clone(), cx);
                    toast_window.refresh();
                });
                if update_result.is_ok() {
                    return;
                }
                self.app_toast_window = None;
            } else {
                // The popup is exact-sized and bottom-anchored; gpui windows can
                // resize but not move, so a height change recreates the window at
                // the new bottom-anchored bounds.
                let _ = handle.update(cx, |_, toast_window, _| {
                    toast_window.remove_window();
                });
                self.app_toast_window = None;
            }
        }
        let bounds = Bounds {
            origin: point(
                anchor.x - px(GPUI_APP_TOAST_WIDTH / 2.0),
                anchor.y - stack_height - px(GPUI_APP_TOAST_BOTTOM_MARGIN),
            ),
            size: size(px(GPUI_APP_TOAST_WINDOW_WIDTH), stack_height),
        };
        let app = cx.entity().downgrade();
        let main_window_native_view = self.parent_ns_view;
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        self.app_toast_window = cx
            .open_window(options, move |toast_window, cx| {
                remove_gpui_app_toast_popup_window_chrome(toast_window, main_window_native_view);
                cx.new(|_| GpuiAppToastWindow {
                    app,
                    toasts,
                    hovered_toast_id: None,
                })
            })
            .ok();
        self.app_toast_window_height = stack_height;
    }

    pub(crate) fn dispatch_gpui_sidebar_remote_event(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachines 2026-06-24-16:48:
        GPUI remote-machine status and presentation events are Rust-owned sidebar-only messages. Send only sanitized state enums, machine ids, and gxserver presentation snapshots/deltas into SidebarApp; remote auth tokens, SSH host/user/key data, paths from Settings commands, URLs, daemon response bodies, stdout/stderr, and command text must stay out of CEF globals and logs.

        CDXC:GPUIRemoteSessions 2026-06-24-17:19:
        Response-capable remote session requests may dispatch only request ids, success state, generic errors, and explicit safe metadata results such as previous-session search rows. Mutating remote session responses must be sanitized before this event boundary so renderer code never receives launch commands, tokens, SSH details, raw daemon bodies, or provider internals.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let script = format!(
            "window.dispatchEvent(new CustomEvent('{GPUI_SIDEBAR_REMOTE_EVENT_NAME}', {{ detail: {} }})); undefined;",
            message
        );
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    pub(crate) fn dispatch_gpui_remote_machine_status(
        &mut self,
        remote_machine_id: &str,
        state: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_remote_machine_status_with_message(remote_machine_id, state, None, cx);
    }

    /*
    CDXC:GPUIRemoteConnectFeedback 2026-07-12:
    Failure states may carry the already-sanitized connect failure summary
    (the same text shown in the toast) so the sidebar can explain why the
    connect failed inline. Only Rust-authored sanitized messages may pass
    through here — never raw SSH stderr, tokens, hosts, or daemon bodies.
    */
    pub(crate) fn dispatch_gpui_remote_machine_status_with_message(
        &mut self,
        remote_machine_id: &str,
        state: &str,
        message: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        debug_assert!(gpui_remote_gxserver_status_state_is_known(state));
        if gpui_remote_gxserver_status_state_is_broken(state) {
            self.stop_gpui_remote_gxserver_connection(remote_machine_id);
        }
        /*
        CDXC:GPUIRemoteConnectOverlay 2026-08-07:
        Every connect transition funnels through here, so this is where the
        render tree learns a machine's reachability. The terminal body draws a
        status overlay for remote sessions whose machine is not connected, and
        the workspace re-arms restored remote tabs on the connected edge. Only
        the bounded wire state slug is retained — never hosts, tokens, or the
        sanitized message text.
        */
        self.remote_machine_connect_states
            .insert(remote_machine_id.to_string(), state.to_string());
        if state == GpuiRemoteGxserverConnectState::Connected.wire_status_state() {
            self.attach_surfaced_remote_workspace_terminals(remote_machine_id, cx);
        }
        let mut payload = serde_json::json!({
            "machineId": remote_machine_id,
            "state": state,
            "type": "remoteMachineStatus",
        });
        if let Some(message) = message.map(str::trim).filter(|message| !message.is_empty()) {
            payload["message"] = serde_json::Value::String(message.to_string());
        }
        self.dispatch_gpui_sidebar_remote_event(payload, cx);
    }

    pub(crate) fn dispatch_gpui_settings_action_status(
        &mut self,
        action: &str,
        available: bool,
        message: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            gpui_settings_action_status_message(action, available, message),
            cx,
        );
    }

    pub(crate) fn run_gpui_app_modal_sidebar_status_task<F>(&mut self, task: F, cx: &mut gpui::Context<Self>)
    where
        F: FnOnce() -> serde_json::Value + Send + 'static,
    {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload = background.spawn(async move { task() }).await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload, cx);
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_app_modal_and_titlebar_status_task<F>(
        &mut self,
        task: F,
        cx: &mut gpui::Context<Self>,
    ) where
        F: FnOnce() -> serde_json::Value + Send + 'static,
    {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload = background.spawn(async move { task() }).await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload.clone(), cx);
                this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&payload, cx);
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_progressive_agent_hook_status_task(
        &mut self,
        requested_agent_ids: Option<Vec<String>>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        macOS parity: hook-status probes run per provider in priority order
        (codex, claude, opencode, pi first) and every partial result posts
        immediately, so Settings/Tips hook warnings populate progressively
        instead of waiting behind the slowest provider probe. Install and
        uninstall stay single batched calls like macOS. One status request
        runs at a time; overlapping requests drop like the macOS in-flight
        guard, and a failed provider probe posts the error payload and stops
        the walk like the macOS catch path.
        */
        if self.agent_hook_status_request_in_flight {
            return;
        }
        self.agent_hook_status_request_in_flight = true;
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let mut merged: Option<serde_json::Value> = None;
            for agent_id in gpui_ordered_agent_hook_status_agent_ids(requested_agent_ids) {
                let payload = background
                    .spawn(async move {
                        gpui_agent_hook_status_message(
                            "/api/readAgentHookStatus",
                            Some(HashSet::from([agent_id])),
                            "Unable to inspect agent hook status.",
                        )
                    })
                    .await;
                let is_error = payload
                    .get("errorMessage")
                    .and_then(serde_json::Value::as_str)
                    .is_some();
                let next = if is_error {
                    payload
                } else {
                    match merged.as_ref() {
                        Some(previous) => gpui_merge_agent_hook_status_messages(previous, &payload),
                        None => payload,
                    }
                };
                let dispatched = this.update(cx, |this, cx| {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(next.clone(), cx);
                    this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&next, cx);
                });
                if dispatched.is_err() {
                    return;
                }
                if is_error {
                    break;
                }
                merged = Some(next);
            }
            let _ = this.update(cx, |this, _| {
                this.agent_hook_status_request_in_flight = false;
            });
        })
        .detach();
    }

    pub(crate) fn run_gpui_ghostex_cli_settings_action(
        &mut self,
        action: GpuiGhostexCliSettingsAction,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsCliInstall 2026-06-24-12:56:
        Settings integration actions in GPUI must mutate only bounded local install targets and then refresh the shared `ghostexCliStatus` contract. CLI repair uses packaged app resources only; skill install/uninstall actions use fixed Ghostex-owned command or directory targets and never accept React-provided shell text, paths, stdout, stderr, URLs, tokens, or environment data.
        */
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            // The status message probe runs `cua-driver check_permissions`
            // (5s timeout plus an unbounded pipe-reader join) — it must run
            // in the same background task as the action, never inside the
            // main-thread completion update (CDXC:GPUISettingsCliInstall
            // 2026-07-11: previously stalled the UI up to 5s per action).
            let (result, status_message) = background
                .spawn(async move {
                    let result = gpui_run_ghostex_cli_settings_action(action);
                    let status_message =
                        gpui_ghostex_cli_status_message(Some(result.message.as_str()));
                    (result, status_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(status_message, cx);
                this.dispatch_gpui_settings_action_status(
                    result.action_id,
                    result.available,
                    result.message.as_str(),
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    result.toast_level,
                    result.toast_title,
                    result.message.as_str(),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_cua_driver_install_or_update(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        {
            self.start_gpui_cua_driver_install_or_update_terminal(window, cx);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let result = gpui_open_url(GPUI_CUA_DRIVER_RELEASES_URL);
            let (toast_level, toast_title, detail) = if result.is_ok() {
                (
                    "info",
                    "Cua Driver downloads opened",
                    "Cua Driver installation and updates are available from the Cua GitHub releases page on this platform.",
                )
            } else {
                (
                    "warning",
                    "Unable to open Cua downloads",
                    "Ghostex could not open the Cua GitHub releases page.",
                )
            };
            self.run_gpui_app_modal_and_titlebar_status_task(
                move || gpui_ghostex_cli_status_message(Some(detail)),
                cx,
            );
            self.dispatch_gpui_app_modal_toast(toast_level, toast_title, detail, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn start_gpui_cua_driver_install_or_update_terminal(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIDesktopControlSettings 2026-08-09:
        Installing or updating Cua Driver can take minutes and prints useful
        progress. Run it in a real command-pane terminal tab so the user can
        watch the official installer/updater instead of staring at a silent
        Settings spinner. The tab opens without stealing typing focus like
        every other command Action.
        */
        let GpuiCuaDriverCommandAction {
            command,
            command_id,
            running_message,
            tab_title,
            toast_title,
        } = gpui_cua_driver_command_action();
        self.open_gpui_command_action_terminal(
            command_id.to_string(),
            tab_title.to_string(),
            command,
            false,
            false,
            window,
            cx,
        );
        self.run_gpui_app_modal_and_titlebar_status_task(
            move || gpui_ghostex_cli_status_message(Some(running_message)),
            cx,
        );
        self.dispatch_gpui_app_modal_toast("info", toast_title, running_message, cx);
    }

    pub(crate) fn install_gpui_gte_from_homebrew(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_install_gte_from_homebrew() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_gpui_settings_action_status(
                    GPUI_GTE_INSTALL_ACTION_ID,
                    result.available,
                    result.message,
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    result.toast_level,
                    result.toast_title,
                    result.message,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn refresh_gpui_daemon_sessions_state_in_background(
        &mut self,
        error_message: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_daemon_sessions_active_project_id();
        let focused_session_id = self.gpui_daemon_sessions_focused_session_id();
        self.run_gpui_app_modal_sidebar_status_task(
            move || {
                gpui_daemon_sessions_state_message(
                    error_message,
                    active_project_id.as_deref(),
                    focused_session_id.as_deref(),
                )
            },
            cx,
        );
    }

    pub(crate) fn refresh_gpui_quick_access_sessions_state_in_background(
        &mut self,
        mut sidebar_state_message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIQuickAccessSessions 2026-08-08:
        The reusable app-modal hydrate is settings-oriented and intentionally
        carries no session groups. Read the authoritative gxserver presentation
        off the UI thread when the Sessions page opens, project it into the
        existing SidebarSessionGroup contract, and apply it as a normal
        sessionState message. Closed-session paging remains independent, so the
        modal can display whichever real dataset arrives first without a fake
        loading row or a second list area.
        */
        let active_project_id = self.gpui_daemon_sessions_active_project_id();
        self.run_gpui_app_modal_sidebar_status_task(
            move || {
                let groups = gpui_read_gxserver_presentation_snapshot()
                    .map(|snapshot| {
                        gpui_quick_access_sidebar_groups_from_presentation_snapshot(
                            &snapshot,
                            active_project_id.as_deref(),
                        )
                    })
                    .unwrap_or_default();
                sidebar_state_message["groups"] = serde_json::Value::Array(groups);
                sidebar_state_message["type"] =
                    serde_json::Value::String("sessionState".to_string());
                sidebar_state_message
            },
            cx,
        );
    }

    pub(crate) fn gpui_daemon_sessions_active_project_id(&self) -> Option<String> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|project_id| project_id.0.clone())
    }

    pub(crate) fn gpui_daemon_sessions_focused_session_id(&self) -> Option<String> {
        self.sidebar_gxserver_presentation_focus_state
            .focused_session_id
            .clone()
    }

    pub(crate) fn gpui_app_modal_active_project_id(&self) -> Option<String> {
        gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
            .map(str::to_string)
    }

    pub(crate) fn gpui_titlebar_browser_url_allowed(&self, url: &str) -> bool {
        if self.titlebar_tips_panel_open && gpui_titlebar_tips_browser_url_allowed(url) {
            return true;
        }
        self.titlebar_resources_panel_open && gpui_titlebar_resources_browser_url_allowed(url)
    }

    pub(crate) fn update_gpui_ghostty_visible_settings(
        &mut self,
        mutate_settings: fn(&mut serde_json::Map<String, serde_json::Value>),
        write_ghostty_config: fn() -> Result<
            shared_settings::SharedGhosttyConfigFileWriteStatus,
            shared_settings::SharedGhosttyConfigFileError,
        >,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsGhosttyConfig 2026-06-24-12:24:
        Apply/reset Ghostty actions must mirror macOS by updating the visible shared Settings keys and merging the bounded managed Ghostty config file. They may not accept a React-provided config path, create a fallback file after failure, or claim live embedded reload because GPUI has no safe Ghostty app config reload/update FFI yet.
        */
        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        mutate_settings(&mut settings_object);
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            self.dispatch_gpui_settings_action_status(
                "ghosttySettings",
                false,
                "GPUI could not update shared Ghostty Settings.",
                cx,
            );
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        match write_ghostty_config() {
            Ok(_) => {
                self.dispatch_gpui_settings_action_status(
                    "ghosttySettings",
                    true,
                    "Shared Ghostty Settings and the managed Ghostty config file were saved. Existing GPUI terminals are not live reloaded; changes affect external Ghostty reloads and future/recreated GPUI surfaces.",
                    cx,
                );
            }
            Err(_) => {
                let message = "Shared Ghostty Settings were saved, but GPUI could not write the managed Ghostty config file. Existing embedded terminals were not live reloaded.";
                self.dispatch_gpui_settings_action_status("ghosttySettings", false, message, cx);
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not update Ghostty config",
                    message,
                    cx,
                );
            }
        }
    }

    pub(crate) fn open_gpui_ghostex_folder(&mut self, cx: &mut gpui::Context<Self>) {
        let folder_path = shared_settings::ghostex_storage_paths().data_dir.clone();
        let open_result = fs::create_dir_all(&folder_path)
            .map_err(|_| "GPUI could not prepare the Ghostex support folder.".to_string())
            .and_then(|_| gpui_open_path(&folder_path));
        if let Err(message) = open_result {
            self.dispatch_open_gpui_app_modal_sidebar_state_payload(
                gpui_ghostex_folder_stats_error_message(&message),
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open Ghostex folder",
                &message,
                cx,
            );
        }
    }

    pub(crate) fn open_gpui_ghostty_config_file(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISettingsGhosttyConfig 2026-06-24-12:24:
        The Settings config-file button should open the bounded selected Ghostty config path. Prepare only that path, create an empty file if it is missing, avoid surfacing raw paths in status/toast copy, and report failure honestly instead of opening a parent folder or a second fallback config file.
        */
        let path = match shared_settings::prepare_ghostty_config_file_for_open() {
            Ok(path) => path,
            Err(_) => {
                let message = "GPUI could not prepare the selected Ghostty config file.";
                self.dispatch_gpui_settings_action_status(
                    "openGhosttyConfigFile",
                    false,
                    message,
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not open Ghostty config",
                    message,
                    cx,
                );
                return;
            }
        };
        if let Err(message) = gpui_open_path(&path) {
            self.dispatch_gpui_settings_action_status("openGhosttyConfigFile", false, &message, cx);
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open Ghostty config",
                &message,
                cx,
            );
            return;
        }
        self.dispatch_gpui_settings_action_status(
            "openGhosttyConfigFile",
            true,
            "Ghostty config file opened with the OS file handler.",
            cx,
        );
    }

    pub(crate) fn open_gpui_agents_hub_path_in_finder(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(path) = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_agents_hub_open_path_in_finder(path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(message) = result {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not open Agents Hub path",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_gpui_agents_hub_file_in_built_in_editor(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(file_path) = command
            .get("filePath")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_agents_hub_source_open_target(file_path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(pending) => {
                    let project_path = pending.project_path.clone();
                    this.pending_source_file_open = Some(pending);
                    this.close_gpui_app_modal_window_and_restore_command_focus(cx);
                    this.dispatch_gpui_os_integration_command_message(
                        serde_json::json!({
                            "action": "openProjectPaths",
                            "projects": [{
                                "path": project_path.to_string_lossy(),
                            }],
                        }),
                        cx,
                    );
                    this.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
                    this.focus_project_editor_surface(TitlebarMode::Source, window, cx);
                }
                Err(message) => {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not open Agents Hub file",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_save_agents_hub_file_command(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIAgentsHubBridge 2026-06-24-12:26:
        Agents Hub saves are real file writes, but the writer boundary must validate the selected file against the current catalog-derived allowlist before touching disk. Do not trust React-provided paths, log file content, create fallback draft stores, or claim success without refreshing the shared modal state.
        */
        let Some(file_path) = command
            .get("filePath")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let Some(content) = command
            .get("content")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_save_agents_hub_file(file_path, content) })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(catalog_message) => {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(catalog_message, cx);
                    this.dispatch_gpui_app_modal_toast(
                        "success",
                        "File saved",
                        "Agents Hub refreshed the saved file metadata.",
                        cx,
                    );
                }
                Err(message) => {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not save Agents Hub file",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_gpui_trusted_url(
        &mut self,
        url: &'static str,
        label: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Err(message) = gpui_open_url(url) {
            self.dispatch_gpui_settings_action_status(label, false, &message, cx);
            self.dispatch_gpui_app_modal_toast("warning", "Could not open link", &message, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn open_gpui_macos_system_settings_url(
        &mut self,
        url: &'static str,
        action: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Err(message) = gpui_open_url(url) {
            self.dispatch_gpui_settings_action_status(action, false, &message, cx);
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Could not open System Settings",
                &message,
                cx,
            );
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn open_gpui_macos_system_settings_url(
        &mut self,
        _url: &'static str,
        action: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        let message = "This System Settings action is only available on macOS.";
        self.dispatch_gpui_settings_action_status(action, false, message, cx);
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            gpui_ghostex_cli_status_message(Some(message)),
            cx,
        );
        self.dispatch_gpui_app_modal_toast("warning", "Unsupported on this OS", message, cx);
    }

    pub(crate) fn request_gpui_macos_notification_permission(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISettingsNotifications 2026-06-24-12:44:
        The GPUI Settings notification permission action mirrors the native Settings boundary: read macOS authorization, request only alert permission when it is notDetermined, report denied as a system-settings repair path, and never fake success with an in-app notification fallback.
        */
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let status = background
                .spawn(async move { gpui_request_macos_notification_permission() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_notification_permission_status_message(status),
                    cx,
                );
                this.dispatch_gpui_settings_action_status(
                    "requestMacOSNotificationPermission",
                    status.available(),
                    status.message(),
                    cx,
                );
                this.dispatch_gpui_app_modal_toast(
                    status.toast_level(),
                    status.toast_title(),
                    status.message(),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn deliver_gpui_macos_settings_test_notification(
        &mut self,
        completion_sound_enabled: bool,
        played_sound: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_deliver_macos_settings_test_notification() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(status) = result.permission_status() {
                    this.dispatch_open_gpui_app_modal_sidebar_state_payload(
                        gpui_notification_permission_status_message(status),
                        cx,
                    );
                }
                let (message, available) = gpui_macos_notification_test_action_message(
                    result,
                    completion_sound_enabled,
                    played_sound,
                );
                this.dispatch_gpui_settings_action_status(
                    "testAgentTaskCompletion",
                    available,
                    message,
                    cx,
                );
                if result == GpuiMacOSNotificationDeliveryResult::Sent {
                    this.dispatch_gpui_app_modal_toast(
                        "success",
                        "Test notification sent",
                        message,
                        cx,
                    );
                } else {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Test notification unavailable",
                        message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn play_gpui_completion_sound_preview(
        &mut self,
        sound: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let normalized_sound = gpui_normalize_completion_sound(sound);
        match gpui_play_completion_sound(normalized_sound) {
            Ok(()) => true,
            Err(message) => {
                self.dispatch_open_gpui_app_modal_sidebar_state_payload(
                    gpui_sound_preview_status_message(false, &message),
                    cx,
                );
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Sound preview unavailable",
                    &message,
                    cx,
                );
                false
            }
        }
    }

    pub(crate) fn test_gpui_agent_task_completion(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISettingsNotifications 2026-06-24-12:44:
        Settings test-agent-completion parity is intentionally bounded to the local test action. Continue the existing completion-sound preview when enabled, then send one generic no-sound macOS banner only when the Settings notification toggle is enabled; do not route real session attention notifications, names, paths, command text, URLs, or click-to-focus metadata through this path.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings = settings_snapshot.object();
        let completion_sound_enabled =
            json_bool_field(settings, "completionBellEnabled").unwrap_or(true);
        let notifications_enabled =
            json_bool_field(settings, "showMacOSAttentionNotifications").unwrap_or(true);
        let mut played_sound = false;
        if completion_sound_enabled {
            played_sound = self.play_gpui_completion_sound_preview(
                json_string_field(settings, "completionSound"),
                cx,
            );
        }

        if notifications_enabled {
            self.deliver_gpui_macos_settings_test_notification(
                completion_sound_enabled,
                played_sound,
                cx,
            );
            return;
        }

        if completion_sound_enabled {
            let (available, message) = if played_sound {
                (
                    true,
                    "Played the configured completion sound. macOS attention notifications are disabled in Settings.",
                )
            } else {
                (
                    false,
                    "The completion sound preview failed and macOS attention notifications are disabled in Settings.",
                )
            };
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                available,
                message,
                cx,
            );
        } else if !played_sound {
            let message = "Current Settings have completion sounds and macOS attention notifications disabled.";
            self.dispatch_gpui_settings_action_status(
                "testAgentTaskCompletion",
                false,
                message,
                cx,
            );
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Completion alerts are disabled",
                message,
                cx,
            );
        }
    }

    pub(crate) fn refresh_open_gpui_app_modal_sidebar_state_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn sync_gpui_portless_settings_after_save(
        &mut self,
        previous_settings: &serde_json::Map<String, serde_json::Value>,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
        Saved Settings changes in GPUI must synchronize Portless enabled/protocol state to gxserver through `/api/updatePortlessState`, matching the shared modal's production behavior without running privileged Portless admin scripts from this slice.

        CDXC:GPUISettingsPortlessBridge 2026-06-24-11:48:
        Portless Settings fan-out must stay bounded to shared contract values: missing or malformed `portlessEnabled` behaves as enabled, protocol is restricted to HTTP/HTTPS, and saved changes emit only `setEnabled` plus enabled-only `setProtocol` metadata RPCs so local Settings saves cannot carry paths, URLs, commands, tokens, or native admin payloads.
        */
        let previous_enabled = gpui_settings_portless_enabled(previous_settings);
        let next_enabled = gpui_settings_portless_enabled(next_settings);
        if previous_enabled != next_enabled {
            // macOS `syncPortlessEnabledSetting`: enabling re-arms the setup
            // prompt for this run; disabling suppresses it.
            if next_enabled {
                self.portless_setup_prompt_suppressed_until_restart = false;
            } else {
                self.suppress_gpui_portless_setup_prompt_for_this_run();
            }
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetEnabled {
                    enabled: next_enabled,
                },
                cx,
            );
        }

        let previous_protocol = gpui_settings_portless_protocol(previous_settings);
        let next_protocol = gpui_settings_portless_protocol(next_settings);
        if next_enabled && previous_protocol != next_protocol {
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetProtocol {
                    protocol: next_protocol,
                },
                cx,
            );
        }
    }

    pub(crate) fn sync_gpui_removed_remote_machine_passwords_after_settings_save(
        &mut self,
        previous_settings: &serde_json::Map<String, serde_json::Value>,
        next_settings: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemoteMachinesSettings 2026-06-24-13:36:
        Normal GPUI Settings saves mirror native-sidebar cleanup for deleted Remote Machines: after the settings object is already saved, remove Keychain passwords for disappeared machines that carried `sshPasswordSaved === true`. This cleanup is silent and best-effort, carries only bounded ids, and never blocks the settings save or logs machine/user/host/password details.
        */
        let removed_ids =
            gpui_removed_remote_machine_password_ids(previous_settings, next_settings);
        if removed_ids.is_empty() {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            background
                .spawn(async move {
                    for remote_machine_id in removed_ids {
                        let _ =
                            gpui_save_remote_machine_password_to_keychain(&remote_machine_id, "");
                    }
                })
                .await;
        })
        .detach();
    }

    pub(crate) fn update_gpui_portless_state_in_background(
        &mut self,
        update: GpuiPortlessStateUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (portless_state, sidebar_state_message) = background
                .spawn(async move {
                    let portless_state = gpui_update_portless_gxserver_state(update).ok();
                    let sidebar_state_message =
                        gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
                            portless_state.clone(),
                            active_project_id.as_deref(),
                        );
                    (portless_state, sidebar_state_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
                if let Some(portless_state) = portless_state {
                    this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
                }
            });
        })
        .detach();
    }

    /// GPUI equivalent of macOS `maybeOpenPortlessSetupPrompt`: macOS
    /// re-evaluates on every sidebar HUD publish, while GPUI owns Portless
    /// state in Rust and evaluates after daemon bootstrap and after every
    /// Portless state update. Suppression is memory-only for this app run
    /// after Postpone, Cancel, Disable, or launching an admin action.
    pub(crate) fn maybe_open_gpui_portless_setup_prompt(
        &mut self,
        portless_state: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            return;
        }
        if self.active_portless_setup_prompt_mode.is_some()
            || self.portless_setup_prompt_suppressed_until_restart
        {
            return;
        }
        // GPUI hosts one app-modal window; auto-opening the prompt would
        // replace whatever modal the user has open (macOS stacks child
        // windows), so an occupied host defers the prompt to a later check.
        if self.app_modal_window.is_some() {
            /*
            CDXC:GPUIPortlessPromptDeferral 2026-08-18:
            "Later check" used to mean "whenever some other Portless state
            update happens to run again", which for a prompt resolved during
            startup meant never: the prompt was dropped for the whole run.
            Remember the deferral so closing the user's modal re-runs the
            check, and keep the run-scoped suppression untouched so Postpone,
            Cancel, and Disable still stop it for good.
            */
            self.portless_setup_prompt_pending_modal_close = true;
            return;
        }
        let settings_portless_enabled = gpui_settings_portless_enabled(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let Some((mode, protocol)) =
            gpui_resolve_portless_setup_prompt(settings_portless_enabled, portless_state)
        else {
            return;
        };
        self.active_portless_setup_prompt_mode = Some(mode);
        let modal = GpuiAppModalKind::PortlessSetup;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "mode": mode.as_str(),
            "protocol": protocol.as_str(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }

    pub(crate) fn start_gpui_portless_setup_prompt_check(&mut self, cx: &mut gpui::Context<Self>) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            self.suppress_gpui_portless_setup_prompt_for_this_run();
            self.update_gpui_portless_state_in_background(
                GpuiPortlessStateUpdate::SetEnabled { enabled: false },
                cx,
            );
            return;
        }
        if self.active_portless_setup_prompt_mode.is_some()
            || self.portless_setup_prompt_suppressed_until_restart
        {
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let portless_state = background
                .spawn(async { gpui_sidebar_portless_state_with_presentation() })
                .await;
            let Some(portless_state) = portless_state else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
            });
        })
        .detach();
    }

    pub(crate) fn suppress_gpui_portless_setup_prompt_for_this_run(&mut self) {
        self.portless_setup_prompt_suppressed_until_restart = true;
        self.active_portless_setup_prompt_mode = None;
        self.portless_setup_prompt_pending_modal_close = false;
    }

    /// CDXC:GPUIPortlessPromptDeferral 2026-08-18: the app-modal host is free
    /// again, so a prompt that was deferred because the user had a modal open
    /// gets its check re-run instead of being lost for the rest of the run.
    pub(crate) fn resume_deferred_gpui_portless_setup_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.portless_setup_prompt_pending_modal_close {
            return;
        }
        self.portless_setup_prompt_pending_modal_close = false;
        self.start_gpui_portless_setup_prompt_check(cx);
    }

    pub(crate) fn handle_gpui_set_portless_enabled_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if command.get("enabled").and_then(serde_json::Value::as_bool) != Some(false) {
            return;
        }
        // The setup prompt's Disable button (macOS
        // `setPortlessEnabledFromSetupPrompt` suppresses before saving).
        self.suppress_gpui_portless_setup_prompt_for_this_run();
        let mut settings_object = shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .clone();
        settings_object.insert(
            "portlessEnabled".to_string(),
            serde_json::Value::Bool(false),
        );
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
        self.update_gpui_portless_state_in_background(
            GpuiPortlessStateUpdate::SetEnabled { enabled: false },
            cx,
        );
    }

    pub(crate) fn handle_gpui_portless_admin_action_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !GPUI_PORTLESS_APP_INTEGRATION_ENABLED {
            return;
        }
        let Some(action) = command
            .get("action")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_portless_admin_action)
        else {
            return;
        };
        /*
        CDXC:GPUIPortlessAdminBridge 2026-06-24-14:28:
        GPUI Settings/setup Portless admin commands now run through a fixed macOS helper equivalent to the reviewed Swift PortlessAdminClient. Accept only bounded action/protocol/requestId metadata, use the bundled Web/code-server Node plus Web/portless CLI runtime, record only sanitized result fields in gxserver, and refresh the app-modal HUD without logging scripts, paths, URLs, stdout/stderr, command text, tokens, environment values, project data, or user content.
        */
        let Some(request_id) = gpui_portless_admin_request_id(command) else {
            return;
        };
        let protocol = command
            .get("protocol")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_portless_protocol);
        // macOS suppresses the setup prompt when a non-remove admin action
        // launches (`runTrackedPortlessAdminAction`) and again when one fails
        // (`recordPortlessAdminResultInGxserver`); launch-time suppression
        // covers both.
        if action != GpuiPortlessAdminAction::Remove {
            self.suppress_gpui_portless_setup_prompt_for_this_run();
        }
        self.run_gpui_portless_admin_action_in_background(action, protocol, request_id, cx);
    }

    pub(crate) fn run_gpui_portless_admin_action_in_background(
        &mut self,
        action: GpuiPortlessAdminAction,
        protocol: Option<GpuiPortlessProtocol>,
        request_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (result, portless_state, sidebar_state_message) = background
                .spawn(async move {
                    let result = gpui_run_portless_admin_action(action, protocol, request_id);
                    let portless_state = gpui_update_portless_gxserver_state(
                        GpuiPortlessStateUpdate::RecordAdminResult {
                            action: result.action,
                            ok: result.ok,
                            protocol: result.protocol,
                        },
                    )
                    .ok()
                    .map(|state| gpui_portless_state_with_admin_result(state, &result));
                    let sidebar_state_message =
                        gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
                            portless_state.clone(),
                            active_project_id.as_deref(),
                        );
                    (result, portless_state, sidebar_state_message)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(result.message(), cx);
                if let Some(portless_state) = portless_state {
                    this.maybe_open_gpui_portless_setup_prompt(&portless_state, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn update_gpui_project_settings_metadata_in_background(
        &mut self,
        update: GpuiProjectSettingsMetadataUpdate,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUISettingsProjectMetadata 2026-06-24-11:59:
        GPUI Settings project metadata edits are gxserver-owned. Apply worktree command, Beads display-key, and Beads directory changes through `/api/updateProject`, then rehydrate the open modal from real gxserver/shared Settings state instead of mutating local fake project rows or shelling out.
        */
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let sidebar_state_message = background
                .spawn(async move {
                    let _ = gpui_update_project_settings_metadata(update);
                    gpui_app_modal_sidebar_state_message_for_active_project_id(
                        active_project_id.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.refresh_open_gpui_app_modal_sidebar_state(sidebar_state_message, cx);
            });
        })
        .detach();
    }

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

    pub(crate) fn stop_gpui_keep_awake_runtime_with_options(&mut self, suppress_auto_start: bool) -> bool {
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

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_gpui_keep_awake_lid_sleep_prevention(
        &mut self,
        settings: shared_settings::SharedKeepAwakeTitlebarSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarKeepAwake 2026-06-26-00:09:
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
    pub(crate) fn spawn_gpui_keep_awake_lid_sleep_disable_cleanup(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |_this, _cx| {
            let _ = background
                .spawn(async move { gpui_set_lid_sleep_prevention_enabled(false, false) })
                .await;
        })
        .detach();
    }

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

    pub(crate) fn check_gpui_keep_awake_power_rules_in_background(&mut self, cx: &mut gpui::Context<Self>) {
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
    pub(crate) fn gpui_keep_awake_power_probe_options(&self) -> Option<GpuiKeepAwakePowerProbeOptions> {
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
