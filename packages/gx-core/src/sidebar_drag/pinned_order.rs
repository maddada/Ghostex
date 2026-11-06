//! One order for pinned rows drawn together: the Project sidebar's Pinned section and the Unified
//! sidebar's pinned cards.
//!
//! CDXC:Sidebar 2026-11-07 WHY:
//! User: "im not able to rearrange pinned sessions in the sidebar anymore". A Pinned section can
//! hold rows of several projects, which the old move rules (session_move.rs `plan_pinned_move`,
//! one project's own pinned rows) could not reorder: another computer's copy of the project in the
//! one list (sidebar_view/merged.rs, 2026-11-05) and a worktree's sessions folded into their parent
//! (sidebar_view/worktree_fold.rs, 2026-11-06). A drag that involved one of them was refused or
//! landed where it was. The order is now written over the rows as they are drawn: each pinned row
//! takes `sidebarOrder` 1000, 2000, ... in its new place through the pin call on its own machine
//! (`setSessionPinned` with `sidebarOrder`, which gxserver keeps), and both sidebars sort pinned
//! rows by that one number (sidebar_view/ordering.rs `sort_pinned`, unified_sidebar/build.rs).

use serde_json::{json, Value};

/// A pinned drop: the rows' new order, and the pin calls that write it (only for rows whose
/// number changes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinnedReorder {
    pub order: Vec<String>,
    pub messages: Vec<Value>,
}

/// `rows` (sidebar session ids with their saved order, as drawn) with `moved` placed before or
/// after `target`. `None` when either is not one of them or nothing moves.
pub fn plan_pinned_reorder(
    rows: &[(String, Option<i64>)],
    moved: &str,
    target: &str,
    after: bool,
) -> Option<PinnedReorder> {
    if moved == target || !rows.iter().any(|(id, _)| id == moved) {
        return None;
    }
    let drawn: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
    let mut order: Vec<String> = drawn.iter().filter(|id| *id != moved).cloned().collect();
    let index = order.iter().position(|id| id == target)? + usize::from(after);
    order.insert(index, moved.to_string());
    if order == drawn {
        return None;
    }
    let messages = order
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let sidebar_order = ((index + 1) * 1000) as i64;
            let saved = rows
                .iter()
                .find(|(row, _)| row == id)
                .and_then(|(_, order)| *order);
            (saved != Some(sidebar_order)).then(|| {
                json!({
                    "type": "setSessionPinned",
                    "sessionId": id,
                    "pinned": true,
                    "sidebarOrder": sidebar_order,
                })
            })
        })
        .collect();
    Some(PinnedReorder { order, messages })
}

#[cfg(test)]
#[path = "pinned_order_tests.rs"]
mod tests;
