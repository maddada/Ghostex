// C1 wave-3 re-cluster: workspace shell-state persistence: restore-version checks, workspace/session-mapping/chat-mode JSON round trips, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWorkspaceShellStateRestoreVersion {
    Current,
    LegacyUnversioned,
}

pub(crate) fn gpui_workspace_shell_state_restore_version(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<GpuiWorkspaceShellStateRestoreVersion> {
    if let Some(version) = object.get("version") {
        return matches!(
            version,
            serde_json::Value::Number(number)
                if number.as_u64() == Some(GPUI_WORKSPACE_SHELL_STATE_VERSION)
        )
        .then_some(GpuiWorkspaceShellStateRestoreVersion::Current);
    }

    gpui_workspace_shell_state_is_legacy_unversioned_object(object)
        .then_some(GpuiWorkspaceShellStateRestoreVersion::LegacyUnversioned)
}

pub(crate) fn gpui_workspace_shell_state_has_current_required_fields(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_string_field(object, "activeMode")
        .and_then(TitlebarMode::from_slug)
        .is_some()
        && object
            .get("shellFocus")
            .is_some_and(|value| value.is_object())
        && object
            .get("previousNonCommandFocus")
            .is_some_and(|value| value.is_null() || value.is_object())
        && object
            .get("agentsWorkspace")
            .is_some_and(|value| value.is_object())
        && object
            .get("commandPane")
            .is_some_and(|value| value.is_object())
        && object
            .get("browserProfiles")
            .is_some_and(|value| value.is_object())
        && object
            .get("browserTabs")
            .is_some_and(|value| value.is_object())
        && object
            .get("projectEditorShell")
            .is_some_and(|value| value.is_object())
}

pub(crate) fn gpui_workspace_shell_state_is_legacy_unversioned_object(
    object: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    json_string_field(object, "activeMode")
        .and_then(TitlebarMode::from_slug)
        .is_some()
        && object
            .get("shellFocus")
            .is_some_and(|value| value.is_object())
        && object
            .get("agentsWorkspace")
            .is_some_and(|value| value.is_object())
        && object
            .get("commandPane")
            .is_some_and(|value| value.is_object())
        && object
            .get("browserTabs")
            .is_some_and(|value| value.is_object())
        && object
            .get("projectEditorShell")
            .is_some_and(|value| value.is_object())
}

pub(crate) fn gpui_workspace_shell_state_json(app: &GhostexGpuiApp) -> serde_json::Value {
    let active_mode = app.available_titlebar_mode_or_agents(app.active_mode);
    // CDXC:GPUISessionChatViewPersistence 2026-07-31: bare shell session ids
    // only (layout metadata) — which sessions last showed Session Chat.
    let mut agents_chat_mode_session_ids = app
        .agents_chat_mode_sessions
        .iter()
        .filter(|session_id| app.agents_workspace.has_session(**session_id))
        .map(|session_id| session_id.0)
        .collect::<Vec<_>>();
    agents_chat_mode_session_ids.sort_unstable();
    serde_json::json!({
        "version": GPUI_WORKSPACE_SHELL_STATE_VERSION,
        "activeMode": active_mode.element_slug(),
        "shellFocus": shell_focus_to_shell_state_json(app.shell_focus),
        "previousNonCommandFocus": app
            .previous_non_command_focus
            .map(shell_focus_to_shell_state_json),
        "petOverlayActivitiesVisible": app.gpui_pet_overlay_activities_visible,
        "agentsWorkspace": workspace_model_to_shell_state_json(&app.agents_workspace),
        "agentsWorkspaceProjectId": app.agents_workspace_project_id,
        "agentsWorkspacesByProject": app
            .parked_agents_workspaces_by_project
            .iter()
            .map(|(project_id, workspace_json)| {
                (project_id.clone(), workspace_json.clone())
            })
            .collect::<serde_json::Map<_, _>>(),
        "agentsWorkspaceSessionMappings": local_workspace_session_mappings_to_shell_state_json(
            &app.local_workspace_session_mappings,
            &app.agents_workspace,
        ),
        "agentsWorkspaceRemoteSessionMappings": remote_workspace_session_mappings_to_shell_state_json(
            &app.remote_attach_sessions,
            &app.agents_workspace,
            app.agents_workspace_project_id.as_deref(),
        ),
        "agentsChatModeSessions": agents_chat_mode_session_ids,
        "agentsDelayedSends": agents_delayed_sends_to_shell_state_json(
            &app.local_workspace_session_mappings,
            &app.agents_workspace,
            &app.agents_delayed_send_timers,
            &app.agents_send_when_stopped_watchers,
            SystemTime::now(),
        ),
        "commandPane": command_pane_model_to_shell_state_json_with_delayed_send_timers(
            &app.command_pane,
            &app.command_delayed_send_timers,
            SystemTime::now(),
        ),
        "commandPaneProjectId": app.command_pane_project_id,
        "commandPanesByProject": app
            .parked_command_panes_by_project
            .iter()
            .map(|(project_id, pane_json)| (project_id.clone(), pane_json.clone()))
            .collect::<serde_json::Map<_, _>>(),
        "pendingCommandSessionCleanup": pending_command_gxserver_cleanup_to_shell_state(
            &app.pending_command_gxserver_cleanup,
        ),
        "browserProfiles": browser_profile_model_to_shell_state_json(&app.browser_profiles),
        "browserTabs": browser_tab_model_to_shell_state_json(&app.browser_tabs),
        "browserTabsProjectId": app.browser_tabs_project_id,
        "browserTabsByProject": app
            .parked_browser_tabs_by_project
            .iter()
            .map(|(project_id, tabs)| {
                (project_id.clone(), browser_tab_model_to_shell_state_json(tabs))
            })
            .collect::<serde_json::Map<_, _>>(),
        "projectEditorShell": project_editor_shell_to_shell_state_json(&app.project_editor_shell),
        "projectViewStates": app
            .project_view_states_for_shell_state()
            .iter()
            .map(|(project_id, state)| {
                (project_id.clone(), project_view_state_to_shell_state_json(state))
            })
            .collect::<serde_json::Map<_, _>>(),
    })
}

pub(crate) fn persist_gpui_workspace_shell_state(app: &GhostexGpuiApp) {
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
    Phase 10 persistence re-audit keeps this as the only GPUI-owned workspace shell-state writer. It may write writer-owned layout/focus/tab/profile/lifecycle metadata, bounded canonical gxserver P/G identities, the validated bounded command Action selector used for restart reuse, safe Agents Delayed Send trigger/remaining-time checkpoints, plus the `petOverlayActivitiesVisible` UI boolean only; pet activity payloads, pet titles, raw settings JSON, terminal content, command text, stdout/stderr, project paths, file paths, raw URLs/query/fragment, page titles, profile paths, cookies, credentials, tokens, raw payloads, private user content, and runtime surface data must stay out at the serializer boundary.
    */
    let path = gpui_workspace_shell_state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&gpui_workspace_shell_state_json(app)) {
        let _ = fs::write(path, data);
    }
}

pub(crate) fn sole_local_workspace_mapping_project_id(
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
) -> Option<String> {
    let mut project_ids = mappings.keys().map(|key| key.project_id.as_str());
    let project_id = project_ids.next()?;
    project_ids
        .all(|candidate| candidate == project_id)
        .then(|| project_id.to_string())
}

pub(crate) fn agents_workspace_project_state_to_shell_state_json(
    workspace: &WorkspaceModel,
    workspace_project_id: Option<&str>,
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_mappings: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    chat_mode_sessions: &HashSet<TerminalSessionId>,
    timers: &HashMap<TerminalSessionId, GpuiCommandDelayedSendTimer>,
    watchers: &HashMap<TerminalSessionId, GpuiAgentsSendWhenStoppedWatcher>,
    now: SystemTime,
) -> serde_json::Value {
    serde_json::json!({
        "workspace": workspace_model_to_shell_state_json(workspace),
        "sessionMappings": local_workspace_session_mappings_to_shell_state_json(
            mappings,
            workspace,
        ),
        "remoteSessionMappings": remote_workspace_session_mappings_to_shell_state_json(
            remote_mappings,
            workspace,
            workspace_project_id,
        ),
        "chatModeSessions": agents_chat_mode_sessions_to_shell_state_json(
            chat_mode_sessions,
            workspace,
        ),
        "delayedSends": agents_delayed_sends_to_shell_state_json(
            mappings,
            workspace,
            timers,
            watchers,
            now,
        ),
    })
}

pub(crate) fn agents_workspace_project_state_from_shell_state(
    value: &serde_json::Value,
    workspace_project_id: Option<&str>,
) -> Option<(
    WorkspaceModel,
    HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    HashSet<TerminalSessionId>,
    Vec<GpuiAgentsDelayedSendRestoreIntent>,
)> {
    let object = value.as_object()?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "workspace"
                | "sessionMappings"
                | "remoteSessionMappings"
                | "chatModeSessions"
                | "delayedSends"
        )
    }) {
        return None;
    }
    let workspace = object
        .get("workspace")
        .and_then(workspace_model_from_shell_state)?;
    let mappings = object
        .get("sessionMappings")
        .and_then(|value| local_workspace_session_mappings_from_shell_state(value, &workspace))?;
    let remote_mappings = match object.get("remoteSessionMappings") {
        Some(value) => remote_workspace_session_mappings_from_shell_state(
            value,
            &workspace,
            workspace_project_id,
        )?,
        None => HashMap::new(),
    };
    let chat_mode_sessions =
        agents_chat_mode_sessions_from_shell_state(object.get("chatModeSessions"), &workspace);
    let delayed_sends = match object.get("delayedSends") {
        Some(value) => agents_delayed_send_restore_intents_from_shell_state(value, &mappings)?,
        None => Vec::new(),
    };
    Some((
        workspace,
        mappings,
        remote_mappings,
        chat_mode_sessions,
        delayed_sends,
    ))
}

pub(crate) fn agents_chat_mode_sessions_to_shell_state_json(
    sessions: &HashSet<TerminalSessionId>,
    workspace: &WorkspaceModel,
) -> serde_json::Value {
    let mut session_ids = sessions
        .iter()
        .filter(|session_id| workspace.has_session(**session_id))
        .map(|session_id| session_id.0)
        .collect::<Vec<_>>();
    session_ids.sort_unstable();
    serde_json::json!(session_ids)
}

pub(crate) fn agents_chat_mode_sessions_from_shell_state(
    value: Option<&serde_json::Value>,
    workspace: &WorkspaceModel,
) -> HashSet<TerminalSessionId> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_u64)
        .map(TerminalSessionId)
        .filter(|session_id| workspace.has_session(*session_id))
        .collect()
}

pub(crate) fn agents_delayed_sends_to_shell_state_json(
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    workspace: &WorkspaceModel,
    timers: &HashMap<TerminalSessionId, GpuiCommandDelayedSendTimer>,
    watchers: &HashMap<TerminalSessionId, GpuiAgentsSendWhenStoppedWatcher>,
    now: SystemTime,
) -> serde_json::Value {
    /*
    CDXC:GPUIAgentsDelayedSendPersistence 2026-07-22:
    Agents Delayed Send restart state is keyed only by the canonical gxserver
    project/session identity already accepted by the workspace mapping parser.
    Fixed timers keep the same bounded remaining-time checkpoint as command
    timers; status triggers keep only their enum scope and re-evaluate live
    activity after launch. Never persist shell ids, mount/runtime owners,
    generations, titles, paths, commands, terminal content, or status payloads.
    */
    let mut entries = mappings
        .iter()
        .filter_map(|(key, shell_session_id)| {
            if !workspace.has_session(*shell_session_id) {
                return None;
            }
            let mut entry = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            if let Some(timer) = timers.get(shell_session_id).copied() {
                let remaining_ms = timer.remaining_ms(now);
                if remaining_ms == 0 {
                    return None;
                }
                entry["trigger"] = serde_json::json!("timer");
                entry["remainingMs"] = serde_json::json!(remaining_ms);
                return Some((shell_session_id.0, entry));
            }
            let watcher = watchers.get(shell_session_id)?;
            entry["trigger"] = match &watcher.scope {
                GpuiAgentsSendWhenStoppedScope::Session => {
                    serde_json::json!("agentFinishesWorking")
                }
                GpuiAgentsSendWhenStoppedScope::Project(project_id)
                    if project_id == &key.project_id =>
                {
                    serde_json::json!("allAgentsFinishWorking")
                }
                GpuiAgentsSendWhenStoppedScope::Project(_) => return None,
            };
            Some((shell_session_id.0, entry))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(shell_session_id, _)| *shell_session_id);
    serde_json::Value::Array(entries.into_iter().map(|(_, entry)| entry).collect())
}

pub(crate) fn agents_delayed_send_restore_intents_from_shell_state(
    value: &serde_json::Value,
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
) -> Option<Vec<GpuiAgentsDelayedSendRestoreIntent>> {
    let entries = value.as_array()?;
    if entries.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return None;
    }
    let mut restored_session_ids = HashSet::with_capacity(entries.len());
    let mut intents = Vec::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        let project_id = json_string_field(object, "projectId")?.trim();
        let session_id = json_string_field(object, "sessionId")?.trim();
        let trigger = json_string_field(object, "trigger")?;
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_sidebar_local_gxserver_session_id_allowed(session_id)
        {
            return None;
        }
        let key = GpuiLocalWorkspaceSessionKey {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        let shell_session_id = *mappings.get(&key)?;
        if !restored_session_ids.insert(shell_session_id) {
            return None;
        }
        let trigger = match trigger {
            "timer"
                if object.len() == 4
                    && object.contains_key("remainingMs")
                    && object.keys().all(|key| {
                        matches!(
                            key.as_str(),
                            "projectId" | "sessionId" | "trigger" | "remainingMs"
                        )
                    }) =>
            {
                GpuiAgentsDelayedSendRestoreTrigger::Timer {
                    remaining_ms: object
                        .get("remainingMs")
                        .and_then(gpui_command_delayed_send_restore_remaining_ms)?,
                }
            }
            "agentFinishesWorking"
                if object.len() == 3
                    && object.keys().all(|key| {
                        matches!(key.as_str(), "projectId" | "sessionId" | "trigger")
                    }) =>
            {
                GpuiAgentsDelayedSendRestoreTrigger::WhenAgentFinishesWorking
            }
            "allAgentsFinishWorking"
                if object.len() == 3
                    && object.keys().all(|key| {
                        matches!(key.as_str(), "projectId" | "sessionId" | "trigger")
                    }) =>
            {
                GpuiAgentsDelayedSendRestoreTrigger::WhenAllAgentsFinishWorking {
                    project_id: project_id.to_string(),
                }
            }
            _ => return None,
        };
        intents.push(GpuiAgentsDelayedSendRestoreIntent {
            session_id: shell_session_id,
            trigger,
        });
    }
    Some(intents)
}

pub(crate) fn workspace_model_to_shell_state_json(model: &WorkspaceModel) -> serde_json::Value {
    /*
    CDXC:GPUIAgentsTabStatus 2026-06-22-16:27:
    Agents tab status persistence is intentionally limited to enum/boolean shell metadata so restored placeholder tabs keep their semantic dots without storing delayed-send deadlines, labels, commands, terminal output, paths, tokens, private titles, or user content.
    */
    serde_json::json!({
        "terminalSessions": model
            .terminal_sessions
            .iter()
            .map(|session| {
                serde_json::json!({
                    "id": session.id.0,
                    "presentationState": session.presentation_state.element_slug(),
                    "activity": session.activity.element_slug(),
                    "agentIcon": session.agent_icon,
                    "kind": session.kind.shell_state_slug(),
                    "delayedSendActive": session.delayed_send_active,
                })
            })
            .collect::<Vec<_>>(),
        "root": workspace_node_to_shell_state_json(&model.root),
        "focusedPaneId": model.focused_pane.0,
        "focusModePaneId": model
            .focus_mode_pane
            .map(|pane_id| serde_json::json!(pane_id.0))
            .unwrap_or(serde_json::Value::Null),
        "nextPaneId": model.next_pane_id,
        "nextSplitId": model.next_split_id,
        "nextSessionId": model.next_session_id,
    })
}

pub(crate) fn workspace_node_to_shell_state_json(node: &WorkspaceNode) -> serde_json::Value {
    match node {
        WorkspaceNode::Leaf(leaf) => serde_json::json!({
            "type": "leaf",
            "paneId": leaf.pane_id.0,
            "activeSessionId": leaf.tab_group.active_tab.0,
            "tabs": leaf
                .tab_group
                .tabs
                .iter()
                .map(|tab| serde_json::json!(tab.session_id.0))
                .collect::<Vec<_>>(),
        }),
        WorkspaceNode::Split(split) => serde_json::json!({
            "type": "split",
            "splitId": split.id.0,
            "axis": split.axis.element_slug(),
            "ratio": json_number_f32(workspace_split_ratio(split.ratio)),
            "defaultRatio": json_number_f32(workspace_split_ratio(split.default_ratio)),
            "first": workspace_node_to_shell_state_json(&split.first),
            "second": workspace_node_to_shell_state_json(&split.second),
        }),
    }
}

pub(crate) fn workspace_model_from_shell_state(
    value: &serde_json::Value,
) -> Option<WorkspaceModel> {
    let object = value.as_object()?;
    let sessions = json_array_field(object, "terminalSessions")?
        .iter()
        .map(terminal_session_from_shell_state)
        .collect::<Option<Vec<_>>>()?;
    if has_duplicate_u64(
        &sessions
            .iter()
            .map(|session| session.id.0)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    let session_ids = sessions
        .iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let root = workspace_node_from_shell_state(object.get("root")?, &session_ids)?;
    let empty_root_pane_id = workspace_empty_root_leaf_id(&root);
    let workspace_is_empty = sessions.is_empty() && empty_root_pane_id.is_some();
    /*
    CDXC:GPUIWorkspacePersistence 2026-06-26-05:23:
    The macOS workspace can close the last visible terminal and keep the project open. GPUI shell-state restore therefore accepts only the exact empty Agents root-leaf shape with zero terminal sessions, while split layouts and non-empty session lists must still reference real terminal tabs.
    */
    if sessions.is_empty() != workspace_is_empty {
        return None;
    }
    let mut pane_ids = Vec::new();
    collect_workspace_leaf_ids(&root, &mut pane_ids);
    let mut all_pane_ids = Vec::new();
    collect_workspace_all_leaf_ids(&root, &mut all_pane_ids);
    if (!workspace_is_empty && pane_ids.is_empty())
        || all_pane_ids.is_empty()
        || has_duplicate_u64(
            &all_pane_ids
                .iter()
                .map(|pane_id| pane_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let mut referenced_session_ids = Vec::new();
    collect_workspace_node_session_ids(&root, &mut referenced_session_ids);
    if (!workspace_is_empty && referenced_session_ids.is_empty())
        || has_duplicate_u64(
            &referenced_session_ids
                .iter()
                .map(|session_id| session_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let terminal_sessions = if workspace_is_empty {
        Vec::new()
    } else {
        sessions
            .into_iter()
            .filter(|session| referenced_session_ids.contains(&session.id))
            .collect::<Vec<_>>()
    };
    if !workspace_is_empty && terminal_sessions.is_empty() {
        return None;
    }

    let first_pane_id = pane_ids.first().copied().or(empty_root_pane_id)?;
    let focused_pane = json_u64_field(object, "focusedPaneId")
        .map(WorkspacePaneId)
        .filter(|pane_id| all_pane_ids.contains(pane_id))
        .unwrap_or(first_pane_id);
    let next_pane_id = json_u64_field(object, "nextPaneId").unwrap_or(0).max(
        all_pane_ids
            .iter()
            .map(|pane_id| pane_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let mut split_ids = Vec::new();
    collect_workspace_split_ids(&root, &mut split_ids);
    if has_duplicate_u64(
        &split_ids
            .iter()
            .map(|split_id| split_id.0)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }
    let next_split_id = json_u64_field(object, "nextSplitId").unwrap_or(0).max(
        split_ids
            .iter()
            .map(|split_id| split_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    let next_session_id = json_u64_field(object, "nextSessionId").unwrap_or(0).max(
        referenced_session_ids
            .iter()
            .map(|session_id| session_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );

    let mut model = WorkspaceModel {
        terminal_sessions,
        root,
        focused_pane,
        focus_mode_pane: None,
        next_pane_id,
        next_split_id,
        next_session_id,
    };
    if let Some(focus_mode_pane) = object
        .get("focusModePaneId")
        .and_then(json_u64_value)
        .map(WorkspacePaneId)
        .filter(|pane_id| model.find_leaf(*pane_id).is_some())
    {
        model.focus_mode_pane = Some(focus_mode_pane);
        if model.focus_mode_eligible_leaf_count() <= 1
            || !model.leaf_is_focus_mode_eligible(focus_mode_pane)
        {
            model.focus_mode_pane = None;
        }
    }
    model.normalize_workspace_tree();
    Some(model)
}

pub(crate) fn terminal_session_from_shell_state(
    value: &serde_json::Value,
) -> Option<TerminalSession> {
    let object = value.as_object()?;
    let id = TerminalSessionId(json_u64_field(object, "id")?);
    if id.0 == 0 {
        return None;
    }
    let presentation_state = json_string_field(object, "presentationState")
        .and_then(TerminalSessionPresentationState::from_slug)
        .unwrap_or(TerminalSessionPresentationState::Running);
    let activity = json_string_field(object, "activity")
        .and_then(AgentTerminalActivity::from_slug)
        .unwrap_or_default();
    let agent_icon = json_string_field(object, "agentIcon")
        .and_then(|value| gpui_sidebar_agent_icon(Some(value)));
    let kind = json_string_field(object, "kind")
        .and_then(AgentsWorkspaceSessionKind::from_sidebar_kind)
        .unwrap_or_default();
    let delayed_send_active = json_bool_field(object, "delayedSendActive").unwrap_or(false);
    let mut session =
        TerminalSession::placeholder(id, terminal_session_title_for_id(id), presentation_state)
            .with_activity(activity)
            .with_agent_icon(agent_icon)
            .with_kind(kind)
            .with_delayed_send_active(delayed_send_active);
    if presentation_state == TerminalSessionPresentationState::Mounting {
        /*
        CDXC:GPUITerminalActivationRuntimeGuard 2026-06-23-18:00:
        Shell-state JSON intentionally stores only the visible `mounting` presentation, not whether that Mounting tab came from a new startup, failed retry, wake, materialize, or reattach action. Restored Mounting sessions therefore come back as non-startup-eligible placeholders so a pre-restart wake/reattach state cannot create a new Ghostty process or claim a parked runtime owner that no longer exists.

        CDXC:GPUITerminalActivationRuntimeGuard 2026-06-23-18:12:
        Slice 229 keeps restored `presentationState:"mounting"` out of startup eligibility at the restore boundary itself. New Mounting terminal creation and in-process failed-startup retry set eligibility through runtime-only transitions after restore, not through persisted shell state.
        */
        session.set_presentation_state_with_startup_eligibility(presentation_state, false);
    }
    Some(session)
}

pub(crate) fn workspace_node_from_shell_state(
    value: &serde_json::Value,
    session_ids: &[TerminalSessionId],
) -> Option<WorkspaceNode> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "leaf" => {
            let pane_id = WorkspacePaneId(json_u64_field(object, "paneId")?);
            if pane_id.0 == 0 {
                return None;
            }
            let tabs = json_array_field(object, "tabs")?
                .iter()
                .map(json_u64_value)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .map(TerminalSessionId)
                .collect::<Vec<_>>();
            if tabs.is_empty() {
                if session_ids.is_empty() {
                    return Some(WorkspaceNode::Leaf(WorkspaceLeaf {
                        pane_id,
                        tab_group: WorkspaceTabGroup {
                            tabs: Vec::new(),
                            active_tab: TerminalSessionId(0),
                        },
                    }));
                }
                return None;
            }
            if has_duplicate_u64(
                &tabs
                    .iter()
                    .map(|session_id| session_id.0)
                    .collect::<Vec<_>>(),
            ) || tabs
                .iter()
                .any(|session_id| !session_ids.contains(session_id))
            {
                return None;
            }
            let active_tab = json_u64_field(object, "activeSessionId")
                .map(TerminalSessionId)
                .filter(|session_id| tabs.contains(session_id))
                .unwrap_or(tabs[0]);
            Some(WorkspaceNode::Leaf(WorkspaceLeaf {
                pane_id,
                tab_group: WorkspaceTabGroup {
                    tabs: tabs
                        .into_iter()
                        .map(|session_id| WorkspaceTab { session_id })
                        .collect(),
                    active_tab,
                },
            }))
        }
        "split" => {
            let split_id = WorkspaceSplitId(json_u64_field(object, "splitId")?);
            if split_id.0 == 0 {
                return None;
            }
            Some(WorkspaceNode::Split(WorkspaceSplit {
                id: split_id,
                axis: json_string_field(object, "axis").and_then(WorkspaceSplitAxis::from_slug)?,
                ratio: json_f32_field(object, "ratio")
                    .map(workspace_split_ratio)
                    .unwrap_or(0.5),
                default_ratio: json_f32_field(object, "defaultRatio")
                    .map(workspace_split_ratio)
                    .unwrap_or(0.5),
                first: Box::new(workspace_node_from_shell_state(
                    object.get("first")?,
                    session_ids,
                )?),
                second: Box::new(workspace_node_from_shell_state(
                    object.get("second")?,
                    session_ids,
                )?),
            }))
        }
        _ => None,
    }
}

pub(crate) fn collect_workspace_node_session_ids(
    node: &WorkspaceNode,
    session_ids: &mut Vec<TerminalSessionId>,
) {
    match node {
        WorkspaceNode::Leaf(leaf) => {
            session_ids.extend(leaf.tab_group.tabs.iter().map(|tab| tab.session_id));
        }
        WorkspaceNode::Split(split) => {
            collect_workspace_node_session_ids(&split.first, session_ids);
            collect_workspace_node_session_ids(&split.second, session_ids);
        }
    }
}

pub(crate) fn collect_workspace_split_ids(
    node: &WorkspaceNode,
    split_ids: &mut Vec<WorkspaceSplitId>,
) {
    match node {
        WorkspaceNode::Leaf(_) => {}
        WorkspaceNode::Split(split) => {
            split_ids.push(split.id);
            collect_workspace_split_ids(&split.first, split_ids);
            collect_workspace_split_ids(&split.second, split_ids);
        }
    }
}
