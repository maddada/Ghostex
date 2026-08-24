// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar runtime settings, host-message dispatch, sidebar divider/collapse

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Animation;
use gpui::AnimationExt as _;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::WindowExt;
use gpui_component::notification::Notification;

use crate::app::consts::*;
use crate::app::ffi::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn refresh_sidebar_runtime_settings_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        self.refresh_sidebar_runtime_settings_from_shared_settings(&settings, cx)
    }

    pub(crate) fn refresh_sidebar_runtime_settings_from_shared_settings(
        &mut self,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIProjectSidebarBridge 2026-06-23-08:23:
        GPUI runtime settings polling is intentionally narrow: read the shared sidebar settings snapshot once, pass strict debuggingMode/showBetaFeatures plus the saved object to SidebarApp normalization, skip unchanged payloads, and refresh the sidebar CEF bridge only. Browser CEF tabs, generic settings buses, filesystem watchers, path heuristics, and persisted/logged raw settings data stay out of this path.

        CDXC:GPUISettingsPersistence 2026-06-24-11:14:
        Settings saves use this same sidebar CEF runtime-settings refresh path immediately after the shared service write succeeds. The save path must not wait for polling, add a broad settings event bus, or leak raw Settings JSON into Browser tabs, logs, paths, titles, commands, tokens, stdout/stderr, or user content.

        CDXC:GPUISourceRuntime 2026-06-24-23:17:
        code-server consumes VS Code settings-link choices only at process launch. When shared Settings changes those choices while Source is awake, restart the GPUI-owned runtime through the same lazy Source path instead of mutating a live process or trusting renderer-provided launch flags.

        CDXC:GPUITitlebarKeepAwake 2026-06-25-23:49:
        Keep Awake automation is part of the existing Settings save/runtime refresh path. A saved beta/control disable stops the GPUI-owned hold and suppresses future autostarts, while launch/display/delayed-send rules are re-evaluated immediately without adding a broad Settings event bus.

        CDXC:GPUITitlebarKeepAwake 2026-06-26-00:29:
        Settings refresh also re-evaluates the Working-session automatic hold against app-owned terminal model state. Keep this in the existing narrow refresh path instead of introducing a broad settings or terminal event bus.
        */
        self.sync_gpui_keep_awake_automation_from_settings(settings, cx);
        /*
        CDXC:GlobalActions 2026-08-01-16:00:
        The tab strip draws every frame, so which built-in buttons are visible is
        cached here rather than re-read from the settings file during render.
        This runs before the unchanged-snapshot early return below, because the
        button toggles are not part of the sidebar runtime snapshot that guards
        it — a settings change that only hid a tab strip button would otherwise
        never reach the strip.
        */
        let next_built_in_buttons = settings.tab_strip_built_in_buttons();
        if self.tab_strip_built_in_buttons != next_built_in_buttons {
            self.tab_strip_built_in_buttons = next_built_in_buttons;
            cx.notify();
        }
        let next_snapshot = sidebar_runtime_settings_snapshot_from_shared_settings(settings);
        let Some(next_snapshot) = changed_sidebar_runtime_settings_snapshot(
            &self.sidebar_runtime_settings_snapshot,
            next_snapshot,
        ) else {
            return false;
        };
        let source_code_server_settings_changed =
            SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(
                &self.sidebar_runtime_settings_snapshot,
            ) != SourceCodeServerRuntimeSettings::from_sidebar_runtime_settings(&next_snapshot);

        self.sidebar_runtime_settings_snapshot = next_snapshot.clone();
        if let Some(sidebar) = self.sidebar.clone() {
            let next_snapshot = next_snapshot.clone();
            sidebar.update(cx, |surface, _| {
                surface.refresh_sidebar_runtime_settings(next_snapshot);
            });
        }
        if self.coerce_active_mode_to_available_project_context(cx) {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
        if source_code_server_settings_changed {
            self.restart_source_code_server_runtime_after_settings_change(cx);
        }
        true
    }

    pub(crate) fn refresh_sidebar_gxserver_bootstrap_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.refresh_sidebar_gxserver_bootstrap(false, cx)
    }

    pub(crate) fn replay_sidebar_gxserver_bootstrap(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.refresh_sidebar_gxserver_bootstrap(true, cx)
    }

    pub(crate) fn refresh_sidebar_gxserver_bootstrap(
        &mut self,
        force_replay: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
        Reuse the existing narrow sidebar polling cadence to notice gxserver token bootstrap availability after load. The poll reads only the existing token helper, fixed local gxserver constants, the current explicit sidebar active-project id, and the exact local focus key when it matches the stored focused session. Update only the sidebar CEF bridge on actual snapshot change and do not add file watchers, logs, persistence, Browser/workarea/modal exposure, fake gxserver sessions, or fallback project/session id inference.
        */
        let next_bootstrap = gpui_sidebar_gxserver_bootstrap(
            self.latest_sidebar_project_snapshot.as_ref(),
            &self.sidebar_gxserver_presentation_focus_state,
            self.local_workspace_latest_focus_key.as_ref(),
        );
        if !force_replay && self.sidebar_gxserver_bootstrap == next_bootstrap {
            return false;
        }

        self.sidebar_gxserver_bootstrap = next_bootstrap.clone();
        if let Some(sidebar) = self.sidebar.clone() {
            sidebar.update(cx, |surface, _| {
                surface.refresh_sidebar_gxserver_bootstrap(next_bootstrap.clone());
            });
        }
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Session Chat surfaces carry either the local loopback bootstrap or the
        owning remote machine's loopback SSH-tunnel bootstrap. Refresh each
        surface from its session identity so a local bootstrap replay cannot
        redirect an already-open remote chat to the local daemon.
        */
        for (session_id, surface) in &self.agents_chat_surfaces {
            let bootstrap = self.agents_session_chat_gxserver_bootstrap(*session_id);
            surface.update(cx, |surface, _| {
                surface.refresh_session_chat_gxserver_bootstrap(bootstrap);
            });
        }
        self.reconcile_agents_pane_surfaces(cx);
        true
    }

    /// Worktree modal commands cross from the app-modal window into the
    /// sidebar runtime through a fixed type + field allowlist. The runtime
    /// revalidates project/worktree identity against gxserver state before any
    /// mutation, so this forward carries only the shared modal contract fields
    /// and never invents scope.
    pub(crate) fn forward_gpui_worktree_modal_command_to_sidebar(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let allowed_string_fields: &[&str] = match command_type {
            "requestProjectWorktrees" => {
                &["requestId", "projectId", "projectPath", "remoteMachineId"]
            }
            "createProjectWorktree" => &[
                "agentId",
                "baseBranch",
                "existingWorktreeKey",
                "existingWorktreePath",
                "mode",
                "projectId",
                "projectPath",
                "prompt",
                "remoteMachineId",
            ],
            "confirmDeleteWorktree" => &["projectId"],
            /*
            CDXC:WorktreeRename 2026-08-09-18:40:
            The rename confirmation carries the typed name across the modal
            boundary. It is NOT a path: the runtime revalidates the project and
            gxserver derives the destination folder from the name itself, so this
            forward can never point a move at a directory of the caller's
            choosing. `renameBranch` is a boolean and needs the explicit block
            below — the string loop would drop it silently.
            */
            "confirmRenameWorktree" => &["projectId", "name"],
            "commitWorktreeBeforeDelete" => &["groupId"],
            _ => return false,
        };
        let mut message = serde_json::Map::new();
        message.insert("type".to_string(), serde_json::json!(command_type));
        for field in allowed_string_fields {
            if let Some(value) = command.get(*field).and_then(serde_json::Value::as_str) {
                message.insert((*field).to_string(), serde_json::json!(value));
            }
        }
        if command_type == "confirmDeleteWorktree" {
            for field in ["deleteLocalBranch", "deleteRemoteBranch"] {
                if let Some(value) = command.get(field).and_then(serde_json::Value::as_bool) {
                    message.insert(field.to_string(), serde_json::json!(value));
                }
            }
        }
        if command_type == "confirmRenameWorktree" {
            if let Some(value) = command
                .get("renameBranch")
                .and_then(serde_json::Value::as_bool)
            {
                message.insert("renameBranch".to_string(), serde_json::json!(value));
            }
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_worktree_modal_command_script(&serde_json::Value::Object(message));
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /// Git review commands originate in the owned app-modal window, while the
    /// pending review and gxserver client live in the sidebar runtime. Forward
    /// only the shared review contract so the runtime can revalidate request
    /// ids and selected paths against its gxserver-derived pending request.
    pub(crate) fn forward_gpui_git_commit_modal_command_to_sidebar(
        &mut self,
        command_type: &str,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let allowed_string_fields: &[&str] = match command_type {
            "confirmSidebarGitCommit" | "confirmSidebarGitDirectMerge" => {
                &["agentId", "message", "requestId"]
            }
            "runSidebarGitMultipleCommits" => &["agentId", "requestId"],
            "openSidebarGitChangedFileDiff" => &["filePath", "requestId"],
            "cancelSidebarGitCommit" => &["requestId"],
            _ => return false,
        };
        let mut message = serde_json::Map::new();
        message.insert("type".to_string(), serde_json::json!(command_type));
        for field in allowed_string_fields {
            if let Some(value) = command.get(*field).and_then(serde_json::Value::as_str) {
                message.insert((*field).to_string(), serde_json::json!(value));
            }
        }
        if matches!(
            command_type,
            "confirmSidebarGitCommit" | "confirmSidebarGitDirectMerge"
        ) {
            for field in ["commitOnNewRef", "deleteWorktreeAfter"] {
                if let Some(value) = command.get(field).and_then(serde_json::Value::as_bool) {
                    message.insert(field.to_string(), serde_json::json!(value));
                }
            }
            if let Some(file_paths) = command
                .get("filePaths")
                .and_then(serde_json::Value::as_array)
            {
                let file_paths = file_paths
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                message.insert("filePaths".to_string(), serde_json::json!(file_paths));
            }
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_git_commit_modal_command_script(&serde_json::Value::Object(message));
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /// "Start new conversation" in the Export Transcript result dialog. The
    /// dialog only knows which configured agent the user picked; the sidebar
    /// runtime owns the exported path, the source project, and the gxserver
    /// create-session path, so forward the chosen agent id and nothing else.
    pub(crate) fn forward_gpui_export_transcript_modal_command_to_sidebar(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = serde_json::Map::new();
        message.insert(
            "type".to_string(),
            serde_json::json!("startExportedTranscriptConversation"),
        );
        if let Some(agent_id) = command.get("agentId").and_then(serde_json::Value::as_str) {
            message.insert("agentId".to_string(), serde_json::json!(agent_id));
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_export_transcript_modal_command_script(&serde_json::Value::Object(message));
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /// Reveal the exported markdown file in the OS file manager. The path comes
    /// from the Rust-held open payload of the dialog that is asking, never from
    /// the modal page's own message, and remote exports hold no local path.
    pub(crate) fn reveal_gpui_exported_transcript(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.pending_export_transcript_reveal_path.clone() else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move { gpui_reveal_path_in_finder(Path::new(&path)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(message) = result {
                    this.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not reveal the exported transcript",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn dispatch_gpui_sidebar_host_message(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-24-23:49:
        Command-pane Action run-state feedback targets only the first-party GPUI sidebar CEF surface and the typed `window.ghostexGpui.onSidebarHostMessage` callback installed by the SidebarApp runtime. The generated script carries only existing sidebar message JSON and must not expose generic eval IPC, command text, paths, terminal output, status-file paths, tokens, or persisted shell-state fields.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_host_message_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn handle_gpui_pick_workspace_folder_message(&mut self, cx: &mut gpui::Context<Self>) {
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
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_workspace_folder_picked_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    /// GPUI port of the macOS OS-integration entry points
    /// (`application(_:open:)` → `handleOSIntegrationURL` +
    /// `dispatchOSIntegrationFileOpenPaths`, AppDelegate.swift). URLs arrive
    /// through gpui's `application:openURLs:` delegate (`cx.on_open_urls`):
    /// `ghostex://terminal|open|edit` actions plus Finder Open-With file://
    /// opens. `.command/.tool/.sh` files never execute without the
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
        if action == "terminal" {
            self.open_gpui_os_integration_quick_terminal(
                query_value("command"),
                query_value("cwd"),
                query_value("title"),
                cx,
            );
            return;
        }
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

    /// `ghostex://terminal?command&cwd&title` → a terminal session in the
    /// project registered at cwd. macOS creates a client-side projectless
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
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let resolved_cwd = background
                .spawn(async move { gpui_os_integration_resolved_terminal_cwd(cwd) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.switch_workarea_from_hotkey(TitlebarMode::Agents, window, cx);
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
    #[cfg(target_os = "macos")]
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
                            id: "gpui-os-integration-open-missing".to_string(),
                            level: GpuiAppToastLevel::from_raw(Some("warning")),
                            title: "Path does not exist".to_string(),
                            description: Some(
                                "Ghostex could not open a requested path.".to_string(),
                            ),
                            loading: false,
                            persistent: false,
                            duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
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
                CDXC:DisabledPluginRouting 2026-08-23:
                Registering the project is still the right half of an OS open
                request, but with Code turned off in Settings → Customize there
                is no editor to reveal the path in. Keep the project and hand
                back the path instead of switching to a disabled workarea.
                */
                if !this.titlebar_mode_available(TitlebarMode::Source) {
                    this.copy_path_for_disabled_project_workarea(
                        &requested_path_text,
                        "Code",
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
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_os_integration_command_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn handle_gpui_pick_repository_folder_message(&mut self, cx: &mut gpui::Context<Self>) {
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

    pub(crate) fn handle_gpui_list_app_icons_message(&mut self, cx: &mut gpui::Context<Self>) {
        let source_id = app_icon::source_id_from_settings(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            app_icon::list_state(&source_id),
            cx,
        );
    }

    pub(crate) fn handle_gpui_set_app_icon_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(source_id) = message.get("sourceId").and_then(serde_json::Value::as_str) else {
            return;
        };
        let current_source_id = app_icon::source_id_from_settings(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        self.dispatch_open_gpui_app_modal_sidebar_state_payload(
            app_icon::select_state(source_id, &current_source_id),
            cx,
        );
    }

    pub(crate) fn handle_gpui_pick_app_icon_file_message(&mut self, cx: &mut gpui::Context<Self>) {
        let current_source_id = app_icon::source_id_from_settings(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Icon".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let state = app_icon::picked_file_state(&path, &current_source_id);
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_sidebar_state_payload(state, cx);
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_pick_terminal_background_image_message(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Image".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "type": "terminalBackgroundImageFilePicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn handle_gpui_pick_worktree_images_message(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach Images".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let paths: Vec<String> = paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            if paths.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "paths": paths,
                        "type": "worktreeImageFilesPicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn next_local_workspace_lifecycle_request_id(&mut self) -> Option<u64> {
        let (request_id, next_request_id) =
            next_available_gpui_local_workspace_lifecycle_request_id(
                self.next_local_workspace_lifecycle_request_id,
                |candidate| {
                    self.local_workspace_lifecycle_requests
                        .contains_key(&candidate)
                },
            )?;
        self.next_local_workspace_lifecycle_request_id = next_request_id;
        Some(request_id)
    }

    pub(crate) fn dispatch_gpui_workspace_terminal_lifecycle_request(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
        Native GPUI workspace tab lifecycle uses a fixed Rust-to-sidebar callback, not a generic renderer bus. The request contains only request id, action, bounded gxserver project/session ids, and optional replacement ids so the sidebar can perform gxserver lifecycle ownership while Rust keeps pane/tab ownership local.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_workspace_terminal_lifecycle_request_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_gpui_native_app_shot_capture(
        &mut self,
        capture: GpuiAppShotCapture,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.dispatch_gpui_native_app_shot_capture(capture, cx) {
            return;
        }
        window.push_notification(
            Notification::warning("App Shot captured, but the GPUI sidebar is not ready."),
            cx,
        );
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "App Shot Failed",
            "The GPUI sidebar is not ready to stage the App Shot.",
            cx,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_native_app_shot_capture(
        &mut self,
        capture: GpuiAppShotCapture,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUIAppShots 2026-06-25-23:28:
        Native Rust owns App Shot capture, path creation, and settings reads. CEF receives only a transient first-party capture payload so the gxserver sidebar runtime can format the macOS-parity prompt, try focused/recent existing-session insertion, or create a prompt-agent session; this capture bridge must not accept renderer-provided screenshot paths, persist capture data, log app/window/path text, or become generic eval IPC.

        CDXC:GPUIAppShots 2026-06-26-04:27:
        Focused/recent App Shot staging may target a remote row only through the separate fixed prompt bridge and an already-mounted remote attach Agents surface. Capture metadata remains first-party and cannot authorize renderer paths, SSH details, URLs, tokens, commands, output, or terminal text.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let mut payload = serde_json::Map::new();
        payload.insert(
            "type".to_string(),
            serde_json::Value::String(GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_TYPE.to_string()),
        );
        payload.insert(
            "version".to_string(),
            serde_json::json!(GPUI_SIDEBAR_NATIVE_APP_SHOT_MESSAGE_VERSION),
        );
        payload.insert(
            "appName".to_string(),
            serde_json::Value::String(capture.app_name),
        );
        payload.insert(
            "imagePath".to_string(),
            serde_json::Value::String(capture.image_path),
        );
        if let Some(bundle_identifier) = capture.bundle_identifier {
            payload.insert(
                "bundleIdentifier".to_string(),
                serde_json::Value::String(bundle_identifier),
            );
        }
        if let Some(window_title) = capture.window_title {
            payload.insert(
                "windowTitle".to_string(),
                serde_json::Value::String(window_title),
            );
        }
        if let Some(window_width) = capture.window_width {
            payload.insert("windowWidth".to_string(), serde_json::json!(window_width));
        }
        if let Some(window_height) = capture.window_height {
            payload.insert("windowHeight".to_string(), serde_json::json!(window_height));
        }
        if let Some(trigger) = capture.trigger {
            payload.insert("trigger".to_string(), serde_json::Value::String(trigger));
        }
        let script = gpui_native_app_shot_capture_script(&serde_json::Value::Object(payload));
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn refresh_sidebar_command_pane_sessions_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let sessions = self.command_pane.sidebar_command_session_sources(
            self.shell_focus == ShellFocusTarget::CommandPane,
            &self.command_delayed_send_timers,
            &self.command_close_after_done_timers,
            SystemTime::now(),
        );
        let snapshot = sessions.to_string();
        if self.sidebar_command_pane_sessions_snapshot == snapshot {
            return false;
        }
        if !self.dispatch_gpui_sidebar_command_pane_sessions(&sessions, cx) {
            return false;
        }
        self.sidebar_command_pane_sessions_snapshot = snapshot;
        true
    }

    pub(crate) fn refresh_sidebar_agents_delayed_sends_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.prune_local_workspace_session_mappings();
        let now_system = SystemTime::now();
        let now_instant = Instant::now();
        let mut sessions = self
            .local_workspace_session_mappings
            .iter()
            .filter_map(|(key, shell_session_id)| {
                let external_session_id =
                    gpui_combined_presentation_session_id(&key.project_id, &key.session_id);
                if let Some(timer) = self
                    .agents_delayed_send_timers
                    .get(shell_session_id)
                    .copied()
                {
                    let remaining_ms = timer.remaining_ms(now_system);
                    return Some(serde_json::json!({
                        "delayedSendDeadlineAt": gpui_iso8601_utc(timer.deadline_at),
                        "delayedSendRemainingLabel":
                            gpui_command_delayed_send_countdown_label(remaining_ms),
                        "delayedSendRemainingMs": remaining_ms,
                        "sessionId": external_session_id,
                    }));
                }
                let watcher = self
                    .agents_send_when_stopped_watchers
                    .get(shell_session_id)?;
                let is_working = self.gpui_agents_send_when_stopped_scope_is_working(
                    *shell_session_id,
                    &watcher.scope,
                )?;
                Some(serde_json::json!({
                    "delayedSendRemainingLabel": gpui_agents_send_when_stopped_remaining_label(
                        watcher,
                        is_working,
                        now_instant,
                    ),
                    "sendWhenAllProjectSessionsStopActive": matches!(
                        &watcher.scope,
                        GpuiAgentsSendWhenStoppedScope::Project(_)
                    ),
                    "sendWhenAgentStopsActive": matches!(
                        &watcher.scope,
                        GpuiAgentsSendWhenStoppedScope::Session
                    ),
                    "sessionId": external_session_id,
                }))
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            left.get("sessionId")
                .and_then(serde_json::Value::as_str)
                .cmp(&right.get("sessionId").and_then(serde_json::Value::as_str))
        });
        let sessions = serde_json::Value::Array(sessions);
        let snapshot = sessions.to_string();
        if self.sidebar_agents_delayed_sends_snapshot == snapshot {
            return false;
        }
        if !self.dispatch_gpui_sidebar_agents_delayed_sends(&sessions, cx) {
            return false;
        }
        self.sidebar_agents_delayed_sends_snapshot = snapshot;
        true
    }

    /*
    CDXC:SidebarBrowserTabReveal 2026-08-18:
    Record that a newly opened Browser tab should be revealed in the sidebar.
    Only the tab identity is stored; the reveal itself is sent once the tab has
    actually been published to the sidebar, so the sidebar never receives a
    request for a row it cannot resolve. A second open replaces an unsent
    request because only the newest tab is worth revealing.
    */
    pub(crate) fn request_sidebar_browser_tab_reveal(&mut self, tab_id: BrowserTabId) {
        let Some(project_id) = self.browser_tabs_project_id.clone() else {
            return;
        };
        self.pending_sidebar_browser_tab_reveal =
            Some(PendingSidebarBrowserTabReveal { project_id, tab_id });
    }

    pub(crate) fn dispatch_pending_sidebar_browser_tab_reveal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(pending) = self.pending_sidebar_browser_tab_reveal.take() else {
            return;
        };
        self.sidebar_browser_tab_reveal_request_id =
            self.sidebar_browser_tab_reveal_request_id.wrapping_add(1);
        let request_id = self.sidebar_browser_tab_reveal_request_id;
        let script = gpui_sidebar_reveal_browser_tab_script(&serde_json::json!({
            "projectId": pending.project_id,
            "requestId": request_id,
            "tabId": pending.tab_id.0.to_string(),
        }));
        if let Some(sidebar) = self.sidebar.clone() {
            sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script));
        }
    }

    pub(crate) fn refresh_gpui_sidebar_browser_tabs_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let browser_mode_is_visible = self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser);
        let focused_browser_tab_id = if browser_mode_is_visible {
            match self.shell_focus {
                ShellFocusTarget::BrowserPane(pane_id) => {
                    self.browser_tabs.active_tab_id_for_pane(pane_id)
                }
                ShellFocusTarget::BrowserSurface => self
                    .browser_tabs
                    .active_tab_id_for_pane(self.browser_tabs.focused_pane),
                _ => None,
            }
        } else {
            None
        };
        let active_browser_project_id = self.browser_tabs_project_id.clone();
        let active_browser_surface_tab_ids = self
            .browser_surfaces
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let mut projects = self
            .parked_browser_tabs_by_project
            .iter()
            .filter(|(project_id, _)| self.browser_tabs_project_id.as_ref() != Some(*project_id))
            .map(|(project_id, tabs)| (project_id.as_str(), tabs))
            .collect::<Vec<_>>();
        if let Some(project_id) = self.browser_tabs_project_id.as_deref() {
            projects.push((project_id, &self.browser_tabs));
        }
        projects.sort_by(|left, right| left.0.cmp(right.0));

        let tabs = projects
            .into_iter()
            .flat_map(|(project_id, model)| {
                let project_is_active = active_browser_project_id.as_deref() == Some(project_id);
                let awake_tab_ids = if project_is_active {
                    active_browser_surface_tab_ids.clone()
                } else {
                    HashSet::new()
                };
                let visible_tab_ids = model
                    .rendered_leaf_order()
                    .into_iter()
                    .filter_map(|pane_id| model.active_tab_id_for_pane(pane_id))
                    .collect::<HashSet<_>>();
                /*
                CDXC:GPUIBrowserSidebarSessions 2026-07-12:
                Only loaded tabs project as sidebar browser sessions. The
                address-only "New Tab" placeholder (including the in-place
                reset left behind by closing the last tab) stays out of the
                sidebar, so closing the last browser tab removes its sidebar
                row, and committing an address in the browser address bar
                surfaces the loaded tab as a fresh sidebar session.
                */
                model
                    .tabs
                    .iter()
                    .filter(|tab| tab.state == BrowserTabState::Loaded)
                    .map(move |tab| {
                        serde_json::json!({
                            "isActive": project_is_active && focused_browser_tab_id == Some(tab.id),
                            "isSleeping": !awake_tab_ids.contains(&tab.id),
                            "isVisible": browser_mode_is_visible
                                && project_is_active
                                && visible_tab_ids.contains(&tab.id),
                            "projectId": project_id,
                            "tabId": tab.id.0.to_string(),
                            "faviconUrl": tab
                                .runtime_favicon_fetch
                                .as_ref()
                                .map(|source| source.url.as_str()),
                            "title": tab.display_title(),
                            "url": tab.address_value(),
                        })
                    })
            })
            .collect::<Vec<_>>();
        /*
        CDXC:SidebarBrowserTabReveal 2026-08-18:
        A reveal may only be sent for a tab this snapshot actually carries;
        otherwise the sidebar would be asked to expand and scroll to a row it
        has never been told about. A pending reveal whose tab is missing from a
        published snapshot is dropped, because the tab it named is gone.
        */
        let pending_reveal_is_published =
            self.pending_sidebar_browser_tab_reveal
                .as_ref()
                .is_some_and(|pending| {
                    let pending_tab_id = pending.tab_id.0.to_string();
                    tabs.iter().any(|tab| {
                        tab.get("projectId").and_then(serde_json::Value::as_str)
                            == Some(pending.project_id.as_str())
                            && tab.get("tabId").and_then(serde_json::Value::as_str)
                                == Some(pending_tab_id.as_str())
                    })
                });
        let snapshot = serde_json::Value::Array(tabs).to_string();
        if self.sidebar_browser_tabs_snapshot == snapshot {
            if pending_reveal_is_published {
                self.dispatch_pending_sidebar_browser_tab_reveal(cx);
            }
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_browser_tabs_script(&snapshot);
        if !sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script)) {
            return false;
        }
        self.sidebar_browser_tabs_snapshot = snapshot;
        if pending_reveal_is_published {
            self.dispatch_pending_sidebar_browser_tab_reveal(cx);
        } else {
            self.pending_sidebar_browser_tab_reveal = None;
        }
        true
    }

    pub(crate) fn refresh_gpui_sidebar_displayed_sessions_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:AutoSleepDisplayedSessions 2026-08-20:
        Auto Sleep ("Sleep idle agent sessions") runs in the sidebar runtime,
        which only knows which rows it last saw selected. That is not the same
        thing as what this shell is rendering: a session switched to Chat view
        parks its terminal behind the chat surface, and a gxserver reconnect
        drops the runtime's local focus/visible sets entirely. Either way an
        idle agent the user is sitting in front of looked retirable.

        Rust is the only party that knows what is on screen, so it publishes
        that set (Agents rendered leaves in Agents mode, companion terminal
        mount slots in the project-editor modes — chat-mode sessions included,
        because the tab still owns its pane) and the sweep protects it. The
        bridge carries bounded local gxserver session ids only: no titles,
        paths, commands, terminal output, or project bodies.
        */
        let session_ids = self
            .gpui_sidebar_visible_local_session_ids()
            .into_iter()
            .filter(|session_id| {
                gpui_sidebar_local_gxserver_session_id_allowed(session_id.as_str())
            })
            .collect::<Vec<_>>();
        let snapshot = serde_json::Value::Array(
            session_ids
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        )
        .to_string();
        if self.sidebar_displayed_sessions_snapshot == snapshot {
            return false;
        }
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_displayed_sessions_script(&snapshot);
        if !sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script)) {
            return false;
        }
        self.sidebar_displayed_sessions_snapshot = snapshot;
        true
    }

    pub(crate) fn dispatch_gpui_sidebar_command_pane_sessions(
        &mut self,
        sessions: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:50:
        Command-pane session indicators use a dedicated first-party sidebar bridge callback and cached `window.ghostexGpui.commandPaneSessions` value so restored tabs can hydrate before React installs listeners. The script may carry only sanitized session summaries, never action command text, cwd, env, status-file paths, terminal output, or project paths.
        */
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_command_pane_sessions_script(sessions);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn dispatch_gpui_sidebar_agents_delayed_sends(
        &mut self,
        sessions: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let script = gpui_sidebar_agents_delayed_sends_script(sessions);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn dispatch_gpui_sidebar_command_run_state(
        &mut self,
        command_id: &str,
        run_id: &str,
        state: GpuiSidebarCommandRunState,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.sidebar_command_run_feedback_states
            .entry(command_id.to_string())
            .or_default()
            .apply_run_state(run_id, state);
        self.dispatch_gpui_sidebar_host_message(
            serde_json::json!({
                "commandId": command_id,
                "runId": run_id,
                "state": state.as_str(),
                "type": "sidebarCommandRunStateChanged",
            }),
            cx,
        )
    }

    pub(crate) fn dispatch_gpui_sidebar_command_run_state_cleared(
        &mut self,
        command_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.sidebar_command_run_feedback_states.remove(command_id);
        self.dispatch_gpui_sidebar_host_message(
            serde_json::json!({
                "commandId": command_id,
                "type": "sidebarCommandRunStateCleared",
            }),
            cx,
        )
    }

    pub(crate) fn dispatch_gpui_command_action_completions(
        &mut self,
        completions: Vec<CommandPaneActionRunCompletion>,
        cx: &mut gpui::Context<Self>,
    ) {
        for completion in completions {
            self.dispatch_gpui_sidebar_command_run_state(
                &completion.command_id,
                &completion.run_id,
                completion.run_state(),
                cx,
            );
            if let Some(action) =
                gpui_project_board_action_for_command_id(completion.command_id.as_str())
            {
                self.dispatch_gpui_project_board_command_completed(
                    action,
                    completion.exit_code,
                    cx,
                );
            }
            if completion.should_play_completion_sound() {
                let _ = gpui_play_completion_sound(gpui_action_completion_sound_from_settings());
            }
            /*
            CDXC:GPUIDesktopControlSettings 2026-08-09:
            Cua Driver install/update runs as a normal command Action, so its
            exit is the only honest completion signal. Complete the bundled
            Ghostex Computer Use skill step and refresh Settings from that exit
            code instead of guessing while the command is still running.
            */
            let cua_driver_update = match completion.command_id.as_str() {
                GPUI_CUA_DRIVER_INSTALL_COMMAND_ID => Some(false),
                GPUI_CUA_DRIVER_UPDATE_COMMAND_ID => Some(true),
                _ => None,
            };
            if let Some(was_update) = cua_driver_update {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::FinishDesktopControlSetup {
                        driver_installed: completion.exit_code == 0,
                        was_update,
                    },
                    cx,
                );
            }
            self.close_completed_gpui_command_action_tab_if_requested(&completion, cx);
        }
    }

    pub(crate) fn close_completed_gpui_command_action_tab_if_requested(
        &mut self,
        completion: &CommandPaneActionRunCompletion,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICommandPaneActions 2026-06-26-04:59:
        Native command-pane Actions keep completed tabs reusable even when older Action definitions requested close-on-exit. Keep this completion close helper as a stale-record guard only; current runtime completions normalize close-on-exit to false and must not remove the Action-owned command tab after sidebar feedback.
        */
        let Some(completed_tab) = self.command_pane.close_completed_action_run_tab(completion)
        else {
            return false;
        };
        self.forget_command_gxserver_session_for_closed_tab(completed_tab.session_id, cx);
        self.prune_gpui_command_delayed_send_timers_for_command_model();
        self.prune_gpui_command_close_after_done_timers_for_command_model();
        if self.command_pane.has_sessions() {
            self.focus_command_pane();
        } else {
            self.restore_previous_non_command_focus_or_default();
        }
        self.scroll_command_group_active_tab(completed_tab.group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn close_gpui_sidebar_command_run(
        &mut self,
        command_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:34:
        Ending a sidebar command run closes only the live command-pane Action tab mapped to that command id and clears sidebar button feedback. The tab is removed from the command-pane model immediately (macOS command close parity); render reconciliation drops any mounted surface. This path must not inspect command text, terminal output, titles, status-file contents, paths, URLs, or persisted shell JSON.
        */
        let slot = self
            .command_pane
            .take_action_session_slot_for_action_close(command_id);
        self.dispatch_gpui_sidebar_command_run_state_cleared(command_id, cx);
        let Some(slot) = slot else {
            cx.notify();
            return false;
        };
        let slot = CommandTerminalBodyMountSlotId {
            group_id: slot.0,
            session_id: slot.1,
        };
        self.clear_gpui_command_delayed_send_timer(slot.session_id);
        self.clear_gpui_command_close_after_done_timer(slot.session_id);
        if !self
            .command_pane
            .close_session(slot.group_id, slot.session_id)
        {
            cx.notify();
            return false;
        }
        self.forget_command_gxserver_session_for_closed_tab(slot.session_id, cx);
        self.clear_command_resize_hover_state_if_command_pane_hidden();
        if self.command_pane.has_sessions() {
            self.focus_command_pane();
        } else {
            self.restore_previous_non_command_focus_or_default();
        }
        self.scroll_command_group_active_tab(slot.group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn sleep_project_editor_mode_from_timer(
        &mut self,
        mode: TitlebarMode,
        token: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_auto_sleep_epochs.epoch(mode) != Some(token)
            || self.active_mode == mode
            || !self.project_editor_shell.is_mode_awake(mode)
        {
            return;
        }

        if !self.project_editor_shell.mark_mode_sleeping(mode) {
            return;
        }
        self.project_editor_auto_sleep_epochs.bump(mode);
        if mode == TitlebarMode::Browser {
            self.update_active_mode_cef_child_visibility(cx);
        }
        if mode == TitlebarMode::Source {
            /*
            macOS `stopCodeServerRuntimeIfEveryEditorSleeping` parity: when the
            last awake Source surface sleeps, the shared code-server process
            exits instead of idling hidden. GPUI's single workspace window has
            exactly one Source surface, so Source-mode sleep IS "every editor
            sleeping"; the click-to-wake path relaunches the runtime through
            the existing ensure/start pipeline.
            */
            self.stop_source_code_server_runtime(cx);
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn render_sidebar_resize_divider(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let line_on_right = self.sidebar_side == GpuiSidebarSide::Left;
        div()
            .id("ghostex-gpui-sidebar-resize-divider")
            .relative()
            .flex_shrink_0()
            .w(px(SIDEBAR_DIVIDER_WIDTH))
            .h_full()
            .cursor_ew_resize()
            .bg(sidebar_background_fill())
            .on_hover(cx.listener(|this, hovered, _, cx| {
                this.set_sidebar_divider_hovering(*hovered, cx);
            }))
            .on_mouse_move(cx.listener(|this, _event: &MouseMoveEvent, _window, cx| {
                this.set_sidebar_divider_hovering(true, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.handle_sidebar_divider_mouse_down(event, window, cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .h_full()
                    .w(px(SIDEBAR_DIVIDER_LINE_WIDTH))
                    .cursor_ew_resize()
                    .bg(sidebar_divider_line_color())
                    .when(line_on_right, |this| this.right_0())
                    .when(!line_on_right, |this| this.left_0()),
            )
            .when(self.sidebar_divider_hover_visible, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .h_full()
                        .w(px(SIDEBAR_DIVIDER_HOVER_LINE_WIDTH))
                        .cursor_ew_resize()
                        .bg(sidebar_divider_hover_line_color())
                        .when(line_on_right, |this| this.right_0())
                        .when(!line_on_right, |this| this.left_0())
                        .with_animation(
                            "ghostex-gpui-sidebar-resize-divider-hover-line",
                            Animation::new(SIDEBAR_DIVIDER_HOVER_FADE_DURATION)
                                .with_easing(gpui::ease_out_quint()),
                            |line, delta| line.opacity(delta),
                        ),
                )
            })
    }

    pub(crate) fn set_sidebar_divider_hovering(&mut self, hovered: bool, cx: &mut gpui::Context<Self>) {
        if self.sidebar_divider_hovering == hovered {
            if !hovered && self.sidebar_divider_hover_visible {
                self.sidebar_divider_hover_visible = false;
                cx.notify();
            }
            return;
        }

        self.sidebar_divider_hover_epoch = self.sidebar_divider_hover_epoch.wrapping_add(1);
        self.sidebar_divider_hovering = hovered;

        if !hovered {
            self.sidebar_divider_hover_visible = false;
            cx.notify();
            return;
        }

        self.sidebar_divider_hover_visible = false;
        let epoch = self.sidebar_divider_hover_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SIDEBAR_DIVIDER_HOVER_DELAY)
                .await;

            let _ = this.update(cx, |this, cx| {
                if this.sidebar_divider_hover_epoch == epoch && this.sidebar_divider_hovering {
                    this.sidebar_divider_hover_visible = true;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn handle_sidebar_root_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.handle_sidebar_drag_move(event, window, cx);

        let hovering = self.sidebar_drag.is_some()
            || self.sidebar_divider_contains_mouse_position(event, window);
        self.set_sidebar_divider_hovering(hovering, cx);
    }

    pub(crate) fn sidebar_divider_contains_mouse_position(
        &self,
        event: &MouseMoveEvent,
        window: &Window,
    ) -> bool {
        self.sidebar_divider_contains_position(event.position, window)
    }

    pub(crate) fn sidebar_divider_contains_position(
        &self,
        position: gpui::Point<Pixels>,
        window: &Window,
    ) -> bool {
        if !gpui_sidebar_chrome_visible(self.sidebar_collapsed) {
            return false;
        }
        let x = position.x.as_f32();
        let y = position.y.as_f32();
        let (start_x, end_x) = gpui_sidebar_divider_x_bounds(
            self.sidebar_side,
            window.bounds().size.width.as_f32(),
            self.sidebar_width,
        );

        y >= TITLEBAR_HEIGHT && x >= start_x && x <= end_x
    }

    pub(crate) fn handle_sidebar_divider_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        window.prevent_default();
        cx.stop_propagation();

        if event.click_count >= 2 {
            self.reset_sidebar_width(window);
            cx.notify();
            return;
        }

        self.sidebar_drag = Some(SidebarDragState {
            start_x: event.position.x.as_f32(),
            start_width: self.sidebar_width,
        });
        self.set_sidebar_divider_hovering(true, cx);
    }

    pub(crate) fn handle_sidebar_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(drag) = self.sidebar_drag else {
            return;
        };

        if !event.dragging() {
            self.finish_sidebar_drag(cx);
            return;
        }

        window.prevent_default();
        cx.stop_propagation();

        let max_width = current_sidebar_max_width(window);
        let delta =
            gpui_sidebar_resize_delta(self.sidebar_side, event.position.x.as_f32(), drag.start_x);
        let next_width = clamp_sidebar_width(drag.start_width + delta, max_width);
        if (next_width - self.sidebar_width).abs() >= 0.5 {
            self.sidebar_width = next_width;
            cx.notify();
        }
    }

    pub(crate) fn handle_sidebar_drag_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.sidebar_drag.is_some() {
            window.prevent_default();
            cx.stop_propagation();
        }
        self.finish_sidebar_drag(cx);
    }

    pub(crate) fn finish_sidebar_drag(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar_drag.take().is_none() {
            return;
        }
        self.clear_sidebar_divider_hover_state();
        persist_sidebar_width_setting(self.sidebar_width);
        cx.notify();
    }

    pub(crate) fn reset_sidebar_width(&mut self, window: &Window) {
        let max_width = current_sidebar_max_width(window);
        let reset_width = read_sidebar_default_width_setting().unwrap_or(SIDEBAR_RESET_WIDTH);
        self.sidebar_width = clamp_sidebar_width(reset_width, max_width);
        self.cancel_sidebar_divider_interaction_state();
        persist_sidebar_width_setting(self.sidebar_width);
    }

    pub(crate) fn cancel_sidebar_divider_interaction_state(&mut self) {
        self.sidebar_drag = None;
        self.clear_sidebar_divider_hover_state();
    }

    pub(crate) fn clear_sidebar_divider_hover_state(&mut self) {
        self.sidebar_divider_hovering = false;
        self.sidebar_divider_hover_visible = false;
        self.sidebar_divider_hover_epoch = self.sidebar_divider_hover_epoch.wrapping_add(1);
    }

    pub(crate) fn toggle_gpui_sidebar_collapsed(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISidebarCollapse 2026-06-26-10:04:
        GPUI Cmd+B and the shared `toggleSidebarCollapsed` action collapse only shell chrome state. Preserve `sidebar_width` and cancel divider interaction state so expanding restores the user's resized sidebar without writing a zero-width setting or leaving stale hover/drag chrome active.
        */
        self.sidebar_collapsed = gpui_next_sidebar_collapsed_state(self.sidebar_collapsed);
        self.cancel_sidebar_divider_interaction_state();
        self.update_sidebar_cef_surface_visibility(cx);
        cx.notify();
    }

    pub(crate) fn move_gpui_sidebar_to_other_side(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISidebarSide 2026-06-26-23:35:
        `moveSidebar` changes only GPUI sidebar placement and persists the shared `sidebarSide` value. Cancel divider drag/hover state at the move boundary so the visible divider cannot keep stale geometry from the old side.
        */
        self.sidebar_side = gpui_next_sidebar_side(self.sidebar_side);
        self.cancel_sidebar_divider_interaction_state();
        write_gpui_sidebar_side_to_shared_settings(self.sidebar_side);
        cx.notify();
    }

    pub(crate) fn update_sidebar_cef_surface_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUISidebarCollapse 2026-07-05:
        Sidebar collapse still removes the sidebar and divider from normal GPUI layout on the next render, but the native CEF child view must hide/show immediately at the toggle boundary. This keeps the titlebar button visually instant without adding overlays, zero-width fallbacks, hit-test rerouting, or persisting a collapsed width.
        */
        let visible = gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        if let Some(sidebar) = self.sidebar.clone() {
            sidebar.update(cx, |surface, _| surface.set_visible(visible));
        }
    }

    pub(crate) fn apply_gpui_sidebar_side_from_saved_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        // The Settings dropdown persists sidebarSide through the patch path;
        // a save whose side differs from the live placement applies the same
        // flip as the moveSidebar command instead of waiting for relaunch.
        // The already-saved snapshot is the source, so nothing is re-written.
        let saved_side = gpui_sidebar_side_from_shared_settings(settings_snapshot);
        if saved_side == self.sidebar_side {
            return;
        }
        self.sidebar_side = saved_side;
        self.cancel_sidebar_divider_interaction_state();
    }

    pub(crate) fn apply_gpui_command_pane_side_from_saved_settings(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        // Settings persists commandsPanelSide through the patch path; a save
        // whose side differs from the live placement re-docks the pane on the
        // next render instead of waiting for relaunch. Any in-flight rail drag
        // belongs to the old axis, so drop it rather than let it keep resizing.
        let saved_side = gpui_command_pane_side_from_shared_settings(settings_snapshot);
        if saved_side == self.command_pane_side {
            return;
        }
        self.command_pane_side = saved_side;
        self.command_pane.resize_drag = None;
        self.clear_command_resize_hover_state();
    }
}
