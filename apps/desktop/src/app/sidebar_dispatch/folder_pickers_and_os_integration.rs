//! Workspace, replacement and repository folder pickers, and macOS OS integration URLs, file opens, script dialogs and quick terminals.

use std::path::PathBuf;

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn handle_gpui_pick_workspace_folder_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            #[cfg(target_os = "windows")]
            let picked_path =
                match windows_terminal_backend::wsl_path_for_windows_path(path.as_path()) {
                    Ok(path) => path,
                    Err(_) => return,
                };
            #[cfg(not(target_os = "windows"))]
            let picked_path = path.to_string_lossy().to_string();
            let mut message = serde_json::json!({
                "path": picked_path,
                "type": "workspaceFolderPicked",
            });
            if let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
            {
                message["name"] = serde_json::json!(name);
            }
            let _ = this.update(cx, |this, cx| {
                this.dispatch_gpui_workspace_folder_picked_message(message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_pick_replacement_project_folder_message(
        &mut self,
        project_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Locate Project Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            #[cfg(target_os = "windows")]
            let picked_path =
                match windows_terminal_backend::wsl_path_for_windows_path(path.as_path()) {
                    Ok(path) => path,
                    Err(_) => return,
                };
            #[cfg(not(target_os = "windows"))]
            let picked_path = path.to_string_lossy().to_string();
            let message = serde_json::json!({
                "path": picked_path,
                "projectId": project_id,
                "type": "replacementProjectFolderPicked",
            });
            let _ = this.update(cx, |this, cx| {
                this.dispatch_gpui_workspace_folder_picked_message(message, cx);
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_workspace_folder_picked_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.gx_store_workspace_folder_picked(&message, cx)
    }

    /// GPUI port of the macOS OS-integration entry points
    /// (`application(_:open:)` → `handleOSIntegrationURL` +
    /// `dispatchOSIntegrationFileOpenPaths`, AppDelegate.swift). URLs arrive
    /// through gpui's `application:openURLs:` delegate (`cx.on_open_urls`):
    /// `ghostex://open|edit|session` actions plus Finder Open-With
    /// file:// opens. `.command/.tool/.sh` files never execute without the
    /// Run/Edit/Cancel consent dialog.
    #[cfg(target_os = "macos")]
    pub(crate) fn receive_gpui_os_integration_urls(
        &mut self,
        urls: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut file_paths = Vec::new();
        for raw_url in urls {
            if self.receive_gpui_link_on_every_platform(raw_url.trim(), window, cx) {
                continue;
            }
            let Ok(parsed) = gpui::http_client::Url::parse(raw_url.trim()) else {
                continue;
            };
            if parsed.scheme().eq_ignore_ascii_case("file") {
                if let Ok(path) = parsed.to_file_path() {
                    file_paths.push(path);
                }
                continue;
            }
            if parsed.scheme().eq_ignore_ascii_case("ghostex") {
                self.handle_gpui_os_integration_ghostex_url(&parsed, window, cx);
            }
        }
        if !file_paths.is_empty() {
            self.dispatch_gpui_os_integration_file_open_paths(file_paths, window, cx);
        }
    }

    /// The links every platform answers, read as the OS gave them: parsing would normalize what the
    /// session route refuses. Returns whether it was one of them.
    fn receive_gpui_link_on_every_platform(
        &mut self,
        link: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if ghostex_gx_protocol::is_session_link_route(link) {
            self.open_session_deep_link(link, window, cx);
            return true;
        }
        if ghostex_gx_protocol::is_terminal_link_route(link) {
            cx.activate(true);
            window.activate_window();
            let notice = ghostex_gx_core::terminal_link_gone_notice();
            self.dispatch_gpui_app_modal_toast("warning", &notice.title, &notice.description, cx);
            return true;
        }
        false
    }

    /// Windows and Linux answer those links and nothing else (session_link_handoff.rs):
    /// `ghostex://open` registers a project, which a web page must not reach on the platforms that
    /// never had that route.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn receive_gpui_os_integration_urls(
        &mut self,
        urls: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        for raw_url in urls {
            self.receive_gpui_link_on_every_platform(raw_url.trim(), window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_gpui_os_integration_ghostex_url(
        &mut self,
        url: &gpui::http_client::Url,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.activate(true);
        window.activate_window();
        let action = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let query_value = |name: &str| -> Option<String> {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        if action == "open" || action == "edit" {
            // macOS accepts both `path` and legacy `file`; line/column are
            // parsed by macOS but GPUI's Source URL gate has no file-target
            // support yet (tracked in deferred-out-of-scope.md).
            let Some(path) = query_value("path")
                .or_else(|| query_value("file"))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                return;
            };
            self.open_gpui_os_integration_paths(vec![PathBuf::from(path)], window, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_os_integration_file_open_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.activate(true);
        window.activate_window();
        let mut open_paths = Vec::new();
        let mut script_paths = Vec::new();
        for path in paths {
            if gpui_os_integration_path_is_script(&path) {
                script_paths.push(path);
            } else {
                open_paths.push(path);
            }
        }
        if !open_paths.is_empty() {
            self.open_gpui_os_integration_paths(open_paths, window, cx);
        }
        if !script_paths.is_empty() {
            self.present_gpui_os_integration_script_dialogs(script_paths, window, cx);
        }
    }

    /// macOS `presentScriptOpenDialogIfNeeded` parity: opening a script file
    /// through Launch Services must never execute immediately. GPUI window
    /// prompts cannot re-enter, so multiple script files present one dialog at
    /// a time.
    #[cfg(target_os = "macos")]
    pub(crate) fn present_gpui_os_integration_script_dialogs(
        &mut self,
        script_paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = window;
        cx.spawn(async move |this, cx| {
            for path in script_paths {
                let detail = path.to_string_lossy().to_string();
                let Ok(receiver) = this.update_in(cx, |_, window, cx| {
                    window.prompt(
                        gpui::PromptLevel::Info,
                        "Open Script",
                        Some(detail.as_str()),
                        &["Run", "Edit", "Cancel"],
                        cx,
                    )
                }) else {
                    return;
                };
                let Ok(answer) = receiver.await else {
                    continue;
                };
                if answer == 0 {
                    let command = gpui_os_integration_script_run_command(&path);
                    let cwd = path
                        .parent()
                        .map(|parent| parent.to_string_lossy().to_string());
                    let title = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string());
                    let _ = this.update(cx, |this, cx| {
                        this.open_gpui_os_integration_quick_terminal(Some(command), cwd, title, cx);
                    });
                } else if answer == 1 {
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.open_gpui_os_integration_paths(vec![path.clone()], window, cx);
                    });
                }
            }
        })
        .detach();
    }

    /// A script's Run: a terminal session in the project registered at cwd. macOS creates a client-side projectless
    /// Quick project; GPUI's sidebar is daemon-derived, so the runtime
    /// registers/reuses the daemon project for that folder instead (delta
    /// recorded in deferred-out-of-scope.md).
    #[cfg(target_os = "macos")]
    pub(crate) fn open_gpui_os_integration_quick_terminal(
        &mut self,
        command: Option<String>,
        cwd: Option<String>,
        title: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref();
        let cwd = cwd.filter(|value| !value.trim().is_empty());
        if cwd.is_none()
            && gpui_active_project_id_from_snapshot(snapshot)
                .is_some_and(|id| id.starts_with("remote:"))
        {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Open Terminal unavailable",
                "Open a local project or include a local folder in the terminal link.",
                cx,
            );
            return;
        }
        let cwd = cwd.or_else(|| {
            gpui_active_local_project_directory(snapshot)
                .map(|path| path.to_string_lossy().into_owned())
        });
        if cwd.is_none() && gpui_active_project_id_from_snapshot(snapshot).is_some() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Open Terminal unavailable",
                "The active project's folder is unavailable. Open a local project first.",
                cx,
            );
            return;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let resolved_cwd = background
                .spawn(async move { gpui_os_integration_resolved_terminal_cwd(cwd) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                // CDXC:Workarea 2026-09-20 WHY:
                // "Open Terminal" used to switch the app to Agents so the new terminal was on
                // screen. The sessions column is always on screen now, so it lands there without
                // closing whatever view the user had open.
                let _ = window;
                let mut message = serde_json::json!({
                    "action": "createQuickTerminal",
                    "cwd": resolved_cwd,
                });
                if let Some(command) = command
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    message["command"] = serde_json::json!(command);
                }
                if let Some(title) = title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    message["title"] = serde_json::json!(title);
                }
                this.dispatch_gpui_os_integration_command_message(message, cx);
            });
        })
        .detach();
    }

    /// Open/edit path targets: resolve each target's project root (git root of
    /// the directory or of a file's parent — macOS
    /// `openNativePathTargetsFromCli` classification), register + focus it
    /// through the runtime, and wake the Source project editor. File/line/
    /// column targeting into code-server is deferred (the Source runtime URL
    /// gate carries folder identity only).
    pub(crate) fn open_gpui_os_integration_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = window;
        let background = cx.background_executor().clone();
        // The requested paths themselves, kept for the disabled-Code toast
        // below: `projects` only carries the git roots they resolved to, which
        // is not what the user asked to open.
        let requested_path_text = paths
            .first()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let (projects, missing_count) = background
                .spawn(async move {
                    let mut projects: Vec<serde_json::Value> = Vec::new();
                    let mut missing_count = 0usize;
                    for path in paths {
                        match gpui_os_integration_project_root_for_path(&path) {
                            Some(project_root) => projects.push(serde_json::json!({
                                "path": project_root.to_string_lossy(),
                            })),
                            None => missing_count += 1,
                        }
                    }
                    (projects, missing_count)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if missing_count > 0 {
                    this.upsert_gpui_app_toast(
                        GpuiAppToast {
                            copy_text: None,
                            id: "gpui-os-integration-open-missing".to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                            title: "Path does not exist".to_string(),
                            description: Some(
                                "Ghostex could not open a requested path.".to_string(),
                            ),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                            action: None,
                            epoch: 0,
                        },
                        cx,
                    );
                }
                if projects.is_empty() {
                    return;
                }
                this.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "openProjectPaths",
                        "projects": projects,
                    }),
                    cx,
                );
                /*
                CDXC:Extensions 2026-08-23:
                Registering the project is still the right half of an OS open
                request, but with Code turned off in Settings → Customize there
                is no editor to reveal the path in. Keep the project and hand
                back the path instead of switching to a disabled workarea.
                */
                if !this.titlebar_mode_available(TitlebarMode::Source) {
                    this.copy_path_for_disabled_project_workarea(
                        &requested_path_text,
                        "VS Code",
                        cx,
                    );
                    return;
                }
                this.switch_workarea_from_hotkey(TitlebarMode::Source, window, cx);
                this.focus_project_editor_surface(TitlebarMode::Source, window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_os_integration_command_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.gx_store_run_os_integration_command(&message, cx)
    }

    pub(crate) fn handle_gpui_pick_repository_folder_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            #[cfg(target_os = "windows")]
            let picked_path =
                match windows_terminal_backend::wsl_path_for_windows_path(path.as_path()) {
                    Ok(path) => path,
                    Err(_) => return,
                };
            #[cfg(not(target_os = "windows"))]
            let picked_path = path.to_string_lossy().to_string();
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "path": picked_path,
                        "type": "repositoryFolderPicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }
}
