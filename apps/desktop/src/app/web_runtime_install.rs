//! Starting, installing, retrying and removing the optional web runtime (CEF) for this process.
//! The state these update, and the user decision behind it, live in app/helpers/web_runtime.rs
//! (CDXC:CefRuntime 2026-09-28).

use futures::StreamExt as _;
use futures::channel::mpsc;

use crate::app::helpers::web_runtime::*;
use crate::app::helpers::web_runtime_pending::{
    PENDING_CHANGES_SUPPORTED, PendingWebRuntimeChange, cancel_pending_web_runtime_change,
    mark_pending_web_runtime_change,
};
use crate::app::model::*;
use crate::component_store::ComponentStoreProgressPhase;
use crate::*;

/// Shown when the runtime cannot be replaced or removed because this process has it loaded.
const WEB_RUNTIME_IN_USE_MESSAGE: &str =
    "The web runtime is in use. Restart Ghostex, then do this before opening a web view.";

/// How often a runtime start waiting for a panel slide checks whether it has settled: one frame.
const WEB_RUNTIME_START_SLIDE_POLL: std::time::Duration = std::time::Duration::from_millis(16);
/// The longest a runtime start waits for panels to settle, well past the slowest slide (320ms).
const WEB_RUNTIME_START_SLIDE_WAIT_MAX: std::time::Duration = std::time::Duration::from_secs(1);

impl GhostexGpuiApp {
    /// Verifies the installed runtime and starts CEF. `request_cef_runtime` calls it once per
    /// process, when the first web view is about to be shown.
    ///
    /// CDXC:CefRuntime 2026-09-30 DECISION:
    /// User: clicking a file link must "react instantly" and "keep the loading stuff to after the view is visible so the user doesn't feel like they need to click again". The first web view of a launch (an HTML file in Files, for example) starts CEF, and both halves of that start used to run on the UI thread inside the click's first frame: the `codesign` checks of the 319MB framework, then `cef::initialize`. The view panel's slide froze shut for seconds with only the toast showing. The checks now run in the background, and `cef::initialize` (which must run on the UI thread) waits for any panel slide to settle, so the view is on screen with its loading state before the start takes the thread.
    pub(crate) fn start_web_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            let readiness = cx
                .background_executor()
                .spawn(async { cef_component_window::verified_cef_runtime_readiness() });
            cx.spawn(async move |this, cx| {
                let readiness = readiness.await;
                crate::app::window::visual_page_modal::open_timing::mark("webRuntimeChecked");
                let started_waiting = std::time::Instant::now();
                loop {
                    let Ok(sliding) = this.update(cx, |this, _| this.panel_motion.animating())
                    else {
                        return;
                    };
                    if !sliding || started_waiting.elapsed() >= WEB_RUNTIME_START_SLIDE_WAIT_MAX {
                        break;
                    }
                    cx.background_executor()
                        .timer(WEB_RUNTIME_START_SLIDE_POLL)
                        .await;
                }
                // One more frame so the settled panel is painted before the start holds the thread.
                cx.background_executor()
                    .timer(WEB_RUNTIME_START_SLIDE_POLL)
                    .await;
                let _ = this.update(cx, |this, cx| match readiness {
                    Ok(cef_component_window::CefRuntimeReadiness::Ready) => this.initialize_cef(cx),
                    Ok(cef_component_window::CefRuntimeReadiness::InstallRequired { .. }) => {
                        this.cef_runtime_requested = false;
                        set_web_runtime_state(WebRuntimeState::NotInstalled);
                        this.refresh_web_runtime_views(cx);
                    }
                    Err(message) => this.fail_web_runtime_start(message, cx),
                });
            })
            .detach();
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        self.initialize_cef(cx);
    }

    /// Verification or `cef::initialize` failed: the web views show the failure notice (and Retry
    /// only when initialization can still be retried in this process).
    pub(crate) fn fail_web_runtime_start(&mut self, message: String, cx: &mut gpui::Context<Self>) {
        self.cef_runtime_requested = false;
        let permanent = cef::init_permanently_failed();
        support_logs::append(
            support_logs::GpuiSupportLog::CrashReports,
            "gpui.cefRuntime.startFailed",
            serde_json::json!({ "error": message, "restartRequired": permanent }),
        );
        if permanent {
            set_web_runtime_state(WebRuntimeState::Unavailable(cef_init_unavailable_notice()));
            self.refresh_web_runtime_views(cx);
            crate::app::workspace_windows::refresh_web_runtime_views_in_other_windows(
                cx.entity_id(),
                cx,
            );
        } else {
            set_web_runtime_state(WebRuntimeState::Failed(message));
            self.refresh_web_runtime_views(cx);
        }
    }

    /// Whether Chromium was started (or attempted initialization) in this process: it cannot be
    /// unloaded or restarted, so its files stay in use until Ghostex quits.
    pub(crate) fn web_runtime_started_in_process(&self) -> bool {
        self.cef_runtime_requested
            || matches!(
                web_runtime_state(),
                WebRuntimeState::Running | WebRuntimeState::Unavailable(_)
            )
            || cef::init_permanently_failed()
    }

    /// Install (a web view's prompt, Plugins) or Reinstall (Plugins). Only a click starts it.
    pub(crate) fn install_web_runtime(&mut self, reinstall: bool, cx: &mut gpui::Context<Self>) {
        if matches!(web_runtime_state(), WebRuntimeState::Installing(_))
            || self.plugin_settings_action_progress.contains_key("cef")
        {
            return;
        }
        self.plugin_settings_action_errors.remove("cef");
        let started = self.web_runtime_started_in_process();
        if started && !reinstall {
            return;
        }
        // CDXC:CefRuntime 2026-09-28 WHY: Windows keeps a loaded DLL's file locked, so replacing the component under a running Chromium fails halfway and leaves no runtime for the next start. The next launch swaps it instead (app/helpers/web_runtime_pending.rs).
        if started && cfg!(target_os = "windows") {
            if let Err(message) =
                mark_pending_web_runtime_change(PendingWebRuntimeChange::Reinstall)
            {
                self.plugin_settings_action_errors.insert("cef", message);
            }
            self.refresh_web_runtime_views(cx);
            return;
        }
        // Installing again means keeping the runtime: a removal waiting for the next launch is off.
        if let Err(message) = cancel_pending_web_runtime_change() {
            self.plugin_settings_action_errors.insert("cef", message);
        }
        self.cef_component_install_generation =
            self.cef_component_install_generation.wrapping_add(1);
        let generation = self.cef_component_install_generation;
        // A reinstall under a running Chromium (macOS) only replaces the files for the next start,
        // so the web views keep their pages and only the Plugins row shows the progress.
        if started {
            self.plugin_settings_action_progress
                .insert("cef", ComponentStoreProgressPhase::Checking);
        } else {
            set_web_runtime_state(WebRuntimeState::Installing(
                ComponentStoreProgressPhase::Checking,
            ));
        }
        self.refresh_web_runtime_views(cx);

        let (progress_tx, mut progress_rx) = mpsc::unbounded();
        cx.spawn(async move |this, cx| {
            while let Some(phase) = progress_rx.next().await {
                let _ = this.update(cx, |this, cx| {
                    if this.cef_component_install_generation != generation {
                        return;
                    }
                    if started {
                        this.plugin_settings_action_progress.insert("cef", phase);
                    } else {
                        set_web_runtime_state(WebRuntimeState::Installing(phase));
                    }
                    this.refresh_web_runtime_views(cx);
                });
            }
        })
        .detach();

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            let result = background
                .spawn(async move {
                    cef_component_window::install_and_verify_cef_component_phases(
                        reinstall,
                        progress_tx,
                    )
                })
                .await;
            #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
            let result: Result<(), String> = {
                let _ = (background, progress_tx, reinstall);
                Err("The web runtime is not available on this platform.".to_string())
            };
            let _ = this.update(cx, |this, cx| {
                if this.cef_component_install_generation != generation {
                    return;
                }
                this.plugin_settings_action_progress.remove("cef");
                match result {
                    Ok(()) if started => {}
                    Ok(()) => {
                        #[cfg(any(
                            target_os = "macos",
                            target_os = "windows",
                            target_os = "linux"
                        ))]
                        cef_component_window::configure_cef_framework_path_for_process();
                        set_web_runtime_state(WebRuntimeState::Installed);
                        this.resume_web_views_after_install(cx);
                    }
                    Err(message) if started => {
                        this.plugin_settings_action_errors.insert("cef", message);
                    }
                    Err(message) => {
                        set_web_runtime_state(WebRuntimeState::Failed(message));
                    }
                }
                this.refresh_web_runtime_views(cx);
            });
        })
        .detach();
    }

    /// Retry on a web view: start again when a runtime is on disk, otherwise install it.
    pub(crate) fn retry_web_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        if cef::init_permanently_failed()
            || matches!(web_runtime_state(), WebRuntimeState::Unavailable(_))
        {
            set_web_runtime_state(WebRuntimeState::Unavailable(cef_init_unavailable_notice()));
            self.refresh_web_runtime_views(cx);
            return;
        }
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        let found = cef_component_window::configure_cef_framework_path_for_process();
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let found = false;
        if found {
            set_web_runtime_state(WebRuntimeState::Installed);
            self.resume_web_views_after_install(cx);
            self.refresh_web_runtime_views(cx);
        } else {
            set_web_runtime_state(WebRuntimeState::NotInstalled);
            self.install_web_runtime(false, cx);
        }
    }

    /// Plugins' Undo for a change waiting for the next launch: the runtime stays as it is.
    pub(crate) fn cancel_web_runtime_pending_change(&mut self, cx: &mut gpui::Context<Self>) {
        self.plugin_settings_action_errors.remove("cef");
        if let Err(message) = cancel_pending_web_runtime_change() {
            self.plugin_settings_action_errors.insert("cef", message);
        }
        self.refresh_web_runtime_views(cx);
    }

    /// Plugins' Uninstall. While Chromium runs in this process it only marks the runtime for
    /// removal at the next launch (on Linux it is refused); a runtime bundled next to the app is
    /// refused.
    pub(crate) fn uninstall_web_runtime(&mut self, cx: &mut gpui::Context<Self>) {
        self.plugin_settings_action_errors.remove("cef");
        let result = self.uninstall_web_runtime_files();
        if let Err(message) = result {
            self.plugin_settings_action_errors.insert("cef", message);
        }
        self.refresh_web_runtime_views(cx);
    }

    fn uninstall_web_runtime_files(&mut self) -> Result<(), String> {
        if matches!(web_runtime_state(), WebRuntimeState::Installing(_)) {
            return Err("The web runtime is being installed.".to_string());
        }
        let started = self.web_runtime_started_in_process();
        if started && !PENDING_CHANGES_SUPPORTED {
            return Err(WEB_RUNTIME_IN_USE_MESSAGE.to_string());
        }
        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        {
            if cef_component_window::cef_runtime_is_bundled() {
                return Err(
                    "This build carries its own web runtime, which cannot be uninstalled."
                        .to_string(),
                );
            }
            // Chromium cannot be unloaded, so the next launch removes it before it can start.
            if started {
                return mark_pending_web_runtime_change(PendingWebRuntimeChange::Uninstall);
            }
            cef_component_window::uninstall_cef_component()?;
            set_web_runtime_state(
                if cef_component_window::configure_cef_framework_path_for_process() {
                    WebRuntimeState::Installed
                } else {
                    WebRuntimeState::NotInstalled
                },
            );
            Ok(())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        Err("The web runtime is not available on this platform.".to_string())
    }

    /// The web views that showed the prompt create their pages now, which starts CEF: the modal
    /// or extension popup that was waiting, the active web workarea view, and the Browser.
    fn resume_web_views_after_install(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gpui_app_modal_deferred_for_cef_kind().is_some() {
            self.request_cef_runtime(cx);
        }
        self.retry_titlebar_extension_popup_after_cef_ready(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        if self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser)
        {
            self.ensure_active_browser_surface(cx);
        }
        self.reload_open_project_browser_tabs_on_startup(cx);
        self.update_active_mode_cef_child_visibility(cx);
    }

    /// Redraws everything that shows the runtime's state: the web views' prompts, the Plugins row
    /// and the prompt dialog of a modal waiting for the runtime.
    pub(crate) fn refresh_web_runtime_views(&mut self, cx: &mut gpui::Context<Self>) {
        // The Plugins status measures the installed component on disk, so it is only rebuilt
        // while something that shows it (Settings) is open.
        if self.app_modal_window.is_some() || self.native_app_modal.is_some() {
            self.refresh_gpui_plugins_modal(cx);
        }
        self.refresh_web_runtime_modal_prompt(cx);
        self.sync_visual_page_modal_with_web_runtime(cx);
        cx.notify();
    }
}
