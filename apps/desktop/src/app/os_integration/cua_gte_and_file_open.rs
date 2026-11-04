// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds cua-driver/GTE install helpers, daemon/quick-access session refresh accessors, terminal settings presets, and Ghostex folder/config/agents-hub file open commands.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, keep-awake

use std::fs;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_cua_driver_install_or_update(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let action = gpui_cua_driver_command_action();
        if action.operation == "install" && !self.refuse_gpui_cua_driver_install(cx) {
            return;
        }
        self.start_gpui_cua_driver_job(action, cx);
    }

    pub(crate) fn handle_gpui_cua_driver_reinstall(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.refuse_gpui_cua_driver_install(cx) {
            return;
        }
        self.start_gpui_cua_driver_job(gpui_cua_driver_reinstall_command_action(), cx);
    }

    pub(crate) fn handle_gpui_cua_driver_uninstall(
        &mut self,
        _window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.start_gpui_cua_driver_job(gpui_cua_driver_uninstall_command_action(), cx);
    }

    /// `false` (after a toast) when this Mac account cannot write /Applications, where Trycua's
    /// installer puts CuaDriver.app.
    fn refuse_gpui_cua_driver_install(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        match gpui_cua_driver_applications_blocked_reason() {
            Some(reason) => {
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Fast Computer & Browser Use can't be installed",
                    &reason,
                    cx,
                );
                false
            }
            None => true,
        }
    }

    pub(crate) fn check_gpui_cua_driver_update(&mut self, cx: &mut gpui::Context<Self>) {
        self.check_gpui_tool_update("Fast Computer & Browser Use", "cuaDriver", cx);
    }

    /// Settings' explicit "check for updates" click on a desktop-control tool: a fresh status
    /// (skipping the cached latest-release answers) and a toast saying what it found.
    pub(crate) fn check_gpui_tool_update(
        &mut self,
        name: &'static str,
        prefix: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let payload =
                background
                    .spawn(async move {
                        gpui_ghostex_cli_status_message_with_cua_update_check(None, true)
                    })
                    .await;
            let (level, title, message) = gpui_tool_update_check_toast(&payload, name, prefix);
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(payload.clone(), cx);
                this.dispatch_gpui_titlebar_tips_sidebar_state_payload(&payload, cx);
                this.dispatch_gpui_app_modal_toast(level, &title, &message, cx);
            });
        })
        .detach();
    }

    /// Runs Trycua's official installer, updater or uninstaller as a background job (see the
    /// CDXC:ManagedTools decision on `GPUI_TRYCUA_INSTALL_COMMAND`): installs and updates finish
    /// Desktop Control setup, uninstalls report their result.
    pub(crate) fn start_gpui_cua_driver_job(
        &mut self,
        action: GpuiInstallJobAction,
        cx: &mut gpui::Context<Self>,
    ) {
        self.start_gpui_install_job(
            &CUA_DRIVER_JOB,
            action,
            |operation, succeeded| match operation {
                "uninstall" => GpuiGhostexCliSettingsAction::FinishTrycuaUninstall { succeeded },
                _ => GpuiGhostexCliSettingsAction::FinishDesktopControlSetup {
                    driver_installed: succeeded,
                    was_update: operation == "update",
                },
            },
            cx,
        );
    }

    /// Runs a tool's official installer, updater or uninstaller as `job`. Settings gets the job's
    /// progress about once a second, and its exit is the completion signal: `finish` turns the
    /// operation and its success into the settings action that reports it and refreshes the status.
    pub(crate) fn start_gpui_install_job(
        &mut self,
        job: &'static GpuiInstallJob,
        action: GpuiInstallJobAction,
        finish: fn(&'static str, bool) -> GpuiGhostexCliSettingsAction,
        cx: &mut gpui::Context<Self>,
    ) {
        let GpuiInstallJobAction {
            script,
            operation,
            running_message,
            toast_title,
        } = action;
        if let Err(message) = job.begin(operation) {
            self.dispatch_gpui_app_modal_toast("warning", toast_title, &message, cx);
            return;
        }
        self.dispatch_gpui_install_job_progress(cx);
        self.dispatch_gpui_app_modal_toast("info", toast_title, running_message, cx);
        let background = cx.background_executor().clone();
        let pump = background.clone();
        cx.spawn(async move |this, cx| {
            loop {
                pump.timer(std::time::Duration::from_secs(1)).await;
                let running = job.running();
                let updated =
                    this.update(cx, |this, cx| this.dispatch_gpui_install_job_progress(cx));
                if !running || updated.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let succeeded = background
                .spawn(async move {
                    let result = job.run_script(&script);
                    job.finish(&result);
                    result.is_ok()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.run_gpui_ghostex_cli_settings_action(finish(operation, succeeded), cx);
            });
        })
        .detach();
    }

    fn dispatch_gpui_install_job_progress(&mut self, cx: &mut gpui::Context<Self>) {
        let payload = gpui_install_job_progress_status_payload();
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(payload.clone(), cx);
        self.dispatch_gpui_titlebar_tips_sidebar_state_payload(&payload, cx);
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

    pub(crate) fn update_gpui_ghostty_visible_settings(
        &mut self,
        mutate_settings: fn(&mut serde_json::Map<String, serde_json::Value>),
        write_ghostty_config: fn() -> Result<
            shared_settings::SharedGhosttyConfigFileWriteStatus,
            shared_settings::SharedGhosttyConfigFileError,
        >,
        cx: &mut gpui::Context<Self>,
    ) {
        // Apply/reset update the visible Settings keys and the managed block of Ghostex's own
        // terminal config; neither ever touches the Ghostty app's config.
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
                "Ghostex could not save the terminal settings.",
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
                    "Terminal settings and Ghostex's terminal config were saved.",
                    cx,
                );
            }
            Err(_) => {
                let message = "Terminal settings were saved, but Ghostex could not write its terminal config file.";
                self.dispatch_gpui_settings_action_status("ghosttySettings", false, message, cx);
                self.dispatch_gpui_app_modal_toast(
                    "warning",
                    "Could not update terminal config",
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
        // The Ghostty app's own config, opened for the user to read or edit; Ghostex never
        // creates it, so a missing file is reported instead.
        let path = match shared_settings::prepare_ghostty_config_file_for_open() {
            Ok(path) => path,
            Err(_) => {
                let message = "The Ghostty app has no config file on this computer yet.";
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
        self.open_gpui_agents_hub_path(path, cx);
    }

    /// Reveals an Agents Hub file, or opens one of its folders, in the OS file manager.
    pub(crate) fn open_gpui_agents_hub_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
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

    /// A bot's Edit SOUL or Edit config (gx_store/create/bot.rs): the file opens in the Code view
    /// of the bot's own project. The view is switched only once the bot is the active project
    /// (`land_pending_source_file_open_on_source_mode`: now when it already is, else when the
    /// focus below lands), so no Code view starts for the project that was active before.
    pub(crate) fn open_bot_file_in_code_view(
        &mut self,
        group_id: String,
        project_path: std::path::PathBuf,
        file_path: std::path::PathBuf,
        cx: &mut gpui::Context<Self>,
    ) {
        // The Customize switch, not the active project's availability: that is the previous
        // project's until the focus lands.
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source) {
            self.copy_path_for_disabled_project_workarea(
                &file_path.to_string_lossy(),
                "VS Code",
                cx,
            );
            return;
        }
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            column: None,
            file_path,
            line: None,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path,
            remote_target: None,
            remote_working_directory: None,
        });
        self.dispatch_native_sidebar_command(
            serde_json::json!({ "type": "focusGroup", "groupId": group_id }),
            cx,
        );
        self.defer_in_main_window(cx, |this, window, cx| {
            this.land_pending_source_file_open_on_source_mode(window, cx);
        });
    }

    /// Docs' "Open in Code view", offered for a Markdown file too large for the Markdown editor:
    /// the file opens in the active project's Code view, the way a chat file link does.
    pub(crate) fn open_docs_file_in_code_view(
        &mut self,
        file_path: std::path::PathBuf,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        if gpui_titlebar_mode_hidden_from_settings(TitlebarMode::Source)
            || !self.titlebar_mode_available(TitlebarMode::Source)
        {
            return Err("VS Code view is not available for this project.".to_string());
        }
        if let Some(reason) = self.embedded_code_editor_unavailable_reason() {
            return Err(reason.to_string());
        }
        let project_path = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.clone())
            .ok_or_else(|| "No active project can open this file in VS Code view.".to_string())?;
        self.pending_source_file_open = Some(PendingSourceFileOpen {
            column: None,
            file_path,
            line: None,
            origin: PendingSourceFileOpenOrigin::SessionChat,
            project_path,
            remote_target: None,
            remote_working_directory: None,
        });
        self.defer_in_main_window(cx, |this, window, cx| {
            this.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
            this.mark_project_editor_mode_awake(TitlebarMode::Source, cx);
            this.focus_project_editor_surface(TitlebarMode::Source, window, cx);
        });
        Ok(())
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
        self.open_gpui_agents_hub_file_path_in_built_in_editor(file_path, cx);
    }

    /// Opens an Agents Hub file in the Code view of its folder's project, closing the Hub first.
    pub(crate) fn open_gpui_agents_hub_file_path_in_built_in_editor(
        &mut self,
        file_path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_agents_hub_source_open_target(file_path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(pending) => {
                    /*
                    CDXC:Extensions 2026-08-23:
                    "Open in built-in editor" names the Code view, so with Code
                    turned off in Settings → Customize there is nothing to open
                    it in. Hand back the resolved path instead of registering
                    the project and parking on a workarea the user disabled.
                    */
                    if !this.titlebar_mode_available(TitlebarMode::Source) {
                        let file_path = pending.file_path.to_string_lossy().to_string();
                        // Close the Agents Hub modal first: the copy toast is a
                        // main-window toast, so it would sit behind the modal
                        // that is still covering the window.
                        this.close_gpui_agents_hub_for_navigation(cx);
                        this.copy_path_for_disabled_project_workarea(&file_path, "VS Code", cx);
                        return;
                    }
                    let project_path = pending.project_path.clone();
                    this.pending_source_file_open = Some(pending);
                    this.close_gpui_agents_hub_for_navigation(cx);
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
        CDXC:AgentLauncher 2026-06-24-12:26:
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
        let message = "This system settings action is unavailable on this platform.";
        self.dispatch_gpui_settings_action_status(action, false, message, cx);
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            gpui_ghostex_cli_status_message(Some(message)),
            cx,
        );
        self.dispatch_gpui_app_modal_toast("warning", "Unsupported on this OS", message, cx);
    }
}
