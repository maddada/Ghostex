// C1 wave-3 re-cluster: the command pane split/leaf tree node type and its tree manipulation functions, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).
#![allow(dead_code)]

use crate::*;


pub(crate) fn collect_command_tabs(
    node: &CommandPaneNode,
    tabs: &mut Vec<(CommandPaneGroupId, CommandSessionId)>,
) {
    match node {
        CommandPaneNode::Leaf(leaf) => {
            tabs.extend(
                leaf.tab_group
                    .tabs
                    .iter()
                    .map(|tab| (leaf.group_id, tab.session_id)),
            );
        }
        CommandPaneNode::Split(split) => {
            collect_command_tabs(&split.first, tabs);
            collect_command_tabs(&split.second, tabs);
        }
    }
}


pub(crate) fn first_command_leaf(node: &CommandPaneNode) -> Option<&CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) if !leaf.tab_group.tabs.is_empty() => Some(leaf),
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            first_command_leaf(&split.first).or_else(|| first_command_leaf(&split.second))
        }
    }
}


pub(crate) fn first_command_leaf_id(node: &CommandPaneNode) -> Option<CommandPaneGroupId> {
    first_command_leaf(node).map(|leaf| leaf.group_id)
}


pub(crate) fn find_command_leaf(
    node: &CommandPaneNode,
    group_id: CommandPaneGroupId,
) -> Option<&CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) => (leaf.group_id == group_id).then_some(leaf),
        CommandPaneNode::Split(split) => find_command_leaf(&split.first, group_id)
            .or_else(|| find_command_leaf(&split.second, group_id)),
    }
}


pub(crate) fn find_command_leaf_mut(
    node: &mut CommandPaneNode,
    group_id: CommandPaneGroupId,
) -> Option<&mut CommandPaneLeaf> {
    match node {
        CommandPaneNode::Leaf(leaf) => (leaf.group_id == group_id).then_some(leaf),
        CommandPaneNode::Split(split) => find_command_leaf_mut(&mut split.first, group_id)
            .or_else(|| find_command_leaf_mut(&mut split.second, group_id)),
    }
}


pub(crate) fn find_command_split(
    node: &CommandPaneNode,
    split_id: CommandPaneSplitId,
) -> Option<&CommandPaneSplit> {
    match node {
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_command_split(&split.first, split_id)
                    .or_else(|| find_command_split(&split.second, split_id))
            }
        }
    }
}


pub(crate) fn find_command_split_mut(
    node: &mut CommandPaneNode,
    split_id: CommandPaneSplitId,
) -> Option<&mut CommandPaneSplit> {
    match node {
        CommandPaneNode::Leaf(_) => None,
        CommandPaneNode::Split(split) => {
            if split.id == split_id {
                Some(split)
            } else {
                find_command_split_mut(&mut split.first, split_id)
                    .or_else(|| find_command_split_mut(&mut split.second, split_id))
            }
        }
    }
}


pub(crate) fn command_node_contains_group(node: &CommandPaneNode, group_id: CommandPaneGroupId) -> bool {
    find_command_leaf(node, group_id).is_some()
}


pub(crate) fn insert_command_leaf_split(
    node: &mut CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    new_leaf: CommandPaneLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) -> bool {
    let should_rebalance_default_axis_chain =
        command_insert_target_axis_chain_uses_native_default_ratios(node, target_group_id, axis);
    let inserted = insert_command_leaf_split_inner(
        node,
        target_group_id,
        new_leaf,
        axis,
        dragged_first,
        split_id,
    );
    if inserted && should_rebalance_default_axis_chain {
        rebalance_command_split_axis_chain_containing_group(node, target_group_id, axis);
    }
    inserted
}


pub(crate) fn insert_command_leaf_split_inner(
    node: &mut CommandPaneNode,
    target_group_id: CommandPaneGroupId,
    new_leaf: CommandPaneLeaf,
    axis: WorkspaceSplitAxis,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) -> bool {
    match node {
        CommandPaneNode::Leaf(leaf) if leaf.group_id == target_group_id => {
            let existing = std::mem::replace(node, command_pane_dummy_node());
            let new_node = CommandPaneNode::Leaf(new_leaf);
            let (first, second) = if dragged_first {
                (new_node, existing)
            } else {
                (existing, new_node)
            };

            *node = CommandPaneNode::Split(CommandPaneSplit {
                id: split_id,
                axis,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            });
            true
        }
        CommandPaneNode::Leaf(_) => false,
        CommandPaneNode::Split(split) => {
            let target_in_first = command_node_contains_group(&split.first, target_group_id);
            let target_in_second = command_node_contains_group(&split.second, target_group_id);
            if split.axis == axis && (target_in_first || target_in_second) {
                insert_command_leaf_at_same_axis_split(
                    split,
                    target_in_first,
                    new_leaf,
                    dragged_first,
                    split_id,
                );
                true
            } else if target_in_first {
                insert_command_leaf_split_inner(
                    &mut split.first,
                    target_group_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            } else {
                insert_command_leaf_split_inner(
                    &mut split.second,
                    target_group_id,
                    new_leaf,
                    axis,
                    dragged_first,
                    split_id,
                )
            }
        }
    }
}


pub(crate) fn insert_command_leaf_at_same_axis_split(
    split: &mut CommandPaneSplit,
    target_in_first: bool,
    new_leaf: CommandPaneLeaf,
    dragged_first: bool,
    split_id: CommandPaneSplitId,
) {
    /*
    CDXC:GPUICommandPaneSplits 2026-06-25-16:18:
    Native same-direction command split insertion happens beside the matching child of the existing split, not inside that child. Preserve that boundary in GPUI's binary model so explicit first-child ratios keep native meaning when the user later inserts before or after a command pane in a resized split.
    */
    let existing_first = take_command_node(&mut split.first);
    let existing_second = take_command_node(&mut split.second);
    let new_node = CommandPaneNode::Leaf(new_leaf);
    let axis = split.axis;

    let (first, second_first, second_second) = match (target_in_first, dragged_first) {
        (true, true) => (new_node, existing_first, existing_second),
        (true, false) => (existing_first, new_node, existing_second),
        (false, true) => (existing_first, new_node, existing_second),
        (false, false) => (existing_first, existing_second, new_node),
    };

    split.first = Box::new(first);
    split.second = Box::new(CommandPaneNode::Split(CommandPaneSplit {
        id: split_id,
        axis,
        ratio: 0.5,
        first: Box::new(second_first),
        second: Box::new(second_second),
    }));
}


pub(crate) fn command_insert_target_axis_chain_uses_native_default_ratios(
    node: &CommandPaneNode,
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
                command_split_axis_tree_uses_native_default_ratios(node, axis)
            } else if target_in_first {
                command_insert_target_axis_chain_uses_native_default_ratios(
                    &split.first,
                    target_group_id,
                    axis,
                )
            } else {
                command_insert_target_axis_chain_uses_native_default_ratios(
                    &split.second,
                    target_group_id,
                    axis,
                )
            }
        }
    }
}


pub(crate) fn command_split_axis_tree_uses_native_default_ratios(
    node: &CommandPaneNode,
    axis: WorkspaceSplitAxis,
) -> bool {
    match node {
        CommandPaneNode::Leaf(_) => true,
        CommandPaneNode::Split(split) => {
            (split.axis != axis || command_split_ratio_matches_native_default(split))
                && command_split_axis_tree_uses_native_default_ratios(&split.first, axis)
                && command_split_axis_tree_uses_native_default_ratios(&split.second, axis)
        }
    }
}


pub(crate) fn command_split_ratio_matches_native_default(split: &CommandPaneSplit) -> bool {
    let Some(expected_ratio) = command_split_native_default_ratio(split) else {
        return false;
    };
    (workspace_split_ratio(split.ratio) - expected_ratio).abs() < 0.001
}


pub(crate) fn command_split_native_default_ratio(split: &CommandPaneSplit) -> Option<f32> {
    let first_count = command_node_leaf_count(&split.first);
    let second_count = command_node_leaf_count(&split.second);
    let total_count = first_count + second_count;
    if total_count < 2 {
        return None;
    }
    Some(workspace_split_ratio(
        first_count as f32 / total_count as f32,
    ))
}


pub(crate) fn command_node_leaf_count(node: &CommandPaneNode) -> usize {
    match node {
        CommandPaneNode::Leaf(leaf) => usize::from(!leaf.tab_group.tabs.is_empty()),
        CommandPaneNode::Split(split) => {
            command_node_leaf_count(&split.first) + command_node_leaf_count(&split.second)
        }
    }
}




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
