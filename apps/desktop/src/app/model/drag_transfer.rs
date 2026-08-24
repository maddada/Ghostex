// C1 wave-3 re-cluster: cross-pane placeholder transfer between the workspace and command pane during a tab drag, and its rollback snapshot, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn workspace_tab_insertion_index(
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
    tab_index: usize,
) -> usize {
    let relative_x = position.x.as_f32() - bounds.origin.x.as_f32();
    let half_width = bounds.size.width.as_f32() / 2.0;

    if relative_x > half_width {
        tab_index + 1
    } else {
        tab_index
    }
}

pub(crate) fn workspace_pane_body_drop_zone(
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> WorkspaceDropZone {
    /*
    CDXC:GPUIWorkspaceDragDrop 2026-07-08:
    Agents pane-body drops mirror native `paneDropPlacement`: classify against local pane coordinates with 24% edge bands, horizontal edges winning corners by check order, and top/bottom mapped to GPUI's visual coordinate system.
    */
    let width = bounds.size.width.as_f32();
    let height = bounds.size.height.as_f32();
    if !width.is_finite() || !height.is_finite() || width <= 1.0 || height <= 1.0 {
        return WorkspaceDropZone::Center;
    }

    let local_x = (position.x.as_f32() - bounds.origin.x.as_f32()) / width;
    let local_y = (position.y.as_f32() - bounds.origin.y.as_f32()) / height;
    if local_x <= WORKSPACE_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Left;
    }
    if local_x >= 1.0 - WORKSPACE_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Right;
    }
    if local_y <= WORKSPACE_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Top;
    }
    if local_y >= 1.0 - WORKSPACE_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Bottom;
    }
    WorkspaceDropZone::Center
}

pub(crate) fn command_pane_body_drop_zone(
    bounds: Bounds<Pixels>,
    position: gpui::Point<Pixels>,
) -> WorkspaceDropZone {
    /*
    CDXC:GPUICommandPane 2026-06-22-06:13:
    Command-pane drag/drop supports horizontal edge split intent while enforcing command-only layout rules. Left and right edge zones split command tab groups horizontally; center, top, and bottom all group into the target command group so vertical command splits are not created.

    CDXC:GPUICommandPaneDragDrop 2026-06-25-19:44:
    Native `commandPaneDropPlacement` uses only command-body horizontal geometry: widths at or below one pixel return center, local X <= 24% splits left, local X >= 76% splits right, and every vertical position stays a center group drop. Do not route through workspace pane drop math here because its min/max edge-band clamp and top/bottom competition are workspace-only behavior.
    */
    let width = bounds.size.width.as_f32();
    if !width.is_finite() || width <= 1.0 {
        return WorkspaceDropZone::Center;
    }

    let local_x = (position.x.as_f32() - bounds.origin.x.as_f32()) / width;
    if local_x <= COMMAND_PANE_BODY_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Left;
    }
    if local_x >= 1.0 - COMMAND_PANE_BODY_DROP_EDGE_BAND_FRACTION {
        return WorkspaceDropZone::Right;
    }
    WorkspaceDropZone::Center
}

pub(crate) fn transfer_command_placeholder_to_workspace(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_group_id: CommandPaneGroupId,
    source_session_id: CommandSessionId,
    target_pane_id: WorkspacePaneId,
    zone: WorkspaceDropZone,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    transfer_command_placeholder_to_workspace_with_source_close(
        agents_workspace,
        command_pane,
        source_group_id,
        source_session_id,
        target_pane_id,
        zone,
        |command_pane, group_id, session_id| command_pane.close_session(group_id, session_id),
    )
}

pub(crate) fn transfer_command_placeholder_to_workspace_with_source_close(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_group_id: CommandPaneGroupId,
    source_session_id: CommandSessionId,
    target_pane_id: WorkspacePaneId,
    zone: WorkspaceDropZone,
    mut close_source: impl FnMut(&mut CommandPaneModel, CommandPaneGroupId, CommandSessionId) -> bool,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-06-22-15:55:
    Command-to-Agents transfer is transactional at the placeholder model boundary. Validate the source command tab, insert the selected Mounting Agents shell session first, then remove the command session through normal command close semantics so the last command tab collapses the command pane only after Agents insertion succeeds.

    CDXC:GPUICommandWorkspaceTransfer 2026-06-25-19:28:
    Command-to-Agents body drops must roll back the newly inserted Mounting Agents placeholder if command-session removal fails after insertion. Restore the previous Agents focus and active tab while preserving the command source, and keep the transfer title-only with no command text, stdout/stderr, paths, process state, libghostty state, or terminal content crossing surfaces.
    */
    let source_has_session = command_pane
        .find_leaf(source_group_id)
        .is_some_and(|leaf| leaf.tab_group.has_session(source_session_id));
    if !source_has_session {
        return None;
    }

    let title = command_pane.session(source_session_id)?.title.clone();
    let focused_pane_before = agents_workspace.focused_pane;
    let focus_mode_pane_before = agents_workspace.focus_mode_pane;
    let target_active_before = agents_workspace
        .find_leaf(target_pane_id)
        .and_then(|leaf| leaf.tab_group.active_session_id());
    let inserted =
        agents_workspace.add_placeholder_session_from_command_title(target_pane_id, title, zone)?;

    if close_source(command_pane, source_group_id, source_session_id) {
        Some(inserted)
    } else {
        rollback_command_to_workspace_insert(
            agents_workspace,
            inserted.0,
            inserted.1,
            target_pane_id,
            target_active_before,
            focused_pane_before,
            focus_mode_pane_before,
        );
        None
    }
}

pub(crate) fn transfer_command_placeholder_to_workspace_tab_strip(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_group_id: CommandPaneGroupId,
    source_session_id: CommandSessionId,
    target_pane_id: WorkspacePaneId,
    insertion_index: usize,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    transfer_command_placeholder_to_workspace_tab_strip_with_source_close(
        agents_workspace,
        command_pane,
        source_group_id,
        source_session_id,
        target_pane_id,
        insertion_index,
        |command_pane, group_id, session_id| command_pane.close_session(group_id, session_id),
    )
}

pub(crate) fn transfer_command_placeholder_to_workspace_tab_strip_with_source_close(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_group_id: CommandPaneGroupId,
    source_session_id: CommandSessionId,
    target_pane_id: WorkspacePaneId,
    insertion_index: usize,
    mut close_source: impl FnMut(&mut CommandPaneModel, CommandPaneGroupId, CommandSessionId) -> bool,
) -> Option<(WorkspacePaneId, TerminalSessionId)> {
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-06-22-16:04:
    Tab-strip command-to-Agents drops must be transactional at the shell model boundary: validate the command source, insert the selected Mounting Agents shell session at the requested tab-strip index first, then remove the command source through existing close semantics so final command sessions collapse only after the Agents tab exists. If command close fails after insertion, remove the inserted Agents session instead of leaving a duplicate shell tab.

    CDXC:GPUICommandWorkspaceTransfer 2026-06-25-19:28:
    Command-to-Agents tab-strip rollback must preserve the exact pre-transfer Agents tab order, active tab, and focus when command removal fails after a successful index insert. The inserted placeholder is the only rolled-back Agents state, and the command source remains a command-pane placeholder without moving command text, stdout/stderr, paths, process state, libghostty state, or terminal content.
    */
    let source_has_session = command_pane
        .find_leaf(source_group_id)
        .is_some_and(|leaf| leaf.tab_group.has_session(source_session_id));
    if !source_has_session {
        return None;
    }

    let title = command_pane.session(source_session_id)?.title.clone();
    let focused_pane_before = agents_workspace.focused_pane;
    let focus_mode_pane_before = agents_workspace.focus_mode_pane;
    let target_active_before = agents_workspace
        .find_leaf(target_pane_id)
        .and_then(|leaf| leaf.tab_group.active_session_id());
    let inserted = agents_workspace.insert_placeholder_session_from_command_title_at(
        target_pane_id,
        insertion_index,
        title,
    )?;

    if close_source(command_pane, source_group_id, source_session_id) {
        Some(inserted)
    } else {
        rollback_command_to_workspace_insert(
            agents_workspace,
            inserted.0,
            inserted.1,
            target_pane_id,
            target_active_before,
            focused_pane_before,
            focus_mode_pane_before,
        );
        None
    }
}

pub(crate) fn rollback_command_to_workspace_insert(
    agents_workspace: &mut WorkspaceModel,
    inserted_pane_id: WorkspacePaneId,
    inserted_session_id: TerminalSessionId,
    target_pane_id: WorkspacePaneId,
    target_active_before: Option<TerminalSessionId>,
    focused_pane_before: WorkspacePaneId,
    focus_mode_pane_before: Option<WorkspacePaneId>,
) {
    /*
    CDXC:GPUICommandWorkspaceTransfer 2026-06-25-19:28:
    A failed command-source removal must undo only the just-created Agents placeholder. Bypass the normal final-tab close guard only for this rollback path, then restore the previous Agents selection/focus so a failed cross-surface move leaves no duplicate shell tab and no implied runtime/content transfer.
    */
    if !agents_workspace.close_tab(inserted_pane_id, inserted_session_id)
        && let Some((_tab, source_is_empty)) =
            agents_workspace.remove_tab_for_move(inserted_pane_id, inserted_session_id)
    {
        agents_workspace
            .terminal_sessions
            .retain(|session| session.id != inserted_session_id);
        if source_is_empty {
            agents_workspace.collapse_empty_leaf(inserted_pane_id);
        }
        agents_workspace.normalize_workspace_tree();
    }

    if let Some(active_before) = target_active_before
        && let Some(target_leaf) = agents_workspace.find_leaf_mut(target_pane_id)
        && target_leaf.tab_group.has_session(active_before)
    {
        target_leaf.tab_group.active_tab = active_before;
    }

    if agents_workspace.find_leaf(focused_pane_before).is_some() {
        agents_workspace.focused_pane = focused_pane_before;
    }
    agents_workspace.focus_mode_pane =
        focus_mode_pane_before.filter(|pane_id| agents_workspace.find_leaf(*pane_id).is_some());
    agents_workspace.normalize_workspace_tree();
}

pub(crate) struct CommandPaneTransferRollbackSnapshot {
    pub(crate) mode: CommandPaneMode,
    pub(crate) last_expanded_mode: CommandPaneMode,
    pub(crate) focused_group: CommandPaneGroupId,
    pub(crate) focus_mode_group: Option<CommandPaneGroupId>,
    pub(crate) active_sessions: Vec<(CommandPaneGroupId, CommandSessionId)>,
}

pub(crate) fn command_pane_transfer_rollback_snapshot(
    command_pane: &CommandPaneModel,
) -> CommandPaneTransferRollbackSnapshot {
    CommandPaneTransferRollbackSnapshot {
        mode: command_pane.mode,
        last_expanded_mode: command_pane.last_expanded_mode,
        focused_group: command_pane.focused_group,
        focus_mode_group: command_pane.focus_mode_group,
        active_sessions: command_pane
            .group_order()
            .into_iter()
            .filter_map(|group_id| {
                command_pane
                    .find_leaf(group_id)
                    .and_then(|leaf| leaf.tab_group.active_session_id())
                    .map(|active_session_id| (group_id, active_session_id))
            })
            .collect(),
    }
}

pub(crate) fn rollback_workspace_to_command_insert(
    command_pane: &mut CommandPaneModel,
    inserted_group_id: CommandPaneGroupId,
    inserted_session_id: CommandSessionId,
    snapshot: CommandPaneTransferRollbackSnapshot,
) {
    /*
    CDXC:GPUICommandPaneDragDrop 2026-06-25-19:45:
    Agents-to-command rollback must undo only the just-created command placeholder after Agents source removal fails. Restore command-pane mode, focused group, and per-group active tabs so body and tab-strip drops leave command group order/session ids intact while preserving all Agents runtime/content state on the Agents side.

    CDXC:GPUICommandPaneDragDrop 2026-06-26-04:43:
    Rollback must also restore the previous command Focus group after a placeholder insert activates or clears Focus, because failed Agents source close leaves the prior command-pane visibility contract in force.
    */
    let _ = command_pane.close_session(inserted_group_id, inserted_session_id);

    for (group_id, active_session_id) in snapshot.active_sessions {
        if let Some(leaf) = command_pane.find_leaf_mut(group_id)
            && leaf.tab_group.has_session(active_session_id)
        {
            leaf.tab_group.active_session = active_session_id;
        }
    }

    if command_pane.find_leaf(snapshot.focused_group).is_some() {
        command_pane.focused_group = snapshot.focused_group;
    }
    command_pane.mode = snapshot.mode;
    command_pane.last_expanded_mode = snapshot.last_expanded_mode;
    command_pane.focus_mode_group = snapshot.focus_mode_group;
    command_pane.clear_focus_mode_if_invalid();
}

pub(crate) fn transfer_workspace_placeholder_to_command_pane(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_pane_id: WorkspacePaneId,
    source_session_id: TerminalSessionId,
    target_group_id: CommandPaneGroupId,
    zone: WorkspaceDropZone,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    transfer_workspace_placeholder_to_command_pane_with_source_close(
        agents_workspace,
        command_pane,
        source_pane_id,
        source_session_id,
        target_group_id,
        zone,
        |workspace, pane_id, session_id| workspace.close_tab(pane_id, session_id),
    )
}

pub(crate) fn transfer_workspace_placeholder_to_command_pane_with_source_close(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_pane_id: WorkspacePaneId,
    source_session_id: TerminalSessionId,
    target_group_id: CommandPaneGroupId,
    zone: WorkspaceDropZone,
    mut close_source: impl FnMut(&mut WorkspaceModel, WorkspacePaneId, TerminalSessionId) -> bool,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandPaneDragDrop 2026-06-25-19:45:
    Agents-to-command body drops are transactional at the placeholder boundary. Preflight the Agents final-root transfer guard, insert only a command title placeholder, then close the Agents source; if that close fails, remove only the inserted command placeholder and restore prior command-pane selection/focus without moving command text, terminal content, paths, process state, libghostty state, stdout/stderr, or Agents runtime content.
    */
    if !agents_workspace.can_transfer_tab_to_command_pane(source_pane_id, source_session_id) {
        return None;
    }

    let title = agents_workspace.session(source_session_id)?.title.clone();
    let rollback_snapshot = command_pane_transfer_rollback_snapshot(command_pane);
    let inserted =
        command_pane.add_placeholder_session_from_workspace_title(target_group_id, title, zone)?;

    if close_source(agents_workspace, source_pane_id, source_session_id) {
        Some(inserted)
    } else {
        rollback_workspace_to_command_insert(
            command_pane,
            inserted.0,
            inserted.1,
            rollback_snapshot,
        );
        None
    }
}

pub(crate) fn transfer_workspace_placeholder_to_command_tab_strip(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_pane_id: WorkspacePaneId,
    source_session_id: TerminalSessionId,
    target_group_id: CommandPaneGroupId,
    insertion_index: usize,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    transfer_workspace_placeholder_to_command_tab_strip_with_source_close(
        agents_workspace,
        command_pane,
        source_pane_id,
        source_session_id,
        target_group_id,
        insertion_index,
        |workspace, pane_id, session_id| workspace.close_tab(pane_id, session_id),
    )
}

pub(crate) fn transfer_workspace_placeholder_to_command_tab_strip_with_source_close(
    agents_workspace: &mut WorkspaceModel,
    command_pane: &mut CommandPaneModel,
    source_pane_id: WorkspacePaneId,
    source_session_id: TerminalSessionId,
    target_group_id: CommandPaneGroupId,
    insertion_index: usize,
    mut close_source: impl FnMut(&mut WorkspaceModel, WorkspacePaneId, TerminalSessionId) -> bool,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandPaneDragDrop 2026-06-22-16:18:
    Agents-to-command tab-strip transfer is transactional at the placeholder shell boundary. Validate the visible Agents source and final-root transfer guard first, insert the command placeholder at the requested index, then close the Agents source; if that final close fails, remove the inserted command placeholder so the shell does not duplicate the tab.

    CDXC:GPUICommandPaneDragDrop 2026-06-25-19:45:
    Failed Agents source close after a command tab-strip insert must restore the prior command-pane mode, focused group, and active tab for every existing command group. The inserted command placeholder is the only rolled-back command state; Agents runtime/content state remains untouched on the Agents side.

    CDXC:GPUICommandPaneDragDrop 2026-06-26-04:43:
    Successful tab-strip insertion relies on the command model insertion path to clear Focus when the target group is outside current Focus; failed source close then restores the rollback snapshot, including `focus_mode_group`.
    */
    if !agents_workspace.can_transfer_tab_to_command_pane(source_pane_id, source_session_id) {
        return None;
    }

    let title = agents_workspace.session(source_session_id)?.title.clone();
    let rollback_snapshot = command_pane_transfer_rollback_snapshot(command_pane);
    let inserted = command_pane.insert_placeholder_session_from_workspace_title_at(
        target_group_id,
        insertion_index,
        title,
    )?;

    if close_source(agents_workspace, source_pane_id, source_session_id) {
        Some(inserted)
    } else {
        rollback_workspace_to_command_insert(
            command_pane,
            inserted.0,
            inserted.1,
            rollback_snapshot,
        );
        None
    }
}
