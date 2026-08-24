// C1 wave-3 re-cluster: command pane shell-state persistence: command model/session/delayed-send/startup JSON round trips, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn command_pane_model_to_shell_state_json_with_delayed_send_timers(
    model: &CommandPaneModel,
    delayed_send_timers: &HashMap<CommandSessionId, GpuiCommandDelayedSendTimer>,
    now: SystemTime,
) -> serde_json::Value {
    command_pane_model_to_shell_state_json_with_optional_delayed_send_timers(
        model,
        Some(delayed_send_timers),
        Some(now),
    )
}

pub(crate) fn command_pane_model_to_shell_state_json_with_optional_delayed_send_timers(
    model: &CommandPaneModel,
    delayed_send_timers: Option<&HashMap<CommandSessionId, GpuiCommandDelayedSendTimer>>,
    now: Option<SystemTime>,
) -> serde_json::Value {
    /*
    CDXC:GPUICommandTabStatus 2026-06-22-16:40:
    Command tab status persistence is limited to enum/boolean shell metadata so restored command placeholders keep semantic status indicators without storing command text, command output, terminal content, delayed-send deadlines, private titles, paths, tokens, or user content.

    CDXC:GPUICommandPaneReuse 2026-08-13:
    Persist the bounded Action command id as tab ownership metadata. A restored
    running tab cannot use the idle title-only recovery rule, so without this
    selector a repeated Quick Action allocates a second local tab before the
    daemon lookup discovers that both tabs target the same gxserver session.
    Run ids, per-action sound preferences, status-file paths, and command text
    remain process-only.

    CDXC:GPUICommandTabSleep 2026-06-25-14:27:
    Command tab sleep is safe shell lifecycle metadata. Persist only the isSleeping boolean beside activity and delayed-send state so restored tabs stay parked without storing command text, output, paths, process ids, status-file paths, or terminal content.

    CDXC:GPUICommandDelayedSend 2026-06-25-15:11:
    Live GPUI Delayed Send timers are process-memory contracts that press Return later through the exact mounted Ghostty surface. App-level persistence may snapshot only the deadline and remaining milliseconds for restart re-arm; model-only persistence still writes only semantic restored delayed-send placeholders.

    CDXC:GPUICommandCloseAfterDone 2026-06-25-15:24:
    Close After Done arming is safe command lifecycle metadata. Persist only the armed boolean so restored command-pane Action tabs can keep the user request, while deadlines/countdowns/generations stay process-local and restart from the current visible done state.

        CDXC:GPUICommandDelayedSend 2026-06-25-15:46:
        Sleeping command tabs preserve Delayed Send and Close After Done intent in shell state like native session records. Timer-owned Delayed Send writes only the safe restart checkpoint, while non-runtime restored placeholders keep the boolean intent.

    CDXC:GPUIFocusedSplits 2026-06-25-16:05:
    Command split axis is shell layout metadata. Persist it so command-pane split geometry round-trips without storing command text, terminal content, paths, process ids, or runtime mount state. Focused command hotkeys still write horizontal command splits for both directions to match native.

    CDXC:GPUICommandDelayedSend 2026-06-25-16:41:
    App-level command-pane persistence now mirrors native delayed-send restart behavior by writing only a live timer's UTC deadline and remaining-duration checkpoint. The model-only serializer still emits no deadlines, and neither path stores command text, titles, terminal content, paths, runtime ids, stdout/stderr, or countdown labels.

    CDXC:GPUICommandPane 2026-06-25-17:37:
    Native hides an emptied Commands panel without retaining the last resize height. Persist command-pane height only while command sessions exist; an empty hidden panel restores from the current Workspace default instead of an old user-resized ratio.

    CDXC:GPUICommandFocusMode 2026-06-25-21:40:
    Command Focus mode persistence stores only the focused command group id as reversible layout metadata. Restore validates that the group still exists and has more than one visible awake command owner before hiding any command split peers; no command text, terminal content, paths, runtime ids, or surface state are serialized.

    CDXC:GPUICommandDelayedSend 2026-06-25-22:40:
    App-level Delayed Send timer checkpoints belong to command-tab membership, not arbitrary stored command-session rows. Serialize restart checkpoints only for sessions still attached to a command group so orphaned rows cannot re-arm or redirect a timer after layout repair.

    CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
    Command-pane restart parity persists the command-surface gxserver project/session ids, bounded display title, and validated bounded Action selector for each command tab. The daemon still owns scrollback and process state through zmx; shell JSON must not grow command text, cwd, env, terminal output, status-file paths, tokens, or raw attach commands.
    */
    let mut state = serde_json::json!({
        "terminalSessions": model
            .terminal_sessions
            .iter()
            .map(|session| {
                let session_has_command_group =
                    command_pane_group_for_session(model, session.id).is_some();
                let restored_timer = delayed_send_timers
                    .and_then(|timers| timers.get(&session.id).copied())
                    .filter(|_| {
                        session_has_command_group
                            && session.delayed_send_active
                            && session.delayed_send_timer_owned
                    })
                    .and_then(|timer| now.map(|now| (timer, timer.remaining_ms(now))))
                    .filter(|(_, remaining_ms)| *remaining_ms > 0);
                let mut session_json = serde_json::json!({
                    "id": session.id.0,
                    "activity": if session.is_sleeping {
                        CommandTerminalActivity::Idle.element_slug()
                    } else {
                        session.activity.element_slug()
                    },
                    "delayedSendActive": restored_timer.is_some()
                        || (session.delayed_send_active && !session.delayed_send_timer_owned),
                    "closeAfterDone": session.close_after_done_armed,
                    "title": session.title,
                    "isSleeping": session.is_sleeping,
                });
                if let Some(key) = session.gxserver_session_key.as_ref()
                    && let Some(object) = session_json.as_object_mut()
                {
                    object.insert(
                        "gxserverProjectId".to_string(),
                        serde_json::Value::String(key.project_id.clone()),
                    );
                    object.insert(
                        "gxserverSessionId".to_string(),
                        serde_json::Value::String(key.session_id.clone()),
                    );
                }
                if let Some(command_id) = session.action_command_id.as_ref()
                    && let Some(object) = session_json.as_object_mut()
                {
                    object.insert(
                        "actionCommandId".to_string(),
                        serde_json::Value::String(command_id.clone()),
                    );
                }
                if let Some((timer, remaining_ms)) = restored_timer
                    && let Some(object) = session_json.as_object_mut()
                {
                    object.insert(
                        "delayedSendDeadlineAt".to_string(),
                        serde_json::json!(gpui_iso8601_utc(timer.deadline_at)),
                    );
                    object.insert(
                        "delayedSendRemainingMs".to_string(),
                        serde_json::json!(remaining_ms),
                    );
                }
                session_json
            })
            .collect::<Vec<_>>(),
        "root": command_pane_node_to_shell_state_json(&model.root),
        "focusedGroupId": model.focused_group.0,
        "focusModeGroupId": model
            .focus_mode_group
            .map(|group_id| serde_json::json!(group_id.0))
            .unwrap_or(serde_json::Value::Null),
        "mode": model.mode.element_slug(),
        "lastExpandedMode": model.last_expanded_mode.element_slug(),
        "nextGroupId": model.next_group_id,
        "nextSplitId": model.next_split_id,
        "nextSessionId": model.next_session_id,
    });
    if model.has_sessions() {
        state["heightRatio"] = json_number_f32(command_pane_height_ratio(model.height_ratio));
        state["widthRatio"] = json_number_f32(command_pane_width_ratio(model.width_ratio));
    }
    state
}

pub(crate) fn command_pane_node_to_shell_state_json(node: &CommandPaneNode) -> serde_json::Value {
    match node {
        CommandPaneNode::Leaf(leaf) => serde_json::json!({
            "type": "leaf",
            "groupId": leaf.group_id.0,
            "activeSessionId": leaf.tab_group.active_session.0,
            "tabs": leaf
                .tab_group
                .tabs
                .iter()
                .map(|tab| serde_json::json!(tab.session_id.0))
                .collect::<Vec<_>>(),
        }),
        CommandPaneNode::Split(split) => serde_json::json!({
            "type": "split",
            "splitId": split.id.0,
            "axis": split.axis.element_slug(),
            "ratio": json_number_f32(workspace_split_ratio(split.ratio)),
            "first": command_pane_node_to_shell_state_json(&split.first),
            "second": command_pane_node_to_shell_state_json(&split.second),
        }),
    }
}

pub(crate) fn command_pane_model_from_shell_state_with_default_height_px(
    value: &serde_json::Value,
    content_height: f32,
    default_height_px: f32,
) -> Option<CommandPaneModel> {
    let object = value.as_object()?;
    let sessions = json_array_field(object, "terminalSessions")?
        .iter()
        .map(command_session_from_shell_state)
        .collect::<Option<Vec<_>>>()?;
    if has_duplicate_u64(
        &sessions
            .iter()
            .map(|session| session.id.0)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    if sessions.is_empty() {
        return Some(CommandPaneModel {
            terminal_sessions: Vec::new(),
            root: command_pane_dummy_node(),
            focused_group: CommandPaneGroupId(0),
            focus_mode_group: None,
            mode: CommandPaneMode::Collapsed,
            last_expanded_mode: CommandPaneMode::Pinned,
            height_ratio: command_pane_default_height_ratio_for_default_height_px(
                default_height_px,
                content_height,
            ),
            width_ratio: COMMAND_PANE_DEFAULT_WIDTH_RATIO,
            resize_drag: None,
            next_group_id: json_u64_field(object, "nextGroupId").unwrap_or(1).max(1),
            next_split_id: json_u64_field(object, "nextSplitId").unwrap_or(1).max(1),
            next_session_id: json_u64_field(object, "nextSessionId").unwrap_or(1).max(1),
        });
    }

    let session_ids = sessions
        .iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let root = command_pane_node_from_shell_state(object.get("root")?, &session_ids)?;
    let mut group_ids = Vec::new();
    collect_command_leaf_ids(&root, &mut group_ids);
    if group_ids.is_empty()
        || has_duplicate_u64(
            &group_ids
                .iter()
                .map(|group_id| group_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let mut referenced_session_ids = Vec::new();
    collect_command_node_session_ids(&root, &mut referenced_session_ids);
    if referenced_session_ids.is_empty()
        || has_duplicate_u64(
            &referenced_session_ids
                .iter()
                .map(|session_id| session_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let terminal_sessions = sessions
        .into_iter()
        .filter(|session| referenced_session_ids.contains(&session.id))
        .collect::<Vec<_>>();
    if terminal_sessions.is_empty() {
        return None;
    }

    let focused_group = json_u64_field(object, "focusedGroupId")
        .map(CommandPaneGroupId)
        .filter(|group_id| group_ids.contains(group_id))
        .unwrap_or(group_ids[0]);
    let mut split_ids = Vec::new();
    collect_command_split_ids(&root, &mut split_ids);
    if has_duplicate_u64(
        &split_ids
            .iter()
            .map(|split_id| split_id.0)
            .collect::<Vec<_>>(),
    ) {
        return None;
    }

    let mode = command_pane_mode_for_current_release(
        json_string_field(object, "mode")
            .and_then(CommandPaneMode::from_slug)
            .unwrap_or(CommandPaneMode::Pinned),
    );
    let last_expanded_mode = command_pane_mode_for_current_release(
        json_string_field(object, "lastExpandedMode")
            .and_then(CommandPaneMode::from_slug)
            .filter(|mode| !matches!(mode, CommandPaneMode::Collapsed))
            .unwrap_or(CommandPaneMode::Pinned),
    );

    let mut model = CommandPaneModel {
        terminal_sessions,
        root,
        focused_group,
        focus_mode_group: None,
        mode,
        last_expanded_mode,
        height_ratio: json_f32_field(object, "heightRatio")
            .map(command_pane_height_ratio)
            .unwrap_or_else(|| {
                command_pane_default_height_ratio_for_default_height_px(
                    default_height_px,
                    content_height,
                )
            }),
        width_ratio: json_f32_field(object, "widthRatio")
            .map(command_pane_width_ratio)
            .unwrap_or(COMMAND_PANE_DEFAULT_WIDTH_RATIO),
        resize_drag: None,
        next_group_id: json_u64_field(object, "nextGroupId").unwrap_or(0).max(
            group_ids
                .iter()
                .map(|group_id| group_id.0)
                .max()
                .unwrap_or(0)
                + 1,
        ),
        next_split_id: json_u64_field(object, "nextSplitId").unwrap_or(0).max(
            split_ids
                .iter()
                .map(|split_id| split_id.0)
                .max()
                .unwrap_or(0)
                + 1,
        ),
        next_session_id: json_u64_field(object, "nextSessionId").unwrap_or(0).max(
            referenced_session_ids
                .iter()
                .map(|session_id| session_id.0)
                .max()
                .unwrap_or(0)
                + 1,
        ),
    };
    if let Some(focus_mode_group) = object
        .get("focusModeGroupId")
        .and_then(json_u64_value)
        .map(CommandPaneGroupId)
        .filter(|group_id| group_ids.contains(group_id))
    {
        model.focus_mode_group = Some(focus_mode_group);
        model.clear_focus_mode_if_invalid();
    }

    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-08-13:
    A command-surface gxserver session has exactly one local command-tab owner.
    Older shell state could contain duplicate local tabs after Action reuse found
    the daemon session only after allocating a placeholder. Repair that persisted
    state deterministically in layout order so future restores and Action clicks
    keep the original tab instead of retaining two views of the same process.
    */
    let mut seen_gxserver_sessions = HashSet::new();
    let duplicate_tabs = model
        .flat_tab_ids()
        .into_iter()
        .filter(|(_, session_id)| {
            model
                .session(*session_id)
                .and_then(|session| session.gxserver_session_key.clone())
                .is_some_and(|key| !seen_gxserver_sessions.insert(key))
        })
        .collect::<Vec<_>>();
    for (group_id, session_id) in duplicate_tabs {
        model.close_session(group_id, session_id);
    }
    Some(model)
}

pub(crate) fn command_session_from_shell_state(
    value: &serde_json::Value,
) -> Option<CommandTerminalSession> {
    let object = value.as_object()?;
    let id = CommandSessionId(json_u64_field(object, "id")?);
    if id.0 == 0 {
        return None;
    }
    let is_sleeping = json_bool_field(object, "isSleeping").unwrap_or(false);
    let activity = if is_sleeping {
        CommandTerminalActivity::Idle
    } else {
        json_string_field(object, "activity")
            .and_then(CommandTerminalActivity::from_slug)
            .unwrap_or_default()
    };
    let delayed_send_active = json_bool_field(object, "delayedSendActive").unwrap_or(false);
    let close_after_done_armed = json_bool_field(object, "closeAfterDone").unwrap_or(false);
    let title = command_session_title_from_shell_state(object, id);
    let gxserver_session_key = command_session_gxserver_key_from_shell_state(object);
    let action_command_id = command_session_action_command_id_from_shell_state(object);
    let mut session = CommandTerminalSession::placeholder(id, title)
        .with_activity(activity)
        .with_delayed_send_active(delayed_send_active)
        .with_close_after_done_armed(close_after_done_armed)
        .with_gxserver_session_key(gxserver_session_key)
        .with_sleeping(is_sleeping);
    if let Some(command_id) = action_command_id {
        session = session.with_action_command_id(command_id);
    }
    Some(session)
}

pub(crate) fn command_session_action_command_id_from_shell_state(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    valid_action_command_id(json_string_field(object, "actionCommandId")?)
}

pub(crate) fn valid_action_command_id(value: &str) -> Option<String> {
    let command_id = value.trim();
    (!command_id.is_empty()
        && command_id.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !command_id.contains('\0')
        && !command_id.chars().any(char::is_control))
    .then(|| command_id.to_string())
}

pub(crate) fn command_session_title_from_shell_state(
    object: &serde_json::Map<String, serde_json::Value>,
    id: CommandSessionId,
) -> String {
    if let Some(title) = json_string_field(object, "title")
        .map(str::trim)
        .filter(|title| {
            !title.is_empty()
                && title.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !title.contains('\0')
                && !title.chars().any(char::is_control)
        })
    {
        return title.to_string();
    }
    command_session_title_for_id(id)
}

pub(crate) fn command_session_gxserver_key_from_shell_state(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<GpuiLocalWorkspaceSessionKey> {
    /*
    CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
    Old command-pane shell state has no daemon identity. Treat missing or invalid gxserver ids as absent so legacy tabs can be recreated through the normal Phase 1 creation path; only a complete validated local project/session pair becomes a restore attach key.
    */
    let project_id = json_string_field(object, "gxserverProjectId")?
        .trim()
        .to_string();
    let session_id = json_string_field(object, "gxserverSessionId")?
        .trim()
        .to_string();
    if !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
        || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
    {
        return None;
    }
    Some(GpuiLocalWorkspaceSessionKey {
        project_id,
        session_id,
    })
}

pub(crate) fn command_gxserver_session_mappings_from_command_model(
    command_pane: &CommandPaneModel,
) -> HashMap<CommandSessionId, GpuiLocalWorkspaceSessionKey> {
    command_pane
        .terminal_sessions
        .iter()
        .filter_map(|session| {
            session
                .gxserver_session_key
                .clone()
                .map(|key| (session.id, key))
        })
        .collect()
}

pub(crate) fn pending_command_gxserver_cleanup_from_shell_state(
    value: Option<&serde_json::Value>,
) -> HashSet<GpuiLocalWorkspaceSessionKey> {
    let Some(entries) = value.and_then(serde_json::Value::as_array) else {
        return HashSet::new();
    };
    entries
        .iter()
        .take(512)
        .filter_map(|entry| {
            let object = entry.as_object()?;
            let project_id = json_string_field(object, "projectId")?.trim().to_string();
            let session_id = json_string_field(object, "sessionId")?.trim().to_string();
            if !gpui_remote_sidebar_project_id_allowed(project_id.as_str())
                || !gpui_remote_sidebar_session_id_allowed(session_id.as_str())
            {
                return None;
            }
            Some(GpuiLocalWorkspaceSessionKey {
                project_id,
                session_id,
            })
        })
        .collect()
}

pub(crate) fn pending_command_gxserver_cleanup_to_shell_state(
    pending: &HashSet<GpuiLocalWorkspaceSessionKey>,
) -> serde_json::Value {
    let mut entries = pending.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.project_id
            .cmp(&right.project_id)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    serde_json::Value::Array(
        entries
            .into_iter()
            .map(|key| {
                serde_json::json!({
                    "projectId": key.project_id,
                    "sessionId": key.session_id,
                })
            })
            .collect(),
    )
}

pub(crate) fn collect_command_pane_shell_state_leaf_active_session_ids(
    node: Option<&serde_json::Value>,
    active_session_ids: &mut HashSet<u64>,
) {
    let Some(object) = node.and_then(serde_json::Value::as_object) else {
        return;
    };
    if let Some(active_session_id) = object
        .get("activeSessionId")
        .and_then(serde_json::Value::as_u64)
    {
        active_session_ids.insert(active_session_id);
    }
    collect_command_pane_shell_state_leaf_active_session_ids(
        object.get("first"),
        active_session_ids,
    );
    collect_command_pane_shell_state_leaf_active_session_ids(
        object.get("second"),
        active_session_ids,
    );
}

pub(crate) fn split_command_pane_shell_state_json_by_gxserver_project(
    pane_json: &serde_json::Value,
    fallback_project_id: Option<&str>,
) -> Vec<(String, serde_json::Value)> {
    /*
    CDXC:GPUICommandPanePerProject 2026-07-10:
    One-time migration for the pre-per-project global command pane: every
    persisted command tab already carries its owning gxserver project id, so
    the mixed panel splits into one single-leaf panel per project (rows
    without a valid id belong to the fallback active project). Split output
    reuses the writer-owned shell-state shape; split layout collapses to one
    tab group because the old split tree cannot be partitioned meaningfully.
    */
    let Some(object) = pane_json.as_object() else {
        return Vec::new();
    };
    let Some(sessions) = object
        .get("terminalSessions")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let mut preferred_active_session_ids = HashSet::new();
    collect_command_pane_shell_state_leaf_active_session_ids(
        object.get("root"),
        &mut preferred_active_session_ids,
    );

    let mut rows_by_project: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    for row in sessions {
        let Some(row_object) = row.as_object() else {
            continue;
        };
        let Some(project_id) = json_string_field(row_object, "gxserverProjectId")
            .map(str::trim)
            .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
            .map(str::to_string)
            .or_else(|| fallback_project_id.map(str::to_string))
        else {
            continue;
        };
        match rows_by_project
            .iter_mut()
            .find(|(existing, _)| *existing == project_id)
        {
            Some((_, rows)) => rows.push(row.clone()),
            None => rows_by_project.push((project_id, vec![row.clone()])),
        }
    }

    rows_by_project
        .into_iter()
        .filter_map(|(project_id, rows)| {
            let session_ids = rows
                .iter()
                .filter_map(|row| row.get("id").and_then(serde_json::Value::as_u64))
                .collect::<Vec<_>>();
            if session_ids.len() != rows.len() {
                return None;
            }
            let active_session_id = session_ids
                .iter()
                .copied()
                .find(|session_id| preferred_active_session_ids.contains(session_id))
                .or_else(|| session_ids.first().copied())?;
            let next_group_id = object
                .get("nextGroupId")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1)
                .max(1);
            let next_split_id = object
                .get("nextSplitId")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let next_session_id = object
                .get("nextSessionId")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .max(session_ids.iter().copied().max().unwrap_or(0) + 1);
            let mut pane = serde_json::json!({
                "terminalSessions": rows,
                "root": {
                    "type": "leaf",
                    "groupId": 0,
                    "activeSessionId": active_session_id,
                    "tabs": session_ids,
                },
                "focusedGroupId": 0,
                "focusModeGroupId": serde_json::Value::Null,
                "mode": object
                    .get("mode")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("pinned")),
                "lastExpandedMode": object
                    .get("lastExpandedMode")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!("pinned")),
                "nextGroupId": next_group_id,
                "nextSplitId": next_split_id,
                "nextSessionId": next_session_id,
            });
            if let Some(height_ratio) = object.get("heightRatio") {
                pane["heightRatio"] = height_ratio.clone();
            }
            if let Some(width_ratio) = object.get("widthRatio") {
                pane["widthRatio"] = width_ratio.clone();
            }
            Some((project_id, pane))
        })
        .collect()
}

pub(crate) fn command_delayed_send_restore_timers_from_shell_state(
    value: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> Vec<GpuiCommandDelayedSendRestoreTimer> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-22:40:
    Restore-time Delayed Send timers require live command-tab group membership resolved through the command pane, not just a stored terminal-session row. Orphaned session rows are stale persistence data and must not re-arm timers or fall back to another command group.
    */
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    let Some(sessions) = json_array_field(object, "terminalSessions") else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter_map(|value| {
            let object = value.as_object()?;
            if json_bool_field(object, "delayedSendActive") != Some(true) {
                return None;
            }
            let session_id = CommandSessionId(json_u64_field(object, "id")?);
            if command_pane_group_for_session(command_pane, session_id).is_none()
                || command_pane.session(session_id).is_none()
            {
                return None;
            }
            let remaining_ms = object
                .get("delayedSendRemainingMs")
                .and_then(gpui_command_delayed_send_restore_remaining_ms)?;
            Some(GpuiCommandDelayedSendRestoreTimer {
                session_id,
                remaining_ms,
            })
        })
        .collect()
}

pub(crate) fn command_delayed_send_stale_runtime_timer_session_ids(
    command_pane: &CommandPaneModel,
    delayed_send_timers: &HashMap<CommandSessionId, GpuiCommandDelayedSendTimer>,
) -> Vec<CommandSessionId> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-27-05:50:
    Delayed Send runtime timers require the same live command-tab membership as modal submissions and restore checkpoints: a current command group reference plus a stored command session row. Stale root tab ids whose session row disappeared must prune their timers instead of being treated as mounted-capable command terminals.
    */
    delayed_send_timers
        .keys()
        .copied()
        .filter(|session_id| {
            command_pane_group_for_session(command_pane, *session_id).is_none()
                || command_pane.session(*session_id).is_none()
        })
        .collect()
}

pub(crate) fn command_startup_activity_restore_intents_from_shell_state(
    value: &serde_json::Value,
    command_pane: &CommandPaneModel,
) -> Vec<GpuiCommandStartupActivityRestoreIntent> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    let Some(sessions) = json_array_field(object, "terminalSessions") else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter_map(|value| {
            let object = value.as_object()?;
            let activity = json_string_field(object, "activity")
                .and_then(CommandTerminalActivity::from_slug)?;
            if !matches!(
                activity,
                CommandTerminalActivity::Working | CommandTerminalActivity::Attention
            ) {
                return None;
            }
            let session_id = CommandSessionId(json_u64_field(object, "id")?);
            if command_pane.session(session_id).is_none() {
                return None;
            }
            Some(GpuiCommandStartupActivityRestoreIntent {
                session_id,
                activity,
            })
        })
        .collect()
}

pub(crate) fn command_pane_apply_startup_activity_restore_intents(
    command_pane: &mut CommandPaneModel,
    restore_intents: &[GpuiCommandStartupActivityRestoreIntent],
) -> bool {
    /*
    CDXC:GPUICommandStartupRestore 2026-06-25-17:25:
    Native command-panel restoreActivity treats Working as a one-shot startup wake hint and Attention as a wake plus visible status. GPUI must parse those raw activity hints before sleeping-session normalization, then use normal visible command-pane layout to expand/select/wake the restored tab; Working is cleared to Idle after the wake while Attention remains visible.

    CDXC:GPUICommandStartupRestore 2026-06-26-04:29:
    Restore-time command focus normalization must compare the target against `focused_group_active_session_id`, not `active_group_and_session_id`, because active fallback can report the first command tab while `focused_group` is stale. Select the restored live tab so native restore leaves the mounted command body as the command focus target.
    */
    let mut changed = false;
    for restore_intent in restore_intents {
        if command_pane.session(restore_intent.session_id).is_none() {
            continue;
        }
        let Some(target_group_id) =
            command_pane_group_for_session(command_pane, restore_intent.session_id)
        else {
            continue;
        };
        let focused_before = command_pane.focused_group_active_session_id();
        if focused_before != Some((target_group_id, restore_intent.session_id))
            && command_pane.select_session_in_group(target_group_id, restore_intent.session_id)
        {
            changed = true;
        }
        if !command_pane.is_expanded() {
            command_pane.expand();
            changed = true;
        }
        let Some(session) = command_pane.session_mut(restore_intent.session_id) else {
            continue;
        };
        if session.is_sleeping {
            session.is_sleeping = false;
            changed = true;
        }
        let restored_activity = match restore_intent.activity {
            CommandTerminalActivity::Idle => CommandTerminalActivity::Idle,
            CommandTerminalActivity::Working => CommandTerminalActivity::Idle,
            CommandTerminalActivity::Attention => CommandTerminalActivity::Attention,
        };
        if session.activity != restored_activity {
            session.activity = restored_activity;
            changed = true;
        }
    }
    changed
}

pub(crate) fn command_pane_apply_delayed_send_restore_intent(
    command_pane: &mut CommandPaneModel,
    session_id: CommandSessionId,
) -> bool {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-16:56:
    Native startup restores command-panel terminal sessions with active Delayed Send deadlines so the pending Enter has a live terminal when the timer fires. GPUI should wake only this persisted restore path while preserving the existing in-process manual Sleep rule that parks active timers until the user wakes the tab.

    CDXC:GPUICommandDelayedSend 2026-06-25-17:19:
    A restored GPUI Delayed Send timer needs the command body to exist through normal visible layout because GPUI command terminals do not use hidden/offscreen mounts. Promote the restored command tab to the active visible command-pane body during startup restore so the timer is not stranded behind a collapsed pane or inactive tab.

    CDXC:GPUICommandDelayedSend 2026-06-26-04:29:
    Delayed Send restore must normalize stale command focus even when `active_group_and_session_id` would fall back to the restored tab. Compare against `focused_group_active_session_id` so the resumed timer's mounted body is also the live command focus target.
    */
    let Some(target_group_id) = command_pane_group_for_session(command_pane, session_id) else {
        return false;
    };
    if command_pane.session(session_id).is_none() {
        return false;
    }
    let focused_before = command_pane.focused_group_active_session_id();
    let mut changed = false;
    if focused_before != Some((target_group_id, session_id))
        && command_pane.select_session_in_group(target_group_id, session_id)
    {
        changed = true;
    }
    if !command_pane.is_expanded() {
        command_pane.expand();
        changed = true;
    }
    let Some(session) = command_pane.session_mut(session_id) else {
        return changed;
    };
    changed = changed
        || !session.delayed_send_active
        || !session.delayed_send_timer_owned
        || session.is_sleeping;
    session.set_delayed_send_active(true, true);
    session.is_sleeping = false;
    changed
}

pub(crate) fn command_pane_node_from_shell_state(
    value: &serde_json::Value,
    session_ids: &[CommandSessionId],
) -> Option<CommandPaneNode> {
    let object = value.as_object()?;
    match json_string_field(object, "type")? {
        "leaf" => {
            let group_id = CommandPaneGroupId(json_u64_field(object, "groupId")?);
            let raw_tabs = json_array_field(object, "tabs")?
                .iter()
                .map(json_u64_value)
                .collect::<Option<Vec<_>>>()?;
            /*
            CDXC:GPUICommandPaneRestore 2026-06-27-04:15:
            Native command-panel restore repairs stale local pane layout by filtering leaf tab ids against stored command sessions and keeping only the first occurrence of repeated ids. GPUI must normalize each restored command leaf before validating the broader split tree so one stale or duplicate tab reference does not discard the whole command pane.
            */
            let mut seen_tab_ids = HashSet::new();
            let tabs = raw_tabs
                .into_iter()
                .map(CommandSessionId)
                .filter(|session_id| session_ids.contains(session_id))
                .filter(|session_id| seen_tab_ids.insert(session_id.0))
                .collect::<Vec<_>>();
            if tabs.is_empty() || group_id.0 == 0 {
                return None;
            }
            let active_session = json_u64_field(object, "activeSessionId")
                .map(CommandSessionId)
                .filter(|session_id| tabs.contains(session_id))
                .unwrap_or(tabs[0]);
            Some(CommandPaneNode::Leaf(CommandPaneLeaf {
                group_id,
                tab_group: CommandPaneTabGroup {
                    tabs: tabs
                        .into_iter()
                        .map(|session_id| CommandPaneTab { session_id })
                        .collect(),
                    active_session,
                },
            }))
        }
        "split" => {
            let split_id = CommandPaneSplitId(json_u64_field(object, "splitId")?);
            if split_id.0 == 0 {
                return None;
            }
            let first = command_pane_node_from_shell_state(object.get("first")?, session_ids);
            let second = command_pane_node_from_shell_state(object.get("second")?, session_ids);
            /*
            CDXC:GPUICommandPaneRestore 2026-06-27-04:15:
            Native command-panel split restore prunes children that normalize to no valid tabs and collapses a one-child split to the remaining layout. Preserve that repair behavior so stale command leaf data cannot discard a sibling command group that still has valid restored sessions.
            */
            match (first, second) {
                (Some(first), Some(second)) => Some(CommandPaneNode::Split(CommandPaneSplit {
                    id: split_id,
                    axis: json_string_field(object, "axis")
                        .and_then(WorkspaceSplitAxis::from_slug)
                        .unwrap_or(WorkspaceSplitAxis::Horizontal),
                    ratio: json_f32_field(object, "ratio")
                        .map(workspace_split_ratio)
                        .unwrap_or(0.5),
                    first: Box::new(first),
                    second: Box::new(second),
                })),
                (Some(node), None) | (None, Some(node)) => Some(node),
                (None, None) => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn collect_command_leaf_ids(
    node: &CommandPaneNode,
    group_ids: &mut Vec<CommandPaneGroupId>,
) {
    match node {
        CommandPaneNode::Leaf(leaf) => {
            if !leaf.tab_group.tabs.is_empty() {
                group_ids.push(leaf.group_id);
            }
        }
        CommandPaneNode::Split(split) => {
            collect_command_leaf_ids(&split.first, group_ids);
            collect_command_leaf_ids(&split.second, group_ids);
        }
    }
}

pub(crate) fn collect_command_node_session_ids(
    node: &CommandPaneNode,
    session_ids: &mut Vec<CommandSessionId>,
) {
    match node {
        CommandPaneNode::Leaf(leaf) => {
            session_ids.extend(leaf.tab_group.tabs.iter().map(|tab| tab.session_id));
        }
        CommandPaneNode::Split(split) => {
            collect_command_node_session_ids(&split.first, session_ids);
            collect_command_node_session_ids(&split.second, session_ids);
        }
    }
}

pub(crate) fn collect_command_split_ids(
    node: &CommandPaneNode,
    split_ids: &mut Vec<CommandPaneSplitId>,
) {
    match node {
        CommandPaneNode::Leaf(_) => {}
        CommandPaneNode::Split(split) => {
            split_ids.push(split.id);
            collect_command_split_ids(&split.first, split_ids);
            collect_command_split_ids(&split.second, split_ids);
        }
    }
}
