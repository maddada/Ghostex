// C1 wave-3 extraction: a chunk (5/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


pub(crate) fn browser_node_from_shell_state(
    value: &serde_json::Value,
    tab_ids: &[BrowserTabId],
) -> Option<BrowserNode> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "leaf" => {
            let pane_id = BrowserPaneId(json_u64_field(object, "paneId")?);
            if pane_id.0 == 0 {
                return None;
            }
            let tabs = json_array_field(object, "tabs")?
                .iter()
                .map(json_u64_value)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(BrowserTabId)
                .collect::<Vec<_>>();
            if tabs.is_empty()
                || has_duplicate_u64(&tabs.iter().map(|tab_id| tab_id.0).collect::<Vec<_>>())
                || tabs.iter().any(|tab_id| !tab_ids.contains(tab_id))
            {
                return None;
            }
            let active_tab = json_u64_field(object, "activeTabId")
                .map(BrowserTabId)
                .filter(|tab_id| tabs.contains(tab_id))
                .unwrap_or(tabs[0]);
            Some(BrowserNode::Leaf(BrowserLeaf {
                pane_id,
                tab_group: BrowserTabGroup {
                    tabs: tabs
                        .into_iter()
                        .map(|tab_id| BrowserPaneTab { tab_id })
                        .collect(),
                    active_tab,
                },
            }))
        }
        "split" => {
            let split_id = BrowserSplitId(json_u64_field(object, "splitId")?);
            if split_id.0 == 0 {
                return None;
            }
            Some(BrowserNode::Split(BrowserSplit {
                id: split_id,
                axis: json_string_field(object, "axis").and_then(WorkspaceSplitAxis::from_slug)?,
                ratio: json_f32_field(object, "ratio")
                    .map(workspace_split_ratio)
                    .unwrap_or(0.5),
                first: Box::new(browser_node_from_shell_state(
                    object.get("first")?,
                    tab_ids,
                )?),
                second: Box::new(browser_node_from_shell_state(
                    object.get("second")?,
                    tab_ids,
                )?),
            }))
        }
        _ => None,
    }
}


pub(crate) fn browser_navigation_history_from_shell_state(
    value: Option<&serde_json::Value>,
    current_url: &str,
) -> BrowserNavigationHistory {
    let fallback = BrowserNavigationHistory::loaded(current_url);
    let Some(value) = value else {
        return fallback;
    };
    let Some(history) = browser_navigation_history_value_from_shell_state(value, current_url)
    else {
        return fallback;
    };
    history
}


pub(crate) fn browser_navigation_history_value_from_shell_state(
    value: &serde_json::Value,
    current_url: &str,
) -> Option<BrowserNavigationHistory> {
    let object = value.as_object()?;
    let entries = json_array_field(object, "entries")?
        .iter()
        .map(valid_sanitized_browser_history_url)
        .collect::<Option<Vec<_>>>()?;
    let current_index = usize::try_from(json_u64_field(object, "currentIndex")?).ok()?;
    if entries.is_empty()
        || entries.len() > BROWSER_HISTORY_MAX_ENTRIES
        || current_index >= entries.len()
        || entries.get(current_index).map(String::as_str) != Some(current_url)
    {
        return None;
    }

    Some(BrowserNavigationHistory {
        entries,
        current_index: Some(current_index),
    })
}


pub(crate) fn valid_sanitized_browser_history_url(value: &serde_json::Value) -> Option<String> {
    let url = value.as_str()?.trim();
    let sanitized_url = sanitize_browser_tab_url_for_state(url)?;
    (sanitized_url == url).then_some(sanitized_url)
}


pub(crate) fn browser_tab_from_shell_state(
    value: &serde_json::Value,
    browser_profiles: &BrowserProfileModel,
) -> Option<BrowserTab> {
    let object = value.as_object()?;
    let id = BrowserTabId(json_u64_field(object, "id")?);
    if id.0 == 0 {
        return None;
    }
    let requested_state = json_string_field(object, "state")
        .and_then(BrowserTabState::from_slug)
        .unwrap_or(BrowserTabState::AddressOnly);
    let sanitized_url =
        json_string_field(object, "url").and_then(sanitize_browser_tab_url_for_state);
    let (state, url) = match (requested_state, sanitized_url) {
        (BrowserTabState::Loaded, Some(url)) => (BrowserTabState::Loaded, url),
        (BrowserTabState::AddressOnly, Some(url)) => (BrowserTabState::AddressOnly, url),
        _ => (BrowserTabState::AddressOnly, String::new()),
    };

    /*
    CDXC:GPUIBrowserTabTitleCache 2026-07-12:
    Restore the persisted last-displayed title into the runtime title slot so
    the sidebar and tab strip show the pre-restart label until the page loads
    and reports a fresh document title.
    */
    let cached_title = if state == BrowserTabState::Loaded {
        json_string_field(object, "cachedTitle")
            .and_then(|title| sanitize_browser_tab_cached_title(&title))
    } else {
        None
    };
    Some(BrowserTab {
        id,
        profile_id: json_u64_field(object, "profileId")
            .map(BrowserProfileId)
            .filter(|profile_id| browser_profiles.contains_profile(*profile_id))
            .unwrap_or_else(|| browser_profiles.active_profile_id()),
        title: if state == BrowserTabState::Loaded {
            browser_tab_title_for_url(&url)
        } else {
            "New Tab".to_string()
        },
        runtime_page_title: cached_title,
        runtime_favicon_url: None,
        runtime_favicon_image: None,
        runtime_favicon_fetch: None,
        runtime_is_loading: false,
        runtime_can_go_back: false,
        runtime_can_go_forward: false,
        url: url.clone(),
        state,
        navigation_history: if state == BrowserTabState::Loaded {
            browser_navigation_history_from_shell_state(object.get("history"), &url)
        } else {
            BrowserNavigationHistory::empty()
        },
    })
}


pub(crate) fn project_view_state_to_shell_state_json(state: &GpuiProjectViewState) -> serde_json::Value {
    serde_json::json!({
        "activeMode": state.active_mode.element_slug(),
        "companionVisible": state.companion_visible,
        "companionSplitEnabled": state.companion_split_enabled,
        "companionWidthRatio": json_number_f32(project_editor_companion_width_ratio(
            state.companion_width_ratio,
        )),
        "companionSplitRatio": json_number_f32(state.companion_split_ratio.clamp(0.1, 0.9)),
        "companionTopSessionId": state.companion_top_session_id.map(|session_id| session_id.0),
        "companionBottomSessionId": state
            .companion_bottom_session_id
            .map(|session_id| session_id.0),
        "companionFocusedSlot": match state.companion_focused_slot {
            ProjectEditorCompanionTerminalSlot::Top => "top",
            ProjectEditorCompanionTerminalSlot::Bottom => "bottom",
        },
    })
}


pub(crate) fn project_view_state_from_shell_state(value: &serde_json::Value) -> Option<GpuiProjectViewState> {
    let object = value.as_object()?;
    let active_mode = object
        .get("activeMode")
        .and_then(serde_json::Value::as_str)
        .and_then(TitlebarMode::from_slug)?;
    Some(GpuiProjectViewState {
        active_mode,
        companion_visible: json_bool_field(object, "companionVisible").unwrap_or(true),
        companion_split_enabled: json_bool_field(object, "companionSplitEnabled").unwrap_or(false),
        companion_width_ratio: json_f32_field(object, "companionWidthRatio")
            .map(project_editor_companion_width_ratio)
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO),
        companion_split_ratio: json_f32_field(object, "companionSplitRatio")
            .map(|ratio| ratio.clamp(0.1, 0.9))
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO),
        companion_top_session_id: json_u64_field(object, "companionTopSessionId")
            .map(TerminalSessionId),
        companion_bottom_session_id: json_u64_field(object, "companionBottomSessionId")
            .map(TerminalSessionId),
        companion_focused_slot: match object
            .get("companionFocusedSlot")
            .and_then(serde_json::Value::as_str)
        {
            Some("bottom") => ProjectEditorCompanionTerminalSlot::Bottom,
            _ => ProjectEditorCompanionTerminalSlot::Top,
        },
    })
}


pub(crate) fn project_editor_shell_to_shell_state_json(model: &ProjectEditorShellModel) -> serde_json::Value {
    serde_json::json!({
        "leftCompanionVisible": model.left_companion_visible,
        "leftCompanionWidthRatio": json_number_f32(project_editor_companion_width_ratio(
            model.left_companion_width_ratio,
        )),
        "leftCompanionSplitEnabled": model.left_companion_split_enabled,
        "leftCompanionSplitRatio": json_number_f32(
            model.left_companion_split_ratio.clamp(0.1, 0.9),
        ),
        "modeLifecycle": project_editor_lifecycle_to_shell_state_json(model),
        "nextLifecycleRecency": model.next_lifecycle_recency,
    })
}


pub(crate) fn project_editor_shell_from_shell_state(
    value: &serde_json::Value,
    active_mode: TitlebarMode,
) -> Option<ProjectEditorShellModel> {
    let object = value.as_object()?;
    let mut model = ProjectEditorShellModel {
        left_companion_visible: json_bool_field(object, "leftCompanionVisible").unwrap_or(true),
        left_companion_width_ratio: json_f32_field(object, "leftCompanionWidthRatio")
            .map(project_editor_companion_width_ratio)
            .unwrap_or(PROJECT_EDITOR_COMPANION_WIDTH_RATIO),
        left_companion_split_enabled: json_bool_field(object, "leftCompanionSplitEnabled")
            .unwrap_or(true),
        left_companion_split_ratio: json_f32_field(object, "leftCompanionSplitRatio")
            .map(|ratio| ratio.clamp(0.1, 0.9))
            .unwrap_or(PROJECT_EDITOR_COMPANION_SPLIT_RATIO),
        ..ProjectEditorShellModel::shell_default()
    };

    if let Some(entries) = object
        .get("modeLifecycle")
        .and_then(project_editor_lifecycle_from_shell_state)
    {
        for (mode, lifecycle) in entries {
            if let Some(target) = model.lifecycle_mut(mode) {
                *target = lifecycle;
            }
        }
    }

    let max_recency = project_editor_modes()
        .iter()
        .filter_map(|mode| model.lifecycle(*mode).map(|lifecycle| lifecycle.recency))
        .max()
        .unwrap_or(0);
    model.next_lifecycle_recency = json_u64_field(object, "nextLifecycleRecency")
        .unwrap_or(model.next_lifecycle_recency)
        .max(max_recency.saturating_add(1))
        .max(1);
    model.enforce_awake_mode_cap(active_mode);
    Some(model)
}


pub(crate) fn project_editor_lifecycle_to_shell_state_json(
    model: &ProjectEditorShellModel,
) -> serde_json::Value {
    serde_json::Value::Array(
        project_editor_modes()
            .iter()
            .filter_map(|mode| {
                let lifecycle = model.lifecycle(*mode)?;
                Some(serde_json::json!({
                    "mode": mode.element_slug(),
                    "state": lifecycle.state.element_slug(),
                    "recency": lifecycle.recency,
                }))
            })
            .collect(),
    )
}


pub(crate) fn project_editor_lifecycle_from_shell_state(
    value: &serde_json::Value,
) -> Option<Vec<(TitlebarMode, ProjectEditorModeLifecycle)>> {
    Some(
        value
            .as_array()?
            .iter()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                let mode = json_string_field(object, "mode").and_then(TitlebarMode::from_slug)?;
                if !mode.is_project_editor_mode() {
                    return None;
                }
                let state = json_string_field(object, "state")
                    .and_then(ProjectEditorLifecycleState::from_slug)
                    .unwrap_or(ProjectEditorLifecycleState::Sleeping);
                Some((
                    mode,
                    ProjectEditorModeLifecycle {
                        state,
                        recency: json_u64_field(object, "recency").unwrap_or(0),
                    },
                ))
            })
            .collect(),
    )
}


pub(crate) fn shell_focus_to_shell_state_json(focus: ShellFocusTarget) -> serde_json::Value {
    match focus {
        ShellFocusTarget::AgentsPane(pane_id) => {
            serde_json::json!({ "type": "agents-pane", "paneId": pane_id.0 })
        }
        ShellFocusTarget::CommandPane => serde_json::json!({ "type": "command-pane" }),
        ShellFocusTarget::BrowserSurface => serde_json::json!({ "type": "browser-surface" }),
        ShellFocusTarget::BrowserPane(pane_id) => {
            serde_json::json!({ "type": "browser-pane", "paneId": pane_id.0 })
        }
        ShellFocusTarget::ProjectEditorSurface(mode) => serde_json::json!({
            "type": "project-editor-surface",
            "mode": mode.element_slug(),
        }),
        ShellFocusTarget::ProjectEditorCompanion(mode) => serde_json::json!({
            "type": "project-editor-companion",
            "mode": mode.element_slug(),
        }),
    }
}


pub(crate) fn shell_focus_from_shell_state(value: &serde_json::Value) -> Option<ShellFocusTarget> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "agents-pane" => Some(ShellFocusTarget::AgentsPane(WorkspacePaneId(
            json_u64_field(object, "paneId")?,
        ))),
        "command-pane" => Some(ShellFocusTarget::CommandPane),
        "browser-surface" => Some(ShellFocusTarget::BrowserSurface),
        "browser-pane" => Some(ShellFocusTarget::BrowserPane(BrowserPaneId(
            json_u64_field(object, "paneId")?,
        ))),
        "project-editor-surface" => json_string_field(object, "mode")
            .and_then(TitlebarMode::from_slug)
            .map(ShellFocusTarget::ProjectEditorSurface),
        "project-editor-companion" => json_string_field(object, "mode")
            .and_then(TitlebarMode::from_slug)
            .map(ShellFocusTarget::ProjectEditorCompanion),
        _ => None,
    }
}


pub(crate) fn valid_shell_focus_or_default_with_browser_tabs(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    command_pane: &CommandPaneModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> ShellFocusTarget {
    if let Some(focus) = valid_non_command_shell_focus_with_browser_tabs(
        focus,
        active_mode,
        agents_workspace,
        project_editor_shell,
        browser_tabs,
    ) {
        return focus;
    }

    match focus {
        ShellFocusTarget::CommandPane if command_pane.has_sessions() => focus,
        _ => default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell),
    }
}


pub(crate) fn valid_non_command_shell_focus(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
) -> Option<ShellFocusTarget> {
    match focus {
        ShellFocusTarget::BrowserPane(_) => None,
        _ => valid_non_command_shell_focus_with_browser_tabs(
            focus,
            active_mode,
            agents_workspace,
            project_editor_shell,
            &BrowserTabModel::shell_default(),
        ),
    }
}


pub(crate) fn valid_shell_focus_or_default(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    command_pane: &CommandPaneModel,
    project_editor_shell: &ProjectEditorShellModel,
) -> ShellFocusTarget {
    if let Some(focus) =
        valid_non_command_shell_focus(focus, active_mode, agents_workspace, project_editor_shell)
    {
        return focus;
    }

    match focus {
        ShellFocusTarget::CommandPane if command_pane.has_sessions() => focus,
        _ => default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell),
    }
}


pub(crate) fn valid_non_command_shell_focus_with_browser_tabs(
    focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> Option<ShellFocusTarget> {
    match focus {
        ShellFocusTarget::AgentsPane(pane_id)
            if active_mode == TitlebarMode::Agents
                && agents_workspace.find_leaf(pane_id).is_some() =>
        {
            Some(focus)
        }
        ShellFocusTarget::BrowserSurface if active_mode == TitlebarMode::Browser => Some(focus),
        ShellFocusTarget::BrowserPane(pane_id)
            if active_mode == TitlebarMode::Browser
                && browser_tabs.find_leaf(pane_id).is_some() =>
        {
            Some(focus)
        }
        ShellFocusTarget::ProjectEditorSurface(mode)
            if active_mode == mode
                && matches!(
                    mode,
                    TitlebarMode::Source
                        | TitlebarMode::Kanban
                        | TitlebarMode::Automate
                        | TitlebarMode::Manage
                ) =>
        {
            Some(focus)
        }
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode
                && project_editor_shell.left_companion_visible
                && matches!(
                    mode,
                    TitlebarMode::Source
                        | TitlebarMode::Browser
                        | TitlebarMode::Kanban
                        | TitlebarMode::Automate
                        | TitlebarMode::Manage
                ) =>
        {
            Some(focus)
        }
        ShellFocusTarget::CommandPane
        | ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => None,
    }
}


pub(crate) fn default_shell_focus_for_mode(
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    _project_editor_shell: &ProjectEditorShellModel,
) -> ShellFocusTarget {
    match active_mode {
        TitlebarMode::Agents => ShellFocusTarget::AgentsPane(agents_workspace.focused_pane),
        TitlebarMode::Browser => ShellFocusTarget::BrowserSurface,
        TitlebarMode::Source
        | TitlebarMode::Kanban
        | TitlebarMode::Automate
        | TitlebarMode::Manage => ShellFocusTarget::ProjectEditorSurface(active_mode),
    }
}


pub(crate) fn restored_non_command_shell_focus_or_default_with_browser_tabs(
    previous_non_command_focus: Option<ShellFocusTarget>,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
    browser_tabs: &BrowserTabModel,
) -> ShellFocusTarget {
    /*
    CDXC:GPUIKeyboardFocus 2026-06-25-19:27:
    Command-panel collapse may restore only a currently valid non-command focus target. Stale panes, hidden companions, wrong workarea modes, and command-pane focus fall back through the active shell mode instead of persisting unusable keyboard ownership.
    */
    previous_non_command_focus
        .and_then(|focus| {
            valid_non_command_shell_focus_with_browser_tabs(
                focus,
                active_mode,
                agents_workspace,
                project_editor_shell,
                browser_tabs,
            )
        })
        .unwrap_or_else(|| {
            default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell)
        })
}


pub(crate) fn restored_non_command_shell_focus_or_default(
    previous_non_command_focus: Option<ShellFocusTarget>,
    active_mode: TitlebarMode,
    agents_workspace: &WorkspaceModel,
    project_editor_shell: &ProjectEditorShellModel,
) -> ShellFocusTarget {
    previous_non_command_focus
        .and_then(|focus| {
            valid_non_command_shell_focus(
                focus,
                active_mode,
                agents_workspace,
                project_editor_shell,
            )
        })
        .unwrap_or_else(|| {
            default_shell_focus_for_mode(active_mode, agents_workspace, project_editor_shell)
        })
}


pub(crate) fn prune_agents_terminal_startup_body_slot_geometries(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
) {
    let current_slot_ids = if agents_workspace_visible {
        agents_workspace.rendered_terminal_startup_body_slots()
    } else {
        Vec::new()
    };
    startup_body_geometries.retain(|slot_id, _| current_slot_ids.contains(slot_id));
}


pub(crate) fn record_agents_terminal_startup_body_slot_geometry(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    slot_id: AgentsTerminalStartupBodySlotId,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
) {
    prune_agents_terminal_startup_body_slot_geometries(
        agents_workspace_visible,
        agents_workspace,
        startup_body_geometries,
    );

    if agents_workspace_visible && agents_workspace.is_current_terminal_startup_body_slot(slot_id) {
        startup_body_geometries.insert(
            slot_id,
            AgentsTerminalStartupBodyGeometry {
                bounds,
                scale_factor,
            },
        );
    } else {
        startup_body_geometries.remove(&slot_id);
    }
}


pub(crate) fn prune_agents_terminal_parked_owner_body_slot_geometries(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
) {
    let current_slot_ids = if agents_workspace_visible {
        agents_workspace.rendered_terminal_parked_owner_body_slots()
    } else {
        Vec::new()
    };
    parked_owner_body_geometries.retain(|slot_id, _| current_slot_ids.contains(slot_id));
}


pub(crate) fn record_agents_terminal_parked_owner_body_slot_geometry(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
) {
    prune_agents_terminal_parked_owner_body_slot_geometries(
        agents_workspace_visible,
        agents_workspace,
        parked_owner_body_geometries,
    );

    if agents_workspace_visible
        && agents_workspace.is_current_terminal_parked_owner_body_slot(slot_id)
    {
        parked_owner_body_geometries.insert(
            slot_id,
            AgentsTerminalParkedOwnerBodyGeometry {
                bounds,
                scale_factor,
            },
        );
    } else {
        parked_owner_body_geometries.remove(&slot_id);
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn gpui_terminal_ghostty_surface_config_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    /*
    CDXC:GPUITerminalSettings 2026-06-24-11:27:
    GPUI embedded terminal surfaces consume the shared Settings service directly for supported `ghostty_surface_config_s` fields. Only `terminalFontSize` maps to the current FFI request as `font_size`; other Ghostty settings remain unthreaded here because GPUI has no safe direct runtime field or reload contract for them yet.

    CDXC:GPUICommandTerminalSettings 2026-06-27-10:10:
    Command-pane Ghostty surfaces share this bounded GPUI terminal-settings mapper with Agents surfaces. Apply the FFI-supported `terminalFontSize` to recreated/prepared surface requests, and keep font family, theme, cursor, scrollback, clipboard, paste-preview, and mouse settings on the Ghostty config-file path until GhosttyKit exposes a safe live request field or reload contract.
    */
    let terminal_config = settings.terminal_ghostty_surface_config();
    terminal_ghostty_surface::GhosttySurfaceTerminalConfig::with_font_size(
        terminal_config.font_size(),
    )
}


#[cfg(target_os = "macos")]
pub(crate) fn current_gpui_terminal_ghostty_surface_config()
-> terminal_ghostty_surface::GhosttySurfaceTerminalConfig {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_terminal_ghostty_surface_config_from_shared_settings(&settings)
}


#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_host_config_requests<F>(
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: Option<
        &mut HashMap<
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::StartupGhosttySurfaceOwner,
        >,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    startup_launch_payload_source: &AgentsTerminalStartupLaunchPayloadSource,
    terminal_config: terminal_ghostty_surface::GhosttySurfaceTerminalConfig,
    parent_ns_view: *mut std::ffi::c_void,
    create_host_view: F,
) where
    F: FnMut(
        terminal_native_view::TerminalHostNativeViewCreateRequest,
    ) -> Result<
        terminal_native_view::OwnedTerminalHostNativeView,
        terminal_native_view::TerminalHostNativeViewCreateError,
    >,
{
    /*
    CDXC:GPUITerminalStartupNativeHost 2026-06-23-03:23:
    Startup host/config request reconciliation creates hidden host views only from current Mounting launch plans with exact geometry. If render-start clears geometry before the next body canvas records, preserve an already-owned startup host/config only when the current pending record still matches the same runtime id and `AgentsTerminalStartupBodySlotId`; stale pending state, invalid parent/bounds/config, or missing current records must drop the runtime-only state.

    CDXC:GPUITerminalStartupLaunchPayloadSource 2026-06-23-04:00:
    Startup config requests may receive a launch payload only from the runtime-only explicit source for the same launch plan identity. If a future explicit payload fails validation, skip the config request so the hidden startup host/surface is pruned without falling back to terminal titles, status text, project paths, sidebar labels, delayed-send state, or inferred cwd/command/env values.
    */
    terminal_native_view::reconcile_app_owned_terminal_startup_host_native_view(
        startup_host_native_views,
        startup_launch_plans,
        startup_host_preservation_keys,
        parent_ns_view,
        create_host_view,
    );

    let current_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    *startup_config_requests = startup_host_native_views
        .iter()
        .filter_map(|(slot_id, host_view)| {
            let plan = current_launch_plans_by_slot
                .get(slot_id)
                .copied()
                .unwrap_or_else(|| host_view.startup_launch_plan());
            let request =
                terminal_native_view::ghostty_surface_config_request_for_app_owned_terminal_startup_host_native_view(
                    Some(host_view),
                )
                .ok()
                .flatten()?;
            let request = request.with_terminal_config(terminal_config);
            let launch_payload = startup_launch_payload_source
                .payload_for_launch_plan(plan)
                .ok()?;
            let request = if let Some(launch_payload) = launch_payload {
                request.with_launch_payload(launch_payload)
            } else {
                request
            };
            Some((*slot_id, request))
        })
        .collect();
    if let Some(startup_surface_owners) = startup_surface_owners {
        let startup_host_slots_without_config = startup_host_native_views
            .keys()
            .copied()
            .filter(|slot_id| !startup_config_requests.contains_key(slot_id))
            .collect::<Vec<_>>();
        for slot_id in startup_host_slots_without_config {
            startup_surface_owners.remove(&slot_id);
        }
    }
    startup_host_native_views.retain(|slot_id, _| startup_config_requests.contains_key(slot_id));
}


#[cfg(target_os = "macos")]
pub(crate) fn drop_agents_terminal_startup_ghostty_surface_owners_before_host_reconcile(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_host_native_views: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    parent_ns_view: *mut std::ffi::c_void,
) {
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();
    let stale_surface_slot_ids = startup_surface_owners
        .keys()
        .copied()
        .filter(|slot_id| {
            let Some(host_view) = startup_host_native_views.get(slot_id) else {
                return true;
            };

            !terminal_native_view::app_owned_terminal_startup_host_native_view_will_survive_reconcile(
                host_view,
                startup_launch_plans_by_slot.get(slot_id).copied(),
                startup_host_preservation_keys_by_slot.get(slot_id).copied(),
                parent_ns_view,
            )
        })
        .collect::<Vec<_>>();

    for slot_id in stale_surface_slot_ids {
        startup_surface_owners.remove(&slot_id);
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn reconcile_agents_terminal_startup_ghostty_surface_owners<F>(
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    ghostty_app: &mut Option<terminal_ghostty_surface::GhosttyAppOwner>,
    startup_config_requests: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    mut create_ghostty_app: F,
) where
    F: FnMut() -> Result<
        terminal_ghostty_surface::GhosttyAppOwner,
        terminal_ghostty_surface::GhosttySurfaceRuntimeError,
    >,
{
    /*
    CDXC:GPUITerminalStartupGhosttySurface 2026-06-23-03:33:
    Startup Ghostty surface owners are runtime-only consumers of prepared startup config requests and launch-created geometry. Create only when a matching config request and launch plan exist, preserve same-slot/same-runtime owners across geometry-gap preservation, and drop stale or invalid owners without showing/focusing hosts, applying startup results, logging, persisting, or touching Running mount-slot maps.
    */
    let startup_launch_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let startup_host_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();

    startup_surface_owners.retain(|slot_id, owner| {
        if !startup_config_requests.contains_key(slot_id)
            || owner.startup_body_slot_id() != *slot_id
        {
            return false;
        }

        if let Some(plan) = startup_launch_plans_by_slot.get(slot_id) {
            owner.runtime_session_id() == plan.runtime_session_id
        } else {
            startup_host_preservation_keys_by_slot
                .get(slot_id)
                .is_some_and(|key| owner.runtime_session_id() == key.runtime_session_id)
        }
    });

    for plan in startup_launch_plans {
        let plan = *plan;
        let slot_id = plan.startup_body_slot_id;
        if !startup_launch_plans_by_slot
            .get(&slot_id)
            .is_some_and(|current_plan| *current_plan == plan)
        {
            continue;
        }

        let Some(request) = startup_config_requests.get(&slot_id) else {
            startup_surface_owners.remove(&slot_id);
            continue;
        };
        if terminal_ghostty_surface::GhosttySurfacePixelSize::from_gpui_bounds(
            plan.bounds,
            f64::from(plan.scale_factor),
        )
        .is_err()
        {
            startup_surface_owners.remove(&slot_id);
            continue;
        }

        if startup_surface_owners.get(&slot_id).is_some_and(|owner| {
            owner.startup_body_slot_id() != slot_id
                || owner.runtime_session_id() != plan.runtime_session_id
        }) {
            startup_surface_owners.remove(&slot_id);
        }

        if !startup_surface_owners.contains_key(&slot_id) {
            if ghostty_app.is_none() {
                let Ok(app) = create_ghostty_app() else {
                    startup_surface_owners.clear();
                    return;
                };
                *ghostty_app = Some(app);
            }

            let Some(app) = ghostty_app.as_ref() else {
                return;
            };
            let Ok(surface) = terminal_ghostty_surface::StartupGhosttySurfaceOwner::new(
                app,
                slot_id,
                plan.runtime_session_id,
                request,
            ) else {
                startup_surface_owners.remove(&slot_id);
                continue;
            };
            startup_surface_owners.insert(slot_id, surface);
        }

        let update_failed = startup_surface_owners
            .get_mut(&slot_id)
            .is_some_and(|surface| {
                surface
                    .update_content_scale_and_size(plan.bounds, f64::from(plan.scale_factor))
                    .is_err()
            });
        if update_failed {
            startup_surface_owners.remove(&slot_id);
        }
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_startup_surface_metadata_snapshots(
    startup_surface_owners: &HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
) -> Vec<(
    AgentsTerminalRuntimeSessionId,
    AgentsTerminalStartupBodySlotId,
    terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
)> {
    /*
    CDXC:GPUITerminalStartupReadiness 2026-06-23-04:13:
    Reading startup surface metadata prepares only a runtime handoff fact for an exact current startup intent. The caller may promote Ready only through the startup-to-Running owner transfer path, so metadata alone still cannot fake Running, create Failed, persist ids, log tty/process facts, or expose raw terminal data.

    CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
    Surface metadata is sampled only from the current startup-owned surface map entry whose key matches the owner's startup body slot. The snapshot carries redacted booleans only, so runtime failure handling can distinguish process-exited from ready metadata without exposing raw pid, tty, command, cwd/path, env, output, terminal content, or runtime ids outside runtime memory.
    */
    startup_surface_owners
        .iter()
        .filter_map(|(startup_body_slot_id, surface)| {
            (surface.startup_body_slot_id() == *startup_body_slot_id).then(|| {
                (
                    surface.runtime_session_id(),
                    *startup_body_slot_id,
                    surface.metadata_snapshot(),
                )
            })
        })
        .collect()
}


#[cfg(target_os = "macos")]
pub(crate) fn failed_agents_terminal_startup_results_from_metadata(
    startup_coordinator: &AgentsTerminalStartupCoordinator,
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) -> Vec<AgentsTerminalStartupResult> {
    metadata_snapshots
        .into_iter()
        .filter_map(
            |(runtime_session_id, startup_body_slot_id, surface_metadata)| {
                startup_coordinator.produce_failed_startup_result_from_surface_metadata(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    runtime_session_id,
                    startup_body_slot_id,
                    surface_metadata,
                )
            },
        )
        .collect()
}


#[cfg(target_os = "macos")]
pub(crate) fn sync_agents_terminal_startup_readiness_signal_preparations(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    metadata_snapshots: impl IntoIterator<
        Item = (
            AgentsTerminalRuntimeSessionId,
            AgentsTerminalStartupBodySlotId,
            terminal_ghostty_surface::GhosttySurfaceMetadataSnapshot,
        ),
    >,
) {
    startup_coordinator.sync_startup_readiness_signal_preparations(metadata_snapshots);
}


#[cfg(target_os = "macos")]
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    /*
    CDXC:GPUITerminalStartupRuntimeFailure 2026-06-23-04:38:
    Failed startup cleanup retires startup-only runtime state for the exact completion intent. Remove the startup Ghostty surface before its hidden AppKit host, remove prepared config/geometry/payload state, and leave the shell session as the retryable StartupFailed placeholder without creating Running ownership.
    */
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_surface_owners.remove(&completion_intent.startup_body_slot_id);
    startup_config_requests.remove(&completion_intent.startup_body_slot_id);
    startup_host_native_views.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}


#[cfg(not(target_os = "macos"))]
pub(crate) fn prune_agents_terminal_startup_runtime_state_for_completion_intent(
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_launch_payload_source: &mut AgentsTerminalStartupLaunchPayloadSource,
    completion_intent: AgentsTerminalStartupCompletionIntent,
) {
    startup_body_geometries.remove(&completion_intent.startup_body_slot_id);
    startup_launch_payload_source.remove_payload_for_completion_intent(completion_intent);
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_attachment_plan_for_startup_handoff(
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
    terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
        host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
            handoff_plan.mount_slot_id,
        ),
        slot_id: handoff_plan.mount_slot_id,
        bounds: handoff_plan.startup_launch_plan.bounds,
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn transfer_ready_agents_terminal_startup_handoff(
    startup_coordinator: &mut AgentsTerminalStartupCoordinator,
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    startup_body_geometries: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AgentsTerminalStartupBodyGeometry,
    >,
    startup_host_native_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_native_view::AppOwnedTerminalStartupHostNativeView,
    >,
    startup_surface_owners: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::StartupGhosttySurfaceOwner,
    >,
    startup_config_requests: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    running_mount_slot_bounds: &mut HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    running_config_requests: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    handoff_plan: AgentsTerminalStartupReadinessHandoffPlan,
) -> bool {
    /*
    CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
    Ready startup promotion is a single ownership move, not a new launch. Require exact current startup readiness plus empty target Running owner maps, remove the startup host/surface only into local ownership, promote the same shell session to Running, and then insert the same AppKit host and Ghostty surface under the resulting `AgentsTerminalBodyMountSlotId`.
    */
    if startup_coordinator.startup_readiness_handoff_plan_for_runtime_session(
        true,
        workspace,
        runtime_sessions,
        handoff_plan.runtime_session_id(),
    ) != Some(handoff_plan)
    {
        return false;
    }

    let startup_body_slot_id = handoff_plan.startup_body_slot_id();
    let mount_slot_id = handoff_plan.mount_slot_id;
    let attachment_plan = agents_terminal_attachment_plan_for_startup_handoff(handoff_plan);
    if running_host_native_views.contains_key(&mount_slot_id)
        || running_surface_owners.contains_key(&mount_slot_id)
        || running_config_requests.contains_key(&mount_slot_id)
    {
        return false;
    }

    if !startup_host_native_views
        .get(&startup_body_slot_id)
        .is_some_and(|host_view| {
            host_view.startup_launch_plan() == handoff_plan.startup_launch_plan
                && host_view.can_transfer_to_running_attachment_plan(attachment_plan)
        })
    {
        return false;
    }
    if !startup_surface_owners
        .get(&startup_body_slot_id)
        .is_some_and(|surface| {
            surface.startup_body_slot_id() == startup_body_slot_id
                && surface.runtime_session_id() == handoff_plan.runtime_session_id()
        })
    {
        return false;
    }

    let Some(startup_host_view) = startup_host_native_views.remove(&startup_body_slot_id) else {
        return false;
    };
    let Some(startup_surface_owner) = startup_surface_owners.remove(&startup_body_slot_id) else {
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    };

    let changed = startup_coordinator.apply_startup_result(
        workspace,
        runtime_sessions,
        AgentsTerminalStartupResult::Ready {
            completion_intent: handoff_plan.completion_intent,
        },
    );
    if !changed {
        startup_surface_owners.insert(startup_body_slot_id, startup_surface_owner);
        startup_host_native_views.insert(startup_body_slot_id, startup_host_view);
        return false;
    }

    running_mount_slot_bounds.insert(mount_slot_id, handoff_plan.startup_launch_plan.bounds);
    running_host_native_views.insert(
        mount_slot_id,
        startup_host_view.into_running_host_native_view(attachment_plan),
    );
    running_surface_owners.insert(
        mount_slot_id,
        startup_surface_owner.into_running_surface_owner(mount_slot_id),
    );
    startup_config_requests.remove(&startup_body_slot_id);
    startup_body_geometries.remove(&startup_body_slot_id);
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn terminal_presentation_state_can_hold_parked_runtime_owner(session: &TerminalSession) -> bool {
    matches!(
        session.presentation_state,
        TerminalSessionPresentationState::Running
            | TerminalSessionPresentationState::Sleeping
            | TerminalSessionPresentationState::PoppedOutPlaceholder
    ) || (session.presentation_state == TerminalSessionPresentationState::Mounting
        && !session.can_enter_startup_pipeline())
}


#[cfg(target_os = "macos")]
pub(crate) fn prune_agents_terminal_parked_runtime_owners(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Parked Agents owners survive only while the same shell session and process-local runtime id remain current and absent from Running owner maps. The remembered slot stays the proof of where the owner was parked. Running tabs may otherwise park while inactive so tab switches preserve their attached terminal like macOS; stale entries are pruned instead of relaunched, inferred from titles/paths/commands, or promoted to fake Running state.
    */
    parked_runtime_owners.retain(|runtime_session_id, owner| {
        *runtime_session_id == owner.runtime_session_id
            && runtime_sessions.runtime_session_id_for_shell_session(owner.shell_session_id)
                == Some(*runtime_session_id)
            && workspace.has_session(owner.shell_session_id)
            && workspace
                .session(owner.shell_session_id)
                .is_some_and(terminal_presentation_state_can_hold_parked_runtime_owner)
            && !running_host_native_views.contains_key(&owner.mount_slot_id)
            && !running_surface_owners.contains_key(&owner.mount_slot_id)
            && owner.matches_identity(
                *runtime_session_id,
                owner.shell_session_id,
                owner.mount_slot_id,
            )
    });
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_parked_owner_reattach_plan_for_slot(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<AgentsTerminalParkedOwnerReattachPlan> {
    if !agents_workspace_visible || !workspace.is_current_terminal_parked_owner_body_slot(slot_id) {
        return None;
    }
    let session = workspace.session(slot_id.session_id)?;
    if session.presentation_state != TerminalSessionPresentationState::Mounting
        || session.can_enter_startup_pipeline()
    {
        return None;
    }
    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let geometry = parked_owner_body_geometries.get(&slot_id).copied()?;
    let plan = AgentsTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        shell_session_id: slot_id.session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds: geometry.bounds,
        scale_factor: geometry.scale_factor,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_running_parked_owner_reattach_plan_for_slot(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_mount_slot_bounds: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<AgentsTerminalParkedOwnerReattachPlan> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
    Inactive Running Agents tabs keep their AppKit/Ghostty owner parked so switching back to a sidebar-attached session shows the existing terminal immediately instead of creating a blank replacement shell. Reattach only when the same Running slot is current and its body bounds were recorded by the normal mount-slot canvas.
    */
    if !agents_workspace_visible || !workspace.is_current_terminal_body_mount_slot(slot_id) {
        return None;
    }
    if !workspace
        .session(slot_id.session_id)
        .is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Running
        })
    {
        return None;
    }
    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let bounds = *running_mount_slot_bounds.get(&slot_id)?;
    let plan = AgentsTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        shell_session_id: slot_id.session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds,
        scale_factor: 1.0,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_parked_owner_reattach_plans(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    running_mount_slot_bounds: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
) -> Vec<AgentsTerminalParkedOwnerReattachPlan> {
    let mut plans = workspace
        .rendered_terminal_parked_owner_body_slots()
        .into_iter()
        .filter_map(|slot_id| {
            agents_terminal_parked_owner_reattach_plan_for_slot(
                agents_workspace_visible,
                workspace,
                runtime_sessions,
                parked_runtime_owners,
                parked_owner_body_geometries,
                slot_id,
            )
        })
        .collect::<Vec<_>>();
    plans.extend(
        workspace
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .filter_map(|slot_id| {
                agents_terminal_running_parked_owner_reattach_plan_for_slot(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    parked_runtime_owners,
                    running_mount_slot_bounds,
                    slot_id,
                )
            }),
    );
    plans.sort_by_key(|plan| {
        (
            plan.current_mount_slot_id.pane_id.0,
            plan.current_mount_slot_id.session_id.0,
            plan.runtime_session_id.0,
        )
    });
    plans
}


#[cfg(target_os = "macos")]
pub(crate) fn park_agents_terminal_runtime_owner_before_host_detach(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    detach_plan: terminal_surface_host::NativeTerminalSurfaceAttachmentPlan,
) -> bool {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Detaching a Running slot parks ownership when the same shell tab remains a valid Running, Sleeping, popped-out, or non-startup Mounting owner. This preserves inactive running tabs across ordinary tab switches while still requiring the exact AppKit host and Ghostty surface to match the runtime id; otherwise the normal detach/drop path remains honest instead of creating a fallback parked owner.
    */
    let slot_id = detach_plan.slot_id;
    let Some(session) = workspace.session(slot_id.session_id) else {
        return false;
    };
    if !terminal_presentation_state_can_hold_parked_runtime_owner(session)
        || !workspace.session_belongs_to_pane(slot_id.pane_id, slot_id.session_id)
    {
        return false;
    }
    let Some(runtime_session_id) =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
    else {
        return false;
    };
    if parked_runtime_owners.contains_key(&runtime_session_id)
        || parked_runtime_owners.values().any(|owner| {
            owner.mount_slot_id == slot_id || owner.shell_session_id == slot_id.session_id
        })
    {
        return false;
    }
    if !running_host_native_views
        .get(&slot_id)
        .is_some_and(|host| host.attachment_plan().same_attachment_identity(detach_plan))
        || !running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id && surface.runtime_session_id() == runtime_session_id
        })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&slot_id) else {
        running_host_native_views.insert(slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        AgentsTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            slot_id.session_id,
            slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn park_agents_terminal_runtime_owner_for_group_move(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    source_slot_id: AgentsTerminalBodyMountSlotId,
) -> bool {
    /*
    CDXC:GPUISidebarGroupFocus 2026-07-10:
    Before a sidebar-selected Running terminal moves from its old Agents group
    into the currently focused group, park its exact AppKit/Ghostty owners.
    The normal render pass will provide the destination bounds and reattach the
    same process under the new slot; no shell replay, fallback surface, hidden
    overlap, or synthetic input routing participates.
    */
    let Some(session) = workspace.session(source_slot_id.session_id) else {
        return false;
    };
    if session.presentation_state != TerminalSessionPresentationState::Running
        || !workspace.session_belongs_to_pane(source_slot_id.pane_id, source_slot_id.session_id)
    {
        return false;
    }
    let Some(runtime_session_id) =
        runtime_sessions.runtime_session_id_for_shell_session(source_slot_id.session_id)
    else {
        return false;
    };
    if parked_runtime_owners.contains_key(&runtime_session_id) {
        return false;
    }
    if !running_host_native_views
        .get(&source_slot_id)
        .is_some_and(|host| host.attachment_plan().slot_id == source_slot_id)
        || !running_surface_owners
            .get(&source_slot_id)
            .is_some_and(|surface| {
                surface.mount_slot_id() == source_slot_id
                    && surface.runtime_session_id() == runtime_session_id
            })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&source_slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&source_slot_id) else {
        running_host_native_views.insert(source_slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        AgentsTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            source_slot_id.session_id,
            source_slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn transfer_agents_terminal_parked_runtime_owner_reattach(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    running_mount_slot_bounds: &mut HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    running_config_requests: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    plan: AgentsTerminalParkedOwnerReattachPlan,
) -> bool {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Reattach is transactional: require exact current body geometry, exact parked runtime ownership, empty Running owner maps, and the same pane/session slot before moving ownership back. Mounting wake/reattach placeholders transition to Running; already-Running inactive tabs keep their shell state and reclaim the parked host/surface without relaunching or showing a blank replacement.
    */
    let Some(session) = workspace.session(plan.shell_session_id) else {
        return false;
    };
    let presentation_state = session.presentation_state;
    let can_enter_startup_pipeline = session.can_enter_startup_pipeline();
    let current_body_matches = if presentation_state == TerminalSessionPresentationState::Running {
        workspace.is_current_terminal_body_mount_slot(plan.current_mount_slot_id)
            && running_mount_slot_bounds
                .get(&plan.current_mount_slot_id)
                .is_some_and(|bounds| *bounds == plan.bounds)
    } else if presentation_state == TerminalSessionPresentationState::Mounting
        && !can_enter_startup_pipeline
    {
        workspace.is_current_terminal_parked_owner_body_slot(plan.current_mount_slot_id)
            && parked_owner_body_geometries
                .get(&plan.current_mount_slot_id)
                .is_some_and(|geometry| {
                    geometry.bounds == plan.bounds && geometry.scale_factor == plan.scale_factor
                })
    } else {
        false
    };
    if runtime_sessions.runtime_session_id_for_shell_session(plan.shell_session_id)
        != Some(plan.runtime_session_id)
        || !current_body_matches
        || running_host_native_views.contains_key(&plan.current_mount_slot_id)
        || running_surface_owners.contains_key(&plan.current_mount_slot_id)
        || running_config_requests.contains_key(&plan.current_mount_slot_id)
    {
        return false;
    }
    let Some(parked_owner) = parked_runtime_owners.get(&plan.runtime_session_id) else {
        return false;
    };
    if !parked_owner.can_reattach_with_plan(plan) {
        return false;
    }

    let Some(parked_owner) = parked_runtime_owners.remove(&plan.runtime_session_id) else {
        return false;
    };
    if presentation_state == TerminalSessionPresentationState::Mounting {
        let changed = workspace.transition_terminal_session_presentation_state(
            plan.shell_session_id,
            TerminalSessionPresentationState::Mounting,
            TerminalSessionPresentationState::Running,
        );
        if !changed {
            parked_runtime_owners.insert(plan.runtime_session_id, parked_owner);
            return false;
        }
    }

    let (host_native_view, surface_owner) = parked_owner.into_running_owners(plan);
    running_mount_slot_bounds.insert(plan.current_mount_slot_id, plan.bounds);
    running_host_native_views.insert(plan.current_mount_slot_id, host_native_view);
    running_surface_owners.insert(plan.current_mount_slot_id, surface_owner);
    parked_owner_body_geometries.remove(&plan.current_mount_slot_id);
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_session_can_hold_parked_runtime_owner(
    command_pane: &CommandPaneModel,
    slot_id: CommandTerminalBodyMountSlotId,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Native command-panel owner selection parks renderer ownership whenever the command session still belongs to its command group, not only when Sleep hides it. Inactive tabs, collapsed command panels, and Focus-hidden groups may reattach the same host/surface later; removed or stale command sessions still prune.
    */
    command_pane_group_for_session(command_pane, slot_id.session_id) == Some(slot_id.group_id)
        && command_pane.session(slot_id.session_id).is_some()
}


#[cfg(target_os = "macos")]
pub(crate) fn prune_command_terminal_parked_runtime_owners(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Parked command owners survive only for the same command group/session slot while the tab is still part of command-panel state. Stale session membership, close/removal, and Running owner collisions prune the parked process instead of relaunching, retargeting, logging, persisting, or fabricating fallback surfaces.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Owner-selection parity requires inactive, collapsed, and Focus-hidden command tabs to keep their parked runtime owners. Prune only when the command session no longer belongs to that exact command group or a live Running owner already exists for the slot.
    */
    parked_runtime_owners.retain(|runtime_session_id, owner| {
        *runtime_session_id == owner.runtime_session_id
            && command_terminal_runtime_session_id(owner.mount_slot_id) == *runtime_session_id
            && command_terminal_session_can_hold_parked_runtime_owner(
                command_pane,
                owner.mount_slot_id,
            )
            && !running_host_native_views.contains_key(&owner.mount_slot_id)
            && !running_surface_owners.contains_key(&owner.mount_slot_id)
            && owner.matches_identity(*runtime_session_id, owner.mount_slot_id)
    });
}


#[cfg(target_os = "macos")]
pub(crate) fn park_command_terminal_runtime_owner_before_host_detach(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    detach_plan: terminal_surface_host::NativeTerminalSurfaceAttachmentPlan<
        CommandTerminalBodyMountSlotId,
    >,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Command `HideAndDetach` parks ownership only when the command tab still belongs to its exact group. The AppKit host and Ghostty surface must already exist with the exact command slot/runtime identity; close/removal, stale groups, collisions, and missing owners continue through the honest detach/drop path.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Native owner-selection, collapse, and Focus hiding detach visible command panes without freeing their terminal owners. Do not require `isSleeping`; a live inactive command tab should park so reselecting or reopening can reattach the same runtime owner instead of creating a replacement process.
    */
    let slot_id = detach_plan.slot_id;
    if command_pane.session(slot_id.session_id).is_none()
        || command_pane_group_for_session(command_pane, slot_id.session_id)
            != Some(slot_id.group_id)
    {
        return false;
    }
    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    if parked_runtime_owners.contains_key(&runtime_session_id)
        || parked_runtime_owners.values().any(|owner| {
            owner.mount_slot_id == slot_id || owner.mount_slot_id.session_id == slot_id.session_id
        })
    {
        return false;
    }
    if !running_host_native_views
        .get(&slot_id)
        .is_some_and(|host| host.attachment_plan().same_attachment_identity(detach_plan))
        || !running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id && surface.runtime_session_id() == runtime_session_id
        })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&slot_id) else {
        running_host_native_views.insert(slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        CommandTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_parked_owner_reattach_plan_for_slot(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    current_body_bounds: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: CommandTerminalBodyMountSlotId,
) -> Option<CommandTerminalParkedOwnerReattachPlan> {
    if !command_pane.is_current_terminal_body_mount_slot(slot_id)
        || command_pane_group_for_session(command_pane, slot_id.session_id)
            != Some(slot_id.group_id)
        || command_pane
            .session(slot_id.session_id)
            .is_some_and(|session| session.is_sleeping)
    {
        return None;
    }
    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let bounds = current_body_bounds.get(&slot_id).copied()?;
    let plan = CommandTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}


#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_parked_owner_reattach_plans(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    current_body_bounds: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
) -> Vec<CommandTerminalParkedOwnerReattachPlan> {
    let mut plans = command_pane
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .filter_map(|slot_id| {
            command_terminal_parked_owner_reattach_plan_for_slot(
                command_pane,
                parked_runtime_owners,
                current_body_bounds,
                slot_id,
            )
        })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| {
        (
            plan.current_mount_slot_id.group_id.0,
            plan.current_mount_slot_id.session_id.0,
            plan.runtime_session_id.0,
        )
    });
    plans
}


#[cfg(target_os = "macos")]
pub(crate) fn transfer_command_terminal_parked_runtime_owner_reattach(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_mount_slot_bounds: &mut HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    running_config_requests: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    plan: CommandTerminalParkedOwnerReattachPlan,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Command reattach is transactional around exact current command body geometry and empty Running owner maps. A sleeping tab waking to the same group/session slot may receive the parked host/surface owner; mismatches leave normal command mount reconciliation responsible and never create launch payloads, new Ghostty surfaces, logs, persisted runtime ids, or fallback command processes.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Reattach also serves native command-panel owner selection: an inactive or collapsed command tab that becomes the current visible owner may receive its parked host/surface if the same group/session slot and current body bounds match.
    */
    if plan.parked_mount_slot_id != plan.current_mount_slot_id
        || command_terminal_runtime_session_id(plan.current_mount_slot_id)
            != plan.runtime_session_id
        || !command_pane.is_current_terminal_body_mount_slot(plan.current_mount_slot_id)
        || command_pane_group_for_session(command_pane, plan.current_mount_slot_id.session_id)
            != Some(plan.current_mount_slot_id.group_id)
        || command_pane
            .session(plan.current_mount_slot_id.session_id)
            .is_some_and(|session| session.is_sleeping)
        || !running_mount_slot_bounds
            .get(&plan.current_mount_slot_id)
            .is_some_and(|bounds| *bounds == plan.bounds)
        || running_host_native_views.contains_key(&plan.current_mount_slot_id)
        || running_surface_owners.contains_key(&plan.current_mount_slot_id)
        || running_config_requests.contains_key(&plan.current_mount_slot_id)
    {
        return false;
    }
    let Some(parked_owner) = parked_runtime_owners.get(&plan.runtime_session_id) else {
        return false;
    };
    if !parked_owner.can_reattach_with_plan(plan) {
        return false;
    }

    let Some(parked_owner) = parked_runtime_owners.remove(&plan.runtime_session_id) else {
        return false;
    };
    let (host_native_view, surface_owner) = parked_owner.into_running_owners(plan);
    running_mount_slot_bounds.insert(plan.current_mount_slot_id, plan.bounds);
    running_host_native_views.insert(plan.current_mount_slot_id, host_native_view);
    running_surface_owners.insert(plan.current_mount_slot_id, surface_owner);
    true
}


#[cfg(target_os = "macos")]
pub(crate) fn confirmed_agents_terminal_ghostty_surface_close_slots(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> Vec<AgentsTerminalBodyMountSlotId> {
    /*
    CDXC:GPUITerminalGhosttyClose 2026-06-23-04:49:
    Confirmed Ghostty close callbacks are consumed only for exact current Running Agents mount slots. Return slot ids without mutating shell state so the app can route mapped gxserver tabs through sidebar-owned lifecycle while unmapped tabs still close through `WorkspaceModel::close_tab`.

    CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
    Confirmation-needed callbacks are consumed into the Agents pending close-confirm map before this confirmed-close path runs. This path may remove shell state only when the current mounted owner still matches the slot and runtime identity, and it clears only the matching pending entry after the existing close callback is consumed.
    */
    let mut confirmed_slots = Vec::new();
    for slot_id in workspace.rendered_terminal_body_mount_slots() {
        if !workspace.is_current_terminal_body_mount_slot(slot_id)
            || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            continue;
        }
        let confirmed = running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
                    == Some(surface.runtime_session_id())
                && surface.consume_confirmed_close_requested()
        });
        if confirmed {
            confirmed_slots.push(slot_id);
        }
    }
    confirmed_slots
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_confirmed_agents_terminal_ghostty_surface_closes(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    close_confirms: &mut AgentsTerminalCloseConfirmState,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> bool {
    let mut changed = false;
    for slot_id in confirmed_agents_terminal_ghostty_surface_close_slots(
        workspace,
        runtime_sessions,
        running_surface_owners,
    ) {
        if workspace.close_tab(slot_id.pane_id, slot_id.session_id) {
            close_confirms.pending_by_slot.remove(&slot_id);
            changed = true;
        }
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_exited_agents_terminal_ghostty_surfaces(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) -> bool {
    /*
    CDXC:GPUITerminalProcessExit 2026-06-23-05:30:
    Mounted Running Agents process-exit cleanup mirrors native policy by deleting the shell session through `WorkspaceModel::close_tab` without asking Ghostty to close again. Eligibility is limited to exact current Running mount slots with matching process-local runtime ids, and startup maps remain outside this helper so Mounting/Failed startup state cannot be changed by Running process polling.
    */
    let mut changed = false;
    for slot_id in workspace.rendered_terminal_body_mount_slots() {
        if !workspace.is_current_terminal_body_mount_slot(slot_id)
            || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
        {
            continue;
        }

        let process_exited = running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
                    == Some(surface.runtime_session_id())
                && surface.process_exited()
        });
        if process_exited && workspace.close_tab(slot_id.pane_id, slot_id.session_id) {
            changed = true;
        }
    }

    if changed {
        runtime_sessions.reconcile_with_workspace(workspace);
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_confirmed_command_terminal_ghostty_surface_closes(
    command_pane: &mut CommandPaneModel,
    close_confirms: &mut CommandTerminalCloseConfirmState,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) -> bool {
    /*
    CDXC:GPUICommandTerminalGhosttyClose 2026-06-23-05:21:
    Confirmed command Ghostty close callbacks are consumed before command host reconciliation and may remove only the exact current command mount slot through `CommandPaneModel::close_session`. Confirmation-needed callbacks stay in command-only pending runtime state for the normal-layout command close-confirm surface, and this path must not touch Agents runtime maps, startup maps, shell-state JSON, logs, runtime ids, commands, paths, env, output, pids, tty names, or terminal content.

    CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
    Confirmed command closes clear only the matching command pending close-confirm entry after the existing callback path removes shell state. They must not infer confirmation from the pending map or remove command sessions from confirm/cancel actions directly.
    */
    let mut changed = false;
    for slot_id in command_pane.rendered_terminal_body_mount_slots() {
        if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
            continue;
        }
        let confirmed = command_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && surface.runtime_session_id() == command_terminal_runtime_session_id(slot_id)
                && surface.consume_confirmed_close_requested()
        });
        if confirmed && command_pane.close_session(slot_id.group_id, slot_id.session_id) {
            close_confirms.pending_by_slot.remove(&slot_id);
            changed = true;
        }
    }
    changed
}


#[cfg(target_os = "macos")]
pub(crate) fn consume_exited_command_terminal_ghostty_surfaces(
    command_pane: &mut CommandPaneModel,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) -> CommandTerminalProcessExitCleanup {
    /*
    CDXC:GPUICommandTerminalProcessExit 2026-06-23-05:30:
    Mounted command process-exit cleanup is command-pane-only and removes only exact current command body slots through `CommandPaneModel::close_session`. Already-exited surfaces must not receive a Ghostty close request, and this helper cannot touch Agents workspace/runtime/startup maps or command close-confirm callback state.

    CDXC:GPUICommandPane 2026-06-25-11:11:
    Mapped Action sessions that disappear through process-exit cleanup must also finish sidebar button feedback before the command tab is removed. The completion record is derived only from live command ownership plus the matching status-file stamp when available; missing or non-idle status stamps become error feedback so the reused SidebarApp cannot keep an orphaned running state.

    CDXC:GPUICommandTerminalProcessExit 2026-06-26-06:28:
    Native `handleNativeSidebarCommandSessionExit` and `cleanupExitedNativeCommandPaneSession` parity requires removing the exact exited Action command tab, then letting the caller's existing command-model prune paths clear stale Delayed Send and Close After Done runtime intents for that tab. Completion records may carry only command id, run id, completed tab ids, exit code, and sound preference; a matching idle status file supplies the exit code, while missing, working, or mismatched status files report error.
    */
    let mut cleanup = CommandTerminalProcessExitCleanup::default();
    for slot_id in command_pane.rendered_terminal_body_mount_slots() {
        if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
            continue;
        }

        let process_exited = command_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id
                && surface.runtime_session_id() == command_terminal_runtime_session_id(slot_id)
                && surface.process_exited()
        });
        if process_exited {
            let completion = command_pane.take_action_run_completion_for_exited_session(
                slot_id.group_id,
                slot_id.session_id,
            );
            if command_pane.close_session(slot_id.group_id, slot_id.session_id) {
                cleanup.changed = true;
                if let Some(completion) = completion {
                    cleanup.completions.push(completion);
                }
            }
        }
    }
    cleanup
}


pub(crate) fn focused_agents_terminal_surface_mount_slot(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
) -> Option<AgentsTerminalBodyMountSlotId> {
    /*
    CDXC:GPUTerminalGhosttySurfaceFocus 2026-06-22-22:59:
    Real Agents Ghostty surface focus is a runtime decision derived from shell focus only: Agents mode, an AgentsPane focus target, a rendered pane, and that pane's selected Running session. Command pane, Browser/project-editor modes, sleeping or hidden Focus-mode panes, stale panes, and missing sessions must leave every mounted terminal surface unfocused without adding input routing or persisted focus ids.
    */
    if active_mode != TitlebarMode::Agents {
        return None;
    }
    let ShellFocusTarget::AgentsPane(focused_pane_id) = shell_focus else {
        return None;
    };

    agents_workspace
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .find(|slot_id| slot_id.pane_id == focused_pane_id)
}


pub(crate) fn focused_project_editor_companion_terminal_surface_mount_slot(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    selected_session_id: Option<TerminalSessionId>,
) -> Option<ProjectEditorCompanionTerminalBodyMountSlotId> {
    let ShellFocusTarget::ProjectEditorCompanion(mode) = shell_focus else {
        return None;
    };
    if active_mode != mode || !mode.is_project_editor_mode() {
        return None;
    }
    Some(ProjectEditorCompanionTerminalBodyMountSlotId {
        mode,
        session_id: selected_session_id?,
    })
}


pub(crate) fn agents_terminal_surface_focus_states_for_slots(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
    mounted_slot_ids: &[AgentsTerminalBodyMountSlotId],
) -> Vec<(AgentsTerminalBodyMountSlotId, bool)> {
    let focused_slot_id =
        focused_agents_terminal_surface_mount_slot(active_mode, shell_focus, agents_workspace)
            .filter(|slot_id| mounted_slot_ids.contains(slot_id));

    mounted_slot_ids
        .iter()
        .copied()
        .map(|slot_id| (slot_id, Some(slot_id) == focused_slot_id))
        .collect()
}


pub(crate) fn command_terminal_runtime_session_id(
    slot_id: CommandTerminalBodyMountSlotId,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
    Command-pane surfaces need a process-local owner identity for the shared Ghostty owner, but they must not enter the Agents runtime-session registry or any startup map. Derive this transient id only from the command mount slot and keep it scoped to the command surface owner map.
    */
    const COMMAND_RUNTIME_ID_NAMESPACE: u64 = 0xC000_0000_0000_0000;
    AgentsTerminalRuntimeSessionId(
        COMMAND_RUNTIME_ID_NAMESPACE
            | ((slot_id.group_id.0 & 0x7FFF_FFFF) << 32)
            | (slot_id.session_id.0 & 0xFFFF_FFFF),
    )
}


pub(crate) fn command_terminal_runtime_session_id_from_gxserver_key(
    key: &GpuiLocalWorkspaceSessionKey,
) -> AgentsTerminalRuntimeSessionId {
    /*
    CDXC:GPUICommandPaneGxserverAttach 2026-07-04:
    GPUI-engine command terminals use the daemon project/session identity as
    their runtime owner key once gxserver creation succeeds. Keep the value
    process-local and numeric for existing terminal runtime maps, but derive it
    only from the real gxserver ids instead of the command-pane mount slot.
    */
    const COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE: u64 = 0xD000_0000_0000_0000;
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in key
        .project_id
        .bytes()
        .chain([b':'])
        .chain(key.session_id.bytes())
    {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    AgentsTerminalRuntimeSessionId(
        COMMAND_GXSERVER_RUNTIME_ID_NAMESPACE | (hash & 0x0FFF_FFFF_FFFF_FFFF),
    )
}


#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_config_request_with_launch_payload_source(
    slot_id: CommandTerminalBodyMountSlotId,
    request: terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    launch_payload_source: &mut CommandTerminalLaunchPayloadSource,
) -> Result<
    terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
> {
    /*
    CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:59:
    Command Ghostty config requests may attach launch payloads only after the explicit Action/plain-cwd command source validates a payload for the same current body slot and runtime key. If that explicit payload is invalid, the caller must omit or prune the config request instead of falling back to inferred cwd, command, env, initial input, wait policy, status, titles, labels, paths, shell state, logs, terminal content, or delayed-send state.

    CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:47:
    Config preparation consumes explicit command launch payloads exactly once for the current mount slot. A failed conversion still consumes the payload so stale startup data cannot survive into a remount or be replaced by inferred launch data.
    */
    let launch_payload = launch_payload_source.take_payload_for_mount_slot(slot_id)?;
    Ok(match launch_payload {
        Some(launch_payload) => request.with_launch_payload(launch_payload),
        None => request,
    })
}


#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_config_request_with_launch_payload_source(
    slot_id: AgentsTerminalBodyMountSlotId,
    runtime_session_id: AgentsTerminalRuntimeSessionId,
    request: terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    launch_payload_source: &mut AgentsTerminalLaunchPayloadSource,
) -> Result<
    terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    terminal_ghostty_surface::GhosttySurfaceConfigRequestError,
> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
    Local gxserver sidebar attach must not wait behind the Mounting startup card. The first config request for the exact Running Agents mount slot consumes the attach launch payload once; invalid payloads prune that mount request instead of falling back to a blank shell or inferred command.
    */
    let launch_payload =
        launch_payload_source.take_payload_for_mount_slot(runtime_session_id, slot_id)?;
    Ok(match launch_payload {
        Some(launch_payload) => request.with_launch_payload(launch_payload),
        None => request,
    })
}


pub(crate) fn focused_command_terminal_surface_mount_slot(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<CommandTerminalBodyMountSlotId> {
    /*
    CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
    Command Ghostty focus is mirrored only when shell focus is the command pane and the focused command group has a mounted active session. Agents, Browser, project-editor focus, collapsed panes, inactive command tabs, and missing sessions clear command focus without synthetic input routing.
    */
    if shell_focus != ShellFocusTarget::CommandPane {
        return None;
    }
    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    let slot_id = CommandTerminalBodyMountSlotId {
        group_id,
        session_id,
    };
    command_pane
        .is_current_terminal_body_mount_slot(slot_id)
        .then_some(slot_id)
}


/// Press-any-key wake for sleeping Agents bodies mirrors the command-pane
/// rule: only the visible focused pane's selected sleeping tab wakes, and only
/// from plain alphanumeric keys; Focus-mode-hidden panes have no visible
/// placeholder and stay parked.
pub(crate) fn focused_sleeping_agents_placeholder_wake_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &WorkspaceModel,
    keystroke: &Keystroke,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    if active_mode != TitlebarMode::Agents
        || !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke)
    {
        return None;
    }
    let ShellFocusTarget::AgentsPane(pane_id) = shell_focus else {
        return None;
    };
    if !agents_workspace.rendered_leaf_order().contains(&pane_id) {
        return None;
    }
    let leaf = agents_workspace.find_leaf(pane_id)?;
    let session_id = leaf.tab_group.active_tab;
    agents_workspace
        .session(session_id)
        .is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Sleeping
        })
        .then_some((pane_id, session_id))
}


pub(crate) fn focused_sleeping_command_placeholder_wake_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
    keystroke: &Keystroke,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-14:49:
    Keyboard wake is scoped to the command pane's focused active tab and only when that command session is parked sleeping. Non-command focus, running command terminals, collapsed/missing groups, and non-alphanumeric keys must not create terminals, reroute input, or mutate shell state.

    CDXC:GPUICommandSleepingPlaceholder 2026-06-25-19:07:
    Native key wake belongs to the visible AppKit placeholder first responder. A collapsed command strip may remember command focus, but it has no visible placeholder body, so alphanumeric keys must not wake parked command tabs until the panel is expanded.

    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-04:36:
    Keyboard wake resolves through the visible command body owner, so only the exact focused selected sleeping tab can wake. Stale selected ids, missing sessions, inactive siblings, and collapsed panes have no visible placeholder owner and must not wake or create a terminal.
    */
    if shell_focus != ShellFocusTarget::CommandPane
        || !command_pane.is_expanded()
        || !command_pane_sleeping_placeholder_keystroke_requests_wake(keystroke)
    {
        return None;
    }

    let leaf = command_pane.find_leaf(command_pane.focused_group)?;
    let body_owner = command_pane.visible_command_body_owner_for_leaf(leaf)?;
    body_owner
        .is_sleeping
        .then_some((body_owner.group_id, body_owner.session_id))
}


pub(crate) fn focused_command_pane_create_split_hotkey_source(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUIFocusedCommandHotkeys 2026-06-26-06:47:
    Cmd+T/Cmd+D command-placeholder creation is a native responder path only while the Commands panel is visibly expanded. Require expanded command-pane focus plus a live focused source session before allocating command tabs or splits, so stale/collapsed command focus no-ops instead of creating hidden command sessions; clicked command-panel creation keeps its explicit hidden-open route.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some()
        .then_some((group_id, session_id))
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaneFocusedSessionHotkeyAction {
    Rename,
    DelayedSend,
    CloseAfterDone,
    Sleep,
    Wake,
    Close,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiFocusedPaneHotkeyAction {
    CreateSession,
    OpenCommandsPanel,
    OpenBrowserPane,
    SplitRight,
    SplitDown,
    MergeAllTabs,
    RotatePanesClockwise,
    RuntimeNoOp(GpuiFocusedPaneRuntimeAction),
    CommandSession(GpuiCommandPaneFocusedSessionHotkeyAction),
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiFocusedPaneRuntimeAction {
    ForkSession,
    ReloadSession,
    PopOutPane,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiCommandPaletteTabCycleHotkeyAction {
    Previous,
    Next,
}


impl GpuiCommandPaletteTabCycleHotkeyAction {
    pub(crate) fn reverse(self) -> bool {
        matches!(self, Self::Previous)
    }
}


pub(crate) fn gpui_command_palette_tab_cycle_hotkey_action(
    action_id: &str,
) -> Option<GpuiCommandPaletteTabCycleHotkeyAction> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-07:32:
    Shared command-palette tab-cycle rows post `focusPreviousSession` and `focusNextSession` through `runGhostexHotkeyAction`. GPUI maps only those exact ids to the `cycle_focused_tab(reverse)` direction so command, Agents, and Browser focus keep Ctrl-Shift-Tab/Ctrl-Tab parity; numbered session-slot rows are intentionally excluded because SidebarApp owns rendered slot order.
    */
    match action_id {
        "focusPreviousSession" => Some(GpuiCommandPaletteTabCycleHotkeyAction::Previous),
        "focusNextSession" => Some(GpuiCommandPaletteTabCycleHotkeyAction::Next),
        _ => None,
    }
}


pub(crate) fn gpui_command_palette_sidebar_slot_hotkey_action_id(action_id: &str) -> Option<&str> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-23:20:
    Numbered `focusSessionSlot1` through `focusSessionSlot9` rows are rendered-sidebar slot commands, not Rust tab-cycle commands. Delegate only those exact action ids to SidebarApp so its DOM slot ownership resolves focus, while `focusPreviousSession`/`focusNextSession` stay on GPUI tab-cycle routing and jump-to-project ids cannot loop back through native.
    */
    match action_id {
        "focusSessionSlot1" | "focusSessionSlot2" | "focusSessionSlot3" | "focusSessionSlot4"
        | "focusSessionSlot5" | "focusSessionSlot6" | "focusSessionSlot7" | "focusSessionSlot8"
        | "focusSessionSlot9" => Some(action_id),
        _ => None,
    }
}


pub(crate) fn gpui_command_palette_project_slot_hotkey_number(action_id: &str) -> Option<u8> {
    /*
    CDXC:GPUIProjectHotkeys 2026-06-26-23:42:
    Project slot hotkeys are rendered-sidebar project commands. GPUI must delegate `jumpToProject1` through `jumpToProject9` to SidebarApp because Rust does not own the rendered project row order and must avoid the `nativeHotkey` bounce path that SidebarApp forwards back to native.
    */
    match action_id {
        "jumpToProject1" => Some(1),
        "jumpToProject2" => Some(2),
        "jumpToProject3" => Some(3),
        "jumpToProject4" => Some(4),
        "jumpToProject5" => Some(5),
        "jumpToProject6" => Some(6),
        "jumpToProject7" => Some(7),
        "jumpToProject8" => Some(8),
        "jumpToProject9" => Some(9),
        _ => None,
    }
}


pub(crate) fn gpui_command_pane_focused_session_hotkey_action(
    action_id: &str,
) -> Option<GpuiCommandPaneFocusedSessionHotkeyAction> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-15:01:
    GPUI handles command-pane focused Close After Done, Sleep, Wake, and Close action ids from the shared command-palette hotkey bridge. Other hotkey ids must continue to use their existing modal or shell handlers instead of being swallowed by command-pane lifecycle code.

    Delayed Send is a real focused-session action in GPUI. Route it through the
    command-session branch so the command palette and configured hotkey open
    the existing timer modal for the exact focused command terminal.
    */
    match action_id {
        "renameActiveSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Rename),
        "delayedSend" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::DelayedSend),
        "closeAfterDone" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::CloseAfterDone),
        "sleepFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Sleep),
        "wakeFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Wake),
        "closeFocusedSession" => Some(GpuiCommandPaneFocusedSessionHotkeyAction::Close),
        _ => None,
    }
}


pub(crate) fn gpui_focused_pane_hotkey_action(action_id: &str) -> Option<GpuiFocusedPaneHotkeyAction> {
    /*
    CDXC:GPUICommandPalette 2026-06-25-17:32:
    The shared command palette posts focused-pane commands through `runGhostexHotkeyAction`. GPUI must route supported pane actions through the same shell helpers as direct keybindings so command-pane focus can split commands, non-command focus can open Browser, and Agents-only merge can no-op exactly like the native focused-pane dispatcher without fabricating fork/reload/pop-out runtime behavior.

    CDXC:GPUICommandPalette 2026-06-27-05:30:
    Native focused-pane Fork, Reload, and Pop Out actions still enter the focused-pane dispatcher, then command terminals consume them through the command-panel titlebar branch's default no-op because command sessions do not own those runtime semantics. GPUI should consume those ids explicitly as runtime no-ops so they cannot fall through to modal/sidebar fallback routes, while still not inventing fake command clone, reload, or pop-out behavior.

    CDXC:GPUICommandPalette 2026-06-26-06:47:
    `openBrowserPane` remains a recognized focused-pane command-palette id, but command-terminal focus decides at execution time whether it no-ops through the native command-panel titlebar branch instead of creating a Browser tab.

    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    `rotatePanesClockwise` must enter the GPUI focused-pane bridge instead of falling through to modal routing. Command-pane, Browser, and project-editor focus no-op by policy; active Agents-pane focus is the only admitted execution target once the WorkspaceModel pure rotation helper is available.

    CDXC:GPUICommandPalette 2026-06-26-07:15:
    `openCommandsPanel` is the shared command-palette, sidebar-header, and F12 open/focus route for the Commands panel. It never collapses an already-focused visible command pane.

    CDXC:GPUICommandPalette 2026-06-26-07:24:
    `createSession` from the command palette must reuse the focused Cmd+T hotkey helper. That preserves command-pane visible-source gating and Agents-pane placeholder targeting instead of adding a separate fallback that could create a session in the wrong surface.
    */
    match action_id {
        "createSession" => Some(GpuiFocusedPaneHotkeyAction::CreateSession),
        "openCommandsPanel" => Some(GpuiFocusedPaneHotkeyAction::OpenCommandsPanel),
        "openBrowserPane" => Some(GpuiFocusedPaneHotkeyAction::OpenBrowserPane),
        "splitMore" => Some(GpuiFocusedPaneHotkeyAction::SplitRight),
        "splitMoreDown" => Some(GpuiFocusedPaneHotkeyAction::SplitDown),
        "mergeAllTabs" => Some(GpuiFocusedPaneHotkeyAction::MergeAllTabs),
        "rotatePanesClockwise" => Some(GpuiFocusedPaneHotkeyAction::RotatePanesClockwise),
        "forkSession" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::ForkSession,
        )),
        "reloadSession" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::ReloadSession,
        )),
        "popOutPane" => Some(GpuiFocusedPaneHotkeyAction::RuntimeNoOp(
            GpuiFocusedPaneRuntimeAction::PopOutPane,
        )),
        _ => gpui_command_pane_focused_session_hotkey_action(action_id)
            .map(GpuiFocusedPaneHotkeyAction::CommandSession),
    }
}


pub(crate) fn gpui_command_palette_switch_workarea_hotkey_mode(action_id: &str) -> Option<TitlebarMode> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-07:24:
    Shared command-palette workarea rows post ordinary hotkey ids through `runGhostexHotkeyAction`. GPUI must translate only those exact switch ids to titlebar modes, with the shared GitHub row targeting the current Browser workarea field, then execute through the same titlebar availability and focus route as Option+1..5.
    */
    match action_id {
        "switchAgentsView" => Some(TitlebarMode::Agents),
        "switchSourceView" => Some(TitlebarMode::Source),
        "switchGitHubView" => Some(TitlebarMode::Browser),
        "switchKanbanView" => Some(TitlebarMode::Kanban),
        "switchManageView" => Some(TitlebarMode::Manage),
        _ => None,
    }
}


pub(crate) fn gpui_source_workarea_allowed_configured_hotkey_action_id(action_id: &str) -> bool {
    matches!(
        action_id,
        "focusLeft"
            | "focusRight"
            | "navigateHistoryBack"
            | "navigateHistoryForward"
            | "openCommandsPanel"
            | "switchAgentsView"
            | "switchSourceView"
            | "switchGitHubView"
            | "switchKanbanView"
            | "switchManageView"
            | "toggleCompanionPane"
    )
}


pub(crate) fn gpui_command_palette_action_slot_index(action_id: &str) -> Option<usize> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Shared Start Action 1-5 command-palette rows post positional `runActionSlot*` ids. Keep this mapper exact and zero-based so GPUI reuses the configured titlebar Actions list order without parsing or transporting private action payload data.
    */
    match action_id {
        "runActionSlot1" => Some(0),
        "runActionSlot2" => Some(1),
        "runActionSlot3" => Some(2),
        "runActionSlot4" => Some(3),
        "runActionSlot5" => Some(4),
        _ => None,
    }
}


pub(crate) fn gpui_command_palette_adjacent_group_focus_direction(
    action_id: &str,
) -> Option<WorkspaceFocusDirection> {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Shared adjacent-group focus rows post `focusPreviousGroup` and `focusNextGroup`, which are render-order commands rather than spatial arrows. Map only those exact ids to the existing previous/next workspace traversal so GPUI mirrors native focusAdjacentGroup without inventing numbered group slots, project jumps, or runtime fallbacks.
    */
    match action_id {
        "focusPreviousGroup" => Some(WorkspaceFocusDirection::Left),
        "focusNextGroup" => Some(WorkspaceFocusDirection::Right),
        _ => None,
    }
}


pub(crate) fn gpui_command_palette_adjacent_group_focus_source_allowed(shell_focus: ShellFocusTarget) -> bool {
    /*
    CDXC:GPUICommandPalette 2026-06-26-10:04:
    Adjacent-group focus is scoped to the command/Agents render-order model. Browser and project-editor focus do not enter this route, because doing so would turn a shared command-palette row into a cross-workarea project jump.
    */
    matches!(
        shell_focus,
        ShellFocusTarget::AgentsPane(_) | ShellFocusTarget::CommandPane
    )
}


pub(crate) fn gpui_next_sidebar_collapsed_state(collapsed: bool) -> bool {
    !collapsed
}


pub(crate) fn gpui_sidebar_chrome_visible(sidebar_collapsed: bool) -> bool {
    !sidebar_collapsed
}


pub(crate) fn gpui_next_sidebar_side(side: GpuiSidebarSide) -> GpuiSidebarSide {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    GPUI sidebar placement is a two-state shell model that mirrors native `sidebarSide`. Moving the sidebar flips only left/right placement; width and collapsed state remain separate user preferences.
    */
    match side {
        GpuiSidebarSide::Left => GpuiSidebarSide::Right,
        GpuiSidebarSide::Right => GpuiSidebarSide::Left,
    }
}


pub(crate) fn gpui_sidebar_body_chrome_order(
    side: GpuiSidebarSide,
    sidebar_collapsed: bool,
) -> Vec<GpuiSidebarBodyChromePart> {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    The GPUI body row uses normal non-overlapping siblings for sidebar placement parity. Expanded left renders sidebar/divider/workspace, expanded right renders workspace/divider/sidebar, and collapsed mode removes sidebar chrome without mutating the saved width.
    */
    if !gpui_sidebar_chrome_visible(sidebar_collapsed) {
        return vec![GpuiSidebarBodyChromePart::Workspace];
    }
    match side {
        GpuiSidebarSide::Left => vec![
            GpuiSidebarBodyChromePart::Sidebar,
            GpuiSidebarBodyChromePart::Divider,
            GpuiSidebarBodyChromePart::Workspace,
        ],
        GpuiSidebarSide::Right => vec![
            GpuiSidebarBodyChromePart::Workspace,
            GpuiSidebarBodyChromePart::Divider,
            GpuiSidebarBodyChromePart::Sidebar,
        ],
    }
}


pub(crate) fn gpui_sidebar_resize_delta(side: GpuiSidebarSide, current_x: f32, start_x: f32) -> f32 {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    Right-side sidebar resizing reverses the horizontal delta because the visible divider sits on the workspace edge. Dragging that divider left grows the sidebar, matching native AppKit layout math.
    */
    match side {
        GpuiSidebarSide::Left => current_x - start_x,
        GpuiSidebarSide::Right => start_x - current_x,
    }
}


pub(crate) fn gpui_sidebar_divider_x_bounds(
    side: GpuiSidebarSide,
    window_width: f32,
    sidebar_width: f32,
) -> (f32, f32) {
    match side {
        GpuiSidebarSide::Left => (sidebar_width, sidebar_width + SIDEBAR_DIVIDER_WIDTH),
        GpuiSidebarSide::Right => {
            let start = window_width - sidebar_width - SIDEBAR_DIVIDER_WIDTH;
            (start, start + SIDEBAR_DIVIDER_WIDTH)
        }
    }
}


pub(crate) fn gpui_focused_pane_open_browser_hotkey_should_open(shell_focus: ShellFocusTarget) -> bool {
    /*
    CDXC:GPUICommandPalette 2026-06-26-06:47:
    Native `runFocusedPaneHotkeyAction("openBrowserPane")` dispatches through `handleNativeTerminalTitleBarAction`; a live command terminal hits the command-panel branch and default-returns. GPUI should preserve that no-op for CommandPane focus while keeping Browser creation for Agents, Browser, and project-editor focus.
    */
    !matches!(shell_focus, ShellFocusTarget::CommandPane)
}


pub(crate) fn gpui_focused_pane_rotate_agents_hotkey_target(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
) -> Option<WorkspacePaneId> {
    /*
    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    Native focused-pane rotation runs only for the active workspace pane group. GPUI must preserve command-terminal default-return behavior and keep Browser/project-editor focus inert, so only `active_mode == Agents` plus `ShellFocusTarget::AgentsPane(_)` may reach the future workspace rotation mutation.
    */
    match shell_focus {
        ShellFocusTarget::AgentsPane(pane_id) if active_mode == TitlebarMode::Agents => {
            Some(pane_id)
        }
        ShellFocusTarget::CommandPane
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_)
        | ShellFocusTarget::AgentsPane(_) => None,
    }
}


pub(crate) fn apply_rotate_agents_panes_hotkey_model(
    active_mode: TitlebarMode,
    shell_focus: ShellFocusTarget,
    agents_workspace: &mut WorkspaceModel,
) -> Option<WorkspacePaneId> {
    /*
    CDXC:GPUIFocusedPaneRotation 2026-06-26-06:56:
    The focused-pane rotate hotkey is a thin app-route over the pure Agents workspace rotation model. Keep command/Browser/project-editor focus as no-ops before mutating the workspace, and return the post-rotation focused pane so the app shell can restore focus and clear runtime-only drag/metrics state.
    */
    let _pane_id = gpui_focused_pane_rotate_agents_hotkey_target(active_mode, shell_focus)?;
    if !agents_workspace.rotate_panes_clockwise() {
        return None;
    }
    Some(agents_workspace.focused_pane)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FocusedCommandPaneCloseDecision {
    CloseCommandTab {
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    },
    InterceptNoOp,
    FallThroughToActiveMode,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusedSurfaceCloseDecision {
    CloseCommandTab {
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    },
    InterceptNoOp,
    CloseProjectEditorCompanionSession(TitlebarMode),
    CloseAgentsActiveTab,
    CloseBrowserActiveTab,
    NoOp,
}


pub(crate) fn focused_command_pane_close_decision(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> FocusedCommandPaneCloseDecision {
    /*
    CDXC:GPUICommandClose 2026-06-26-05:33:
    Cmd-W should follow native command-panel responder ownership. Expanded live command focus closes awake command tabs, expanded live sleeping placeholders consume the shortcut without closing, and stale/collapsed command focus falls through to the active workspace or Browser close path instead of swallowing Cmd-W.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    }

    let Some((group_id, session_id)) = command_pane.focused_group_active_session_id() else {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    };
    let Some(session) = command_pane.session(session_id) else {
        return FocusedCommandPaneCloseDecision::FallThroughToActiveMode;
    };
    if session.is_sleeping {
        FocusedCommandPaneCloseDecision::InterceptNoOp
    } else {
        FocusedCommandPaneCloseDecision::CloseCommandTab {
            group_id,
            session_id,
        }
    }
}


pub(crate) fn focused_surface_close_decision(
    shell_focus: ShellFocusTarget,
    active_mode: TitlebarMode,
    command_pane: &CommandPaneModel,
) -> FocusedSurfaceCloseDecision {
    /*
    CDXC:GPUIFocusedClose 2026-06-27-02:58:
    Cmd-W routing mirrors `native/sidebar/native-hotkey-source.test.ts`: expanded live command focus wins first, sleeping command placeholders consume the shortcut, and focused Source/Browser/Kanban/Automate/Manage main project-editor surfaces never inherit Browser tab or workspace-tab close behavior. Browser tabs remain closeable through BrowserSurface, exact BrowserPane focus, or stale/collapsed command fallthrough into Browser's active-surface policy.

    CDXC:GPUIFocusedClose 2026-07-29-04:29:
    A focused companion is a session surface, so Cmd-W closes its exact focused terminal through the existing workspace lifecycle owner. Companion collapse remains exclusive to its visible titlebar control and must not substitute for session close.
    */
    match focused_command_pane_close_decision(shell_focus, command_pane) {
        FocusedCommandPaneCloseDecision::CloseCommandTab {
            group_id,
            session_id,
        } => {
            return FocusedSurfaceCloseDecision::CloseCommandTab {
                group_id,
                session_id,
            };
        }
        FocusedCommandPaneCloseDecision::InterceptNoOp => {
            return FocusedSurfaceCloseDecision::InterceptNoOp;
        }
        FocusedCommandPaneCloseDecision::FallThroughToActiveMode => {}
    }

    match shell_focus {
        ShellFocusTarget::ProjectEditorCompanion(mode)
            if active_mode == mode && mode.is_project_editor_mode() =>
        {
            FocusedSurfaceCloseDecision::CloseProjectEditorCompanionSession(mode)
        }
        ShellFocusTarget::ProjectEditorSurface(mode) if mode.is_project_editor_mode() => {
            FocusedSurfaceCloseDecision::NoOp
        }
        ShellFocusTarget::BrowserSurface | ShellFocusTarget::BrowserPane(_)
            if active_mode == TitlebarMode::Browser =>
        {
            FocusedSurfaceCloseDecision::CloseBrowserActiveTab
        }
        ShellFocusTarget::AgentsPane(_) if active_mode == TitlebarMode::Agents => {
            FocusedSurfaceCloseDecision::CloseAgentsActiveTab
        }
        ShellFocusTarget::CommandPane => match active_mode {
            TitlebarMode::Agents => FocusedSurfaceCloseDecision::CloseAgentsActiveTab,
            TitlebarMode::Browser => FocusedSurfaceCloseDecision::CloseBrowserActiveTab,
            TitlebarMode::Source
            | TitlebarMode::Kanban
            | TitlebarMode::Automate
            | TitlebarMode::Manage => FocusedSurfaceCloseDecision::NoOp,
        },
        ShellFocusTarget::AgentsPane(_)
        | ShellFocusTarget::BrowserSurface
        | ShellFocusTarget::BrowserPane(_)
        | ShellFocusTarget::ProjectEditorSurface(_)
        | ShellFocusTarget::ProjectEditorCompanion(_) => FocusedSurfaceCloseDecision::NoOp,
    }
}


pub(crate) fn focused_command_pane_close_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-15:05:
    Close Focused Session from the shared command-palette bridge should close the command terminal only when the command pane owns shell focus. Non-command focus remains out of this command-pane parity path so GPUI does not widen command-pane work into unrelated surface close behavior.

    CDXC:GPUICommandClose 2026-06-25-17:37:
    Focused command close requires an expanded command pane with an active command tab, matching native live first-responder routing. Collapsed command strips can show tabs but do not own terminal typing focus and must not close from focused-session commands.

    CDXC:GPUICommandClose 2026-06-25-18:24:
    Native Cmd-W over the Commands panel requires `commandPanelFocusedResponderSessionId`, which accepts active command terminals rather than sleeping placeholder-only command tabs. Keep focused command close out of sleeping command tabs; tab close buttons, middle-click, and context-menu close scopes still own explicit sleeping-tab close.
    */
    if let FocusedCommandPaneCloseDecision::CloseCommandTab {
        group_id,
        session_id,
    } = focused_command_pane_close_decision(shell_focus, command_pane)
    {
        Some((group_id, session_id))
    } else {
        None
    }
}


pub(crate) fn focused_command_pane_sleep_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-14:56:
    Sleep Focused Session should target the active command terminal only when the command pane owns shell focus and is visibly expanded, matching native focused-session routing from AppKit first responder state. Collapsed strips, non-command focus, missing sessions, and already sleeping command tabs must no-op instead of mutating stale command state.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some_and(|session| !session.is_sleeping)
        .then_some((group_id, session_id))
}


pub(crate) fn focused_command_pane_rename_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-16:33:
    Rename Active Session is a focused-session action in native command panes. In GPUI, route it only when the expanded command pane owns shell focus, then open the shared Rename Session modal for the active command tab without deriving titles from command text, paths, output, or persisted shell JSON.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }
    command_pane.focused_group_active_session_id()
}


pub(crate) fn focused_command_pane_wake_target(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandFocusedSessionActions 2026-06-25-15:01:
    Wake Focused Session is the inverse focused command-terminal lifecycle action. Resolve only the expanded command pane's active sleeping tab while it owns shell focus, matching native command-palette focused-session routing without waking non-command focus, running command tabs, or collapsed command strips.
    */
    if shell_focus != ShellFocusTarget::CommandPane || !command_pane.is_expanded() {
        return None;
    }

    let (group_id, session_id) = command_pane.focused_group_active_session_id()?;
    command_pane
        .session(session_id)
        .is_some_and(|session| session.is_sleeping)
        .then_some((group_id, session_id))
}


pub(crate) fn command_terminal_surface_focus_states_for_slots(
    shell_focus: ShellFocusTarget,
    command_pane: &CommandPaneModel,
    mounted_slot_ids: &[CommandTerminalBodyMountSlotId],
) -> Vec<(CommandTerminalBodyMountSlotId, bool)> {
    let focused_slot_id = focused_command_terminal_surface_mount_slot(shell_focus, command_pane)
        .filter(|slot_id| mounted_slot_ids.contains(slot_id));

    mounted_slot_ids
        .iter()
        .copied()
        .map(|slot_id| (slot_id, Some(slot_id) == focused_slot_id))
        .collect()
}


pub(crate) fn terminal_close_confirm_surface_signature(
    family: TerminalCloseConfirmSurfaceFamily,
) -> TerminalCloseConfirmSurfaceSignature {
    /*
    CDXC:GPUITerminalCloseConfirm 2026-06-23-20:04:
    Close-confirm UI copy must stay generic while the action is enabled by the real GhosttyKit `needs_confirm_quit` ABI contract. It may identify only the safe family scope, Keep Open cancel action, and generic close action; it must not display session names, terminal titles, command text, paths, URLs, stdout/stderr, terminal content, runtime ids, tokens, raw callback payloads, or fallback close behavior.
    */
    let message = match family {
        TerminalCloseConfirmSurfaceFamily::Agents => {
            "An Agents terminal is asking for confirmation before closing."
        }
        TerminalCloseConfirmSurfaceFamily::Command => {
            "A command terminal is asking for confirmation before closing."
        }
    };

    TerminalCloseConfirmSurfaceSignature {
        title: "Terminal close requested",
        message,
        keep_open_label: "Keep Open",
        confirm_action_label: "Close Terminal",
    }
}


#[cfg(target_os = "macos")]
pub(crate) fn pending_agents_terminal_close_confirm_for_slot(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<PendingAgentsTerminalCloseConfirm> {
    if !workspace.is_current_terminal_body_mount_slot(slot_id)
        || !workspace.can_close_tab(slot_id.pane_id, slot_id.session_id)
    {
        return None;
    }

    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let surface = running_surface_owners.get(&slot_id)?;
    (surface.mount_slot_id() == slot_id
        && surface.runtime_session_id() == runtime_session_id
        && surface.needs_confirm_quit())
    .then_some(PendingAgentsTerminalCloseConfirm {
        slot_id,
        runtime_session_id,
    })
}


#[cfg(target_os = "macos")]
pub(crate) fn pending_command_terminal_close_confirm_for_slot(
    command_pane: &CommandPaneModel,
    command_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    slot_id: CommandTerminalBodyMountSlotId,
) -> Option<PendingCommandTerminalCloseConfirm> {
    if !command_pane.is_current_terminal_body_mount_slot(slot_id) {
        return None;
    }

    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    let surface = command_surface_owners.get(&slot_id)?;
    (surface.mount_slot_id() == slot_id
        && surface.runtime_session_id() == runtime_session_id
        && surface.needs_confirm_quit())
    .then_some(PendingCommandTerminalCloseConfirm {
        slot_id,
        runtime_session_id,
    })
}


pub(crate) fn terminal_session_title_for_id(_id: TerminalSessionId) -> String {
    "Terminal Session".to_string()
}


pub(crate) fn command_session_title_for_id(_id: CommandSessionId) -> String {
    /*
    CDXC:GPUICommandPane 2026-06-25-11:56:
    Restored GPUI command placeholders must use the same generic `Command Terminal` fallback as newly created macOS command-pane terminals. Do not derive fallback titles from command ids or persist visible/private command titles, because shell-state JSON is layout metadata rather than command-content storage.
    */
    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string()
}


pub(crate) fn gpui_command_pane_sidebar_indicator_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()
        && normalized.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !normalized.chars().any(char::is_control))
    .then_some(normalized)
}


pub(crate) fn sanitize_browser_tab_url_for_state(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(BROWSER_ADDRESS_ONLY_CEF_URL) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let content_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let without_query = &rest[..content_end];
    let authority_end = without_query.find('/').unwrap_or(without_query.len());
    let authority = &without_query[..authority_end];
    let path = &without_query[authority_end..];
    let authority = authority.rsplit('@').next().unwrap_or(authority).trim();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}{path}"))
}


pub(crate) fn browser_placeholder_safe_origin_url(sanitized_url: &str) -> Option<String> {
    let (scheme, rest) = sanitized_url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }

    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..authority_end]
        .rsplit('@')
        .next()
        .unwrap_or("")
        .trim();
    if authority.is_empty() {
        return None;
    }

    Some(format!("{scheme}://{authority}"))
}


pub(crate) fn project_editor_companion_width_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.10, 0.85)
}


pub(crate) fn project_editor_companion_width_ratio_for_span(ratio: f32, content_span: f32) -> f32 {
    let content_span = content_span.max(1.0);
    let companion_min_ratio = (PROJECT_EDITOR_COMPANION_MIN_WIDTH / content_span).clamp(0.10, 0.85);
    let editor_max_ratio = ((content_span - WORKSPACE_MIN_WIDTH) / content_span).clamp(0.10, 0.85);
    let ratio = project_editor_companion_width_ratio(ratio);

    if companion_min_ratio <= editor_max_ratio {
        ratio.clamp(companion_min_ratio, editor_max_ratio)
    } else {
        companion_min_ratio
    }
}


pub(crate) fn json_string_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    object.get(key)?.as_str()
}


pub(crate) fn json_bool_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<bool> {
    object.get(key)?.as_bool()
}


pub(crate) fn json_array_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a Vec<serde_json::Value>> {
    object.get(key)?.as_array()
}


pub(crate) fn json_u64_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(json_u64_value)
}


pub(crate) fn json_u64_value(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(text) => text.parse::<u64>().ok(),
        _ => None,
    }
}


pub(crate) fn json_f32_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f32> {
    object.get(key).and_then(json_value_to_f32)
}

