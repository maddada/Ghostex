//! A session row dropped in the sidebar: the messages the drop posts and where the row lands,
//! from one plan, so the drop line and the move cannot disagree.
//!
//! CDXC:Sidebar 2026-10-01 DECISION:
//! User: "When I drag a session between sessions into Pinned, it's not added to Pinned at the position I dropped it, and we don't show the drop line for it", and "make sure drop lines work perfectly in the sidebar". A session dropped on a row of another section of its group (`moveSessionToSection`) still takes that section's flags (CDXC:Sidebar 2026-09-24 in native_sidebar/section_move.rs), and a drop into Pinned now also lands at the drop position: before or after the hovered pinned row, or at the end of Pinned when dropped on the heading. The pin carries the `sidebarOrder` the order write gives the row, so gxserver's pin-at-the-bottom rule (CDXC:Sessions 2026-09-29) cannot overtake the order write whichever call lands first. The drop line is drawn from [`SessionDrop::landing`], which lays the group out again with the drop's writes applied, so it shows where the row really lands; a drop that moves nothing draws no line.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/session_handoff.rs (a drop on another project),
//! packages/gx-core/src/sidebar_view/drop_landing.rs,
//! apps/desktop/src/app/native_sidebar/drag.rs, apps/desktop/src/app/native_sidebar/section_move.rs.

use serde_json::{json, Value};

use crate::core::Core;
use crate::keys::SessionKey;
use crate::sidebar_view::{
    DropLanding, DropWrites, SidebarInputs, SidebarViewModel, TreeDropTarget,
};

use super::inventory::{group_by_id, group_of_session};
use super::session_handoff::{check_session_handoff, SessionHandoffCheck, SessionHandoffDrop};
use super::session_move::plan_session_move;

/// What a session drop does, and where the row ends up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionDrop {
    /// The messages to post, in order: flags first (`setSessionParked`, `setSessionPinned`), then
    /// the order (`syncSessionOrder`, `moveSessionToGroup`).
    pub messages: Vec<Value>,
    /// `None` when the list did not build the group (the row cannot be drawn there).
    pub landing: Option<DropLanding>,
    /// The command as planned: a drop on a row of a coordinator's tree aims at the coordinator
    /// (`SidebarViewModel::tree_drop_target`), so this is what the host must perform.
    pub command: Value,
    /// The drop hands the session over to a new one in another project instead of moving a row:
    /// no messages and no landing, and the host asks first.
    pub handoff: Option<SessionHandoffDrop>,
}

/// Whether this payload is one [`plan_session_drop`] answers.
pub fn owns_session_drop_command(command: &Value) -> bool {
    matches!(
        command.get("type").and_then(Value::as_str),
        Some("moveSession" | "moveSessionToSection")
    )
}

/// The drop, or `None` when it is refused (nothing would happen, so nothing is drawn either).
pub fn plan_session_drop(
    core: &Core,
    inputs: &SidebarInputs,
    model: &SidebarViewModel,
    command: &Value,
    now_ms: u64,
) -> Option<SessionDrop> {
    let session_id = command.get("sessionId")?.as_str()?;
    // A pinned row next to another pinned row of the same drawn list, whatever project or machine
    // each is from (pinned_order.rs); ahead of the handoff check, which refuses a folded worktree
    // row inside its parent.
    if let Some(drop) = plan_drawn_pinned_move(inputs, model, command) {
        return drop;
    }
    // Ahead of every move rule below, which would refuse a row leaving its project.
    match check_session_handoff(core, inputs, command) {
        SessionHandoffCheck::Handoff(handoff) => {
            return Some(SessionDrop {
                messages: Vec::new(),
                landing: None,
                command: command.clone(),
                handoff: Some(handoff),
            });
        }
        SessionHandoffCheck::Refused(_) => return None,
        SessionHandoffCheck::NotAcross => {}
    }
    let mut command = command.clone();
    // A section heading of another project aims at that project and at nothing else; one of
    // another group of the row's own project takes the row into that group.
    if command.get("handoffOnly").and_then(Value::as_bool) == Some(true) {
        if !heading_takes_row_into_group(core, inputs, &command, session_id) {
            return None;
        }
        if let Some(object) = command.as_object_mut() {
            object.remove("handoffOnly");
            object.remove("headingSection");
        }
    }
    if let (Some(group_id), Some(hovered)) = (
        command.get("groupId").and_then(Value::as_str),
        command.get("targetSessionId").and_then(Value::as_str),
    ) {
        match model.tree_drop_target(group_id, session_id, hovered) {
            TreeDropTarget::Row => {}
            TreeDropTarget::OwnTree => return None,
            TreeDropTarget::Coordinator {
                session_id: coordinator,
                position,
            } => {
                command["targetSessionId"] = json!(coordinator);
                command["position"] = json!(position);
            }
        }
    }
    let messages = match command.get("type")?.as_str()? {
        "moveSession" => plan_session_move(core, inputs, &command)?.messages,
        "moveSessionToSection" => plan_section_move(core, inputs, model, &command)?,
        _ => return None,
    };
    if messages.is_empty() {
        return None;
    }
    let writes = drop_writes(core, inputs, &command, session_id, &messages)?;
    let landing = model.preview_session_drop(session_id, &writes, now_ms);
    Some(SessionDrop {
        messages,
        landing,
        command,
        handoff: None,
    })
}

/// Whether a session dropped on a section heading of `groupId` moves into that group: the heading
/// is one of another group than the row's own (the project's when the row is in one of its session
/// groups, a session group's when it is not), and not Pinned or Parked, whose flag the move would
/// not give. The move itself is the group header's (`plan_session_move`), which keeps the row in
/// its project.
///
/// CDXC:Sidebar 2026-11-10 WHY:
/// User, of a session group drawn under a project's collapsed Sessions heading: "allow dragging into and out of this pls". Dropping a session on a heading of its own project's other group did nothing: the heading was treated as another project's, which only hands a session over. With the project's Sessions heading collapsed, that heading was the only place outside the group to drop on.
fn heading_takes_row_into_group(
    core: &Core,
    inputs: &SidebarInputs,
    command: &Value,
    session_id: &str,
) -> bool {
    let Some(group_id) = command.get("groupId").and_then(Value::as_str) else {
        return false;
    };
    if matches!(
        command.get("headingSection").and_then(Value::as_str),
        Some("pinned" | "parked")
    ) {
        return false;
    }
    group_of_session(core, inputs, session_id).is_some_and(|source| source.group_id != group_id)
}

/// A `moveSession` of a pinned row onto another pinned row drawn in the same Pinned section:
/// `None` when it is not one (the ordinary rules answer it), `Some(None)` when it is refused
/// (Pinned order is Most recently updated, or nothing moves), else the pin calls that renumber the
/// section and where the row lands (CDXC:Sidebar 2026-11-07 WHY in pinned_order.rs).
fn plan_drawn_pinned_move(
    inputs: &SidebarInputs,
    model: &SidebarViewModel,
    command: &Value,
) -> Option<Option<SessionDrop>> {
    if command.get("type").and_then(Value::as_str) != Some("moveSession") {
        return None;
    }
    let session_id = command.get("sessionId")?.as_str()?;
    let group_id = command.get("groupId")?.as_str()?;
    let hovered = command.get("targetSessionId")?.as_str()?;
    let (target, position) = match model.tree_drop_target(group_id, session_id, hovered) {
        TreeDropTarget::Row => (
            hovered.to_string(),
            command.get("position")?.as_str()?.to_string(),
        ),
        TreeDropTarget::OwnTree => return None,
        TreeDropTarget::Coordinator {
            session_id: coordinator,
            position,
        } => (coordinator, position.to_string()),
    };
    let group = model
        .view()
        .groups
        .iter()
        .find(|group| group.core.group_id == group_id)?;
    let section = group
        .core
        .sections
        .iter()
        .find(|section| section.id == crate::sidebar_view::SectionId::Pinned)?;
    let drawn = |id: &str| section.session_ids.iter().any(|drawn| drawn == id);
    if !drawn(session_id) || !drawn(&target) {
        return None;
    }
    if inputs.settings.pinned_session_order == crate::sidebar_view::PinnedSessionOrder::Recent {
        return Some(None);
    }
    // A coordinator's open threads are drawn in its section without being pinned themselves; the
    // pin calls that renumber the section must not pin them.
    let rows: Vec<(String, Option<i64>)> = section
        .session_ids
        .iter()
        .filter_map(|id| {
            let row = &group
                .core
                .sessions
                .iter()
                .find(|session| session.row.sidebar_session_id == *id)?
                .row;
            row.is_pinned.then(|| (id.clone(), row.sidebar_order))
        })
        .collect();
    if !rows.iter().any(|(id, _)| id == session_id) {
        return None;
    }
    let Some(reorder) =
        super::pinned_order::plan_pinned_reorder(&rows, session_id, &target, position == "after")
    else {
        return Some(None);
    };
    if reorder.messages.is_empty() {
        return Some(None);
    }
    let index = reorder.order.iter().position(|id| id == session_id)?;
    let mut planned = command.clone();
    planned["targetSessionId"] = json!(target);
    planned["position"] = json!(position);
    // The host performs it from these messages, not from the project order write
    // (native_sidebar/drag.rs `finish_native_sidebar_drop`).
    planned["pinnedReorder"] = json!(true);
    Some(Some(SessionDrop {
        messages: reorder.messages,
        landing: Some(DropLanding {
            group_id: group_id.to_string(),
            section: crate::sidebar_view::SectionId::Pinned.as_str().to_string(),
            after_session_id: index
                .checked_sub(1)
                .and_then(|before| reorder.order.get(before))
                .cloned(),
            before_session_id: reorder.order.get(index + 1).cloned(),
            unchanged: false,
        }),
        command: planned,
        handoff: None,
    }))
}

/// `moveSessionToSection`: the flags of the section the row is dropped into, and for Pinned the
/// position the drop names.
///
/// The command carries `sessionId`, `groupId`, `from` and `section` (the drawn sections the
/// renderer resolved), plus `targetSessionId` and `position` when it was dropped on a row.
///
/// CDXC:Sidebar 2026-11-13 WHY:
/// User, of a project whose sessions run on another computer (pinned coordinators among them): "I can't drag to move a session between different sections for this project (can't reorder pinned sessions too)". A drop into Pinned used to write the position as the project's session order (`syncSessionOrder` over the row's own project), which keeps no order for another computer's project and names a group the one list does not draw when the row is another computer's copy merged into this one, so the drop was refused. It now renumbers the pinned rows as the target group draws them, as a drop among pinned rows does (pinned_order.rs): each pinned row whose place changes, the dropped one included, gets `sidebarOrder` 1000, 2000, ... through the pin call on its own machine, and both sidebars sort pinned rows by that number.
pub fn plan_section_move(
    core: &Core,
    inputs: &SidebarInputs,
    model: &SidebarViewModel,
    command: &Value,
) -> Option<Vec<Value>> {
    let session_id = command.get("sessionId")?.as_str()?;
    let from = command.get("from")?.as_str()?;
    let to = command.get("section")?.as_str()?;
    let group = group_of_session(core, inputs, session_id)?;
    let pinned = group
        .rows
        .iter()
        .find(|row| row.sidebar_session_id == session_id)?
        .is_pinned;
    let parked = from == "parked";
    let mut messages = Vec::new();
    match to {
        "pinned" => {
            if parked {
                messages.push(set_parked(session_id, false));
            }
            let target = command
                .get("targetSessionId")
                .and_then(Value::as_str)
                .zip(command.get("position").and_then(Value::as_str));
            // A parked row that kept its pin goes back to its own place unless the drop names one.
            if pinned && target.is_none() {
                return Some(messages);
            }
            let drawn = drawn_pinned_rows(model, command.get("groupId")?.as_str()?, session_id)?;
            let order = pinned_order(&drawn, session_id, target);
            for (index, id) in order.iter().enumerate() {
                let sidebar_order = ((index + 1) * 1000) as i64;
                let moved = id == session_id;
                let saved = drawn
                    .iter()
                    .find(|(row, _)| row == id)
                    .and_then(|(_, order)| *order);
                if moved || saved != Some(sidebar_order) {
                    messages.push(json!({
                        "type": "setSessionPinned",
                        "sessionId": id,
                        "pinned": true,
                        "sidebarOrder": sidebar_order,
                    }));
                }
            }
        }
        "sessions" => {
            if parked {
                messages.push(set_parked(session_id, false));
            }
            if pinned {
                messages.push(
                    json!({"type": "setSessionPinned", "sessionId": session_id, "pinned": false}),
                );
            }
        }
        "parked" => messages.push(set_parked(session_id, true)),
        _ => return None,
    }
    Some(messages)
}

fn set_parked(session_id: &str, parked: bool) -> Value {
    json!({"type": "setSessionParked", "sessionId": session_id, "parked": parked})
}

/// The pinned rows `group_id` draws (its Pinned section, a parked pinned row aside, and rows a
/// folded heading or coordinator hides included), in drawn order with their saved `sidebarOrder`,
/// without `session_id`. Rows of every machine and project the group lists are among them.
fn drawn_pinned_rows(
    model: &SidebarViewModel,
    group_id: &str,
    session_id: &str,
) -> Option<Vec<(String, Option<i64>)>> {
    let group = model
        .view()
        .groups
        .iter()
        .find(|group| group.core.group_id == group_id)?;
    Some(
        group
            .core
            .sessions
            .iter()
            .filter(|session| {
                session.row.is_pinned
                    && !session.row.is_parked
                    && session.row.sidebar_session_id != session_id
            })
            .map(|session| {
                (
                    session.row.sidebar_session_id.clone(),
                    session.row.sidebar_order,
                )
            })
            .collect(),
    )
}

/// The pinned rows with `session_id` at the drop position (the end when the drop names no pinned
/// row).
fn pinned_order(
    drawn: &[(String, Option<i64>)],
    session_id: &str,
    target: Option<(&str, &str)>,
) -> Vec<String> {
    let mut pinned: Vec<String> = drawn.iter().map(|(id, _)| id.clone()).collect();
    let index = target
        .and_then(|(target, position)| {
            pinned
                .iter()
                .position(|id| id == target)
                .map(|index| index + usize::from(position == "after"))
        })
        .unwrap_or(pinned.len());
    pinned.insert(index, session_id.to_string());
    pinned
}

/// What the messages change, for the landing preview.
fn drop_writes(
    core: &Core,
    inputs: &SidebarInputs,
    command: &Value,
    session_id: &str,
    messages: &[Value],
) -> Option<DropWrites> {
    let mut writes = DropWrites {
        group_id: command.get("groupId")?.as_str()?.to_string(),
        ..DropWrites::default()
    };
    for message in messages {
        let text = |key: &str| message.get(key).and_then(Value::as_str);
        match text("type") {
            Some("setSessionPinned") => {
                if let Some(order) = message.get("sidebarOrder").and_then(Value::as_i64) {
                    writes
                        .sidebar_orders
                        .push((text("sessionId")?.to_string(), order));
                }
                if text("sessionId") == Some(session_id) {
                    writes.pinned = message.get("pinned")?.as_bool();
                }
            }
            Some("setSessionParked") if text("sessionId") == Some(session_id) => {
                writes.parked = message.get("parked")?.as_bool()
            }
            Some("syncSessionOrder") => {
                writes.group_id = text("groupId")?.to_string();
                writes.order = Some(
                    message
                        .get("sessionIds")?
                        .as_array()?
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                );
            }
            Some("moveSessionToGroup") => {
                let target = group_by_id(core, inputs, text("groupId")?)?;
                // A row cannot leave its project (`plan_order_write` refuses it there too).
                let project = SessionKey::parse_sidebar_session_id(session_id)?.project_key();
                if target.project.as_ref() != Some(&project) {
                    return None;
                }
                let mut order = target.session_ids();
                order.retain(|id| id != session_id);
                let at = message
                    .get("targetIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .clamp(0, order.len() as i64) as usize;
                order.insert(at, session_id.to_string());
                writes.group_id = target.group_id;
                writes.order = Some(order);
            }
            _ => {}
        }
    }
    Some(writes)
}
