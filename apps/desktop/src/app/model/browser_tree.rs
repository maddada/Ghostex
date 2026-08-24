// C1 wave-3 re-cluster: the browser pane/split tree node manipulation functions, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

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

pub(crate) fn find_browser_leaf(
    node: &BrowserNode,
    pane_id: BrowserPaneId,
) -> Option<&BrowserLeaf> {
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

pub(crate) fn find_browser_split(
    node: &BrowserNode,
    split_id: BrowserSplitId,
) -> Option<&BrowserSplit> {
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

pub(crate) fn find_browser_leaf_id_for_tab(
    node: &BrowserNode,
    tab_id: BrowserTabId,
) -> Option<BrowserPaneId> {
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
