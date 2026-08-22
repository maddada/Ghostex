// C1 wave-3 re-cluster: the Agents workspace pane/split tree node types and the tree manipulation, close-focus, and session-mapping functions over them, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WorkspacePaneId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct WorkspaceSplitId(pub(crate) u64);


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceSplitAxis {
    Horizontal,
    Vertical,
}


impl WorkspaceSplitAxis {
    pub(crate) fn from_slug(value: &str) -> Option<Self> {
        match value {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }

    pub(crate) fn element_slug(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceDropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceDropTarget {
    TabStrip(usize),
    PaneBody(WorkspaceDropZone),
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceDropFeedback {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) target: WorkspaceDropTarget,
}


#[derive(Clone, Copy)]
pub(crate) struct WorkspaceCloseFocusBounds {
    pub(crate) left: f32,
    pub(crate) right: f32,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}


pub(crate) struct WorkspaceCloseFocusRect {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) bounds: WorkspaceCloseFocusBounds,
    pub(crate) center_x: f32,
    pub(crate) center_y: f32,
    pub(crate) path: Vec<usize>,
    pub(crate) has_tabs: bool,
}


#[derive(Clone)]
pub(crate) struct DraggedWorkspaceTab {
    pub(crate) source_pane_id: WorkspacePaneId,
    pub(crate) session_id: TerminalSessionId,
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) agent_icon: Option<&'static str>,
}


pub(crate) struct WorkspaceTabDragPreview {
    pub(crate) title: String,
    pub(crate) presentation_state: TerminalSessionPresentationState,
    pub(crate) tab_status: AgentTerminalTabStatus,
    pub(crate) agent_icon: Option<&'static str>,
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceTab {
    pub(crate) session_id: TerminalSessionId,
}


#[derive(Clone)]
pub(crate) struct WorkspaceLeaf {
    pub(crate) pane_id: WorkspacePaneId,
    pub(crate) tab_group: WorkspaceTabGroup,
}


#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct WorkspaceSplit {
    pub(crate) id: WorkspaceSplitId,
    pub(crate) axis: WorkspaceSplitAxis,
    pub(crate) ratio: f32,
    pub(crate) default_ratio: f32,
    pub(crate) first: Box<WorkspaceNode>,
    pub(crate) second: Box<WorkspaceNode>,
}


#[allow(dead_code)]
#[derive(Clone)]
pub(crate) enum WorkspaceNode {
    Split(WorkspaceSplit),
    Leaf(WorkspaceLeaf),
}


pub(crate) fn workspace_terminal_session_mapping_get(
    key: &GpuiWorkspaceTerminalSessionKey,
    local_workspace_session_mappings: &HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) -> Option<TerminalSessionId> {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.get(key).copied()
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => remote_attach_sessions.get(key).copied(),
    }
}


pub(crate) fn workspace_terminal_session_mapping_insert(
    key: GpuiWorkspaceTerminalSessionKey,
    shell_session_id: TerminalSessionId,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &mut HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.insert(key, shell_session_id);
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => {
            remote_attach_sessions.insert(key, shell_session_id);
        }
    }
}


pub(crate) fn workspace_terminal_session_mapping_remove(
    key: &GpuiWorkspaceTerminalSessionKey,
    local_workspace_session_mappings: &mut HashMap<GpuiLocalWorkspaceSessionKey, TerminalSessionId>,
    remote_attach_sessions: &mut HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
) {
    match key {
        GpuiWorkspaceTerminalSessionKey::Local(key) => {
            local_workspace_session_mappings.remove(key);
        }
        GpuiWorkspaceTerminalSessionKey::Remote(key) => {
            remote_attach_sessions.remove(key);
        }
    }
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
