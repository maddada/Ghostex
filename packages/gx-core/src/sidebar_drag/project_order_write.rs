//! `syncGroupOrder`: what a project reorder writes into the workspace session groups document.
//!
//! CDXC:Projects 2026-09-21 WHY:
//! This is the second half of `moveGroup` and it is a DIFFERENT writer from `syncSessionOrder`:
//! the project order and each project's user-made group order both live in the workspace session
//! groups document (K4), so a project drag writes the collections document AND this one, through
//! two separate guards. It is kept apart from `order_write.rs` because it needs the projection's
//! worktree metadata (`normalizeWorkspaceProjectOrder` re-nests the order before storing it) where
//! the session order messages need only the document.
//!
//! **The re-nest is not cosmetic.** The order posted by a drop has already been nested by
//! `moveProjectsWithWorktrees`, but the ids that reach the document are the PROJECT ids of the
//! groups, and a project with user-made groups contributes its id several times. Storing that list
//! as given would put a project in the order more than once; `syncGpuiWorkspaceProjectOrder`
//! de-duplicates, and the re-nest runs first so the de-duplication keeps the first position.
//!
//! Ported from `syncWorkspaceGroupOrder` and `normalizeWorkspaceProjectOrder` in the deleted
//! `gxserver-runtime/workspace-groups-sync.ts` (see git history).
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/project_move.rs.

use serde_json::Value;

use crate::core::Core;
use crate::keys::{parse_workspace_subgroup_id, MachineId, ProjectKey};
use crate::project_docs::{order_projects_with_worktrees, ProjectOrderItem};
use crate::sidebar_view::projects::build_project_meta;
use crate::sidebar_view::SidebarInputs;

use super::inventory::workspace_project_order;
use super::order_write::OrderWritePlan;

/// The one message type this file answers.
pub const PROJECT_ORDER_MESSAGE_TYPE: &str = "syncGroupOrder";

/// Whether this message is the project order write.
pub fn owns_project_order_message(message: &Value) -> bool {
    message.get("type").and_then(Value::as_str) == Some(PROJECT_ORDER_MESSAGE_TYPE)
}

/// What `syncGroupOrder` writes, or `None` for a message that is not one.
///
/// CDXC:Projects 2026-09-25 WHY:
/// An order holding ANY remote project group id is not a document edit at all:
/// `syncWorkspaceGroupOrder` sent it down that machine's tunnel as a whole-order replacement of the
/// machine's own project order, and refused outright when the ids named more than one machine or
/// held anything else, so it is planned as exactly that call (`OrderWrite::RemoteProjectOrder`) or
/// as nothing. It used to be declined here and performed by the old runtime.
pub fn plan_project_order_write(
    core: &Core,
    inputs: &SidebarInputs,
    document: &crate::workspace_groups::WorkspaceGroupsDocument,
    message: &Value,
) -> Option<OrderWritePlan> {
    if !owns_project_order_message(message) {
        return None;
    }
    let group_ids: Vec<&str> = message
        .get("groupIds")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    // `remoteReferences.some(Boolean)`: a remote PROJECT group id, never a user-made group's.
    if group_ids.iter().any(is_remote_group_id) {
        return Some(plan_remote_project_order(core, &group_ids));
    }
    let project_ids: Vec<String> = group_ids
        .iter()
        .filter_map(|group_id| ProjectKey::parse_sidebar_group_id(group_id))
        .filter(|project| project.machine.is_local())
        .map(|project| project.project_id)
        .collect();
    let mut next = document.clone();
    let mut edited = false;
    // `if (projectIds.length > 0)`: an order with no project group in it (every row a user-made
    // group) leaves the project order alone rather than emptying it.
    if !project_ids.is_empty() {
        let normalized = normalize_project_order(core, inputs, &project_ids);
        // Another computer's projects placed by a drag in the one list keep their places
        // (`sidebar_view/merged_order.rs`): this order names this computer's projects only.
        let normalized = keep_missing_ids(&document.project_order, &normalized, |id| {
            ProjectKey::parse_workspace_project_id(id)
                .is_some_and(|project| !project.machine.is_local())
        });
        if let Some(document) = next.sync_project_order(&normalized) {
            next = document;
            edited = true;
        }
    }
    sync_subgroup_orders(&mut next, &mut edited, &group_ids);
    // `if (this.workspaceGroups === before) return`: nothing moved, so no key is written and no
    // push is booked. This is the one place in the project moves where a write really is skipped,
    // and it is skipped because the shipped code skips it.
    if !edited {
        return Some(OrderWritePlan::refused("orderUnchanged"));
    }
    Some(OrderWritePlan::edit(next))
}

/// The order a drag in the one list writes (`merged_project_move.rs`): every computer's projects
/// by workspace project id, into this computer's own project order, and each project's user-made
/// groups in the order the list now draws them.
pub fn plan_merged_project_order_write(
    document: &crate::workspace_groups::WorkspaceGroupsDocument,
    group_ids: &[String],
    project_ids: &[String],
) -> OrderWritePlan {
    let mut next = document.clone();
    let mut edited = false;
    if !project_ids.is_empty() {
        // A project the list does not hold now (a computer left out while it is offline, a project
        // parked in Recent Projects) keeps its place for when it is back.
        let order = keep_missing_ids(&document.project_order, project_ids, |_| true);
        if let Some(document) = next.sync_project_order(&order) {
            next = document;
            edited = true;
        }
    }
    let group_ids: Vec<&str> = group_ids.iter().map(String::as_str).collect();
    sync_subgroup_orders(&mut next, &mut edited, &group_ids);
    if !edited {
        return OrderWritePlan::refused("orderUnchanged");
    }
    OrderWritePlan::edit(next)
}

/// `next` with every id of `previous` it does not name and `keep` accepts put back right after
/// the id it followed in `previous` (at the front when none did).
fn keep_missing_ids(
    previous: &[String],
    next: &[String],
    keep: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut order: Vec<String> = next.to_vec();
    let mut anchor: Option<String> = None;
    let mut inserted_at_front = 0;
    // Where the last id put back after each anchor went, so several keep their own order.
    let mut last_after: std::collections::HashMap<String, String> = Default::default();
    for id in previous {
        if next.contains(id) {
            anchor = Some(id.clone());
            continue;
        }
        if !keep(id) || order.contains(id) {
            continue;
        }
        let at = match &anchor {
            None => {
                inserted_at_front += 1;
                inserted_at_front - 1
            }
            Some(anchor_id) => {
                let after = last_after.get(anchor_id).unwrap_or(anchor_id);
                order
                    .iter()
                    .position(|candidate| candidate == after)
                    .map_or(order.len(), |index| index + 1)
            }
        };
        order.insert(at, id.clone());
        if let Some(anchor_id) = &anchor {
            last_after.insert(anchor_id.clone(), id.clone());
        }
    }
    order
}

/// The subgroup orders, by project, in the order the projects first appear.
fn sync_subgroup_orders(
    next: &mut crate::workspace_groups::WorkspaceGroupsDocument,
    edited: &mut bool,
    group_ids: &[&str],
) {
    let mut by_project: Vec<(String, Vec<String>)> = Vec::new();
    for group_id in group_ids {
        // A remote project's user-made groups live in this document too, under its machine-scoped
        // key, and `parseGpuiWorkspaceSessionSubgroupId` reads them like any other.
        let Some((project, subgroup_id)) = parse_workspace_subgroup_id(group_id) else {
            continue;
        };
        let key = project.to_workspace_project_id();
        match by_project
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            Some((_, order)) => order.push(subgroup_id),
            None => by_project.push((key, vec![subgroup_id])),
        }
    }
    for (project_id, order) in &by_project {
        if let Some(document) = next.sync_subgroup_order(project_id, order) {
            *next = document;
            *edited = true;
        }
    }
}

/// `normalizeWorkspaceProjectOrder`: re-nest the ids so every worktree follows its parent.
fn normalize_project_order(
    core: &Core,
    inputs: &SidebarInputs,
    project_ids: &[String],
) -> Vec<String> {
    let machine = MachineId::Local;
    let Some(entry) = core.presentation().machine(&machine) else {
        return project_ids.to_vec();
    };
    let meta = build_project_meta(
        entry,
        workspace_project_order(core, &machine),
        inputs.host.parked_project_ids(&machine),
    );
    let items: Vec<ProjectOrderItem> = project_ids
        .iter()
        .map(|project_id| ProjectOrderItem {
            order_id: project_id.clone(),
            project_id: project_id.clone(),
            parent_project_id: meta
                .overlay(project_id)
                .and_then(|overlay| overlay.worktree.as_ref())
                .map(|worktree| worktree.parent_project_id.clone()),
        })
        .collect();
    order_projects_with_worktrees(&items)
        .into_iter()
        .map(|item| item.project_id)
        .collect()
}

/// `parseGpuiRemotePresentationGroupId`: a project group id naming a machine.
fn is_remote_group_id(group_id: &&str) -> bool {
    remote_project_of(group_id).is_some()
}

fn remote_project_of(group_id: &str) -> Option<ProjectKey> {
    ProjectKey::parse_sidebar_group_id(group_id).filter(|project| !project.machine.is_local())
}

/// `updateRemoteWorkspaceGroups(machineId, projectOrder)`: every id a remote project group of ONE
/// machine, sent with the machine's own user-made groups as they were last received.
fn plan_remote_project_order(core: &Core, group_ids: &[&str]) -> OrderWritePlan {
    let projects: Vec<Option<ProjectKey>> =
        group_ids.iter().map(|id| remote_project_of(id)).collect();
    let machine = match projects.first() {
        Some(Some(project)) => project.machine.clone(),
        _ => return OrderWritePlan::refused("remoteOrderMixed"),
    };
    if projects
        .iter()
        .any(|project| project.as_ref().map(|project| &project.machine) != Some(&machine))
    {
        return OrderWritePlan::refused("remoteOrderMixed");
    }
    let MachineId::Remote(machine_id) = &machine else {
        return OrderWritePlan::refused("remoteOrderMixed");
    };
    let project_order: Vec<String> = projects
        .into_iter()
        .flatten()
        .map(|project| project.project_id)
        .collect();
    let held_projects = core
        .presentation()
        .machine(&machine)
        .and_then(|entry| entry.side_state().workspace_groups.as_ref())
        .map(|groups| serde_json::to_value(&groups.projects).unwrap_or_default())
        .unwrap_or_else(|| serde_json::json!({}));
    OrderWritePlan {
        writes: vec![super::order_write::OrderWrite::RemoteProjectOrder {
            machine_id: machine_id.clone(),
            state: serde_json::json!({
                "projectOrder": project_order,
                "projects": held_projects,
            }),
        }],
        refusal: None,
    }
}

#[cfg(test)]
mod tests {
    use super::keep_missing_ids;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn another_computers_projects_keep_their_places_in_a_local_reorder() {
        let previous = ids(&[
            "remote:arch:project:P9",
            "P1",
            "remote:win:project:P7",
            "remote:win:project:P8",
            "P2",
        ]);
        // P2 dragged above P1 on this computer's own list.
        let next = keep_missing_ids(&previous, &ids(&["P2", "P1"]), |id| {
            id.starts_with("remote:")
        });
        assert_eq!(
            next,
            ids(&[
                "remote:arch:project:P9",
                "P2",
                "P1",
                "remote:win:project:P7",
                "remote:win:project:P8"
            ])
        );
    }

    #[test]
    fn ids_the_filter_refuses_are_dropped() {
        let previous = ids(&["P1", "P3", "P2"]);
        let next = keep_missing_ids(&previous, &ids(&["P2", "P1"]), |id| {
            id.starts_with("remote:")
        });
        assert_eq!(next, ids(&["P2", "P1"]));
    }
}
