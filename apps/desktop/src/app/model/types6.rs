// C1 wave-3 extraction: a chunk (6/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


pub(crate) fn json_number_f32(value: f32) -> serde_json::Value {
    serde_json::Value::Number(
        serde_json::Number::from_f64(value as f64).unwrap_or_else(|| serde_json::Number::from(0)),
    )
}


pub(crate) fn has_duplicate_u64(values: &[u64]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values.iter().skip(index + 1).any(|other| other == value))
}


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


pub(crate) fn split_resize_content_span(
    child_bounds: &[Bounds<Pixels>],
    axis: WorkspaceSplitAxis,
) -> Option<f32> {
    let first = child_bounds.first()?;
    let second = child_bounds.get(2)?;
    let span = match axis {
        WorkspaceSplitAxis::Horizontal => first.size.width.as_f32() + second.size.width.as_f32(),
        WorkspaceSplitAxis::Vertical => first.size.height.as_f32() + second.size.height.as_f32(),
    };

    (span > 1.0).then_some(span)
}


pub(crate) fn split_pane_resize_minimum_for_axis(axis: WorkspaceSplitAxis) -> f32 {
    match axis {
        WorkspaceSplitAxis::Horizontal => PANE_RESIZE_MINIMUM_WIDTH,
        WorkspaceSplitAxis::Vertical => PANE_RESIZE_MINIMUM_HEIGHT,
    }
}


pub(crate) fn split_drag_ratio_bounds_from_minimums(
    minimum_before: f32,
    minimum_after: f32,
    content_span: f32,
) -> Option<(f32, f32)> {
    let span = content_span.max(1.0);
    let lower = (minimum_before / span).max(0.1);
    let upper = (1.0 - minimum_after / span).min(0.9);
    (lower <= upper).then_some((lower, upper))
}


pub(crate) fn workspace_node_axis_pane_count(node: &WorkspaceNode, axis: WorkspaceSplitAxis) -> usize {
    match node {
        WorkspaceNode::Leaf(_) => 1,
        WorkspaceNode::Split(split) => {
            let first = workspace_node_axis_pane_count(&split.first, axis);
            let second = workspace_node_axis_pane_count(&split.second, axis);
            if split.axis == axis {
                first + second
            } else {
                first.max(second)
            }
        }
    }
}


pub(crate) fn command_node_axis_pane_count(node: &CommandPaneNode, axis: WorkspaceSplitAxis) -> usize {
    match node {
        CommandPaneNode::Leaf(_) => 1,
        CommandPaneNode::Split(split) => {
            let first = command_node_axis_pane_count(&split.first, axis);
            let second = command_node_axis_pane_count(&split.second, axis);
            if split.axis == axis {
                first + second
            } else {
                first.max(second)
            }
        }
    }
}


pub(crate) fn browser_node_axis_pane_count(node: &BrowserNode, axis: WorkspaceSplitAxis) -> usize {
    match node {
        BrowserNode::Leaf(_) => 1,
        BrowserNode::Split(split) => {
            let first = browser_node_axis_pane_count(&split.first, axis);
            let second = browser_node_axis_pane_count(&split.second, axis);
            if split.axis == axis {
                first + second
            } else {
                first.max(second)
            }
        }
    }
}


pub(crate) fn split_resize_event_position(axis: WorkspaceSplitAxis, position: gpui::Point<Pixels>) -> f32 {
    match axis {
        WorkspaceSplitAxis::Horizontal => position.x.as_f32(),
        WorkspaceSplitAxis::Vertical => position.y.as_f32(),
    }
}


pub(crate) fn pane_focus_bounds_from_child_bounds(child_bounds: &[Bounds<Pixels>]) -> Option<Bounds<Pixels>> {
    let mut bounds_iter = child_bounds
        .iter()
        .filter(|bounds| bounds.size.width.as_f32() > 1.0 && bounds.size.height.as_f32() > 1.0);
    let first = bounds_iter.next()?;
    let mut left = first.origin.x.as_f32();
    let mut top = first.origin.y.as_f32();
    let mut right = left + first.size.width.as_f32();
    let mut bottom = top + first.size.height.as_f32();

    for bounds in bounds_iter {
        let child_left = bounds.origin.x.as_f32();
        let child_top = bounds.origin.y.as_f32();
        left = left.min(child_left);
        top = top.min(child_top);
        right = right.max(child_left + bounds.size.width.as_f32());
        bottom = bottom.max(child_top + bounds.size.height.as_f32());
    }

    let width = right - left;
    let height = bottom - top;
    if width <= 1.0 || height <= 1.0 {
        return None;
    }

    Some(Bounds::from_corners(
        gpui::point(px(left), px(top)),
        gpui::point(px(right), px(bottom)),
    ))
}


pub(crate) fn focus_bounds_center(bounds: Bounds<Pixels>) -> (f32, f32) {
    (
        bounds.origin.x.as_f32() + bounds.size.width.as_f32() / 2.0,
        bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0,
    )
}


pub(crate) fn spatial_focus_score(
    current_bounds: Bounds<Pixels>,
    candidate_bounds: Bounds<Pixels>,
    direction: WorkspaceFocusDirection,
) -> Option<(f32, f32, f32)> {
    let (current_x, current_y) = focus_bounds_center(current_bounds);
    let (candidate_x, candidate_y) = focus_bounds_center(candidate_bounds);
    let delta_x = candidate_x - current_x;
    let delta_y = candidate_y - current_y;

    let (primary_distance, secondary_distance) = match direction {
        WorkspaceFocusDirection::Left if delta_x < -SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (-delta_x, delta_y.abs())
        }
        WorkspaceFocusDirection::Right if delta_x > SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (delta_x, delta_y.abs())
        }
        WorkspaceFocusDirection::Up if delta_y < -SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (-delta_y, delta_x.abs())
        }
        WorkspaceFocusDirection::Down if delta_y > SPATIAL_FOCUS_HALF_PLANE_TOLERANCE => {
            (delta_y, delta_x.abs())
        }
        _ => return None,
    };
    let squared_distance = delta_x.mul_add(delta_x, delta_y * delta_y);

    Some((primary_distance, secondary_distance, squared_distance))
}


pub(crate) fn nearest_spatial_focus_target(
    current_bounds: Bounds<Pixels>,
    current_target: SpatialFocusTarget,
    direction: WorkspaceFocusDirection,
    candidates: &[FocusCandidate],
) -> Option<SpatialFocusTarget> {
    candidates
        .iter()
        .filter(|candidate| candidate.target != current_target)
        .filter_map(|candidate| {
            spatial_focus_score(current_bounds, candidate.bounds, direction).map(
                |(primary_distance, secondary_distance, squared_distance)| {
                    (
                        candidate.target,
                        candidate.order,
                        primary_distance,
                        secondary_distance,
                        squared_distance,
                    )
                },
            )
        })
        .min_by(|left, right| {
            left.2
                .total_cmp(&right.2)
                .then_with(|| left.3.total_cmp(&right.3))
                .then_with(|| left.4.total_cmp(&right.4))
                .then_with(|| left.1.cmp(&right.1))
        })
        .map(|(target, _, _, _, _)| target)
}


pub(crate) fn render_order_focus_target(
    targets: &[SpatialFocusTarget],
    current_target: Option<SpatialFocusTarget>,
    direction: WorkspaceFocusDirection,
) -> Option<SpatialFocusTarget> {
    if targets.is_empty() {
        return None;
    }

    if let Some(current_target) = current_target {
        let current_index = targets
            .iter()
            .position(|target| *target == current_target)
            .unwrap_or(0);
        if direction.uses_previous_order_fallback() {
            current_index
                .checked_sub(1)
                .and_then(|index| targets.get(index).copied())
        } else {
            targets.get(current_index + 1).copied()
        }
    } else if direction.uses_previous_order_fallback() {
        targets.last().copied()
    } else {
        targets.first().copied()
    }
}


pub(crate) fn workspace_render_order_focus_targets(
    pane_ids: Vec<WorkspacePaneId>,
    command_is_expanded: bool,
    command_has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    let mut targets = pane_ids
        .into_iter()
        .map(SpatialFocusTarget::AgentsPane)
        .collect::<Vec<_>>();
    /*
    CDXC:GPUICommandTabKeyboardParity 2026-06-25-23:35:
    Render-order keyboard fallback must target the same live expanded command groups as spatial focus. Do not append a generic command-pane target for collapsed strips, empty panels, or stored sessions that no longer belong to a rendered command group.
    */
    targets.extend(command_pane_render_order_focus_targets(
        command_is_expanded,
        command_has_sessions,
        command_group_ids,
    ));
    targets
}


pub(crate) fn command_pane_render_order_focus_targets(
    is_expanded: bool,
    has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    if !is_expanded || !has_sessions {
        return Vec::new();
    }

    command_group_ids
        .into_iter()
        .map(SpatialFocusTarget::CommandPaneGroup)
        .collect()
}


pub(crate) fn project_editor_render_order_focus_targets_for_state(
    mode: TitlebarMode,
    left_companion_visible: bool,
    browser_is_awake: bool,
    browser_pane_ids: Vec<BrowserPaneId>,
    command_is_expanded: bool,
    command_has_sessions: bool,
    command_group_ids: Vec<CommandPaneGroupId>,
) -> Vec<SpatialFocusTarget> {
    let mut targets = Vec::new();
    if left_companion_visible {
        targets.push(SpatialFocusTarget::ProjectEditorCompanion(mode));
    }

    if mode == TitlebarMode::Browser && browser_is_awake {
        if browser_pane_ids.is_empty() {
            targets.push(SpatialFocusTarget::ProjectEditorSurface(mode));
        } else {
            targets.extend(
                browser_pane_ids
                    .into_iter()
                    .map(SpatialFocusTarget::BrowserPane),
            );
        }
    } else {
        targets.push(SpatialFocusTarget::ProjectEditorSurface(mode));
    }

    targets.extend(command_pane_render_order_focus_targets(
        command_is_expanded,
        command_has_sessions,
        command_group_ids,
    ));

    targets
}


pub(crate) fn workspace_split_ratio(ratio: f32) -> f32 {
    ratio.clamp(0.1, 0.9)
}


pub(crate) fn command_pane_height_ratio(ratio: f32) -> f32 {
    ratio.clamp(COMMAND_PANE_MIN_HEIGHT_RATIO, COMMAND_PANE_MAX_HEIGHT_RATIO)
}


pub(crate) fn command_pane_width_ratio(ratio: f32) -> f32 {
    ratio.clamp(COMMAND_PANE_MIN_WIDTH_RATIO, COMMAND_PANE_MAX_WIDTH_RATIO)
}


pub(crate) fn command_pane_default_height_px_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> f32 {
    /*
    CDXC:GPUICommandPane 2026-06-25-11:29:
    The GPUI command-pane default-height reader mirrors shared Settings normalization: accept only JSON numbers, round to whole pixels, clamp to 40px-600px, and use the 125px product default for missing or malformed values. This keeps app startup, missing shell-state restore, and resize-rail reset aligned with the macOS command pane without persisting a second setting.
    */
    settings
        .object()
        .get("commandsPanelDefaultHeightPx")
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.round() as f32)
        .unwrap_or(COMMAND_PANE_DEFAULT_HEIGHT_PX)
        .clamp(
            COMMAND_PANE_MIN_DEFAULT_HEIGHT_PX,
            COMMAND_PANE_MAX_DEFAULT_HEIGHT_PX,
        )
}


pub(crate) fn gpui_click_to_wake_sleeping_sessions_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> bool {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-23:24:
    GPUI Agents and command tab selection must honor the shared macOS `clickToWakeSleepingSessions` setting. Missing or malformed settings default to true, where tab selection keeps a sleeping placeholder cold; strict false makes selecting a sleeping tab wake it immediately.
    */
    settings
        .object()
        .get("clickToWakeSleepingSessions")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}


pub(crate) fn command_pane_click_to_wake_sleeping_sessions_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> bool {
    gpui_click_to_wake_sleeping_sessions_from_shared_settings(settings)
}


pub(crate) fn command_pane_sleeping_tab_selection_wake_target(
    command_pane: &CommandPaneModel,
    group_id: CommandPaneGroupId,
    session_id: CommandSessionId,
    click_to_wake_enabled: bool,
) -> Option<(CommandPaneGroupId, CommandSessionId)> {
    /*
    CDXC:GPUICommandTabWake 2026-06-27-04:25:
    Command-tab selection has exactly one eager wake case: strict `clickToWakeSleepingSessions: false` on a live sleeping tab in the clicked command group. Default click-to-wake keeps the selected sleeping tab parked so the visible body placeholder owns the later wake affordance; stale group/session ids must not fall back to focused command state.
    */
    if click_to_wake_enabled {
        return None;
    }
    let leaf = command_pane.find_leaf(group_id)?;
    if !leaf.tab_group.has_session(session_id) {
        return None;
    }
    command_pane
        .session(session_id)
        .is_some_and(|session| session.is_sleeping)
        .then_some((group_id, session_id))
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label(
    active_session_is_sleeping: bool,
    click_to_wake_enabled: bool,
) -> Option<&'static str> {
    (active_session_is_sleeping && click_to_wake_enabled)
        .then_some(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL)
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_is_private_data_safe(label: &str) -> bool {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    The paint path may render only the fixed native wake affordance. Keep this guard at the writer boundary so future canvas callers cannot paint command text, session titles, paths, URLs, tokens, or terminal content into the sleeping command body.
    */
    label == COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelFrame {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}


#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelLine {
    pub(crate) text: String,
    pub(crate) measured_width: f32,
}


pub(crate) struct CommandPaneSleepingPlaceholderWakeLabelPaintState {
    pub(crate) frame: CommandPaneSleepingPlaceholderWakeLabelFrame,
    pub(crate) label_lines: Vec<gpui::ShapedLine>,
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_max_size(
    body_width: f32,
    body_height: f32,
) -> Option<(f32, f32)> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Match native body-bound visibility exactly: the sleeping wake label has no geometry when the body cannot provide positive `body.width - 8` and `body.height - 16` label limits.
    */
    if !body_width.is_finite() || !body_height.is_finite() {
        return None;
    }

    let max_label_width =
        body_width - (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_HORIZONTAL_PADDING * 2.0);
    let max_label_height =
        body_height - (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_VERTICAL_PADDING * 2.0);
    (max_label_width > 0.0 && max_label_height > 0.0).then_some((max_label_width, max_label_height))
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_frame(
    body_width: f32,
    body_height: f32,
    measured_width: f32,
    measured_height: f32,
) -> Option<CommandPaneSleepingPlaceholderWakeLabelFrame> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Native centers a measured label frame inside the command body after applying the exact AppKit clamps: width is ceil(measured width)+8 with a 1px floor, height is ceil(measured height) with an 18px floor, and both are clamped to the body-derived max label size.
    */
    if !measured_width.is_finite()
        || !measured_height.is_finite()
        || measured_width < 0.0
        || measured_height < 0.0
    {
        return None;
    }

    let (max_label_width, max_label_height) =
        command_pane_sleeping_placeholder_wake_label_max_size(body_width, body_height)?;
    let label_width = (measured_width.ceil()
        + (COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_HORIZONTAL_PADDING * 2.0))
        .max(1.0)
        .min(max_label_width);
    let label_height = measured_height
        .ceil()
        .max(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT)
        .min(max_label_height);

    Some(CommandPaneSleepingPlaceholderWakeLabelFrame {
        x: (body_width - label_width) / 2.0,
        y: (body_height - label_height) / 2.0,
        width: label_width,
        height: label_height,
    })
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_char_wrap_lines(
    label: &str,
    max_label_width: f32,
    mut measure: impl FnMut(&str) -> Option<f32>,
) -> Option<Vec<CommandPaneSleepingPlaceholderWakeLabelLine>> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    AppKit uses character wrapping for the wake label, not word wrapping. Split the fixed label by measured character fit so narrow command bodies break within words exactly because the body bounds require it.
    */
    if !command_pane_sleeping_placeholder_wake_label_is_private_data_safe(label)
        || !max_label_width.is_finite()
        || max_label_width <= 0.0
    {
        return None;
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0.0;

    for character in label.chars() {
        let mut candidate = current.clone();
        candidate.push(character);
        let candidate_width = measure(&candidate)?;
        if !candidate_width.is_finite() || candidate_width < 0.0 {
            return None;
        }

        if !current.is_empty() && candidate_width > max_label_width {
            lines.push(CommandPaneSleepingPlaceholderWakeLabelLine {
                text: current,
                measured_width: current_width,
            });
            current = character.to_string();
            current_width = measure(&current)?;
            if !current_width.is_finite() || current_width < 0.0 {
                return None;
            }
        } else {
            current = candidate;
            current_width = candidate_width;
        }
    }

    if !current.is_empty() {
        lines.push(CommandPaneSleepingPlaceholderWakeLabelLine {
            text: current,
            measured_width: current_width,
        });
    }

    (!lines.is_empty()).then_some(lines)
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_text_run(len: usize) -> gpui::TextRun {
    let mut font = gpui::font(".SystemUIFont");
    font.weight = FontWeight::MEDIUM;
    gpui::TextRun {
        len,
        font,
        color: command_pane_sleeping_placeholder_wake_label_color(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_shape_line(
    label: &str,
    window: &mut Window,
) -> gpui::ShapedLine {
    window.text_system().shape_line(
        gpui::SharedString::from(label.to_string()),
        px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_FONT_SIZE),
        &[command_pane_sleeping_placeholder_wake_label_text_run(
            label.len(),
        )],
        None,
    )
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_prepaint(
    bounds: Bounds<Pixels>,
    label: &'static str,
    window: &mut Window,
) -> Option<CommandPaneSleepingPlaceholderWakeLabelPaintState> {
    /*
    CDXC:GPUICommandSleepingPlaceholder 2026-06-27-00:22:
    Runtime wake-label layout must come from this paint pass's exact command-body bounds. Measure the fixed label with 13px medium system text, character-wrap within `body.width - 8`, clamp the centered frame to native limits, and produce no paint state when native would hide it.
    */
    if !command_pane_sleeping_placeholder_wake_label_is_private_data_safe(label) {
        return None;
    }

    let body_width = bounds.size.width.as_f32();
    let body_height = bounds.size.height.as_f32();
    let (max_label_width, _) =
        command_pane_sleeping_placeholder_wake_label_max_size(body_width, body_height)?;
    let wrapped_lines = command_pane_sleeping_placeholder_wake_label_char_wrap_lines(
        label,
        max_label_width,
        |line| {
            Some(
                command_pane_sleeping_placeholder_wake_label_shape_line(line, window)
                    .width()
                    .as_f32(),
            )
        },
    )?;
    let measured_width = wrapped_lines
        .iter()
        .map(|line| line.measured_width)
        .fold(0.0, f32::max);
    let measured_height =
        wrapped_lines.len() as f32 * COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT;
    let frame = command_pane_sleeping_placeholder_wake_label_frame(
        body_width,
        body_height,
        measured_width,
        measured_height,
    )?;
    let label_lines = wrapped_lines
        .iter()
        .map(|line| command_pane_sleeping_placeholder_wake_label_shape_line(&line.text, window))
        .collect();

    Some(CommandPaneSleepingPlaceholderWakeLabelPaintState { frame, label_lines })
}


pub(crate) fn command_pane_sleeping_placeholder_wake_label_paint(
    body_bounds: Bounds<Pixels>,
    paint_state: CommandPaneSleepingPlaceholderWakeLabelPaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let frame = paint_state.frame;
    let label_bounds = Bounds::new(
        gpui::point(
            body_bounds.left() + px(frame.x),
            body_bounds.top() + px(frame.y),
        ),
        size(px(frame.width), px(frame.height)),
    );
    let measured_height = paint_state.label_lines.len() as f32
        * COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT;
    let mut line_origin = gpui::point(
        label_bounds.left(),
        label_bounds.top() + px((frame.height - measured_height).max(0.0) / 2.0),
    );

    window.with_content_mask(
        Some(ContentMask {
            bounds: label_bounds,
        }),
        |window| {
            for line in paint_state.label_lines {
                let _ = line.paint(
                    line_origin,
                    px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT),
                    gpui::TextAlign::Center,
                    Some(px(frame.width)),
                    window,
                    cx,
                );
                line_origin.y += px(COMMAND_PANE_SLEEPING_PLACEHOLDER_WAKE_LABEL_LINE_HEIGHT);
            }
        },
    );
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CommandPaneDelayedSendBadgeFrame {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}


pub(crate) struct CommandPaneDelayedSendBadgePaintState {
    pub(crate) frame: CommandPaneDelayedSendBadgeFrame,
    pub(crate) label_line: gpui::ShapedLine,
}


pub(crate) fn command_pane_delayed_send_badge_label_is_private_data_safe(label: &str) -> bool {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-27-00:07:
    The body badge label is generated from a runtime countdown only. Keep the canvas writer bounded to the countdown grammar so future callers cannot accidentally paint command text, titles, paths, URLs, or terminal content into the terminal body.
    */
    let mut parts = label.split(':');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(second) = parts.next() else {
        return false;
    };
    let third = parts.next();
    if parts.next().is_some() {
        return false;
    }

    let two_digit_component =
        |component: &str| component.len() == 2 && component.bytes().all(|b| b.is_ascii_digit());
    match third {
        Some(seconds) => {
            (2..=3).contains(&first.len())
                && first.bytes().all(|b| b.is_ascii_digit())
                && two_digit_component(second)
                && two_digit_component(seconds)
        }
        None => two_digit_component(first) && two_digit_component(second),
    }
}


pub(crate) fn command_pane_delayed_send_badge_fitting_size(label_text_width: f32) -> Option<(f32, f32)> {
    if !label_text_width.is_finite() || label_text_width <= 0.0 {
        return None;
    }

    Some((
        label_text_width.ceil() + COMMAND_PANE_DELAYED_SEND_BADGE_TOTAL_HORIZONTAL_PADDING,
        COMMAND_PANE_DELAYED_SEND_BADGE_MIN_HEIGHT,
    ))
}


pub(crate) fn command_pane_delayed_send_badge_frame(
    body_width: f32,
    body_height: f32,
    label_text_width: f32,
) -> Option<CommandPaneDelayedSendBadgeFrame> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-25-19:13:
    Keep GPUI's Delayed Send body-badge geometry tied to native's exact terminal body contract: no badge for tiny bodies, 60px total horizontal fitting padding, centered placement, and width/height clamps based only on the current body rectangle. Do not substitute command-group bounds or retained layout maps when same-pass body bounds are unavailable.
    */
    if !body_width.is_finite()
        || !body_height.is_finite()
        || body_width <= COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_WIDTH
        || body_height <= COMMAND_PANE_DELAYED_SEND_BADGE_MIN_BODY_HEIGHT
    {
        return None;
    }

    let (fitting_width, fitting_height) =
        command_pane_delayed_send_badge_fitting_size(label_text_width)?;
    let badge_width = fitting_width
        .min((body_width - COMMAND_PANE_DELAYED_SEND_BADGE_BODY_WIDTH_CLAMP_INSET).max(0.0));
    let badge_height = fitting_height
        .min((body_height - COMMAND_PANE_DELAYED_SEND_BADGE_BODY_HEIGHT_CLAMP_INSET).max(0.0));
    let x = (body_width / 2.0 - badge_width / 2.0)
        .max(0.0)
        .min(body_width - badge_width);
    let y = (body_height / 2.0 - badge_height / 2.0)
        .max(0.0)
        .min(body_height - badge_height);

    Some(CommandPaneDelayedSendBadgeFrame {
        x,
        y,
        width: badge_width,
        height: badge_height,
    })
}


pub(crate) fn command_pane_delayed_send_badge_prepaint(
    bounds: Bounds<Pixels>,
    label: String,
    window: &mut Window,
) -> Option<CommandPaneDelayedSendBadgePaintState> {
    /*
    CDXC:GPUICommandDelayedSend 2026-06-27-00:07:
    Runtime badge layout must come from this paint pass's command-body bounds. Shape the private countdown first, derive the native badge frame from the shaped width and exact body size, and return no paint state when native would hide the badge.
    */
    if !command_pane_delayed_send_badge_label_is_private_data_safe(&label) {
        return None;
    }

    let label_len = label.len();
    let label_line = window.text_system().shape_line(
        gpui::SharedString::from(label),
        px(COMMAND_PANE_DELAYED_SEND_BADGE_FONT_SIZE),
        &[gpui::TextRun {
            len: label_len,
            font: gpui::font(COMMAND_PANE_DELAYED_SEND_BADGE_FONT_FAMILY).bold(),
            color: command_pane_delayed_send_badge_text_color(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let frame = command_pane_delayed_send_badge_frame(
        bounds.size.width.as_f32(),
        bounds.size.height.as_f32(),
        label_line.width().as_f32(),
    )?;

    Some(CommandPaneDelayedSendBadgePaintState { frame, label_line })
}


pub(crate) fn command_pane_delayed_send_badge_paint(
    body_bounds: Bounds<Pixels>,
    paint_state: CommandPaneDelayedSendBadgePaintState,
    window: &mut Window,
    cx: &mut App,
) {
    let frame = paint_state.frame;
    let badge_bounds = Bounds::new(
        gpui::point(
            body_bounds.left() + px(frame.x),
            body_bounds.top() + px(frame.y),
        ),
        size(px(frame.width), px(frame.height)),
    );
    window.paint_quad(gpui::quad(
        badge_bounds,
        px(COMMAND_PANE_DELAYED_SEND_BADGE_CORNER_RADIUS),
        command_pane_delayed_send_badge_background_color(),
        px(1.0),
        command_pane_delayed_send_badge_border_color(),
        gpui::BorderStyle::Solid,
    ));

    let label_origin = gpui::point(
        badge_bounds.left(),
        badge_bounds.top() + px((frame.height - COMMAND_PANE_DELAYED_SEND_BADGE_LINE_HEIGHT) / 2.0),
    );
    window.with_content_mask(
        Some(ContentMask {
            bounds: badge_bounds,
        }),
        |window| {
            let _ = paint_state.label_line.paint(
                label_origin,
                px(COMMAND_PANE_DELAYED_SEND_BADGE_LINE_HEIGHT),
                gpui::TextAlign::Center,
                Some(px(frame.width)),
                window,
                cx,
            );
        },
    );
}


pub(crate) fn command_pane_default_height_ratio_for_default_height_px(
    default_height_px: f32,
    content_height: f32,
) -> f32 {
    command_pane_height_ratio(default_height_px / content_height.max(1.0))
}


pub(crate) fn command_pane_content_height(window: &Window) -> f32 {
    (window.bounds().size.height.as_f32() - TITLEBAR_HEIGHT).max(1.0)
}


pub(crate) fn command_pane_workspace_width(
    window: &Window,
    sidebar_width: f32,
    sidebar_collapsed: bool,
) -> f32 {
    /*
    CDXC:GPUICommandPaneSide 2026-08-16:
    Collapsing the sidebar removes both the sidebar and its divider from the
    body row without mutating the saved width, so the workspace really does
    own the whole window width there. The right dock sizes its column from
    this number, so it has to follow that collapse instead of subtracting a
    sidebar that is not on screen.
    */
    let sidebar_chrome_width = if gpui_sidebar_chrome_visible(sidebar_collapsed) {
        sidebar_width + SIDEBAR_DIVIDER_WIDTH
    } else {
        0.0
    };
    (window.bounds().size.width.as_f32() - sidebar_chrome_width).max(0.0)
}


pub(crate) fn command_pane_panel_chrome_width(workspace_width: f32, floating: bool) -> f32 {
    if floating {
        (workspace_width - COMMAND_PANE_FLOATING_MARGIN * 2.0).max(0.0)
    } else {
        workspace_width.max(0.0)
    }
}


pub(crate) fn command_pane_owner_content_width(panel_chrome_width: f32) -> f32 {
    (panel_chrome_width - COMMAND_PANE_OUTER_CONTENT_RIGHT_INSET).max(0.0)
}


pub(crate) fn command_pane_height_for_ratio(ratio: f32, content_height: f32) -> f32 {
    command_pane_height_ratio(ratio) * content_height.max(1.0)
}


pub(crate) fn command_pane_width_for_ratio(ratio: f32, content_width: f32) -> f32 {
    command_pane_width_ratio(ratio) * content_width.max(1.0)
}


pub(crate) fn command_pane_resize_drag_height_ratio(
    drag: CommandPaneResizeDragState,
    current_y: f32,
    content_height: f32,
) -> f32 {
    /*
    CDXC:GPUICommandPaneResize 2026-06-25-19:13:
    Native `beginCommandsPanelResize` stores the command panel's absolute start height and start Y, then `continueCommandsPanelResize` applies one signed pointer delta and clamps the resulting ratio to 5%-90%.
    GPUI pointer Y is top-origin, so visual upward motion is `start_y - current_y`; keep that conversion in one helper so drag handling cannot regress the AppKit sign/clamp contract.
    */
    let content_height = content_height.max(1.0);
    let upward_delta = drag.start_position - current_y;
    command_pane_height_ratio((drag.start_extent + upward_delta) / content_height)
}


pub(crate) fn command_pane_resize_drag_width_ratio(
    drag: CommandPaneResizeDragState,
    current_x: f32,
    content_width: f32,
) -> f32 {
    // The right-docked pane grows as the divider moves left, so leftward
    // pointer motion is `start_position - current_x`; the same one-helper rule
    // as the bottom rail keeps the sign and clamp contract in one place.
    let content_width = content_width.max(1.0);
    let leftward_delta = drag.start_position - current_x;
    command_pane_width_ratio((drag.start_extent + leftward_delta) / content_width)
}


pub(crate) fn command_pane_floating_height_for_ratio(ratio: f32, content_height: f32) -> f32 {
    /*
    CDXC:GPUICommandPaneFloating 2026-06-25-18:07:
    Native resolves floating command-panel height by first applying the normal command-panel ratio clamp, then capping the frame to workspace height minus the reserved collapsed strip and two floating margins. Match that cap so the floating panel keeps visible top/bottom breathing room instead of clipping like a pinned panel.
    */
    let requested_height = command_pane_height_for_ratio(ratio, content_height);
    let available_height =
        (content_height.max(1.0) - COMMAND_PANE_STRIP_HEIGHT - COMMAND_PANE_FLOATING_MARGIN * 2.0)
            .max(0.0);
    requested_height.min(available_height)
}


pub(crate) fn activate_agents_terminal_placeholder_with_runtime_attempt_identity(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    pane_id: WorkspacePaneId,
    session_id: TerminalSessionId,
) -> bool {
    /*
    CDXC:GPUITerminalStartupRetryIdentity 2026-06-23-18:19:
    Placeholder activation may change durable shell presentation, but retry attempt identity is process-local app/runtime state. Detect the explicit `StartupFailed` edge before shell activation, then rotate the runtime id only after that same shell session becomes startup-eligible `Mounting` so wake/materialize/reattach placeholders keep their existing runtime identity and cannot enter a retry startup attempt.

    CDXC:GPUITerminalRestoredMaterialization 2026-06-23-19:26:
    Restored-unmounted materialization now enters the startup pipeline, but it is not a retry. Keep the process-local runtime id already associated with the durable shell session; sleeping wake and popped-out reattach remain blocked from startup maps and use the separate slice 236 parked-owner contract.
    */
    let retry_activation = workspace.session(session_id).is_some_and(|session| {
        session.presentation_state == TerminalSessionPresentationState::StartupFailed
    });
    let model_changed = workspace.activate_terminal_placeholder_session(pane_id, session_id);

    if retry_activation
        && workspace
            .session(session_id)
            .is_some_and(TerminalSession::can_enter_startup_pipeline)
    {
        runtime_sessions.rotate_runtime_session_id_for_shell_session(session_id);
    }

    model_changed
}


pub(crate) fn focus_existing_local_workspace_terminal_tab_model(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &mut AgentsTerminalRuntimeSessionRegistry,
    pane_id: WorkspacePaneId,
    session_id: TerminalSessionId,
) -> bool {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:34:
    Existing local gxserver-to-GPUI tab mappings should focus and activate the mapped shell tab in place. Reuse the placeholder activation path for non-running placeholders, then select the same tab so sidebar wake/focus cannot create a duplicate attach tab or leave a selected sleeping/restored tab inert.
    */
    if !workspace.session_belongs_to_pane(pane_id, session_id) {
        return false;
    }
    activate_agents_terminal_placeholder_with_runtime_attempt_identity(
        workspace,
        runtime_sessions,
        pane_id,
        session_id,
    );
    workspace.select_tab(pane_id, session_id);
    true
}


#[cfg(target_os = "windows")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiWindowsFirstRunSetupState {
    Checking,
    MissingWsl,
    MissingDistribution,
    ChooseDistribution(Vec<String>),
    ConfiguredDistributionUnavailable(String),
    SettingUp(windows_terminal_backend::WindowsWslSetupPhase),
    Failed(String),
    Ready,
}


#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
pub(crate) enum GpuiWindowsFirstRunSetupAction {
    Retry,
    OpenWslGuide,
    ChooseDistribution(String),
    ClearDistribution,
}


/*
CDXC:SidebarBrowserTabReveal 2026-08-18:
One pending sidebar reveal for a Browser tab the user just opened, held until
that tab reaches the sidebar in a published tab snapshot.
*/
pub(crate) struct PendingSidebarBrowserTabReveal {
    pub(crate) project_id: String,
    pub(crate) tab_id: BrowserTabId,
}

