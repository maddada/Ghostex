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
use crate::keys::{parse_workspace_subgroup_id, SessionKey};
use crate::sidebar_view::{
    DropLanding, DropWrites, SidebarInputs, SidebarViewModel, TreeDropTarget,
};

use super::inventory::{group_by_id, group_of_session, MoveGroup};
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
    // A section heading of another project aims at that project and at nothing else.
    if command.get("handoffOnly").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let mut command = command.clone();
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
        "moveSessionToSection" => plan_section_move(core, inputs, &command)?,
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

/// `moveSessionToSection`: the flags of the section the row is dropped into, and for Pinned the
/// position the drop names.
///
/// The command carries `sessionId`, `groupId`, `from` and `section` (the drawn sections the
/// renderer resolved), plus `targetSessionId` and `position` when it was dropped on a row.
pub fn plan_section_move(
    core: &Core,
    inputs: &SidebarInputs,
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
            let order = pinned_order(&group, session_id, target);
            let mut pin =
                json!({"type": "setSessionPinned", "sessionId": session_id, "pinned": true});
            if is_project_group(&group) {
                // `/api/updateSessionOrder` numbers the rows 1000, 2000, ... in this order.
                let index = order.iter().position(|id| id == session_id)?;
                pin["sidebarOrder"] = json!(((index + 1) * 1000) as i64);
            }
            if !pinned {
                messages.push(pin);
            }
            messages.push(json!({
                "type": "syncSessionOrder",
                "groupId": group.group_id,
                "sessionIds": order,
            }));
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

fn is_project_group(group: &MoveGroup) -> bool {
    parse_workspace_subgroup_id(&group.group_id).is_none()
        && group
            .project
            .as_ref()
            .is_some_and(|project| project.to_sidebar_group_id() == group.group_id)
}

/// The group's rows with `session_id` among the pinned ones at the drop position (the end when the
/// drop names no pinned row), then every unpinned row in its own order.
fn pinned_order(group: &MoveGroup, session_id: &str, target: Option<(&str, &str)>) -> Vec<String> {
    let mut pinned: Vec<String> = group
        .rows
        .iter()
        .filter(|row| row.is_pinned && row.sidebar_session_id != session_id)
        .map(|row| row.sidebar_session_id.clone())
        .collect();
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
        .into_iter()
        .chain(
            group
                .rows
                .iter()
                .filter(|row| !row.is_pinned && row.sidebar_session_id != session_id)
                .map(|row| row.sidebar_session_id.clone()),
        )
        .collect()
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
            Some("setSessionPinned") => writes.pinned = message.get("pinned")?.as_bool(),
            Some("setSessionParked") => writes.parked = message.get("parked")?.as_bool(),
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
