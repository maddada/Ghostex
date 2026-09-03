// C1 wave-4 deferred split: apps/desktop/src/app/os_integration.rs (~3.6k
// lines) further divided into responsibility-scoped submodules, pure move
// (the only edit from the original app/os_integration.rs body is wrapping
// each group of `impl GhostexGpuiApp` methods in its own impl block;
// multiple impl blocks for the same type across files is the established
// pattern used by every sibling file in apps/desktop/src/app/). This file holds cua-driver/GTE install helpers, daemon/quick-access session refresh accessors, Ghostty settings sync, and Ghostex folder/config/agents-hub file open commands.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: updater, first-run onboarding, gxserver bootstrap, OS shells, portless, keep-awake

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
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.start_gpui_cua_driver_install_or_update_terminal(window, cx);
    }

    pub(crate) fn start_gpui_cua_driver_install_or_update_terminal(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Extensions 2026-08-09:
        Installing or updating Trycua can take minutes and prints useful
        progress. Run it in a real command-pane terminal tab so the user can
        watch the official installer/updater instead of staring at a silent
        Settings spinner. The tab opens without stealing typing focus like
        every other command Action.

        CDXC:Extensions 2026-08-24:
        Windows and Linux run the same command-pane installer instead of opening
        a downloads page, so the Settings button matches the command Settings
        shows on every desktop platform.
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
        CDXC:Sessions 2026-08-08:
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
        CDXC:Terminal 2026-06-24-12:24:
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
        CDXC:Terminal 2026-06-24-12:24:
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
                        this.close_gpui_app_modal_window_and_restore_command_focus(cx);
                        this.copy_path_for_disabled_project_workarea(&file_path, "Code", cx);
                        return;
                    }
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
        let message = "This System Settings action is only available on macOS.";
        self.dispatch_gpui_settings_action_status(action, false, message, cx);
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            gpui_ghostex_cli_status_message(Some(message)),
            cx,
        );
        self.dispatch_gpui_app_modal_toast("warning", "Unsupported on this OS", message, cx);
    }
}
