//! Sidebar runtime settings and page themes, the gxserver bootstrap, and messages forwarded to or dispatched from the sidebar host.

use std::path::Path;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::helpers::*;
use crate::app::model::*;
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
            self.sync_gpui_onboarding_system_appearance(system_is_light, cx);
            if settings
                .object()
                .get("sidebarTheme")
                .and_then(serde_json::Value::as_str)
                == Some("system")
            {
                refresh_gpui_visual_settings(&settings);
                apply_gpui_component_theme(cx);
                self.refresh_open_gpui_app_modals_after_settings_change(&settings, cx);
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
            crate::app::view_retention::apply_view_retention_after_settings_change(&settings, cx);
        }
        let appearance_settings_changed = changed && {
            let previous_settings =
                serde_json::from_str::<serde_json::Value>(&previous_settings_json).ok();

            [
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
            })
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
        GPUI runtime settings polling is intentionally narrow: read the shared sidebar settings snapshot once, pass strict debuggingMode and the experimental switches plus the saved object to SidebarApp normalization, skip unchanged payloads, and refresh the sidebar CEF bridge only. Browser CEF tabs, generic settings buses, filesystem watchers, path heuristics, and persisted/logged raw settings data stay out of this path.

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
        // Not part of the runtime snapshot below, and not a code-server launch option: turning it
        // on or off rebuilds the Code pages only (`apply_code_windows_setting`).
        let code_windows_on = settings
            .object()
            .get("codeServerNewWindowsAsTabs")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if self.code_windows_setting_on != code_windows_on {
            self.code_windows_setting_on = code_windows_on;
            self.apply_code_windows_setting(code_windows_on, cx);
        }
        let docs_tabs_on = settings.docs_open_files_as_tabs();
        if self.docs_open_files_as_tabs != docs_tabs_on {
            self.docs_open_files_as_tabs = docs_tabs_on;
            cx.notify();
        }
        let browser_row_on = settings.browser_pages_in_view_row();
        if self.browser_pages_in_view_row != browser_row_on {
            self.browser_pages_in_view_row = browser_row_on;
            // The shown tab moves between the strip and the row, so both scroll to it again.
            self.browser_page_tab_revealed = None;
            self.scroll_all_active_tab_strips();
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
                    | ProjectWorkareaCefSurfaceSlotKey::Work
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

        /*
        CDXC:Extensions 2026-11-05 WHY:
        The bootstrap also carries the focused and visible sessions, so every session switch
        came through here and re-read the extension list, every project and the whole
        presentation snapshot (about 0.9 MB on a Mac with 45 sessions), none of which follows
        focus: with the snapshot reads behind it, most of what gxserver served the desktop
        (measured 2026-11-05, issues-1027/macos-perf). A new connection (another gxserver, URL,
        token or client) still refreshes at once; a focus change refreshes at most every 30
        seconds, like the other unrelated changes (`refresh_extensions_if_stale`).
        */
        let connection_changed = self
            .sidebar_gxserver_bootstrap
            .as_ref()
            .is_none_or(|previous| {
                next_bootstrap.as_ref().is_none_or(|next| {
                    previous.base_url != next.base_url
                        || previous.auth_token != next.auth_token
                        || previous.protocol_version != next.protocol_version
                        || previous.client_id != next.client_id
                })
            });
        self.sidebar_gxserver_bootstrap = next_bootstrap;
        self.sync_gx_store_transport(cx);
        self.refresh_session_chat_runtime_endpoints(false, cx);
        if force_replay || connection_changed {
            self.refresh_extensions_in_background(cx);
        } else {
            self.refresh_extensions_if_stale(cx);
        }
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
        choice: Option<&crate::app::gx_store::NewProjectChoice>,
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
        self.gx_store_note_added_project(&project_id, remote_machine_id.as_deref(), choice, cx);
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
}
