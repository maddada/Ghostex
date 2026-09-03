// C1 wave-3 extraction: the GpuiShellLayoutState struct and impl moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

pub(crate) struct GpuiShellLayoutState {
    pub(crate) active_mode: TitlebarMode,
    pub(crate) shell_focus: ShellFocusTarget,
    pub(crate) previous_non_command_focus: Option<ShellFocusTarget>,
    pub(crate) pet_overlay_activities_visible: bool,
    pub(crate) agents_workspace: WorkspaceModel,
    pub(crate) agents_workspace_project_id: Option<String>,
    pub(crate) parked_agents_workspaces_by_project: HashMap<String, serde_json::Value>,
    pub(crate) local_workspace_session_mappings:
        HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    pub(crate) remote_attach_sessions: HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    pub(crate) agents_chat_mode_sessions: HashSet<TerminalSessionId>,
    pub(crate) command_pane: CommandPaneModel,
    pub(crate) command_pane_project_id: Option<String>,
    pub(crate) parked_command_panes_by_project: HashMap<String, serde_json::Value>,
    pub(crate) command_startup_activity_restore_intents:
        Vec<GpuiCommandStartupActivityRestoreIntent>,
    pub(crate) command_delayed_send_restore_timers: Vec<GpuiCommandDelayedSendRestoreTimer>,
    pub(crate) agents_delayed_send_restore_intents: Vec<GpuiAgentsDelayedSendRestoreIntent>,
    pub(crate) pending_command_gxserver_cleanup: HashSet<GpuiLocalWorkspaceSessionKey>,
    pub(crate) project_editor_shell: ProjectEditorShellModel,
    pub(crate) project_view_states_by_project: HashMap<String, GpuiProjectViewState>,
    pub(crate) browser_profiles: BrowserProfileModel,
    pub(crate) browser_tabs: BrowserTabModel,
    pub(crate) browser_tabs_project_id: Option<String>,
    pub(crate) parked_browser_tabs_by_project: HashMap<String, BrowserTabModel>,
}

impl GpuiShellLayoutState {
    pub(crate) fn shell_default_from_shared_settings(
        content_height: f32,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> Self {
        Self::shell_default_with_command_default_height_px(
            content_height,
            command_pane_default_height_px_from_shared_settings(settings),
        )
    }

    pub(crate) fn shell_default_with_command_default_height_px(
        content_height: f32,
        command_default_height_px: f32,
    ) -> Self {
        /*
        CDXC:CommandPane 2026-07-05:
        Production GPUI Agents tabs are reconciled from the sidebar's live
        active-project session group. Start empty so gxserver-connected runs
        never expose the old demo "first slice" sessions before the sidebar
        bridge publishes the real tab list.
        */
        let agents_workspace = WorkspaceModel::empty_default();
        let shell_focus = ShellFocusTarget::AgentsPane(agents_workspace.focused_pane);
        let browser_profiles = BrowserProfileModel::shell_default();
        let browser_tabs =
            BrowserTabModel::shell_address_only_with_profile(browser_profiles.active_profile_id());
        Self {
            active_mode: TitlebarMode::Agents,
            shell_focus,
            previous_non_command_focus: Some(shell_focus),
            pet_overlay_activities_visible: true,
            agents_workspace,
            agents_workspace_project_id: None,
            parked_agents_workspaces_by_project: HashMap::new(),
            local_workspace_session_mappings: HashMap::new(),
            remote_attach_sessions: HashMap::new(),
            agents_chat_mode_sessions: HashSet::new(),
            command_pane: CommandPaneModel::shell_default_with_default_height_px(
                content_height,
                command_default_height_px,
            ),
            command_pane_project_id: None,
            parked_command_panes_by_project: HashMap::new(),
            command_startup_activity_restore_intents: Vec::new(),
            command_delayed_send_restore_timers: Vec::new(),
            agents_delayed_send_restore_intents: Vec::new(),
            pending_command_gxserver_cleanup: HashSet::new(),
            project_editor_shell: ProjectEditorShellModel::shell_default(),
            project_view_states_by_project: HashMap::new(),
            browser_profiles,
            browser_tabs,
            browser_tabs_project_id: None,
            parked_browser_tabs_by_project: HashMap::new(),
        }
    }

    pub(crate) fn load_or_default(
        content_height: f32,
        fallback_availability: ProjectScopedWorkareaAvailability,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> Self {
        read_json_object(&gpui_workspace_shell_state_path())
            .and_then(|object| {
                Self::from_json_object_with_shared_settings(
                    &object,
                    content_height,
                    fallback_availability,
                    settings,
                )
            })
            .unwrap_or_else(|| Self::shell_default_from_shared_settings(content_height, settings))
    }

    pub(crate) fn from_json_object_with_shared_settings(
        object: &serde_json::Map<String, serde_json::Value>,
        content_height: f32,
        fallback_availability: ProjectScopedWorkareaAvailability,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) -> Option<Self> {
        Self::from_json_object_with_command_default_height_px(
            object,
            content_height,
            fallback_availability,
            command_pane_default_height_px_from_shared_settings(settings),
        )
    }

    pub(crate) fn from_json_object_with_command_default_height_px(
        object: &serde_json::Map<String, serde_json::Value>,
        content_height: f32,
        fallback_availability: ProjectScopedWorkareaAvailability,
        command_default_height_px: f32,
    ) -> Option<Self> {
        /*
        CDXC:Workarea 2026-06-22-06:29:
        GPUI layout persistence is scoped to placeholder shell state only: titlebar mode, tab/split ids, active selections, focus/Focus mode, bounded canonical gxserver P/G identities, the validated bounded command Action selector used for restart reuse, safe Agents Delayed Send trigger/remaining-time checkpoints, command pane mode/height/tree, Browser tab shell ids with sanitized URLs, project-editor companion sizing, project-editor awake/sleeping recency state, and the single `petOverlayActivitiesVisible` boolean. Do not persist pet activity payloads, titles, paths, raw settings JSON, terminal content, command text, stdout/stderr, user paths, project paths, tokens, cookies, secrets, raw page titles, favicon URLs, raw browser query strings, or private user content.

        CDXC:Workarea 2026-06-22-06:29:
        Restoring corrupted or absent GPUI shell state should use the current placeholder defaults because the persisted file is optional app state. This fallback is limited to invalid state-file input and should not mask runtime errors in live layout mutation code.

        CDXC:Workarea 2026-06-22-07:54:
        Command-pane focus restoration persists only the previous non-command focus enum and stable pane/mode id, sharing the same validation path as shellFocus so stale panes, hidden companions, and inactive project-editor modes fall back to the current mode's default surface.

        CDXC:CefRuntime 2026-07-04-01:00:
        Restored active mode coercion uses caller-supplied project-context availability, preferring the live sidebar project snapshot when one has been accepted. Docs/Manage titlebar selection is not hidden behind the debuggingMode/showBetaFeatures gate; projectless contexts still coerce project-scoped modes back to Agents.

        CDXC:Browser 2026-06-23-11:14:
        Restored GPUI Browser profile state is limited to generated numeric profile ids plus the active id. Browser tabs restore their validated numeric profile id independently, falling back to the active generated profile only for older shell state, without persisting profile names, local profile directories, cookies, credentials, history, page titles, query strings, fragments, command text, or user browser data.

        CDXC:Workarea 2026-06-23-13:04:
        Phase 10 cleanup must migrate the original unversioned GPUI shell-state shape instead of dropping safe user layout state. Accept only that known layout object shape, keep Browser profiles defaulted when the old file has no profile block, and continue rejecting unknown future versions so migration cannot become fallback parsing for arbitrary or private data.

        CDXC:Workarea 2026-06-23-13:11:
        Current-version shell state must match the current writer-owned schema instead of partially defaulting missing or malformed sections. The optional state-file fallback still happens at `load_or_default`, but v1 restore itself now fails closed unless the explicit legacy migration path applies.
        */
        let restore_version = gpui_workspace_shell_state_restore_version(object)?;
        let is_legacy_unversioned =
            restore_version == GpuiWorkspaceShellStateRestoreVersion::LegacyUnversioned;
        if restore_version == GpuiWorkspaceShellStateRestoreVersion::Current
            && !gpui_workspace_shell_state_has_current_required_fields(object)
        {
            return None;
        }
        if is_legacy_unversioned && !gpui_workspace_shell_state_is_legacy_unversioned_object(object)
        {
            return None;
        }

        let persisted_active_mode =
            json_string_field(object, "activeMode").and_then(TitlebarMode::from_slug)?;
        let active_mode =
            fallback_availability.available_titlebar_mode_or_agents(persisted_active_mode);
        let agents_workspace = object
            .get("agentsWorkspace")
            .and_then(workspace_model_from_shell_state)?;
        /*
        CDXC:Workarea 2026-07-10:
        The macOS workspace persists each canonical gxserver session identity
        with its local pane-layout record. GPUI must restore the equivalent
        bounded P/G-to-shell mapping before the first sidebar hydrate; otherwise
        reconciliation allocates replacement local terminal records and a
        post-relaunch click takes a new-session materialization path instead of
        reusing the saved tab and attaching its existing zmx provider.

        This optional field migrates files written before identity persistence.
        When present it is validated as writer-owned state against the restored
        Agents model, so stale or malformed mappings cannot retarget a tab.
        */
        let local_workspace_session_mappings = match object.get("agentsWorkspaceSessionMappings") {
            Some(value) => {
                local_workspace_session_mappings_from_shell_state(value, &agents_workspace)?
            }
            None => HashMap::new(),
        };
        let agents_workspace_project_id = json_string_field(object, "agentsWorkspaceProjectId")
            .map(str::trim)
            .filter(|project_id| gpui_workspace_project_key_allowed(project_id))
            .map(str::to_string)
            .or_else(|| sole_local_workspace_mapping_project_id(&local_workspace_session_mappings));
        /*
        CDXC:RemoteMachines 2026-08-06:
        Remote Agents tabs persist the same canonical tab-to-shell identity as
        local tabs, additionally bounded by the saved-machine id. This contains
        no SSH target, path, title, command, token, or terminal content. Files
        written before remote identity persistence migrate with an empty map;
        the first authoritative remote sidebar projection then removes the old
        unidentifiable placeholders and allocates one tab per live session.
        */
        let remote_attach_sessions = match object.get("agentsWorkspaceRemoteSessionMappings") {
            Some(value) => remote_workspace_session_mappings_from_shell_state(
                value,
                &agents_workspace,
                agents_workspace_project_id.as_deref(),
            )?,
            None => HashMap::new(),
        };
        /*
        CDXC:SessionChat 2026-07-31:
        The last-used surface (terminal vs chat) per Agents session survives
        app restarts. Optional writer-owned schema growth (files written before
        this slice still restore); ids are validated against the restored
        workspace so stale entries drop instead of failing the whole restore.
        */
        let agents_chat_mode_sessions = object
            .get("agentsChatModeSessions")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_u64)
                    .map(TerminalSessionId)
                    .filter(|session_id| agents_workspace.has_session(*session_id))
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut parked_agents_workspaces_by_project = object
            .get("agentsWorkspacesByProject")
            .and_then(serde_json::Value::as_object)
            .map(|parked| {
                parked
                    .iter()
                    .filter(|(project_id, workspace_value)| {
                        gpui_workspace_project_key_allowed(project_id.trim())
                            && workspace_value.is_object()
                    })
                    .map(|(project_id, workspace_value)| {
                        (project_id.trim().to_string(), workspace_value.clone())
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        if let Some(project_id) = agents_workspace_project_id.as_ref() {
            parked_agents_workspaces_by_project.remove(project_id);
        }
        let agents_delayed_send_restore_intents = match object.get("agentsDelayedSends") {
            Some(value) => agents_delayed_send_restore_intents_from_shell_state(
                value,
                &local_workspace_session_mappings,
            )?,
            None => Vec::new(),
        };
        let command_pane_value = object.get("commandPane")?;
        let command_pane = command_pane_model_from_shell_state_with_default_height_px(
            command_pane_value,
            content_height,
            command_default_height_px,
        )?;
        /*
        CDXC:CommandPane 2026-07-10:
        Per-project command-pane fields are optional writer-owned schema growth
        so files written before this slice still restore. `commandPane` stays
        the live panel for `commandPaneProjectId`; `commandPanesByProject`
        carries the parked inactive-project panels as the same shell-state
        shape. Invalid ids or non-object entries are dropped as absent instead
        of failing the whole restore.
        */
        let command_pane_project_id = json_string_field(object, "commandPaneProjectId")
            .map(str::trim)
            .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
            .map(str::to_string);
        let parked_command_panes_by_project = object
            .get("commandPanesByProject")
            .and_then(serde_json::Value::as_object)
            .map(|parked| {
                parked
                    .iter()
                    .filter(|(project_id, pane_value)| {
                        gpui_remote_sidebar_project_id_allowed(project_id.trim())
                            && pane_value.is_object()
                    })
                    .map(|(project_id, pane_value)| {
                        (project_id.trim().to_string(), pane_value.clone())
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let command_startup_activity_restore_intents =
            command_startup_activity_restore_intents_from_shell_state(
                command_pane_value,
                &command_pane,
            );
        let command_delayed_send_restore_timers =
            command_delayed_send_restore_timers_from_shell_state(command_pane_value, &command_pane);
        let pending_command_gxserver_cleanup = pending_command_gxserver_cleanup_from_shell_state(
            object.get("pendingCommandSessionCleanup"),
        );
        let project_editor_shell = object
            .get("projectEditorShell")
            .and_then(|value| project_editor_shell_from_shell_state(value, active_mode))?;
        let browser_profiles = match object.get("browserProfiles") {
            Some(value) => browser_profile_model_from_shell_state(value)?,
            None if is_legacy_unversioned => BrowserProfileModel::shell_default(),
            None => return None,
        };
        let mut browser_tabs = object
            .get("browserTabs")
            .and_then(|value| browser_tab_model_from_shell_state(value, &browser_profiles))?;
        let browser_tabs_project_id = json_string_field(object, "browserTabsProjectId")
            .map(str::trim)
            .filter(|project_id| gpui_browser_tabs_project_key_allowed(project_id))
            .map(str::to_string);
        let mut parked_browser_tabs_by_project = object
            .get("browserTabsByProject")
            .and_then(serde_json::Value::as_object)
            .map(|parked| {
                parked
                    .iter()
                    .filter_map(|(project_id, tabs_value)| {
                        let project_id = project_id.trim();
                        gpui_browser_tabs_project_key_allowed(project_id)
                            .then(|| {
                                browser_tab_model_from_shell_state(tabs_value, &browser_profiles)
                                    .map(|tabs| (project_id.to_string(), tabs))
                            })
                            .flatten()
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        if let Some(project_id) = browser_tabs_project_id.as_ref() {
            if let Some(parked_tabs) = parked_browser_tabs_by_project.remove(project_id) {
                // Repair state written by the first project-scoping pass: a
                // temporary projectless tab was incorrectly claimed as live
                // while the real project tabs remained parked under the same
                // owner id. The parked owner model is the project model that
                // existed before the projectless transition, so restore it.
                browser_tabs = parked_tabs;
            }
        }
        let shell_focus = valid_shell_focus_or_default_with_browser_tabs(
            shell_focus_from_shell_state(object.get("shellFocus")?)?,
            active_mode,
            &agents_workspace,
            &command_pane,
            &project_editor_shell,
            &browser_tabs,
        );
        let default_non_command_focus =
            default_shell_focus_for_mode(active_mode, &agents_workspace, &project_editor_shell);
        let restored_previous_non_command_focus = match object.get("previousNonCommandFocus") {
            Some(value) if value.is_null() => None,
            Some(value) => Some(shell_focus_from_shell_state(value)?),
            None => None,
        };
        let previous_non_command_focus = restored_previous_non_command_focus
            .and_then(|focus| {
                valid_non_command_shell_focus_with_browser_tabs(
                    focus,
                    active_mode,
                    &agents_workspace,
                    &project_editor_shell,
                    &browser_tabs,
                )
            })
            .or_else(|| {
                valid_non_command_shell_focus_with_browser_tabs(
                    shell_focus,
                    active_mode,
                    &agents_workspace,
                    &project_editor_shell,
                    &browser_tabs,
                )
            })
            .or(Some(default_non_command_focus));
        let pet_overlay_activities_visible = object
            .get("petOverlayActivitiesVisible")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        /*
        CDXC:Navigation 2026-08-07:
        Same key gate as the parked workspaces: accept a plain local project id
        or a machine-scoped remote key, and drop anything else so a malformed
        entry cannot resurrect a view for a project this app cannot address.
        */
        let project_view_states_by_project = object
            .get("projectViewStates")
            .and_then(serde_json::Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(project_id, _)| gpui_workspace_project_key_allowed(project_id))
                    .filter_map(|(project_id, value)| {
                        project_view_state_from_shell_state(value)
                            .map(|state| (project_id.clone(), state))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        Some(Self {
            active_mode,
            shell_focus,
            previous_non_command_focus,
            pet_overlay_activities_visible,
            agents_workspace,
            agents_workspace_project_id,
            parked_agents_workspaces_by_project,
            local_workspace_session_mappings,
            remote_attach_sessions,
            agents_chat_mode_sessions,
            command_pane,
            command_pane_project_id,
            parked_command_panes_by_project,
            command_startup_activity_restore_intents,
            command_delayed_send_restore_timers,
            agents_delayed_send_restore_intents,
            pending_command_gxserver_cleanup,
            project_editor_shell,
            project_view_states_by_project,
            browser_profiles,
            browser_tabs,
            browser_tabs_project_id,
            parked_browser_tabs_by_project,
        })
    }
}
