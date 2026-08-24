// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: GhostexGpuiApp::new startup wiring

use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.
use std::cell::Cell;

use crate::terminal_surface_host::NativeTerminalSurfaceHost;
use crate::terminal_surface_lifecycle::NativeTerminalSurfaceLifecycleState;
use anyhow::Context as _;
use anyhow::Result;
use gpui::App;
use gpui::AppContext as _;
use gpui::Entity;
use gpui::ScrollHandle;
use gpui::Window;
use gpui::px;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Result<Entity<Self>> {
        let parent = cef_parent_native_view(window)?;
        let project_name = titlebar_project_label_from_latest_sidebar_snapshot(None);
        let sidebar_url = sidebar_url().context("failed to resolve sidebar bundle URL")?;
        let shared_settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let sidebar_runtime_settings_snapshot =
            sidebar_runtime_settings_snapshot_from_shared_settings(&shared_settings_snapshot);
        /*
        Restore eagerness (Decision #3, 2026-07-02): the persisted presentation
        focus state seeds the first sidebar bootstrap so the runtime can
        re-materialize the previously focused running session after its first
        presentation hydrate.
        */
        let sidebar_gxserver_presentation_focus_state =
            load_gpui_gxserver_presentation_focus_state();
        let sidebar_gxserver_bootstrap =
            gpui_sidebar_gxserver_bootstrap(None, &sidebar_gxserver_presentation_focus_state, None);
        let sidebar_side = gpui_sidebar_side_from_shared_settings(&shared_settings_snapshot);
        let command_pane_side =
            gpui_command_pane_side_from_shared_settings(&shared_settings_snapshot);
        let sidebar_width = read_sidebar_width_setting()
            .unwrap_or(DEFAULT_SIDEBAR_WIDTH)
            .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
        let command_pane_initial_content_height = command_pane_content_height(window);
        let shell_layout_state = GpuiShellLayoutState::load_or_default(
            command_pane_initial_content_height,
            ProjectScopedWorkareaAvailability::from_env_bridge(),
            &shared_settings_snapshot,
        );
        let restored_command_gxserver_session_mappings =
            command_gxserver_session_mappings_from_command_model(&shell_layout_state.command_pane);
        let command_startup_activity_restore_intents = shell_layout_state
            .command_startup_activity_restore_intents
            .clone();
        let command_delayed_send_restore_timers = shell_layout_state
            .command_delayed_send_restore_timers
            .clone();
        let agents_delayed_send_restore_intents = shell_layout_state
            .agents_delayed_send_restore_intents
            .clone();
        let pending_command_gxserver_cleanup =
            shell_layout_state.pending_command_gxserver_cleanup.clone();
        let browser_url = shell_layout_state.browser_tabs.active_address_value();
        let project_editor_auto_sleep_policy = ProjectEditorAutoSleepPolicySnapshot::read_current();
        let gpui_pet_overlay_reduce_motion_enabled = gpui_macos_reduce_motion_enabled();
        let app_modal_window_id = Rc::new(Cell::new(None));
        let app_modal_window_id_for_app = app_modal_window_id.clone();
        let startup_restore_wake_pending = shell_layout_state
            .agents_workspace_project_id
            .iter()
            .cloned()
            .chain(
                shell_layout_state
                    .parked_agents_workspaces_by_project
                    .keys()
                    .cloned(),
            )
            .collect();
        #[cfg(target_os = "windows")]
        let windows_first_run_setup_state =
            if load_gpui_first_run_onboarding_state().windows_terminal_setup_complete {
                GpuiWindowsFirstRunSetupState::Ready
            } else {
                GpuiWindowsFirstRunSetupState::Checking
            };

        let main_window_bounds = window.bounds();
        let app = cx.new(move |cx| {
            let mut this = Self {
                parent_ns_view: parent,
                project_name,
                sidebar_url,
                browser_url,
                active_mode: shell_layout_state.active_mode,
                shell_focus: shell_layout_state.shell_focus,
                previous_non_command_focus: shell_layout_state.previous_non_command_focus,
                first_responder_target: FirstResponderTarget::None,
                first_responder_transition_suppressed_by_programmatic_focus: false,
                #[cfg(target_os = "macos")]
                source_workarea_cef_menu_passthrough_active: false,
                #[cfg(target_os = "macos")]
                renderer_edit_hotkey_passthrough_active: false,
                programmatic_focus_depth: 0,
                sidebar_focus_border_handoff: None,
                agents_workspace: shell_layout_state.agents_workspace,
                agents_workspace_project_id: shell_layout_state.agents_workspace_project_id,
                parked_agents_workspaces_by_project: shell_layout_state
                    .parked_agents_workspaces_by_project,
                parked_agents_terminal_runtimes_by_project: HashMap::new(),
                project_switch_settling_until: None,
                project_switch_pending_requests: Vec::new(),
                project_switch_flush_scheduled: false,
                command_pane: shell_layout_state.command_pane,
                command_pane_project_id: shell_layout_state.command_pane_project_id,
                parked_command_panes_by_project: shell_layout_state.parked_command_panes_by_project,
                command_pane_project_epoch: 0,
                project_editor_shell: shell_layout_state.project_editor_shell,
                project_editor_auto_sleep_epochs: ProjectEditorAutoSleepEpochs::default(),
                project_editor_auto_sleep_policy,
                browser_profiles: shell_layout_state.browser_profiles,
                browser_tabs: shell_layout_state.browser_tabs,
                browser_tabs_project_id: shell_layout_state.browser_tabs_project_id,
                parked_browser_tabs_by_project: shell_layout_state.parked_browser_tabs_by_project,
                browser_tabs_project_epoch: 0,
                sidebar_browser_tabs_snapshot: String::new(),
                sidebar_displayed_sessions_snapshot: String::new(),
                pending_sidebar_browser_tab_reveal: None,
                pending_export_transcript_reveal_path: None,
                sidebar_browser_tab_reveal_request_id: 0,
                latest_sidebar_project_snapshot: None,
                navigation_history_state: navigation_history::GpuiNavigationHistoryState::default(),
                titlebar_git_menu_state: None,
                titlebar_actions_snapshot: Vec::new(),
                titlebar_actions_refresh_in_flight: false,
                titlebar_tips_unread_count: TITLEBAR_TIP_IDS.len() as u64,
                updater_started: false,
                update_checking: false,
                update_available: false,
                update_downloading: false,
                update_download_progress: None,
                #[cfg(target_os = "windows")]
                windows_updater: None,
                #[cfg(target_os = "windows")]
                windows_update: None,
                #[cfg(target_os = "windows")]
                windows_ready_update: None,
                #[cfg(target_os = "windows")]
                windows_first_run_setup_state,
                prompt_editor_daemon_open: false,
                portless_setup_prompt_suppressed_until_restart: false,
                active_portless_setup_prompt_mode: None,
                portless_setup_prompt_pending_modal_close: false,
                first_run_onboarding_started: false,
                active_open_target_id: None,
                active_action_command_id: None,
                sidebar_command_run_feedback_states: HashMap::new(),
                keep_awake_runtime: None,
                keep_awake_runtime_generation: 0,
                keep_awake_auto_start_suppressed: false,
                keep_awake_power_ticker_active: false,
                keep_awake_previous_working_session_count: 0,
                keep_awake_working_session_grace_until: None,
                remote_machine_connect_states: HashMap::new(),
                remote_gxserver_connections: HashMap::new(),
                remote_gxserver_connect_generations: HashMap::new(),
                remote_gxserver_watchdog_probe_in_flight: false,
                remote_gxserver_presentation_stream_generation: 0,
                remote_repository_clone_requests: HashMap::new(),
                source_code_server_runtime: SourceCodeServerRuntimeOwner::new(),
                pending_source_file_open: None,
                pending_docs_file_open: None,
                startup_restore_wake_pending,
                remote_workspace_attach_pending: HashSet::new(),
                project_view_states_by_project: shell_layout_state.project_view_states_by_project,
                remote_attach_sessions: shell_layout_state.remote_attach_sessions,
                #[cfg(target_os = "macos")]
                remote_attach_askpass_scripts: HashMap::new(),
                project_workarea_runtime_cef_surfaces: HashMap::new(),
                sidebar_runtime_settings_snapshot,
                sidebar_gxserver_bootstrap,
                sidebar_gxserver_presentation_focus_state,
                sidebar_global_actions: Vec::new(),
                tab_strip_built_in_buttons: shared_settings_snapshot.tab_strip_built_in_buttons(),
                sidebar_session_status_indicators: GpuiSidebarSessionStatusIndicatorsState::default(
                ),
                sidebar_session_status_indicators_snapshot_seen: false,
                session_attention_notification_rate_limiter:
                    GpuiSessionAttentionNotificationRateLimiter::default(),
                sidebar_pet_overlay: GpuiSidebarPetOverlayState::default(),
                local_workspace_latest_focus_key: None,
                local_workspace_session_mappings: shell_layout_state
                    .local_workspace_session_mappings,
                local_workspace_attach_pending: HashSet::new(),
                agents_chat_mode_sessions: shell_layout_state.agents_chat_mode_sessions,
                agents_chat_auto_switch_observed_sessions: HashSet::new(),
                agents_chat_auto_switch_preference: gpui_preferred_agent_interface_from_settings(
                    shared_settings_snapshot.object(),
                ),
                pending_agents_chat_launch_intents: HashSet::new(),
                agents_chat_surfaces: HashMap::new(),
                session_chat_composer_ready_sessions: HashSet::new(),
                pending_session_chat_composer_focus: None,
                pending_session_chat_composer_insert: HashMap::new(),
                pending_session_terminal_composer_insert: HashMap::new(),
                pending_session_chat_draft_handoffs: HashSet::new(),
                session_chat_queued_counts: HashMap::new(),
                session_chat_queued_count_refresh_in_flight: false,
                local_workspace_lifecycle_requests: HashMap::new(),
                next_local_workspace_lifecycle_request_id: 1,
                local_app_shot_session_mappings: HashMap::new(),
                sidebar_command_pane_sessions_snapshot: String::new(),
                sidebar_agents_delayed_sends_snapshot: String::new(),
                sidebar_timer_presentations_replayed_after_ready: false,
                command_delayed_send_timers: HashMap::new(),
                command_delayed_send_generation: 0,
                command_delayed_send_countdown_ticker_active: false,
                command_delayed_send_persistence_ticker_active: false,
                command_gxserver_session_mappings: restored_command_gxserver_session_mappings,
                command_gxserver_attach_pending: HashSet::new(),
                pending_command_gxserver_cleanup,
                command_gxserver_cleanup_in_flight: HashSet::new(),
                agents_sessions_pending_surface_transfer: HashSet::new(),
                command_close_after_done_timers: HashMap::new(),
                command_close_after_done_generation: 0,
                command_close_after_done_countdown_ticker_active: false,
                gxserver_agent_settings_reconciliation_in_flight: false,
                workspace_drop_feedback: None,
                command_drop_feedback: None,
                workspace_tab_drag_active: false,
                pending_workspace_tab_click: None,
                command_tab_drag_active: false,
                pending_command_tab_click: None,
                browser_tab_drop_feedback: None,
                browser_tab_drag_active: false,
                workspace_leaf_layout_bounds: HashMap::new(),
                browser_leaf_layout_bounds: HashMap::new(),
                command_group_layout_bounds: HashMap::new(),
                command_pane_layout_bounds: None,
                project_editor_surface_layout_bounds: None,
                project_editor_companion_layout_bounds: None,
                agents_terminal_mount_slot_bounds: HashMap::new(),
                command_terminal_mount_slot_bounds: HashMap::new(),
                project_editor_companion_terminal_session_id: None,
                project_editor_companion_secondary_terminal_session_id: None,
                project_editor_companion_focused_terminal_slot:
                    ProjectEditorCompanionTerminalSlot::Top,
                project_editor_companion_terminal_mount_slot_bounds: HashMap::new(),
                zmx_persistence_resize_refresh_generation: 0,
                zmx_persistence_last_focused_terminal_slot: None,
                cef_sidebar_creation_retried: false,
                cef_context_initialization_waiting: false,
                agents_terminal_zmx_refresh_recorded_bounds: HashMap::new(),
                command_terminal_zmx_refresh_recorded_bounds: HashMap::new(),
                project_editor_companion_zmx_refresh_recorded_bounds: HashMap::new(),
                terminal_text_focus_handle: cx.focus_handle().tab_stop(false),
                terminal_text_marked_range: None,
                pending_agents_terminal_text_focus_slot: None,
                pending_command_terminal_text_focus_slot: None,
                pending_project_editor_companion_terminal_text_focus_slot: None,
                agents_terminal_startup_body_slot_geometries: HashMap::new(),
                agents_terminal_parked_owner_body_slot_geometries: HashMap::new(),
                agents_terminal_runtime_sessions: AgentsTerminalRuntimeSessionRegistry::new(),
                agents_terminal_startup_coordinator: AgentsTerminalStartupCoordinator::new(),
                agents_terminal_startup_launch_payload_source:
                    AgentsTerminalStartupLaunchPayloadSource::new_empty(),
                agents_terminal_launch_payload_source: AgentsTerminalLaunchPayloadSource::new_empty(
                ),
                command_terminal_launch_payload_source:
                    CommandTerminalLaunchPayloadSource::new_empty(),
                project_editor_companion_terminal_launch_payload_source:
                    ProjectEditorCompanionTerminalLaunchPayloadSource::new_empty(),
                project_editor_companion_terminal_attach_plan_pending: HashSet::new(),
                project_editor_companion_remote_attach_states: HashMap::new(),
                agents_terminal_surface_host: NativeTerminalSurfaceHost::new(),
                agents_terminal_surface_lifecycle: NativeTerminalSurfaceLifecycleState::new(),
                command_terminal_surface_host: NativeTerminalSurfaceHost::new(),
                command_terminal_surface_lifecycle: NativeTerminalSurfaceLifecycleState::new(),
                project_editor_companion_terminal_surface_host: NativeTerminalSurfaceHost::new(),
                project_editor_companion_terminal_surface_lifecycle:
                    NativeTerminalSurfaceLifecycleState::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_ghostty_surfaces: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_parked_runtime_owners: HashMap::new(),
                #[cfg(target_os = "macos")]
                command_terminal_ghostty_surfaces: HashMap::new(),
                #[cfg(target_os = "macos")]
                project_editor_companion_terminal_ghostty_surfaces: HashMap::new(),
                #[cfg(target_os = "macos")]
                command_terminal_parked_runtime_owners: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_close_confirms: AgentsTerminalCloseConfirmState::new(),
                #[cfg(target_os = "macos")]
                command_terminal_close_confirms: CommandTerminalCloseConfirmState::new(),
                #[cfg(target_os = "macos")]
                terminal_close_confirm_dialog_key: None,
                #[cfg(target_os = "macos")]
                agents_terminal_startup_ghostty_surfaces: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_ghostty_app: None,
                #[cfg(target_os = "macos")]
                command_terminal_ghostty_app: None,
                #[cfg(target_os = "macos")]
                agents_terminal_host_native_views: HashMap::new(),
                #[cfg(target_os = "macos")]
                command_terminal_host_native_views: HashMap::new(),
                #[cfg(target_os = "macos")]
                project_editor_companion_terminal_host_native_views: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_startup_host_native_views: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_appkit_focused_host: None,
                #[cfg(target_os = "macos")]
                command_terminal_appkit_focused_host: None,
                #[cfg(target_os = "macos")]
                project_editor_companion_terminal_appkit_focused_host: None,
                #[cfg(target_os = "macos")]
                agents_terminal_ghostty_surface_config_requests: HashMap::new(),
                #[cfg(target_os = "macos")]
                command_terminal_ghostty_surface_config_requests: HashMap::new(),
                #[cfg(target_os = "macos")]
                project_editor_companion_terminal_ghostty_surface_config_requests: HashMap::new(),
                #[cfg(target_os = "macos")]
                agents_terminal_startup_ghostty_surface_config_requests: HashMap::new(),
                workspace_tab_scroll_handles: HashMap::new(),
                browser_tab_scroll_handles: HashMap::new(),
                command_tab_scroll_handles: HashMap::new(),
                command_collapsed_tab_scroll_handle: ScrollHandle::new(),
                workspace_split_layout_metrics: HashMap::new(),
                command_split_layout_metrics: HashMap::new(),
                browser_split_layout_metrics: HashMap::new(),
                project_editor_companion_layout_metrics: None,
                project_editor_companion_split_layout_metrics: None,
                workspace_split_drag: None,
                workspace_split_hovering: None,
                workspace_split_hover_visible: None,
                workspace_split_hover_epoch: 0,
                command_split_drag: None,
                browser_split_drag: None,
                project_editor_companion_drag: None,
                project_editor_companion_split_drag: None,
                project_editor_companion_divider_hovering: None,
                project_editor_companion_divider_hover_visible: None,
                project_editor_companion_divider_hover_epoch: 0,
                project_editor_companion_split_divider_hovering: None,
                project_editor_companion_split_divider_hover_visible: None,
                project_editor_companion_split_divider_hover_epoch: 0,
                hovered_workspace_tab: None,
                hovered_command_tab: None,
                hovered_browser_tab: None,
                command_resize_hovering: None,
                command_resize_hover_visible: None,
                command_resize_hover_epoch: 0,
                gpui_pet_overlay_activities_visible: shell_layout_state
                    .pet_overlay_activities_visible,
                gpui_pet_overlay_avatar_hovered: false,
                gpui_pet_overlay_animation_state: GpuiPetOverlayAnimationState::Idle,
                gpui_pet_overlay_animation_started_at: Instant::now(),
                gpui_pet_overlay_animation_ticker_active: false,
                gpui_pet_overlay_reduce_motion_enabled,
                sidebar_side,
                command_pane_side,
                sidebar_width,
                sidebar_collapsed: false,
                sidebar_drag: None,
                sidebar_divider_hovering: false,
                sidebar_divider_hover_visible: false,
                sidebar_divider_hover_epoch: 0,
                app_modal_window: None,
                app_modal_window_id: app_modal_window_id_for_app,
                app_modal_open_attempt_id: 0,
                app_modal_ready_retry_used: false,
                app_modal_command_return_focus_target: None,
                plugins_modal_window: None,
                plugin_settings_action_progress: HashMap::new(),
                plugin_settings_action_errors: HashMap::new(),
                plugin_settings_action_generations: HashMap::new(),
                #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
                cef_component_window: None,
                #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
                cef_component_install_generation: 0,
                main_window_bounds,
                app_toast_window: None,
                app_toast_window_height: px(0.0),
                app_toast_anchor: None,
                app_toasts: Vec::new(),
                app_toast_epoch: 0,
                app_toast_id_counter: 0,
                agents_terminal_runtime_osc_states: HashMap::new(),
                command_terminal_runtime_osc_states: HashMap::new(),
                agents_gpui_engine_terminals: HashMap::new(),
                command_gpui_engine_terminals: HashMap::new(),
                agents_gpui_engine_close_confirms: HashSet::new(),
                command_gpui_engine_close_confirms: HashSet::new(),
                pending_terminal_paste_confirmation: None,
                terminal_paste_confirmation_dialog_open: false,
                command_gpui_engine_grid_log_states: HashMap::new(),
                terminal_search_inputs: HashMap::new(),
                terminal_search_input_subscriptions: HashMap::new(),
                terminal_search_focus_pending: None,
                agents_delayed_send_timers: HashMap::new(),
                agents_send_when_stopped_watchers: HashMap::new(),
                agents_delayed_send_generation: 0,
                agents_delayed_send_countdown_ticker_active: false,
                agents_delayed_send_persistence_ticker_active: false,
                titlebar_dropdown_focus_handle: cx.focus_handle().tab_stop(false),
                titlebar_dropdown_previous_focus_handle: None,
                titlebar_popup_menu: None,
                titlebar_popup_window: None,
                titlebar_tips_panel_open: false,
                titlebar_tips_panel: None,
                titlebar_resources_panel_open: false,
                titlebar_resources_panel_ready: false,
                titlebar_resources_panel_open_generation: 0,
                titlebar_resources_panel: None,
                titlebar_resources_presentation_groups: Vec::new(),
                titlebar_tips_cli_status: None,
                titlebar_tips_agent_hook_status: None,
                agent_hook_status_request_in_flight: false,
                sidebar: None,
                browser_surfaces: HashMap::new(),
                browser_address_inputs: HashMap::new(),
                browser_address_input_subscriptions: HashMap::new(),
                browser_address_input_editing: HashSet::new(),
                browser_find_states: HashMap::new(),
                browser_media_permission_prompts: HashMap::new(),
                browser_media_permission_decisions: load_gpui_browser_media_permission_decisions(),
                browser_find_inputs: HashMap::new(),
                browser_find_input_subscriptions: HashMap::new(),
                pending_browser_find_focus: None,
                pending_browser_address_focus: None,
                pending_browser_content_focus: None,
            };
            this.scroll_all_active_tab_strips();

            this
        });
        app.update(cx, move |this, cx| {
            let app = cx.weak_entity();
            cx.on_window_closed(move |cx, window_id| {
                if app_modal_window_id.get() != Some(window_id) {
                    return;
                }
                app_modal_window_id.set(None);
                let app = app.clone();
                cx.defer(move |cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.handle_gpui_app_modal_window_closed(window_id, cx);
                    });
                });
            })
            .detach();
            #[cfg(target_os = "macos")]
            register_gpui_app_shots_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_menu_bar_status_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_sidebar_pointer_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_session_attention_notification_callback_target(
                cx.weak_entity(),
                cx.to_async(),
            );
            #[cfg(target_os = "macos")]
            register_gpui_accessibility_display_options_callback_target(
                cx.weak_entity(),
                cx.to_async(),
            );
            #[cfg(target_os = "macos")]
            register_gpui_workspace_power_events_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_sparkle_updater_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_os_integration_callback_target(cx.weak_entity(), cx.to_async());
            #[cfg(target_os = "macos")]
            register_gpui_first_responder_callback_target(
                this.parent_ns_view,
                cx.weak_entity(),
                cx.to_async(),
            );
            #[cfg(target_os = "macos")]
            register_gpui_keyboard_router_target(
                this.parent_ns_view,
                cx.weak_entity(),
                cx.to_async(),
            );
            #[cfg(target_os = "macos")]
            register_gpui_terminal_key_event_callback_target(
                this.parent_ns_view,
                cx.weak_entity(),
                cx.to_async(),
            );
            #[cfg(target_os = "macos")]
            cef::install_first_responder_observer(this.parent_ns_view);
            let startup_activity_changed = this.restore_gpui_command_startup_activity_intents(
                command_startup_activity_restore_intents,
                cx,
            );
            let delayed_send_changed = this
                .restore_gpui_command_delayed_send_timers(command_delayed_send_restore_timers, cx);
            let agents_delayed_send_changed =
                this.restore_gpui_agents_delayed_sends(agents_delayed_send_restore_intents, cx);
            let command_gxserver_restore_started =
                this.restore_command_terminal_gxserver_sessions_from_shell_state(cx);
            this.retry_pending_command_gxserver_cleanup(cx);
            if startup_activity_changed
                || delayed_send_changed
                || agents_delayed_send_changed
                || command_gxserver_restore_started
            {
                this.persist_shell_layout_state();
            }
            this.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
            this.start_project_editor_auto_sleep_policy_polling(cx);
            this.start_command_action_status_polling(cx);
            this.start_session_chat_queued_count_polling(cx);
            this.start_prompt_editor_daemon_polling(cx);
            this.start_gpui_remote_gxserver_watchdog(cx);
            this.refresh_titlebar_actions_in_background(cx);
            this.refresh_gpui_command_close_after_done_timers(cx);
            let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
            this.sync_gpui_keep_awake_automation_from_settings(&settings_snapshot, cx);
            this.reconcile_gpui_gxserver_agent_settings_in_background(cx);
        });

        Ok(app)
    }
}
