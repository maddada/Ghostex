// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar runtime settings, host-message dispatch, sidebar divider/collapse

use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;
use std::time::SystemTime;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::floating_reveal::model::SIDEBAR_HOVER_REVEAL_ACTIVE_POLL;
use crate::app::floating_reveal::model::SIDEBAR_HOVER_REVEAL_IDLE_POLL;

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

use crate::app::consts::*;
use crate::app::ffi::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::resize_rail::*;
use crate::app::window::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn refresh_sidebar_runtime_settings_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let previous_settings_json = self
            .sidebar_runtime_settings_snapshot
            .saved_settings_json
            .clone();
        let system_is_light = refresh_gpui_system_appearance(cx);
        // Windows sends no event this app hears when Transparency effects is switched, so the
        // settings poll re-reads it (macOS reports Reduce Transparency through its accessibility
        // notification instead).
        if cfg!(target_os = "windows") && refresh_window_glass(settings.object()) {
            cx.notify();
        }
        if self.system_color_scheme_is_light != system_is_light {
            self.system_color_scheme_is_light = system_is_light;
            self.notify_native_chat_views(cx);
            if settings
                .object()
                .get("sidebarTheme")
                .and_then(serde_json::Value::as_str)
                == Some("system")
            {
                refresh_gpui_visual_settings(&settings);
                apply_gpui_component_theme(cx);
                let message =
                    self.gpui_app_modal_sidebar_state_message_from_settings_snapshot(&settings);
                self.refresh_open_gpui_app_modal_sidebar_state(message, cx);
                cx.notify();
            }
            if shared_settings::effective_content_color_scheme(
                settings.object(),
                "terminalColorScheme",
            ) == "system"
            {
                refresh_gpui_visual_settings(&settings);
                self.reload_live_gpui_engine_terminal_config(cx);
                cx.notify();
            }
        }
        let changed = self.refresh_sidebar_runtime_settings_from_shared_settings(&settings, cx);
        if changed {
            self.notify_native_chat_views(cx);
        }
        let appearance_settings_changed = changed && {
            let previous_settings =
                serde_json::from_str::<serde_json::Value>(&previous_settings_json).ok();
            let appearance_settings_changed = [
                "sidebarTheme",
                "darkThemePreset",
                "lightThemePreset",
                "themeContrast",
                "themeSidebarContrast",
                "themeWorkAreaContrast",
                "customSidebarTitlebarBackgroundDarknessPercent",
                "customSidebarTitlebarBackgroundTintColor",
                "customSidebarTitlebarLightBackgroundLightnessPercent",
                "customSidebarTitlebarLightBackgroundTintColor",
                "sessionChatTheme",
                "terminalColorScheme",
                "terminalGhosttyLightTheme",
                "terminalGhosttyTheme",
            ]
            .iter()
            .any(|key| {
                previous_settings.as_ref().and_then(|value| value.get(*key))
                    != settings.object().get(*key)
            });
            appearance_settings_changed
        };
        if appearance_settings_changed {
            self.refresh_gpui_shared_settings_consumers_after_save(&settings, cx);
            cx.notify();
        }
        changed
    }

    pub(crate) fn refresh_sidebar_runtime_settings_from_shared_settings(
        &mut self,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CefRuntime 2026-06-23-08:23:
        GPUI runtime settings polling is intentionally narrow: read the shared sidebar settings snapshot once, pass strict debuggingMode/showBetaFeatures plus the saved object to SidebarApp normalization, skip unchanged payloads, and refresh the sidebar CEF bridge only. Browser CEF tabs, generic settings buses, filesystem watchers, path heuristics, and persisted/logged raw settings data stay out of this path.

        CDXC:Settings 2026-06-24-11:14:
        Settings saves use this same sidebar CEF runtime-settings refresh path immediately after the shared service write succeeds. The save path must not wait for polling, add a broad settings event bus, or leak raw Settings JSON into Browser tabs, logs, paths, titles, commands, tokens, stdout/stderr, or user content.

        CDXC:CodeEditor 2026-06-24-23:17:
        code-server consumes VS Code settings-link choices only at process launch. When shared Settings changes those choices while Source is awake, restart the GPUI-owned runtime through the same lazy Source path instead of mutating a live process or trusting renderer-provided launch flags.

        CDXC:KeepAwake 2026-06-25-23:49:
        Keep Awake automation is part of the existing Settings save/runtime refresh path. A saved beta/control disable stops the GPUI-owned hold and suppresses future autostarts, while launch/display/delayed-send rules are re-evaluated immediately without adding a broad Settings event bus.

        CDXC:KeepAwake 2026-06-26-00:29:
        Settings refresh also re-evaluates the Working-session automatic hold against app-owned terminal model state. Keep this in the existing narrow refresh path instead of introducing a broad settings or terminal event bus.
        */
        self.sync_gpui_keep_awake_automation_from_settings(settings, cx);
        /*
        CDXC:AgentLauncher 2026-08-01-16:00:
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
        self.refresh_workarea_page_themes(settings, cx);
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
        self.gx_store_hud_settings_changed(cx);
        self.remote_reconnect_sync_with_settings(cx);
        if self.coerce_active_mode_to_available_project_context(cx) {
            self.update_project_workarea_runtime_cef_surface_visibility(cx);
        }
        if source_code_server_settings_changed {
            self.restart_source_code_server_runtime_after_settings_change(cx);
        }
        true
    }

    pub(crate) fn refresh_workarea_page_themes(
        &self,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        let light = sidebar_uses_light_theme(settings.object());
        let themed = |slot: &ProjectWorkareaCefSurfaceSlotKey| {
            matches!(
                slot,
                ProjectWorkareaCefSurfaceSlotKey::Kanban
                    | ProjectWorkareaCefSurfaceSlotKey::Automate
                    | ProjectWorkareaCefSurfaceSlotKey::Manage
            )
        };
        let live = self
            .project_workarea_runtime_cef_surfaces
            .iter()
            .filter(|(slot, _)| themed(slot))
            .map(|(_, owned)| &owned.surface);
        // Pages kept alive for a left project return without being recreated,
        // so they take the theme change now rather than showing the old one.
        let parked = self
            .parked_project_workarea_surfaces
            .iter()
            .filter(|parked| themed(&parked.slot_key))
            .map(|parked| &parked.owned.surface);
        for surface in live.chain(parked) {
            surface.update(cx, |surface, _| surface.refresh_workarea_theme(light));
        }
        self.refresh_app_modal_pages_window_glass(cx);
        // Browser pages: the colour GPUI paints under the CEF child view must
        // follow the theme, or hiding a page on a switch flashes the old one.
        let browser_background = rgb(if light { 0xffffff } else { 0x0d0d0d }).into();
        let parked_browser = self
            .parked_browser_runtimes_by_project
            .values()
            .flat_map(|runtime| runtime.surfaces.values());
        for surface in self.browser_surfaces.values().chain(parked_browser) {
            surface.update(cx, |surface, cx| {
                surface.set_background(browser_background, cx)
            });
        }
    }

    pub(crate) fn refresh_sidebar_gxserver_bootstrap_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.refresh_sidebar_gxserver_bootstrap(false, cx)
    }

    pub(crate) fn replay_sidebar_gxserver_bootstrap(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.refresh_sidebar_gxserver_bootstrap(true, cx)
    }

    pub(crate) fn refresh_sidebar_gxserver_bootstrap(
        &mut self,
        force_replay: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ServerDaemon 2026-06-24-11:17:
        Reuse the existing narrow sidebar polling cadence to notice gxserver token bootstrap availability after load. The poll reads only the existing token helper, fixed local gxserver constants, the current explicit sidebar active-project id, and the exact local focus key when it matches the stored focused session. Update only the sidebar CEF bridge on actual snapshot change and do not add file watchers, logs, persistence, Browser/workarea/modal exposure, fake gxserver sessions, or fallback project/session id inference.
        */
        let next_bootstrap = gpui_sidebar_gxserver_bootstrap(
            self.latest_sidebar_project_snapshot.as_ref(),
            &self.sidebar_gxserver_presentation_focus_state,
            self.local_workspace_latest_focus_key.as_ref(),
        );
        if !force_replay && self.sidebar_gxserver_bootstrap == next_bootstrap {
            self.refresh_session_chat_runtime_endpoints(false, cx);
            return false;
        }

        self.sidebar_gxserver_bootstrap = next_bootstrap.clone();
        self.sync_gx_store_transport(cx);
        self.refresh_session_chat_runtime_endpoints(false, cx);
        if let Some(handle) = self.app_modal_window {
            let _ = handle.update(cx, |host, _, cx| {
                host.refresh_gxserver_bootstrap(next_bootstrap.clone(), cx);
            });
        }
        self.refresh_extensions_in_background(cx);
        self.reconcile_agents_pane_surfaces(cx);
        true
    }

    /// CDXC:Spaces 2026-09-21 WHY:
    /// The New/Edit Space dialog's confirm and delete. The dialog is an app-modal window, so its
    /// result has to cross back, and only bounded metadata does: the mode enum, a Space id, a name,
    /// an icon id, a colour, an optional member id and the owning machine id, never a Space
    /// document, a project path or daemon state. Supersedes `CDXC:Spaces 2026-08-27`'s placement,
    /// which said SidebarApp owns the Space document: the app owns it now, for this computer and
    /// for a remote machine (gx_store/space_editor.rs). The runtime is no longer told: the page
    /// that read `applySidebarSpaceEditorResult` is gone and nothing else did (ledger R022).
    pub(crate) fn forward_gpui_sidebar_space_editor_result_to_sidebar(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(mode) = command
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .filter(|mode| matches!(*mode, "create" | "delete" | "edit"))
        else {
            return false;
        };
        let mut message = serde_json::Map::new();
        message.insert("mode".to_string(), serde_json::json!(mode));
        message.insert(
            "type".to_string(),
            serde_json::json!("applySidebarSpaceEditorResult"),
        );
        for field in [
            "color",
            "icon",
            "memberCollectionId",
            "memberProjectId",
            "name",
            "remoteMachineId",
            "spaceId",
        ] {
            if let Some(value) = command
                .get(field)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 256)
            {
                message.insert(field.to_string(), serde_json::json!(value));
            }
        }
        // Deleting the Space a section is filtered by leaves that section naming a Space that is
        // gone, and the sidebar's own state is the store's since M5 piece 7c
        // (gx_store/sidebar_ui_paths.rs).
        self.gx_store_note_sidebar_space_editor_result(&message, cx);
        // The document edit itself, for this computer or a remote machine.
        self.gx_store_run_space_editor_result(&message, cx);
        true
    }

    /// CDXC:Spaces 2026-09-15 DECISION:
    /// User: a project added through the Add Project dialog joins the Space that is open in the sidebar and goes to the top of it.
    /// The app applies both halves, for this computer and for a remote machine (gx_store/added_project.rs); the runtime is no longer told, because nothing there read `assignAddedProjectToSelectedSpace` once the sidebar page was gone (ledger R023).
    /// It must run before the project activation so the membership exists when the activation reveal resolves the project's Space.
    pub(crate) fn forward_gpui_added_project_to_sidebar(
        &mut self,
        project_id: &str,
        remote_machine_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let bounded = |value: &str| {
            let value = value.trim();
            (!value.is_empty()
                && value.chars().count() <= 256
                && !value.chars().any(char::is_control))
            .then(|| value.to_string())
        };
        let Some(project_id) = bounded(project_id) else {
            return false;
        };
        let remote_machine_id = remote_machine_id.and_then(bounded);
        self.gx_store_note_added_project(&project_id, remote_machine_id.as_deref(), cx);
        true
    }

    /// An `updateCustomSessionTags` catalog write issued from an app-modal
    /// window (Settings), performed by gx_store/custom_tags_sync.rs. Only a
    /// bounded copy of the catalog goes on: tag ids, names, icon ids, colors,
    /// the order, and the owning machine id.
    pub(crate) fn forward_gpui_custom_session_tags_update_to_sidebar(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        const MAX_TAGS: usize = 256;
        let bounded_text = |value: Option<&serde_json::Value>| {
            value
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty() && text.chars().count() <= 256)
                .filter(|text| !text.chars().any(char::is_control))
                .map(str::to_string)
        };
        let Some(state) = command.get("state").and_then(serde_json::Value::as_object) else {
            return false;
        };
        let Some(order) = state.get("order").and_then(serde_json::Value::as_array) else {
            return false;
        };
        let Some(tags) = state.get("tags").and_then(serde_json::Value::as_object) else {
            return false;
        };
        if order.len() > MAX_TAGS || tags.len() > MAX_TAGS {
            return false;
        }
        let mut bounded_order = Vec::with_capacity(order.len());
        for tag_id in order {
            let Some(tag_id) = bounded_text(Some(tag_id)) else {
                return false;
            };
            bounded_order.push(serde_json::Value::String(tag_id));
        }
        let mut bounded_tags = serde_json::Map::new();
        for (tag_id, tag) in tags {
            let Some(tag) = tag.as_object() else {
                return false;
            };
            let (Some(key), Some(color), Some(icon), Some(name), Some(inner_tag_id)) = (
                bounded_text(Some(&serde_json::Value::String(tag_id.clone()))),
                bounded_text(tag.get("color")),
                bounded_text(tag.get("icon")),
                bounded_text(tag.get("name")),
                bounded_text(tag.get("tagId")),
            ) else {
                return false;
            };
            bounded_tags.insert(
                key,
                serde_json::json!({
                    "color": color,
                    "icon": icon,
                    "name": name,
                    "tagId": inner_tag_id,
                }),
            );
        }
        let mut message = serde_json::Map::new();
        message.insert(
            "state".to_string(),
            serde_json::json!({
                "order": bounded_order,
                "tags": bounded_tags,
            }),
        );
        message.insert(
            "type".to_string(),
            serde_json::json!("updateCustomSessionTags"),
        );
        if let Some(remote_machine_id) = bounded_text(command.get("remoteMachineId")) {
            message.insert(
                "remoteMachineId".to_string(),
                serde_json::Value::String(remote_machine_id),
            );
        }
        self.gx_store_update_custom_session_tags(&serde_json::Value::Object(message), cx);
        true
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
        CDXC:Sidebar 2026-09-25 WHY:
        The app's modals, the New Thread picker and the delayed-send menus hand the sidebar's own
        messages here, and the store answers every one it has an owner for. What is left used to go
        on to the app runtime's `onSidebarHostMessage`, which only re-posted it to a message source
        nothing listens to any more, so it stops here and the answer is `false`.
        */
        if self.gx_store_claim_sidebar_host_message(&message, cx) {
            return true;
        }
        self.gx_store_run_session_edit(&message, cx)
    }

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
                    this.copy_path_for_disabled_project_workarea(&requested_path_text, "Code", cx);
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

    pub(crate) fn handle_gpui_pick_terminal_background_image_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        self.pick_image_for_app_modal(
            |path| {
                serde_json::json!({
                    "path": path,
                    "type": "terminalBackgroundImageFilePicked",
                })
            },
            cx,
        );
    }

    /// Settings -> Window glass -> Custom image: the Choose button of the dark or light picture.
    pub(crate) fn handle_gpui_pick_window_glass_image_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let appearance = if message["appearance"] == "light" {
            "light"
        } else {
            "dark"
        };
        self.pick_image_for_app_modal(
            move |path| {
                serde_json::json!({
                    "appearance": appearance,
                    "path": path,
                    "type": "windowGlassImageFilePicked",
                })
            },
            cx,
        );
    }

    /// Settings -> Window glass -> Live: "Choose a file…" for the dark or light mode's own video. The dialog
    /// cannot filter by type, so a picked file that is not a .mov, .mp4 or .m4v comes back as an
    /// error for the row to show.
    pub(crate) fn handle_gpui_pick_window_glass_video_message(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let appearance = if message["appearance"] == "light" {
            "light"
        } else {
            "dark"
        };
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Video".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let message = if crate::app::helpers::window_glass_video::is_video_file(&path) {
                serde_json::json!({
                    "appearance": appearance,
                    "path": path.to_string_lossy(),
                    "type": "windowGlassVideoFilePicked",
                })
            } else {
                serde_json::json!({
                    "appearance": appearance,
                    "error": "Choose a .mov, .mp4 or .m4v video.",
                    "type": "windowGlassVideoFilePicked",
                })
            };
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }

    /// A native image dialog whose picked absolute path is posted back to the open app-modal
    /// window as the message `reply` builds.
    fn pick_image_for_app_modal(
        &mut self,
        reply: impl FnOnce(String) -> serde_json::Value + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
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
            let message = reply(path.to_string_lossy().into_owned());
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(message, cx);
            });
        })
        .detach();
    }

    /*
    CDXC:Onboarding 2026-08-24:
    The onboarding Get Started page's Browse button. Same round trip as the
    terminal background image picker: native dialog host-side, picked absolute
    path posted back to the open app-modal window, where the first-launch page
    drops it into the project-folder input like a typed path.
    */
    pub(crate) fn handle_gpui_pick_first_launch_project_folder_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose Project Folder".into()),
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
                    Err(message) => {
                        let _ = this.update(cx, |this, cx| {
                            this.dispatch_gpui_app_modal_toast(
                                "warning",
                                "Could not use that project folder",
                                &message,
                                cx,
                            );
                        });
                        return;
                    }
                };
            #[cfg(not(target_os = "windows"))]
            let picked_path = path.to_string_lossy().to_string();
            let _ = this.update(cx, |this, cx| {
                this.dispatch_open_gpui_app_modal_message(
                    serde_json::json!({
                        "path": picked_path,
                        "type": "firstLaunchProjectFolderPicked",
                    }),
                    cx,
                );
            });
        })
        .detach();
    }

    /*
    CDXC:Onboarding 2026-08-24:
    Onboarding Finish crosses from the app-modal window into the sidebar
    runtime over the existing workspaceFolderPicked chain, which already owns
    project registration and focus. `firstLaunchAgentId` additionally asks the
    runtime to start the first session ('terminal' means a plain shell). Only
    the two bounded strings cross the boundary.
    */
    pub(crate) fn handle_gpui_first_launch_create_project_session_message(
        &mut self,
        command: &serde_json::Map<String, serde_json::Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(request_id) = gpui_remote_request_id_from_command(command) else {
            return;
        };
        let Some(path) = command
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
        else {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("Choose a project folder before finishing setup."),
                cx,
            );
            return;
        };
        let Some(agent_id) = command
            .get("agentId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|agent_id| {
                !agent_id.is_empty()
                    && agent_id.len() <= 64
                    && agent_id.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '-' || character == '_'
                    })
            })
            .map(str::to_string)
        else {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("Choose an available agent before finishing setup."),
                cx,
            );
            return;
        };
        #[cfg(target_os = "windows")]
        let project_path = if gpui_add_project_dialog_is_windows_absolute_path(&path) {
            match windows_terminal_backend::wsl_path_for_windows_path(Path::new(&path)) {
                Ok(path) => path,
                Err(message) => {
                    self.dispatch_gpui_first_launch_create_project_session_result(
                        &request_id,
                        false,
                        Some(&message),
                        cx,
                    );
                    return;
                }
            }
        } else {
            path
        };
        #[cfg(not(target_os = "windows"))]
        let project_path = path;
        let dispatched = self.dispatch_gpui_workspace_folder_picked_message(
            serde_json::json!({
                "firstLaunchAgentId": agent_id,
                "path": project_path,
                "requestId": request_id,
                "type": "workspaceFolderPicked",
            }),
            cx,
        );
        if !dispatched {
            self.dispatch_gpui_first_launch_create_project_session_result(
                &request_id,
                false,
                Some("The project sidebar is not available."),
                cx,
            );
        }
    }

    pub(crate) fn dispatch_gpui_first_launch_create_project_session_result(
        &mut self,
        request_id: &str,
        ok: bool,
        error: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut result = serde_json::json!({
            "ok": ok,
            "requestId": request_id,
            "type": "firstLaunchCreateProjectSessionResult",
        });
        if let Some(error) = error {
            result["error"] = serde_json::json!(error);
        }
        self.dispatch_open_gpui_app_modal_message(result, cx);
    }

    pub(crate) fn handle_gpui_pick_worktree_images_message(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
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
        CDXC:Workarea 2026-06-26-07:25:
        Native GPUI workspace tab lifecycle: the request carries only request id, action, bounded gxserver project/session ids, and optional replacement ids; the store performs the gxserver half (gx_store/terminal_lifecycle/lifecycle_requests.rs) while the workspace keeps pane/tab ownership local.
        */
        self.gx_store_run_tab_lifecycle_request(&message, cx)
    }

    /// A capture from the App Shots monitor: staged in Rust (gx_store/app_shot.rs).
    #[cfg(target_os = "macos")]
    pub(crate) fn handle_gpui_native_app_shot_capture(
        &mut self,
        capture: GpuiAppShotCapture,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store_stage_app_shot(capture, cx);
    }

    /// Called after every command-pane change; returns whether a tab's summary (status, focus,
    /// timer labels) moved, which the policy poll and the ready replay use to repaint. The summaries
    /// no longer cross to the app runtime: its HUD's command-session indicators had no reader once
    /// the HUD became Rust's (ledger R032).
    pub(crate) fn refresh_sidebar_command_pane_sessions_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.schedule_close_terminal_view_if_empty(cx);
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
        self.gx_store_set_local_delayed_sends(&sessions, cx);
        self.sidebar_agents_delayed_sends_snapshot = snapshot;
        true
    }

    pub(crate) fn dispatch_gpui_sidebar_command_run_state(
        &mut self,
        command_id: &str,
        run_id: &str,
        state: GpuiSidebarCommandRunState,
    ) {
        self.sidebar_command_run_feedback_states
            .entry(command_id.to_string())
            .or_default()
            .apply_run_state(run_id, state);
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
            CDXC:Extensions 2026-08-09:
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
            if completion.command_id == GPUI_CUA_DRIVER_UNINSTALL_COMMAND_ID {
                self.run_gpui_ghostex_cli_settings_action(
                    GpuiGhostexCliSettingsAction::FinishTrycuaUninstall {
                        succeeded: completion.exit_code == 0,
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
        CDXC:CommandPane 2026-06-26-04:59:
        Native command-pane Actions keep completed tabs reusable even when older Action definitions requested close-on-exit. Keep this completion close helper as a stale-record guard only; current runtime completions normalize close-on-exit to false and must not remove the Action-owned command tab after sidebar feedback.
        */
        let keyboard_owner_before = self.keyboard_owner_session();
        let Some(completed_tab) = self.command_pane.close_completed_action_run_tab(completion)
        else {
            return false;
        };
        self.forget_command_gxserver_session_for_closed_tab(completed_tab.session_id, cx);
        self.prune_gpui_command_delayed_send_timers_for_command_model();
        self.prune_gpui_command_close_after_done_timers_for_command_model();
        if self.command_pane.has_sessions() {
            self.follow_shell_focus_after_surface_removed(
                ShellFocusTarget::CommandPane,
                keyboard_owner_before,
                cx,
            );
        } else {
            self.restore_non_command_focus_after_surface_removed(keyboard_owner_before, cx);
        }
        self.scroll_command_group_active_tab(completed_tab.group_id);
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

    pub(crate) fn render_sidebar_resize_divider(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("ghostex-gpui-sidebar-resize-divider")
            .relative()
            .flex_shrink_0()
            .w(px(SIDEBAR_DIVIDER_WIDTH))
            .h_full()
            // The body row sits 1px under the titlebar so panes can own
            // their top edge; carry the titlebar hairline across the divider.
            .border_t_1()
            .border_color(glass_divider(titlebar_button_border_color()))
            // Under glass the divider sits on the sidebar's tint (nothing tints the window beneath
            // it), with its faint line laid over that.
            .bg(if window_glass_active() {
                sidebar_glass_tint()
            } else {
                sidebar_divider_line_color()
            })
            .when(window_glass_active(), |this| {
                this.child(div().absolute().inset_0().bg(sidebar_divider_line_color()))
            })
            // The workspace beside the sidebar is often a CEF page (Browser, Docs), so the
            // whole grab strip lies over the native sidebar.
            .child(resize_rail_deferred_strip(
                resize_rail_grab_strip(
                    "ghostex-gpui-sidebar-resize-grab-strip",
                    WorkspaceSplitAxis::Horizontal,
                    ResizeRailGrabSide::Leading,
                )
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
                .when(self.sidebar_divider_hover_visible, |this| {
                    this.child(resize_rail_hover_line(
                        "ghostex-gpui-sidebar-resize-divider-hover-line",
                        WorkspaceSplitAxis::Horizontal,
                        ResizeRailGrabSide::Leading,
                    ))
                }),
            ))
    }

    pub(crate) fn set_sidebar_divider_hovering(
        &mut self,
        hovered: bool,
        cx: &mut gpui::Context<Self>,
    ) {
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

        let hovering =
            self.sidebar_drag.is_some() || self.sidebar_divider_contains_mouse_position(event);
        self.set_sidebar_divider_hovering(hovering, cx);
    }

    pub(crate) fn sidebar_divider_contains_mouse_position(&self, event: &MouseMoveEvent) -> bool {
        self.sidebar_divider_contains_position(event.position)
    }

    pub(crate) fn sidebar_divider_contains_position(&self, position: gpui::Point<Pixels>) -> bool {
        if !gpui_sidebar_chrome_visible(self.sidebar_collapsed) {
            return false;
        }
        let x = position.x.as_f32();
        let (start_x, end_x) = gpui_sidebar_divider_x_bounds(self.sidebar_width);

        // CDXC:Sidebar 2026-09-20 WHY:
        // The divider used to start below the titlebar row. That row is gone, so the body row and
        // its divider own the window from its top edge down; the header is a child of the
        // workspace column to the divider's right and never overlaps this band.
        x >= start_x && x <= end_x
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
        // The grab strip stops blocking the mouse once a drag is active, which needs a new frame even
        // when the hover state above was already set.
        cx.notify();
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

        let max_width = current_sidebar_max_width(window, self.active_mode);
        let delta = event.position.x.as_f32() - drag.start_x;
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
        let max_width = current_sidebar_max_width(window, self.active_mode);
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
        CDXC:Sidebar 2026-06-26-10:04:
        GPUI Cmd+B and the shared `toggleSidebarCollapsed` action collapse only shell chrome state. Preserve `sidebar_width` and cancel divider interaction state so expanding restores the user's resized sidebar without writing a zero-width setting or leaving stale hover/drag chrome active.
        */
        self.sidebar_collapsed = gpui_next_sidebar_collapsed_state(self.sidebar_collapsed);
        self.cancel_sidebar_divider_interaction_state();
        self.update_sidebar_cef_surface_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn update_sidebar_cef_surface_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        self.update_sidebar_reveal(false, false, cx);
    }

    /// CDXC:Sidebar 2026-09-20 DECISION:
    /// User (ruling 7B, screen 10): edge-hover floating is on all three platforms, not macOS only.
    /// The gesture, its panel and its dismissal therefore live in `app/floating_reveal/`, which is
    /// shared; only the host that owns the child window is per platform. This supersedes the
    /// macOS-only reveal this function used to forward to.
    pub(crate) fn update_sidebar_reveal(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.update_floating_reveal(requested, keep_under_pointer, cx);
    }

    /// The gesture's clock. Pointer-leave is the only thing no element can report (the panel covers
    /// the strip that armed it), so one sweep owns the whole reveal. It tightens to a frame while a
    /// panel is on screen, because the backends that animate the slide themselves step it here.
    pub(crate) fn start_sidebar_hover_reveal_polling(
        &self,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let mut interval = SIDEBAR_HOVER_REVEAL_IDLE_POLL;
            loop {
                cx.background_executor().timer(interval).await;
                let result = window.update(cx, |_, window, cx| {
                    this.update(cx, |this, cx| this.poll_floating_reveal(window, cx))
                });
                match result {
                    Ok(Ok(panel_open)) => {
                        interval = if panel_open {
                            SIDEBAR_HOVER_REVEAL_ACTIVE_POLL
                        } else {
                            SIDEBAR_HOVER_REVEAL_IDLE_POLL
                        };
                    }
                    _ => break,
                }
            }
        })
        .detach();
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
