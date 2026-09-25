//! `moveSession`: a session dragged inside its group, or onto another one.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! This is the order-write family, not the action family: the drop makes no daemon call in the
//! moment. It posts one of two messages, and what those messages DO is in `order_write.rs`. The
//! planner's whole job is the SET and the ORDER, computed against the projection's membership
//! (`inventory.rs`) and not against the drawn list, plus the four guards the TypeScript applies
//! before it: a pinned row may only be dropped next to another pinned row of its own group, an
//! unpinned one only while the sort mode is Manual, a remote row not at all, and a browser row not
//! at all.
//!
//! **A no-op drop is not a no-op.** Dropping a row exactly where it already is leaves
//! `moveSessionIdsByDropTarget` returning the map unchanged, and `reorderNativeSidebar` posts
//! `syncSessionOrder` anyway; the runtime then wrote the key and booked a push for a document that
//! did not move. That is reproduced rather than optimized away, because the parity gate compared
//! what the shipped code did and a port that wrote nothing there would make one fewer push per drag.
//! The TypeScript was frozen in the deleted `tooling/gx-core/sidebar-page-frozen/reorder.ts`.
//!
//! SEE-ALSO: packages/core-ui/sidebar-dnd.ts (`moveSessionIdsByDropTarget`),
//! apps/desktop/src/app/gx_store/sidebar_drag.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::sidebar_view::SessionSortMode;
use crate::sidebar_view::SidebarInputs;

use super::inventory::{group_by_id, group_of_session, MoveGroup};

/// The prefix a browser row's id carries. A tab is host state and not a daemon session.
const BROWSER_ROW_PREFIX: &str = "gpui-browser:";

/// What a `moveSession` payload does: the messages the renderer posts, in order.
///
/// An EMPTY list is a real answer and not a refusal. Every guard in `reorderNativeSidebar` is a
/// bare `return`, so "this drop is not allowed" and "nothing happens" are the same thing on both
/// sides, and the gate compares that rather than skipping it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionMovePlan {
    pub messages: Vec<Value>,
    /// Why the plan is empty, for the record line. Never compared: the TypeScript has no such
    /// word, it just returns.
    pub refusal: Option<&'static str>,
}

impl SessionMovePlan {
    fn refused(reason: &'static str) -> Self {
        Self {
            messages: Vec::new(),
            refusal: Some(reason),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({ "messages": self.messages })
    }
}

/// Whether this payload is one this file answers.
pub fn owns_session_move_command(command: &Value) -> bool {
    command.get("type").and_then(Value::as_str) == Some("moveSession")
}

/// The `moveSession` command, or `None` when the store must not answer it.
///
/// Refused, with the reason at each refusal. Both refusals are about a row this store cannot see
/// rather than a rule:
///
/// - The dragged row or the hovered row is a BROWSER row (`gpui-browser:`). The desktop host feeds
///   no browser tabs into the store since 2026-09-20, so a tab is not in any group's membership
///   here while the old runtime's own `this.browserTabs` still lists it. A hovered tab would give
///   the two sides different insert indices, so the payload is handed over whole. NOT EXERCISED BY
///   THE GATE, and said here rather than left to look covered: the harness's projection comes from
///   `createSidebarGroups`, which does not splice browser rows (they are added later, in
///   `createNativeSidebarSnapshot`), so the TypeScript half has none either and the two would agree
///   for the wrong reason. Until the harness can carry a tab, this refusal is argued, not measured.
/// - The payload is malformed (no `sessionId`, no `position`, no `groupId`). The TypeScript would
///   read `undefined` through its own guards; a port that guessed a default would post an order the
///   app never posts.
///
/// Everything else is answered, including the four cases the TypeScript refuses by returning,
/// which are answered here as a plan with no messages.
pub fn plan_session_move(
    core: &Core,
    inputs: &SidebarInputs,
    command: &Value,
) -> Option<SessionMovePlan> {
    if !owns_session_move_command(command) {
        return None;
    }
    let session_id = command.get("sessionId")?.as_str()?;
    let group_id = command.get("groupId")?.as_str()?;
    let position = command.get("position")?.as_str()?;
    if position != "before" && position != "after" {
        return None;
    }
    let target_session_id = command.get("targetSessionId").and_then(Value::as_str);
    if session_id.starts_with(BROWSER_ROW_PREFIX)
        || target_session_id.is_some_and(|id| id.starts_with(BROWSER_ROW_PREFIX))
    {
        return None;
    }

    // `state.groupOrder.find(id => sessionIdsByGroup[id]?.includes(sessionId))`, then
    // `groupsById[sourceGroup]` and `groupsById[command.groupId]`.
    let Some(source) = group_of_session(core, inputs, session_id) else {
        return Some(SessionMovePlan::refused("sourceRowNotListed"));
    };
    let Some(target) = group_by_id(core, inputs, group_id) else {
        return Some(SessionMovePlan::refused("targetGroupMissing"));
    };
    if !source.machine.is_local() || !target.machine.is_local() {
        return Some(SessionMovePlan::refused("remoteGroup"));
    }
    // `state.sessionsById[targetSessionId]`, which is every row of every group and not the target
    // group's own list: the hovered row may be in another group entirely, and the TypeScript only
    // asks whether it exists.
    let hovered = match target_session_id {
        None => None,
        Some(target_session_id) => {
            let Some(row) = row_anywhere(core, inputs, target_session_id) else {
                return Some(SessionMovePlan::refused("hoveredRowNotListed"));
            };
            Some(row)
        }
    };
    let source_row = source
        .rows
        .iter()
        .find(|row| row.sidebar_session_id == session_id)
        .expect("group_of_session answers only for a group holding the row");

    if source_row.is_pinned {
        return Some(plan_pinned_move(
            &source,
            &target,
            session_id,
            target_session_id,
            hovered.is_some_and(|pinned| pinned),
            position,
        ));
    }
    // `state.hud.activeSessionsSortMode !== 'manual'`: an unpinned row keeps the order the sort
    // mode gives it, so a drag that cannot be saved is refused rather than shown and lost.
    if inputs.settings.sort_mode != SessionSortMode::Manual {
        return Some(SessionMovePlan::refused("sortModeNotManual"));
    }
    Some(plan_unpinned_move(
        &source,
        &target,
        session_id,
        target_session_id,
        position,
    ))
}

/// The pinned leg. A pinned row may only move among the pinned rows of its OWN group, next to
/// another pinned row, and the order posted is the pinned rows in their new order followed by
/// every unpinned row untouched.
fn plan_pinned_move(
    source: &MoveGroup,
    target: &MoveGroup,
    session_id: &str,
    target_session_id: Option<&str>,
    hovered_is_pinned: bool,
    position: &str,
) -> SessionMovePlan {
    if source.group_id != target.group_id {
        return SessionMovePlan::refused("pinnedAcrossGroups");
    }
    let Some(target_session_id) = target_session_id else {
        return SessionMovePlan::refused("pinnedWithoutHoveredRow");
    };
    if !hovered_is_pinned {
        return SessionMovePlan::refused("pinnedOntoUnpinned");
    }
    let mut pinned: Vec<String> = source
        .rows
        .iter()
        .filter(|row| row.is_pinned && row.sidebar_session_id != session_id)
        .map(|row| row.sidebar_session_id.clone())
        .collect();
    let Some(index) = pinned.iter().position(|id| id == target_session_id) else {
        return SessionMovePlan::refused("hoveredPinnedRowNotInGroup");
    };
    pinned.insert(
        index + usize::from(position == "after"),
        session_id.to_string(),
    );
    let order: Vec<String> = pinned
        .into_iter()
        .chain(
            source
                .rows
                .iter()
                .filter(|row| !row.is_pinned)
                .map(|row| row.sidebar_session_id.clone()),
        )
        .collect();
    SessionMovePlan {
        messages: vec![sync_session_order(&target.group_id, &order)],
        refusal: None,
    }
}

/// The unpinned leg: `moveSessionIdsByDropTarget`, then one of the two messages.
fn plan_unpinned_move(
    source: &MoveGroup,
    target: &MoveGroup,
    session_id: &str,
    target_session_id: Option<&str>,
    position: &str,
) -> SessionMovePlan {
    let source_ids = source.session_ids();
    let target_ids = target.session_ids();
    let same_group = source.group_id == target.group_id;
    let source_index = source_ids
        .iter()
        .position(|id| id == session_id)
        .expect("the source group holds the row");
    // `getTargetInsertIndex`. A drop on the GROUP goes to the start or the end; a drop on a ROW
    // goes before or after it, and a hovered row the target group does not hold answers nothing at
    // all, which leaves the map untouched.
    let insert_index = match target_session_id {
        None => Some(match position {
            "after" => target_ids.len(),
            _ => 0,
        }),
        Some(hovered) => target_ids
            .iter()
            .position(|id| id == hovered)
            .map(|index| index + usize::from(position == "after")),
    };

    if same_group {
        let moved = insert_index.and_then(|insert_index| {
            let adjusted = match insert_index > source_index {
                true => insert_index - 1,
                false => insert_index,
            };
            if adjusted == source_index {
                return None;
            }
            let mut next = source_ids.clone();
            next.remove(source_index);
            next.insert(adjusted.min(next.len()), session_id.to_string());
            Some(next)
        });
        // The message is posted whether or not the order moved: the TypeScript reads
        // `next[groupId]` out of the map it got back, and an unchanged map still holds the group.
        let order = moved.unwrap_or(source_ids);
        return SessionMovePlan {
            messages: vec![sync_session_order(&target.group_id, &order)],
            refusal: None,
        };
    }

    let next_target: Vec<String> = match insert_index {
        None => target_ids,
        Some(insert_index) => {
            let mut next = target_ids;
            let at = insert_index.min(next.len());
            next.insert(at, session_id.to_string());
            next
        }
    };
    // `next[command.groupId]?.indexOf(command.sessionId)`: JavaScript's `indexOf`, so a row the
    // insert never happened for reaches the runtime as -1 and is clamped to the front there. That
    // is reachable, not theoretical: it is what a drop on a row of a THIRD group produces.
    let target_index = next_target
        .iter()
        .position(|id| id == session_id)
        .map(|index| index as i64)
        .unwrap_or(-1);
    SessionMovePlan {
        messages: vec![json!({
            "type": "moveSessionToGroup",
            "groupId": target.group_id,
            "sessionId": session_id,
            "targetIndex": target_index,
        })],
        refusal: None,
    }
}

fn sync_session_order(group_id: &str, session_ids: &[String]) -> Value {
    json!({
        "type": "syncSessionOrder",
        "groupId": group_id,
        "sessionIds": session_ids,
    })
}

/// `state.sessionsById[id]?.isPinned`: whether the row exists in ANY group, and whether it is
/// pinned. `None` is a row the projection has no entry for.
fn row_anywhere(core: &Core, inputs: &SidebarInputs, sidebar_session_id: &str) -> Option<bool> {
    let group = group_of_session(core, inputs, sidebar_session_id)?;
    group
        .rows
        .iter()
        .find(|row| row.sidebar_session_id == sidebar_session_id)
        .map(|row| row.is_pinned)
}
