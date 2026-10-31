//! The shared-settings save fan-out: Ghostty config, gxserver agent settings, sidebar state messages and live terminal config reloads.

use std::cell::RefCell;
use std::time::SystemTime;

use serde_json::{Map, Value};

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/// Settings keys only the Settings window reads back: where it reopens, which
/// window/settings_modal/store.rs saves as the reader scrolls and changes pages.
const SETTINGS_WINDOW_MEMORY_KEYS: &[&str] = &["settingsModalNavigation"];

thread_local! {
    /// The settings the last consumer refresh was given; process-wide, like the settings.
    static LAST_REFRESHED_SETTINGS: RefCell<Option<shared_settings::SharedSidebarSettingsSnapshot>> =
        const { RefCell::new(None) };
}

/// Whether `next` differs from `previous` in the Settings window's own memory and in nothing else.
/// An identical save is not one: a caller may save unchanged settings to have the refresh re-read
/// something kept outside them (the terminal config an import just wrote).
fn only_settings_window_memory_changed(
    previous: &Map<String, Value>,
    next: &Map<String, Value>,
) -> bool {
    let memory = |key: &String| SETTINGS_WINDOW_MEMORY_KEYS.contains(&key.as_str());
    let rest_unchanged = next
        .iter()
        .all(|(key, value)| memory(key) || previous.get(key) == Some(value))
        && previous
            .keys()
            .all(|key| memory(key) || next.contains_key(key));
    rest_unchanged
        && SETTINGS_WINDOW_MEMORY_KEYS
            .iter()
            .any(|key| previous.get(*key) != next.get(*key))
}

impl GhostexGpuiApp {
    pub(crate) fn sync_gpui_ghostty_config_file_after_settings_save(
        &mut self,
        ghostty_config_backed_setting_keys_changed: &[&str],
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        // Only config-backed terminal values are written, into Ghostex's own terminal config; the
        // live reload that follows the save reads it back.
        if ghostty_config_backed_setting_keys_changed.is_empty() {
            return;
        }
        if shared_settings::write_ghostty_terminal_config_from_settings_object(
            settings_snapshot.object(),
            ghostty_config_backed_setting_keys_changed,
        )
        .is_ok()
        {
            return;
        }
        let message = "Settings were saved, but Ghostex could not write its terminal config file. Existing terminals were not reloaded.";
        self.dispatch_gpui_settings_action_status("ghosttySettings", false, message, cx);
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Could not update terminal config",
            message,
            cx,
        );
    }

    pub(crate) fn sync_gpui_gxserver_agent_settings_after_save(
        &mut self,
        previous_agent_settings: shared_settings::SharedGxserverAgentSettings,
        next_agent_settings: shared_settings::SharedGxserverAgentSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-11:39:
        GPUI matches macOS for agent launch policy: shared Settings is the local render cache, while local gxserver owns inherited Accept All and Default Prompt Agent behavior for launchers across clients. After a successful Settings save, post the current two gxserver-owned values only when either changed, and keep token/network/parser failures silent so unavailable gxserver never creates fake daemon state or rolls back the saved local cache.
        */
        if previous_agent_settings == next_agent_settings {
            return;
        }

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let canonical_agent_settings = background
                .spawn(async move { update_gpui_gxserver_agent_settings(&next_agent_settings) })
                .await
                .ok();
            let Some(canonical_agent_settings) = canonical_agent_settings else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.apply_gpui_gxserver_agent_settings_to_local_settings(
                    canonical_agent_settings,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn reconcile_gpui_gxserver_agent_settings_in_background(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-12:14:
        GPUI startup/open-time hydration must match macOS migration semantics for gxserver-owned agent policy. Read `/api/readAgentSettings`; if gxserver has no persisted row, seed it once from current shared Settings, otherwise treat daemon values as canonical and refresh the local render cache through the central settings service without logging tokens, response bodies, paths, commands, or user content.
        */
        if self.gxserver_agent_settings_reconciliation_in_flight {
            return;
        }
        self.gxserver_agent_settings_reconciliation_in_flight = true;

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let hydration_result = background
                .spawn(async move { reconcile_gpui_gxserver_agent_settings_with_daemon() })
                .await
                .ok()
                .flatten();
            let _ = this.update(cx, |this, cx| {
                this.gxserver_agent_settings_reconciliation_in_flight = false;
                if let Some(hydration_result) = hydration_result {
                    this.apply_gpui_gxserver_agent_settings_hydration_result(hydration_result, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message_for_open(
        &mut self,
        modal: GpuiAppModalKind,
        cx: &mut gpui::Context<Self>,
    ) -> serde_json::Value {
        /*
        CDXC:Settings 2026-06-24-12:22:
        Any shared Settings entry point can show agent-owned controls after the React host selects its initial tab. Reconcile gxserver-owned agent policy before hydrating Settings, Hotkeys, Configure Agents, Configure Actions, and Open Targets so entry-specific modal ids do not drift from the canonical Settings route.
        */
        if modal.is_settings_modal_entry() {
            self.reconcile_gpui_gxserver_agent_settings_in_background(cx);
        }
        self.gpui_app_modal_sidebar_state_message_from_held_hydrate(cx)
    }

    pub(crate) fn gpui_app_modal_sidebar_state_message(&self) -> serde_json::Value {
        self.with_gpui_command_pane_sidebar_indicators(gpui_app_modal_sidebar_state_message(
            self.latest_sidebar_project_snapshot.as_ref(),
        ))
    }

    pub(crate) fn with_gpui_command_pane_sidebar_indicators(
        &self,
        mut message: serde_json::Value,
    ) -> serde_json::Value {
        message = self.with_remote_project_action_rows(message);
        /*
        CDXC:CommandPane 2026-06-25-10:50:
        App-modal sidebar hydrates must carry the same command-session indicators as the live GPUI sidebar HUD. Reuse the sanitized command-pane summary and gxserver command rows; never compute from command text, paths, status-file paths, terminal output, logs, or persisted shell-state JSON.
        */
        let commands = message
            .get("hud")
            .and_then(|hud| hud.get("commands"))
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
        let sessions = self.command_pane.sidebar_command_session_sources(
            self.shell_focus == ShellFocusTarget::CommandPane,
            &self.command_delayed_send_timers,
            &self.command_close_after_done_timers,
            SystemTime::now(),
        );
        message["hud"]["commandSessionIndicators"] =
            gpui_sidebar_command_session_indicators_from_command_pane_sources(&commands, &sessions);
        self.with_project_view_scope_options(message)
    }

    pub(crate) fn apply_gpui_gxserver_agent_settings_hydration_result(
        &mut self,
        hydration_result: GpuiGxserverAgentSettingsHydrationResult,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-12:14:
        Startup/open hydration may finish after the user saves Settings. Apply daemon canonical values only if the shared render cache still matches the local values used for the read/seed decision; a newer save uses the existing save-time gxserver sync path instead of being overwritten by a stale startup response.
        */
        if shared_settings::shared_sidebar_settings_snapshot().gxserver_agent_settings()
            != hydration_result.expected_local_settings
        {
            return;
        }
        self.apply_gpui_gxserver_agent_settings_to_local_settings(
            hydration_result.canonical_settings,
            cx,
        );
    }

    pub(crate) fn apply_gpui_gxserver_agent_settings_to_local_settings(
        &mut self,
        canonical_agent_settings: shared_settings::SharedGxserverAgentSettings,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:AgentProviders 2026-06-24-11:39:
        gxserver read/update responses are canonical for inherited agent launch policy. If the daemon reports either agent setting differently than the current GPUI render cache, persist those canonical values through the central shared Settings service and refresh the modal/sidebar settings state again instead of writing a separate cache or logging private daemon details.
        */
        let latest_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        if latest_settings_snapshot.gxserver_agent_settings() == canonical_agent_settings {
            return;
        }

        let mut settings_object = latest_settings_snapshot.object().clone();
        canonical_agent_settings.write_to_settings_object(&mut settings_object);
        let Ok(write_result) =
            shared_settings::write_shared_sidebar_settings_object(settings_object)
        else {
            return;
        };
        self.refresh_gpui_shared_settings_consumers_after_save(&write_result.snapshot, cx);
    }

    pub(crate) fn refresh_gpui_shared_settings_consumers_after_save(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Settings 2026-06-24-11:19:
        After a successful Settings save or gxserver startup/open canonical sync, GPUI refreshes only the settings-dependent runtime state it owns today: app-modal hydrate/sidebarState, sidebar debug/beta booleans through the existing CEF runtime-settings path, project-workarea CEF visibility, project-editor auto-sleep scheduling, supported embedded Ghostty request-map settings, gxserver-owned agent-policy reconciliation, and central-service render reads such as the Browser feedback/profile toolbar controls. This is not full settings fan-out; many action bridges, code-server sync, live Ghostty config reloads, and broad future side effects remain outside this path.
        */
        /*
        CDXC:Settings 2026-10-23 WHY:
        The Settings window saves where it is scrolled to (`settingsModalNavigation`) a moment after every scroll, so that it reopens there across restarts (CDXC:Settings 2026-10-16 in window/settings_modal/restore.rs). Each of those saves ran this whole refresh, which reads the app-modal hydrate from gxserver synchronously on the main thread, reloads every terminal's config and re-applies the theme, so scrolling Settings stalled the app. A save that changes nothing but the Settings window's own memory is now only written; everything that reads a real setting is refreshed as before.
        */
        let memory_only = LAST_REFRESHED_SETTINGS.with_borrow(|last| {
            last.as_ref().is_some_and(|last| {
                only_settings_window_memory_changed(last.object(), settings_snapshot.object())
            })
        });
        LAST_REFRESHED_SETTINGS.with_borrow_mut(|last| *last = Some(settings_snapshot.clone()));
        if memory_only {
            // The save path unbound the previous chords before writing; they are the same chords.
            crate::app::shell_hotkeys::gpui_bind_hotkeys_from_settings(cx);
            return;
        }
        self.refresh_gpui_window_settings_consumers(settings_snapshot, cx);
        // Newly saved hotkey chords bind immediately (the app-action keys of
        // app/shell_hotkeys.rs first). The save boundary first
        // adds targeted Unbind markers for the prior Ghostex action chords, so
        // removed/remapped entries stop dispatching without clearing GPUI or
        // gpui-component's unrelated keymap entries.
        crate::app::shell_hotkeys::gpui_bind_hotkeys_from_settings(cx);
        cx.notify();
        // The other open windows follow the same save; the keymap above is the process's one
        // (app/workspace_windows/).
        let others = crate::app::workspace_windows::other_workspace_window_apps(cx.entity_id());
        if !others.is_empty() {
            let settings_snapshot = settings_snapshot.clone();
            cx.defer(move |cx| {
                for other in others {
                    other.update(cx, |app, cx| {
                        app.refresh_gpui_window_settings_consumers(&settings_snapshot, cx);
                        cx.notify();
                    });
                }
            });
        }
    }

    /// What one window refreshes after a settings save, in whichever window the save came from.
    fn refresh_gpui_window_settings_consumers(
        &mut self,
        settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        self.reschedule_project_editor_auto_sleep_if_policy_changed_from_shared_settings(
            settings_snapshot,
            cx,
        );
        self.apply_gpui_sidebar_visibility_memory_from_saved_settings(settings_snapshot);
        self.apply_gpui_command_pane_side_from_saved_settings(settings_snapshot);
        if self.is_lead_window() {
            self.ghostex_capture_settings_changed(settings_snapshot, cx);
        }
        refresh_gpui_visual_settings(settings_snapshot);
        crate::app::view_retention::apply_view_retention_after_settings_change(
            settings_snapshot,
            cx,
        );
        apply_gpui_component_theme(cx);
        crate::ui_fonts::apply_fonts(settings_snapshot.object(), cx);
        self.native_kanban_notify_appearance(cx);
        self.native_automate_notify_appearance(cx);
        self.native_bot_feed_notify_appearance(cx);
        self.native_docs_notify_appearance(cx);
        self.refresh_sidebar_runtime_settings_from_shared_settings(settings_snapshot, cx);
        self.coerce_active_mode_to_available_project_context(cx);
        self.prune_project_workarea_runtime_cef_surfaces_for_current_gates(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.reload_live_gpui_engine_terminal_config(cx);
        self.refresh_open_gpui_app_modals_after_settings_change(settings_snapshot, cx);
        self.sync_titlebar_account_privacy(cx);
        self.apply_voice_agent_settings(settings_snapshot.object(), cx);
    }

    pub(crate) fn reload_live_gpui_engine_terminal_config(&mut self, cx: &mut gpui::Context<Self>) {
        let shared_engine_settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        #[cfg(target_os = "macos")]
        let mut config = {
            let path = shared_settings::ghostex_terminal_config_path();
            let Ok(config) =
                terminal_ghostty_surface::load_ghostty_terminal_engine_config_from_path(
                    &path,
                    terminal_gpui_engine::ghostty_theme_source(
                        &shared_engine_settings.ghostty_theme,
                    ),
                )
            else {
                return;
            };
            config
        };
        #[cfg(not(target_os = "macos"))]
        let mut config =
            terminal_gpui_engine::GpuiTerminalEngineConfig::from_shared(&shared_engine_settings);
        config.apply_color_scheme(
            &shared_engine_settings,
            gpui_system_uses_light_appearance(),
            gpui_terminal_theme_background(&shared_engine_settings),
        );

        // This setting is app-owned and is not part of Ghostty's finalized
        // config string on macOS.
        config.view.scroll_to_bottom_when_typing =
            shared_engine_settings.scroll_to_bottom_when_typing;
        config.apply_bold_text_setting(&shared_engine_settings);
        config.view.background_image =
            terminal_gpui_engine::terminal_background_image_from_settings(&shared_engine_settings);
        config.view.background_alpha = terminal_default_background_alpha();

        let confirm_close_behavior =
            terminal_gpui_engine::gpui_engine_confirm_close_behavior(&config);
        for record in self
            .agents_gpui_engine_terminals
            .values_mut()
            .chain(self.command_gpui_engine_terminals.values_mut())
            .chain(
                self.parked_agents_terminal_runtimes_by_project
                    .values_mut()
                    .flat_map(|runtime| runtime.gpui_engine_terminals.values_mut()),
            )
        {
            record.confirm_close_behavior = confirm_close_behavior;
            let view = record.view.clone();
            let font = config.font.clone();
            let settings = config.view.clone();
            let colors = config.colors.clone();
            let option_as_alt = config.option_as_alt;
            view.update(cx, |view, cx| {
                view.apply_font(font);
                view.apply_settings(settings);
                view.model_mut().set_option_as_alt(option_as_alt);
                if let Some(colors) = colors {
                    let _ = view.model_mut().set_default_colors(
                        colors.foreground,
                        colors.background,
                        colors.cursor,
                        &colors.palette,
                    );
                }
                view.refresh_appearance(cx);
            });
        }
    }
}
