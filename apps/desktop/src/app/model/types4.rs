// C1 wave-3 extraction: a chunk (4/6, in original file order) of the remaining plain value-type enums/structs/small helper fns moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


pub(crate) fn rebalance_command_split_axis_chain_containing_group(
    node: &mut CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    axis: WorkspaceSplitAxis,
) -> bool {
    match node {
        CommandPaneNode::Leaf(leaf) => leaf.group_id == target_group_id,
        CommandPaneNode::Split(split) => {
            let target_in_first = command_node_contains_group(&split.first, target_group_id);
            let target_in_second = command_node_contains_group(&split.second, target_group_id);
            if !target_in_first && !target_in_second {
                return false;
            }

            if split.axis == axis {
                /*
                CDXC:GPUICommandPaneSplits 2026-06-25-16:14:
                Native command pane layouts flatten repeated same-direction split insertion into one split node whose children default to equal ratios until the user resizes. GPUI still stores binary splits, so rebalance only untouched same-axis chains by visible leaf count after insertion; explicit user-resized ratios stay unchanged instead of being hidden by fallback geometry.
                */
                rebalance_command_split_axis_to_native_default_ratios(node, axis);
                true
            } else if target_in_first {
                rebalance_command_split_axis_chain_containing_group(
                    &mut split.first,
                    target_group_id,
                    axis,
                )
            } else {
                rebalance_command_split_axis_chain_containing_group(
                    &mut split.second,
                    target_group_id,
                    axis,
                )
            }
        }
    }
}


pub(crate) fn rebalance_command_split_axis_to_native_default_ratios(
    node: &mut CommandPaneNode,
    axis: WorkspaceSplitAxis,
) {
    match node {
        CommandPaneNode::Leaf(_) => {}
        CommandPaneNode::Split(split) => {
            if split.axis == axis
                && let Some(default_ratio) = command_split_native_default_ratio(split)
            {
                split.ratio = default_ratio;
            }
            rebalance_command_split_axis_to_native_default_ratios(&mut split.first, axis);
            rebalance_command_split_axis_to_native_default_ratios(&mut split.second, axis);
        }
    }
}


pub(crate) fn collapse_empty_command_leaf(node: &mut CommandPaneNode, group_id: CommandPaneGroupId) -> bool {
    let mut replacement = None;
    let is_empty = match node {
        CommandPaneNode::Leaf(leaf) => leaf.group_id == group_id && leaf.tab_group.tabs.is_empty(),
        CommandPaneNode::Split(split) => {
            if collapse_empty_command_leaf(&mut split.first, group_id) {
                replacement = Some(take_command_node(&mut split.second));
            } else if collapse_empty_command_leaf(&mut split.second, group_id) {
                replacement = Some(take_command_node(&mut split.first));
            }
            false
        }
    };

    if let Some(replacement) = replacement {
        *node = replacement;
        false
    } else {
        is_empty
    }
}


pub(crate) fn take_command_node(node: &mut Box<CommandPaneNode>) -> CommandPaneNode {
    let mut replacement = Box::new(command_pane_dummy_node());
    std::mem::swap(node, &mut replacement);
    *replacement
}


pub(crate) fn command_pane_dummy_node() -> CommandPaneNode {
    CommandPaneNode::Leaf(CommandPaneLeaf {
        group_id: CommandPaneGroupId(0),
        tab_group: CommandPaneTabGroup {
            tabs: Vec::new(),
            active_session: CommandSessionId(0),
        },
    })
}


pub(crate) fn collect_browser_leaf_ids(node: &BrowserNode, pane_ids: &mut Vec<BrowserPaneId>) {
    match node {
        BrowserNode::Leaf(leaf) => {
            if !leaf.tab_group.tabs.is_empty() {
                pane_ids.push(leaf.pane_id);
            }
        }
        BrowserNode::Split(split) => {
            collect_browser_leaf_ids(&split.first, pane_ids);
            collect_browser_leaf_ids(&split.second, pane_ids);
        }
    }
}


pub(crate) fn collect_browser_tab_ids(node: &BrowserNode, tab_ids: &mut Vec<BrowserTabId>) {
    match node {
        BrowserNode::Leaf(leaf) => {
            tab_ids.extend(leaf.tab_group.tabs.iter().map(|tab| tab.tab_id));
        }
        BrowserNode::Split(split) => {
            collect_browser_tab_ids(&split.first, tab_ids);
            collect_browser_tab_ids(&split.second, tab_ids);
        }
    }
}


pub(crate) fn collect_browser_split_ids(node: &BrowserNode, split_ids: &mut Vec<BrowserSplitId>) {
    match node {
        BrowserNode::Leaf(_) => {}
        BrowserNode::Split(split) => {
            split_ids.push(split.id);
            collect_browser_split_ids(&split.first, split_ids);
            collect_browser_split_ids(&split.second, split_ids);
        }
    }
}


pub(crate) fn first_browser_leaf_id(node: &BrowserNode) -> Option<BrowserPaneId> {
    match node {
        BrowserNode::Leaf(leaf) if !leaf.tab_group.tabs.is_empty() => Some(leaf.pane_id),
        BrowserNode::Leaf(_) => None,
        BrowserNode::Split(split) => {
            first_browser_leaf_id(&split.first).or_else(|| first_browser_leaf_id(&split.second))
        }
    }
}


pub(crate) fn first_browser_tab_id(node: &BrowserNode) -> Option<BrowserTabId> {
    match node {
        BrowserNode::Leaf(leaf) => leaf.tab_group.tabs.first().map(|tab| tab.tab_id),
        BrowserNode::Split(split) => {
            first_browser_tab_id(&split.first).or_else(|| first_browser_tab_id(&split.second))
        }
    }
}


pub(crate) fn find_browser_leaf(node: &BrowserNode, pane_id: BrowserPaneId) -> Option<&BrowserLeaf> {
    match node {
        BrowserNode::Leaf(leaf) => (leaf.pane_id == pane_id).then_some(leaf),
        BrowserNode::Split(split) => find_browser_leaf(&split.first, pane_id)
            .or_else(|| find_browser_leaf(&split.second, pane_id)),
    }
}


pub(crate) fn find_browser_leaf_mut(
    node: &mut BrowserNode,
    pane_id: BrowserPaneId,
) -> Option<&mut BrowserLeaf> {
    match node {
        BrowserNode::Leaf(leaf) => (leaf.pane_id == pane_id).then_some(leaf),
        BrowserNode::Split(split) => find_browser_leaf_mut(&mut split.first, pane_id)
            .or_else(|| find_browser_leaf_mut(&mut split.second, pane_id)),
    }
}


pub(crate) fn find_browser_split(node: &BrowserNode, split_id: BrowserSplitId) -> Option<&BrowserSplit> {
    match node {
        BrowserNode::Leaf(_) => None,
        BrowserNode::Split(split) if split.id == split_id => Some(split),
        BrowserNode::Split(split) => find_browser_split(&split.first, split_id)
            .or_else(|| find_browser_split(&split.second, split_id)),
    }
}


pub(crate) fn find_browser_split_mut(
    node: &mut BrowserNode,
    split_id: BrowserSplitId,
) -> Option<&mut BrowserSplit> {
    match node {
        BrowserNode::Leaf(_) => None,
        BrowserNode::Split(split) => {
            if split.id == split_id {
                return Some(split);
            }
            if let Some(found) = find_browser_split_mut(&mut split.first, split_id) {
                return Some(found);
            }
            find_browser_split_mut(&mut split.second, split_id)
        }
    }
}


pub(crate) fn find_browser_leaf_id_for_tab(node: &BrowserNode, tab_id: BrowserTabId) -> Option<BrowserPaneId> {
    match node {
        BrowserNode::Leaf(leaf) if leaf.tab_group.has_tab(tab_id) => Some(leaf.pane_id),
        BrowserNode::Leaf(_) => None,
        BrowserNode::Split(split) => find_browser_leaf_id_for_tab(&split.first, tab_id)
            .or_else(|| find_browser_leaf_id_for_tab(&split.second, tab_id)),
    }
}


pub(crate) fn browser_node_contains_pane(node: &BrowserNode, pane_id: BrowserPaneId) -> bool {
    find_browser_leaf(node, pane_id).is_some()
}


pub(crate) fn insert_browser_leaf_split(
    node: &mut BrowserNode,
    target_pane_id: BrowserPaneId,
    new_leaf: BrowserLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: BrowserSplitId,
) -> bool {
    match node {
        BrowserNode::Leaf(leaf) if leaf.pane_id == target_pane_id => {
            let existing = std::mem::replace(node, browser_dummy_node());
            let new_node = BrowserNode::Leaf(new_leaf);
            let (first, second) = if dragged_first {
                (new_node, existing)
            } else {
                (existing, new_node)
            };

            *node = BrowserNode::Split(BrowserSplit {
                id: split_id,
                axis,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            });
            true
        }
        BrowserNode::Leaf(_) => false,
        BrowserNode::Split(split) => {
            if browser_node_contains_pane(&split.first, target_pane_id) {
                insert_browser_leaf_split(
                    &mut split.first,
                    target_pane_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            } else {
                insert_browser_leaf_split(
                    &mut split.second,
                    target_pane_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            }
        }
    }
}


pub(crate) fn collapse_empty_browser_leaf(node: &mut BrowserNode, pane_id: BrowserPaneId) -> bool {
    let mut replacement = None;
    let is_empty = match node {
        BrowserNode::Leaf(leaf) => leaf.pane_id == pane_id && leaf.tab_group.tabs.is_empty(),
        BrowserNode::Split(split) => {
            if collapse_empty_browser_leaf(&mut split.first, pane_id) {
                replacement = Some(take_browser_node(&mut split.second));
            } else if collapse_empty_browser_leaf(&mut split.second, pane_id) {
                replacement = Some(take_browser_node(&mut split.first));
            }
            false
        }
    };

    if let Some(replacement) = replacement {
        *node = replacement;
        false
    } else {
        is_empty
    }
}


pub(crate) fn take_browser_node(node: &mut Box<BrowserNode>) -> BrowserNode {
    let mut replacement = Box::new(browser_dummy_node());
    std::mem::swap(node, &mut replacement);
    *replacement
}


pub(crate) fn browser_dummy_node() -> BrowserNode {
    BrowserNode::Leaf(BrowserLeaf {
        pane_id: BrowserPaneId(0),
        tab_group: BrowserTabGroup {
            tabs: Vec::new(),
            active_tab: BrowserTabId(0),
        },
    })
}


pub(crate) fn collect_workspace_leaf_ids(node: &WorkspaceNode, pane_ids: &mut Vec<WorkspacePaneId>) {
    match node {
        WorkspaceNode::Leaf(leaf) => {
            if !leaf.tab_group.tabs.is_empty() {
                pane_ids.push(leaf.pane_id);
            }
        }
        WorkspaceNode::Split(split) => {
            collect_workspace_leaf_ids(&split.first, pane_ids);
            collect_workspace_leaf_ids(&split.second, pane_ids);
        }
    }
}


pub(crate) fn collect_workspace_all_leaf_ids(node: &WorkspaceNode, pane_ids: &mut Vec<WorkspacePaneId>) {
    match node {
        WorkspaceNode::Leaf(leaf) => pane_ids.push(leaf.pane_id),
        WorkspaceNode::Split(split) => {
            collect_workspace_all_leaf_ids(&split.first, pane_ids);
            collect_workspace_all_leaf_ids(&split.second, pane_ids);
        }
    }
}


pub(crate) fn workspace_empty_root_leaf_id(node: &WorkspaceNode) -> Option<WorkspacePaneId> {
    match node {
        WorkspaceNode::Leaf(leaf) if leaf.tab_group.tabs.is_empty() => Some(leaf.pane_id),
        WorkspaceNode::Leaf(_) | WorkspaceNode::Split(_) => None,
    }
}


pub(crate) fn collect_workspace_tabs_in_tree_order(node: &WorkspaceNode, tabs: &mut Vec<WorkspaceTab>) {
    match node {
        WorkspaceNode::Leaf(leaf) => tabs.extend(leaf.tab_group.tabs.iter().copied()),
        WorkspaceNode::Split(split) => {
            collect_workspace_tabs_in_tree_order(&split.first, tabs);
            collect_workspace_tabs_in_tree_order(&split.second, tabs);
        }
    }
}


pub(crate) fn collect_workspace_tab_count(node: &WorkspaceNode) -> usize {
    match node {
        WorkspaceNode::Leaf(leaf) => leaf.tab_group.tabs.len(),
        WorkspaceNode::Split(split) => {
            collect_workspace_tab_count(&split.first) + collect_workspace_tab_count(&split.second)
        }
    }
}


pub(crate) fn rotate_workspace_node_clockwise(node: &mut WorkspaceNode) {
    match node {
        WorkspaceNode::Leaf(_) => {}
        WorkspaceNode::Split(split) => {
            rotate_workspace_node_clockwise(&mut split.first);
            rotate_workspace_node_clockwise(&mut split.second);
            match split.axis {
                WorkspaceSplitAxis::Horizontal => {
                    split.axis = WorkspaceSplitAxis::Vertical;
                }
                WorkspaceSplitAxis::Vertical => {
                    split.axis = WorkspaceSplitAxis::Horizontal;
                    std::mem::swap(&mut split.first, &mut split.second);
                    split.ratio = workspace_split_ratio(1.0 - workspace_split_ratio(split.ratio));
                    split.default_ratio =
                        workspace_split_ratio(1.0 - workspace_split_ratio(split.default_ratio));
                }
            }
        }
    }
}


pub(crate) fn find_workspace_leaf(node: &WorkspaceNode, pane_id: WorkspacePaneId) -> Option<&WorkspaceLeaf> {
    match node {
        WorkspaceNode::Leaf(leaf) => (leaf.pane_id == pane_id).then_some(leaf),
        WorkspaceNode::Split(split) => find_workspace_leaf(&split.first, pane_id)
            .or_else(|| find_workspace_leaf(&split.second, pane_id)),
    }
}


pub(crate) fn find_workspace_leaf_mut(
    node: &mut WorkspaceNode,
    pane_id: WorkspacePaneId,
) -> Option<&mut WorkspaceLeaf> {
    match node {
        WorkspaceNode::Leaf(leaf) => (leaf.pane_id == pane_id).then_some(leaf),
        WorkspaceNode::Split(split) => find_workspace_leaf_mut(&mut split.first, pane_id)
            .or_else(|| find_workspace_leaf_mut(&mut split.second, pane_id)),
    }
}


pub(crate) fn find_workspace_split(
    node: &WorkspaceNode,
    split_id: WorkspaceSplitId,
) -> Option<&WorkspaceSplit> {
    match node {
        WorkspaceNode::Leaf(_) => None,
        WorkspaceNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_workspace_split(&split.first, split_id)
                    .or_else(|| find_workspace_split(&split.second, split_id))
            }
        }
    }
}


pub(crate) fn find_workspace_split_mut(
    node: &mut WorkspaceNode,
    split_id: WorkspaceSplitId,
) -> Option<&mut WorkspaceSplit> {
    match node {
        WorkspaceNode::Leaf(_) => None,
        WorkspaceNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_workspace_split_mut(&mut split.first, split_id)
                    .or_else(|| find_workspace_split_mut(&mut split.second, split_id))
            }
        }
    }
}


pub(crate) fn workspace_node_contains_pane(node: &WorkspaceNode, pane_id: WorkspacePaneId) -> bool {
    find_workspace_leaf(node, pane_id).is_some()
}


pub(crate) fn first_workspace_leaf_id(node: &WorkspaceNode) -> Option<WorkspacePaneId> {
    match node {
        WorkspaceNode::Leaf(leaf) if !leaf.tab_group.tabs.is_empty() => Some(leaf.pane_id),
        WorkspaceNode::Leaf(_) => None,
        WorkspaceNode::Split(split) => {
            first_workspace_leaf_id(&split.first).or_else(|| first_workspace_leaf_id(&split.second))
        }
    }
}


pub(crate) fn workspace_close_focus_replacement_leaf_id(
    node: &WorkspaceNode,
    pane_id: WorkspacePaneId,
) -> Option<WorkspacePaneId> {
    let mut panes = Vec::new();
    collect_workspace_close_focus_rects(
        node,
        WorkspaceCloseFocusBounds {
            left: 0.0,
            right: 1.0,
            top: 0.0,
            bottom: 1.0,
        },
        &mut Vec::new(),
        &mut panes,
    );
    let closing_pane = panes.iter().find(|pane| pane.pane_id == pane_id)?;
    let candidates = panes
        .iter()
        .filter(|pane| pane.pane_id != pane_id && pane.has_tabs)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    let sibling_branch_candidates =
        workspace_closing_pane_sibling_branch_candidates(&candidates, &closing_pane.path);
    let focus_candidates = if sibling_branch_candidates.is_empty() {
        candidates.as_slice()
    } else {
        sibling_branch_candidates.as_slice()
    };
    workspace_closest_post_close_focus_rect(focus_candidates, closing_pane).map(|pane| pane.pane_id)
}


pub(crate) fn collect_workspace_close_focus_rects(
    node: &WorkspaceNode,
    bounds: WorkspaceCloseFocusBounds,
    path: &mut Vec<usize>,
    panes: &mut Vec<WorkspaceCloseFocusRect>,
) {
    match node {
        WorkspaceNode::Leaf(leaf) => {
            panes.push(WorkspaceCloseFocusRect {
                pane_id: leaf.pane_id,
                bounds,
                center_x: (bounds.left + bounds.right) / 2.0,
                center_y: (bounds.top + bounds.bottom) / 2.0,
                path: path.clone(),
                has_tabs: !leaf.tab_group.tabs.is_empty(),
            });
        }
        WorkspaceNode::Split(split) => {
            let ratio = workspace_split_ratio(split.ratio);
            let (first_bounds, second_bounds) = match split.axis {
                WorkspaceSplitAxis::Horizontal => {
                    let split_x = bounds.left + (bounds.right - bounds.left) * ratio;
                    (
                        WorkspaceCloseFocusBounds {
                            right: split_x,
                            ..bounds
                        },
                        WorkspaceCloseFocusBounds {
                            left: split_x,
                            ..bounds
                        },
                    )
                }
                WorkspaceSplitAxis::Vertical => {
                    let split_y = bounds.top + (bounds.bottom - bounds.top) * ratio;
                    (
                        WorkspaceCloseFocusBounds {
                            bottom: split_y,
                            ..bounds
                        },
                        WorkspaceCloseFocusBounds {
                            top: split_y,
                            ..bounds
                        },
                    )
                }
            };
            path.push(0);
            collect_workspace_close_focus_rects(&split.first, first_bounds, path, panes);
            path.pop();
            path.push(1);
            collect_workspace_close_focus_rects(&split.second, second_bounds, path, panes);
            path.pop();
        }
    }
}


pub(crate) fn workspace_closing_pane_sibling_branch_candidates<'a>(
    candidates: &[&'a WorkspaceCloseFocusRect],
    closing_pane_path: &[usize],
) -> Vec<&'a WorkspaceCloseFocusRect> {
    let Some((&closing_child_index, parent_path)) = closing_pane_path.split_last() else {
        return Vec::new();
    };
    candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.path.len() > parent_path.len()
                && candidate.path[..parent_path.len()] == *parent_path
                && candidate.path[parent_path.len()] != closing_child_index
        })
        .collect()
}


pub(crate) fn workspace_closest_post_close_focus_rect<'a>(
    candidates: &[&'a WorkspaceCloseFocusRect],
    closing_pane: &WorkspaceCloseFocusRect,
) -> Option<&'a WorkspaceCloseFocusRect> {
    candidates
        .iter()
        .enumerate()
        .min_by(|(left_index, left), (right_index, right)| {
            let left_score = workspace_post_close_pane_focus_score(closing_pane, left);
            let right_score = workspace_post_close_pane_focus_score(closing_pane, right);
            left_score
                .total_cmp(&right_score)
                .then_with(|| left_index.cmp(right_index))
        })
        .map(|(_, pane)| *pane)
}


pub(crate) fn workspace_post_close_pane_focus_score(
    closing_pane: &WorkspaceCloseFocusRect,
    candidate_pane: &WorkspaceCloseFocusRect,
) -> f32 {
    let horizontal_gap = workspace_range_gap(
        closing_pane.bounds.left,
        closing_pane.bounds.right,
        candidate_pane.bounds.left,
        candidate_pane.bounds.right,
    );
    let vertical_gap = workspace_range_gap(
        closing_pane.bounds.top,
        closing_pane.bounds.bottom,
        candidate_pane.bounds.top,
        candidate_pane.bounds.bottom,
    );
    let shares_axis = workspace_ranges_intersect(
        closing_pane.bounds.left,
        closing_pane.bounds.right,
        candidate_pane.bounds.left,
        candidate_pane.bounds.right,
    ) || workspace_ranges_intersect(
        closing_pane.bounds.top,
        closing_pane.bounds.bottom,
        candidate_pane.bounds.top,
        candidate_pane.bounds.bottom,
    );
    let center_distance = (candidate_pane.center_x - closing_pane.center_x).abs()
        + (candidate_pane.center_y - closing_pane.center_y).abs();
    (if shares_axis { 0.0 } else { 1000.0 })
        + (horizontal_gap + vertical_gap) * 100.0
        + center_distance
}


pub(crate) fn workspace_range_gap(start_a: f32, end_a: f32, start_b: f32, end_b: f32) -> f32 {
    if end_a < start_b {
        start_b - end_a
    } else if end_b < start_a {
        start_a - end_b
    } else {
        0.0
    }
}


pub(crate) fn workspace_ranges_intersect(start_a: f32, end_a: f32, start_b: f32, end_b: f32) -> bool {
    start_a.max(start_b) < end_a.min(end_b)
}


pub(crate) fn insert_workspace_leaf_split(
    node: &mut WorkspaceNode,
    target_pane_id: WorkspacePaneId,
    new_leaf: WorkspaceLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: WorkspaceSplitId,
) -> bool {
    match node {
        WorkspaceNode::Leaf(leaf) if leaf.pane_id == target_pane_id => {
            let existing = std::mem::replace(node, workspace_dummy_node());
            let new_node = WorkspaceNode::Leaf(new_leaf);
            let (first, second) = if dragged_first {
                (new_node, existing)
            } else {
                (existing, new_node)
            };

            *node = WorkspaceNode::Split(WorkspaceSplit {
                id: split_id,
                axis,
                ratio: 0.5,
                default_ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            });
            true
        }
        WorkspaceNode::Leaf(_) => false,
        WorkspaceNode::Split(split) => {
            if workspace_node_contains_pane(&split.first, target_pane_id) {
                insert_workspace_leaf_split(
                    &mut split.first,
                    target_pane_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            } else {
                insert_workspace_leaf_split(
                    &mut split.second,
                    target_pane_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            }
        }
    }
}


pub(crate) fn collapse_empty_workspace_leaf(node: &mut WorkspaceNode, pane_id: WorkspacePaneId) -> bool {
    let mut replacement = None;
    let is_empty = match node {
        WorkspaceNode::Leaf(leaf) => leaf.pane_id == pane_id && leaf.tab_group.tabs.is_empty(),
        WorkspaceNode::Split(split) => {
            if collapse_empty_workspace_leaf(&mut split.first, pane_id) {
                replacement = Some(take_workspace_node(&mut split.second));
            } else if collapse_empty_workspace_leaf(&mut split.second, pane_id) {
                replacement = Some(take_workspace_node(&mut split.first));
            }
            false
        }
    };

    if let Some(replacement) = replacement {
        *node = replacement;
        false
    } else {
        is_empty
    }
}


pub(crate) fn take_workspace_node(node: &mut Box<WorkspaceNode>) -> WorkspaceNode {
    let mut replacement = Box::new(workspace_dummy_node());
    std::mem::swap(node, &mut replacement);
    *replacement
}


pub(crate) fn workspace_dummy_node() -> WorkspaceNode {
    workspace_empty_leaf_node(WorkspacePaneId(0))
}


pub(crate) fn workspace_node_is_empty_leaf_for_pane(node: &WorkspaceNode, pane_id: WorkspacePaneId) -> bool {
    matches!(
        node,
        WorkspaceNode::Leaf(leaf)
            if leaf.pane_id == pane_id
                && leaf.tab_group.tabs.is_empty()
                && leaf.tab_group.active_tab == TerminalSessionId(0)
    )
}


pub(crate) fn workspace_leaf_node_from_session_ids(
    pane_id: WorkspacePaneId,
    session_ids: Vec<TerminalSessionId>,
) -> WorkspaceNode {
    let active_tab = session_ids.first().copied().unwrap_or(TerminalSessionId(0));
    WorkspaceNode::Leaf(WorkspaceLeaf {
        pane_id,
        tab_group: WorkspaceTabGroup {
            tabs: session_ids
                .into_iter()
                .map(|session_id| WorkspaceTab { session_id })
                .collect(),
            active_tab,
        },
    })
}


pub(crate) fn workspace_empty_leaf_node(pane_id: WorkspacePaneId) -> WorkspaceNode {
    WorkspaceNode::Leaf(WorkspaceLeaf {
        pane_id,
        tab_group: WorkspaceTabGroup {
            tabs: Vec::new(),
            active_tab: TerminalSessionId(0),
        },
    })
}


pub(crate) fn normalize_workspace_node(
    node: WorkspaceNode,
    valid_session_ids: &HashSet<TerminalSessionId>,
    changed: &mut bool,
) -> Option<WorkspaceNode> {
    match node {
        WorkspaceNode::Leaf(mut leaf) => {
            let before_tabs = leaf.tab_group.tabs.clone();
            leaf.tab_group
                .tabs
                .retain(|tab| valid_session_ids.contains(&tab.session_id));
            if leaf.tab_group.tabs != before_tabs {
                *changed = true;
            }
            if leaf.tab_group.tabs.is_empty() {
                *changed = true;
                return None;
            }
            if !leaf
                .tab_group
                .tabs
                .iter()
                .any(|tab| tab.session_id == leaf.tab_group.active_tab)
            {
                leaf.tab_group.active_tab = leaf.tab_group.tabs[0].session_id;
                *changed = true;
            }
            Some(WorkspaceNode::Leaf(leaf))
        }
        WorkspaceNode::Split(mut split) => {
            let first = normalize_workspace_node(*split.first, valid_session_ids, changed);
            let second = normalize_workspace_node(*split.second, valid_session_ids, changed);
            match (first, second) {
                (Some(first), Some(second)) => {
                    split.first = Box::new(first);
                    split.second = Box::new(second);
                    Some(WorkspaceNode::Split(split))
                }
                (Some(node), None) | (None, Some(node)) => {
                    *changed = true;
                    Some(node)
                }
                (None, None) => {
                    *changed = true;
                    None
                }
            }
        }
    }
}


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


pub(crate) fn local_workspace_session_mappings_to_shell_state_json(
    mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    workspace: &WorkspaceModel,
) -> serde_json::Value {
    let mut entries = mappings
        .iter()
        .filter(|(_, shell_session_id)| workspace.has_session(**shell_session_id))
        .map(|(key, shell_session_id)| {
            (
                shell_session_id.0,
                key.project_id.as_str(),
                key.session_id.as_str(),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    serde_json::Value::Array(
        entries
            .into_iter()
            .map(|(shell_session_id, project_id, session_id)| {
                serde_json::json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "shellSessionId": shell_session_id,
                })
            })
            .collect(),
    )
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


pub(crate) fn remote_workspace_session_mappings_to_shell_state_json(
    mappings: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    workspace: &WorkspaceModel,
    workspace_project_id: Option<&str>,
) -> serde_json::Value {
    let mut entries = mappings
        .iter()
        .filter_map(|(key, shell_session_id)| {
            let scoped_project_id = gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            );
            (workspace_project_id == Some(scoped_project_id.as_str())
                && workspace.has_session(*shell_session_id))
            .then(|| {
                (
                    shell_session_id.0,
                    key.remote_machine_id.as_str(),
                    key.project_id.as_str(),
                    key.session_id.as_str(),
                )
            })
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    serde_json::Value::Array(
        entries
            .into_iter()
            .map(
                |(shell_session_id, remote_machine_id, project_id, session_id)| {
                    serde_json::json!({
                        "remoteMachineId": remote_machine_id,
                        "projectId": project_id,
                        "sessionId": session_id,
                        "shellSessionId": shell_session_id,
                    })
                },
            )
            .collect(),
    )
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


pub(crate) fn local_workspace_session_mappings_from_shell_state(
    value: &serde_json::Value,
    workspace: &WorkspaceModel,
) -> Option<HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>> {
    let entries = value.as_array()?;
    if entries.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return None;
    }
    let mut mappings = HashMap::with_capacity(entries.len());
    let mut mapped_shell_session_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        if object.len() != 3
            || !object.contains_key("projectId")
            || !object.contains_key("sessionId")
            || !object.contains_key("shellSessionId")
        {
            return None;
        }
        let project_id = json_string_field(object, "projectId")?.trim();
        let session_id = json_string_field(object, "sessionId")?.trim();
        let shell_session_id = TerminalSessionId(json_u64_field(object, "shellSessionId")?);
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_sidebar_local_gxserver_session_id_allowed(session_id)
            || !workspace.has_session(shell_session_id)
        {
            return None;
        }
        let key = GpuiLocalWorkspaceSessionKey {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        if mappings.insert(key, shell_session_id).is_some()
            || !mapped_shell_session_ids.insert(shell_session_id)
        {
            return None;
        }
    }
    Some(mappings)
}


pub(crate) fn remote_workspace_session_mappings_from_shell_state(
    value: &serde_json::Value,
    workspace: &WorkspaceModel,
    workspace_project_id: Option<&str>,
) -> Option<HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>> {
    let entries = value.as_array()?;
    if entries.len() > GPUI_SIDEBAR_WORKSPACE_TAB_SESSIONS_MAX {
        return None;
    }
    let mut mappings = HashMap::with_capacity(entries.len());
    let mut mapped_shell_session_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        let object = entry.as_object()?;
        if object.len() != 4
            || !object.contains_key("remoteMachineId")
            || !object.contains_key("projectId")
            || !object.contains_key("sessionId")
            || !object.contains_key("shellSessionId")
        {
            return None;
        }
        let remote_machine_id = json_string_field(object, "remoteMachineId")?.trim();
        let project_id = json_string_field(object, "projectId")?.trim();
        let session_id = json_string_field(object, "sessionId")?.trim();
        let shell_session_id = TerminalSessionId(json_u64_field(object, "shellSessionId")?);
        let remote_machine_id = gpui_normalize_remote_machine_id(remote_machine_id)?;
        let scoped_project_id =
            gpui_remote_scoped_project_id(remote_machine_id.as_str(), project_id);
        if !gpui_remote_sidebar_project_id_allowed(project_id)
            || !gpui_remote_sidebar_session_id_allowed(session_id)
            || workspace_project_id != Some(scoped_project_id.as_str())
            || !workspace.has_session(shell_session_id)
        {
            return None;
        }
        let key = GpuiRemoteAttachSessionKey {
            remote_machine_id,
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
        };
        if mappings.insert(key, shell_session_id).is_some()
            || !mapped_shell_session_ids.insert(shell_session_id)
        {
            return None;
        }
    }
    Some(mappings)
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


pub(crate) fn workspace_model_from_shell_state(value: &serde_json::Value) -> Option<WorkspaceModel> {
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


pub(crate) fn terminal_session_from_shell_state(value: &serde_json::Value) -> Option<TerminalSession> {
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


pub(crate) fn collect_workspace_split_ids(node: &WorkspaceNode, split_ids: &mut Vec<WorkspaceSplitId>) {
    match node {
        WorkspaceNode::Leaf(_) => {}
        WorkspaceNode::Split(split) => {
            split_ids.push(split.id);
            collect_workspace_split_ids(&split.first, split_ids);
            collect_workspace_split_ids(&split.second, split_ids);
        }
    }
}


pub(crate) fn command_pane_model_to_shell_state_json(model: &CommandPaneModel) -> serde_json::Value {
    command_pane_model_to_shell_state_json_with_optional_delayed_send_timers(model, None, None)
}


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


pub(crate) fn command_session_from_shell_state(value: &serde_json::Value) -> Option<CommandTerminalSession> {
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


pub(crate) fn collect_command_leaf_ids(node: &CommandPaneNode, group_ids: &mut Vec<CommandPaneGroupId>) {
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


pub(crate) fn collect_command_split_ids(node: &CommandPaneNode, split_ids: &mut Vec<CommandPaneSplitId>) {
    match node {
        CommandPaneNode::Leaf(_) => {}
        CommandPaneNode::Split(split) => {
            split_ids.push(split.id);
            collect_command_split_ids(&split.first, split_ids);
            collect_command_split_ids(&split.second, split_ids);
        }
    }
}


pub(crate) fn browser_profile_model_to_shell_state_json(model: &BrowserProfileModel) -> serde_json::Value {
    /*
    CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
    Browser profile shell-state serialization is sanitized at the writer boundary: persist only generated numeric profile ids, the active generated id, and the next generated id. Never persist profile display names from user input, filesystem paths, CEF cache directories, imported data choices, cookies, credentials, history, URLs, page titles, command text, or terminal content.
    */
    serde_json::json!({
        "profiles": model
            .profile_ids()
            .map(|profile_id| serde_json::json!(profile_id.0))
            .collect::<Vec<_>>(),
        "activeProfileId": model.active_profile_id().0,
        "nextProfileId": model.next_profile_id.max(BROWSER_PROFILE_FIRST_GENERATED_ID),
    })
}


pub(crate) fn browser_profile_model_from_shell_state(
    value: &serde_json::Value,
) -> Option<BrowserProfileModel> {
    let object = value.as_object()?;
    let mut profiles = json_array_field(object, "profiles")?
        .iter()
        .map(json_u64_value)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .filter(|profile_id| *profile_id >= BROWSER_PROFILE_DEFAULT_ID)
        .map(BrowserProfileId)
        .collect::<Vec<_>>();

    if !profiles.contains(&BrowserProfileId::default_profile()) {
        profiles.push(BrowserProfileId::default_profile());
    }
    profiles.sort_by_key(|profile_id| {
        if *profile_id == BrowserProfileId::default_profile() {
            0
        } else {
            profile_id.0
        }
    });
    profiles.dedup();
    if profiles.is_empty()
        || profiles.len() > BROWSER_PROFILE_MAX_PROFILES
        || has_duplicate_u64(
            &profiles
                .iter()
                .map(|profile_id| profile_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let active_profile = json_u64_field(object, "activeProfileId")
        .map(BrowserProfileId)
        .filter(|profile_id| profiles.contains(profile_id))
        .unwrap_or_else(BrowserProfileId::default_profile);
    let max_profile_id = profiles
        .iter()
        .map(|profile_id| profile_id.0)
        .max()
        .unwrap_or(BROWSER_PROFILE_DEFAULT_ID);
    let next_profile_id = json_u64_field(object, "nextProfileId")
        .unwrap_or(BROWSER_PROFILE_FIRST_GENERATED_ID)
        .max(BROWSER_PROFILE_FIRST_GENERATED_ID)
        .max(max_profile_id.saturating_add(1));

    Some(BrowserProfileModel {
        profiles,
        active_profile,
        next_profile_id,
    })
}


pub(crate) fn browser_tab_model_to_shell_state_json(model: &BrowserTabModel) -> serde_json::Value {
    serde_json::json!({
        "activeTabId": model.active_tab.0,
        "focusedPaneId": model.focused_pane.0,
        "nextPaneId": model.next_pane_id,
        "nextSplitId": model.next_split_id,
        "nextTabId": model.next_tab_id,
        "root": browser_node_to_shell_state_json(&model.root),
        "tabs": model
            .tabs
            .iter()
            .map(|tab| {
                let sanitized_url = sanitize_browser_tab_url_for_state(&tab.url);
                let state = if tab.state == BrowserTabState::Loaded && sanitized_url.is_some() {
                    BrowserTabState::Loaded
                } else {
                    BrowserTabState::AddressOnly
                };
                let history = if state == BrowserTabState::Loaded {
                    browser_navigation_history_to_shell_state_json(
                        &tab.navigation_history,
                        sanitized_url.as_deref(),
                    )
                } else {
                    None
                };
                /*
                CDXC:GPUIBrowserTabTitleCache 2026-07-12:
                Persist the tab's last displayed title so restart shows the
                same sidebar/tab-strip label instead of regressing to the
                URL-host fallback (e.g. "Google.com" for a tab that showed
                "New Tab"). Only Loaded tabs with a sanitized URL carry a
                cached title, and it is bounded before serialization.
                */
                let cached_title = if state == BrowserTabState::Loaded {
                    sanitize_browser_tab_cached_title(&tab.display_title())
                } else {
                    None
                };
                serde_json::json!({
                    "id": tab.id.0,
                    "profileId": tab.profile_id.0,
                    "state": state.element_slug(),
                    "url": sanitized_url.unwrap_or_default(),
                    "history": history,
                    "cachedTitle": cached_title,
                })
            })
            .collect::<Vec<_>>(),
    })
}


pub(crate) fn browser_navigation_history_to_shell_state_json(
    history: &BrowserNavigationHistory,
    sanitized_tab_url: Option<&str>,
) -> Option<serde_json::Value> {
    /*
    CDXC:GPUIBrowserHistoryPrivacy 2026-06-22-10:09:
    Serialize Browser history at the writer boundary with the same strict tab URL sanitizer used for Browser shell state. If the current entry cannot be represented as the tab's sanitized loaded URL, omit the history object so restore rebuilds from the safe loaded URL instead of persisting raw navigation details.
    */
    let current_index = history
        .current_index
        .filter(|index| *index < history.entries.len())?;
    let mut sanitized_entries = Vec::new();
    let mut sanitized_current_index = None;
    for (index, url) in history.entries.iter().enumerate() {
        let sanitized_url = sanitize_browser_tab_url_for_state(url)?;
        if index == current_index {
            sanitized_current_index = Some(sanitized_entries.len());
        }
        sanitized_entries.push(sanitized_url);
    }

    let sanitized_current_index = sanitized_current_index?;
    if sanitized_entries.is_empty()
        || sanitized_entries.len() > BROWSER_HISTORY_MAX_ENTRIES
        || sanitized_tab_url.is_some_and(|url| {
            sanitized_entries
                .get(sanitized_current_index)
                .map(String::as_str)
                != Some(url)
        })
    {
        return None;
    }

    Some(serde_json::json!({
        "entries": sanitized_entries,
        "currentIndex": sanitized_current_index,
    }))
}


pub(crate) fn browser_node_to_shell_state_json(node: &BrowserNode) -> serde_json::Value {
    match node {
        BrowserNode::Leaf(leaf) => serde_json::json!({
            "type": "leaf",
            "paneId": leaf.pane_id.0,
            "activeTabId": leaf.tab_group.active_tab.0,
            "tabs": leaf
                .tab_group
                .tabs
                .iter()
                .map(|tab| serde_json::json!(tab.tab_id.0))
                .collect::<Vec<_>>(),
        }),
        BrowserNode::Split(split) => serde_json::json!({
            "type": "split",
            "splitId": split.id.0,
            "axis": split.axis.element_slug(),
            "ratio": json_number_f32(workspace_split_ratio(split.ratio)),
            "first": browser_node_to_shell_state_json(&split.first),
            "second": browser_node_to_shell_state_json(&split.second),
        }),
    }
}


pub(crate) fn browser_tab_model_from_shell_state(
    value: &serde_json::Value,
    browser_profiles: &BrowserProfileModel,
) -> Option<BrowserTabModel> {
    let object = value.as_object()?;
    let tabs = json_array_field(object, "tabs")?
        .iter()
        .map(|value| browser_tab_from_shell_state(value, browser_profiles))
        .collect::<Option<Vec<_>>>()?;
    if tabs.is_empty() || has_duplicate_u64(&tabs.iter().map(|tab| tab.id.0).collect::<Vec<_>>()) {
        return None;
    }
    let tab_ids = tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
    let root = if let Some(root_value) = object.get("root") {
        browser_node_from_shell_state(root_value, &tab_ids)?
    } else {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Pre-split shell state stored Browser tabs as a flat strip. Restore that older private-data-safe shape as one Browser pane so users keep sanitized tab ids/URLs while the new split tree becomes the durable layout representation after the next save.
        */
        BrowserNode::Leaf(BrowserLeaf {
            pane_id: BrowserPaneId(1),
            tab_group: BrowserTabGroup {
                active_tab: json_u64_field(object, "activeTabId")
                    .map(BrowserTabId)
                    .filter(|tab_id| tab_ids.contains(tab_id))
                    .unwrap_or(tabs[0].id),
                tabs: tab_ids
                    .iter()
                    .copied()
                    .map(|tab_id| BrowserPaneTab { tab_id })
                    .collect(),
            },
        })
    };

    let mut pane_ids = Vec::new();
    collect_browser_leaf_ids(&root, &mut pane_ids);
    if pane_ids.is_empty()
        || has_duplicate_u64(&pane_ids.iter().map(|pane_id| pane_id.0).collect::<Vec<_>>())
    {
        return None;
    }

    let mut referenced_tab_ids = Vec::new();
    collect_browser_tab_ids(&root, &mut referenced_tab_ids);
    if referenced_tab_ids.is_empty()
        || has_duplicate_u64(
            &referenced_tab_ids
                .iter()
                .map(|tab_id| tab_id.0)
                .collect::<Vec<_>>(),
        )
        || referenced_tab_ids
            .iter()
            .any(|tab_id| !tab_ids.contains(tab_id))
    {
        return None;
    }

    let tabs = tabs
        .into_iter()
        .filter(|tab| referenced_tab_ids.contains(&tab.id))
        .collect::<Vec<_>>();
    if tabs.is_empty() {
        return None;
    }
    let first_pane_id = pane_ids.first().copied()?;
    let focused_pane = json_u64_field(object, "focusedPaneId")
        .map(BrowserPaneId)
        .filter(|pane_id| pane_ids.contains(pane_id))
        .unwrap_or(first_pane_id);
    let focused_pane_active_tab =
        find_browser_leaf(&root, focused_pane).and_then(|leaf| leaf.tab_group.active_tab_id());
    let active_tab = json_u64_field(object, "activeTabId")
        .map(BrowserTabId)
        .filter(|tab_id| Some(*tab_id) == focused_pane_active_tab)
        .or(focused_pane_active_tab)
        .or_else(|| first_browser_tab_id(&root))
        .unwrap_or(tabs[0].id);
    let next_pane_id = json_u64_field(object, "nextPaneId")
        .unwrap_or(0)
        .max(pane_ids.iter().map(|pane_id| pane_id.0).max().unwrap_or(0) + 1);
    let mut split_ids = Vec::new();
    collect_browser_split_ids(&root, &mut split_ids);
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
    let next_tab_id = json_u64_field(object, "nextTabId")
        .unwrap_or(0)
        .max(tabs.iter().map(|tab| tab.id.0).max().unwrap_or(0) + 1);

    Some(BrowserTabModel {
        tabs,
        root,
        focused_pane,
        active_tab,
        next_pane_id,
        next_split_id,
        next_tab_id,
    })
}

