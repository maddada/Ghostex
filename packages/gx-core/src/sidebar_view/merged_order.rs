//! The one list's own project order across computers (`merged.rs`).
//!
//! CDXC:RemoteMachines 2026-11-18 DECISION:
//! User: "i can't rearrange the order of these projects here pls fix", then "i can rearrange just the mac ones since they're on the local machine but the non local ones can't be rearranged". With Separate by machine off, a project only another computer has was always drawn after every one of this computer's projects, in that computer's own order, and a drag of it was planned against that computer alone: dropped next to one of this computer's projects it was refused (the target is not in that computer's list), and dropped among its own computer's projects the order write was taken out (a remote tab never saved an order, `ProjectMovePlan::without_group_order`). Now every project of the one list can be dragged into any place. The order is saved in this computer's own project order (the workspace session groups document's `projectOrder`, the same key a project drag on this computer writes and pushes), with another computer's project named by its machine-scoped workspace id (`remote:<machine>:project:<id>`); the list is drawn in that order. Until a drag names another computer's project there, the list is drawn exactly as before, and a project the saved order does not name (one added since, or a computer that just joined) stays right after the project it follows today. The phone and the other computers read their own orders as before; nothing is written to another computer.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/merged_project_move.rs (the drag),
//! packages/gx-core/src/sidebar_drag/project_order_write.rs (the write that keeps these ids).

use std::collections::HashMap;

use crate::keys::{MachineId, ProjectKey};

use super::groups::{GroupKind, GroupPlan};
use super::inputs::LOCAL_MACHINE_ID;

/// One group of the one list, in its order, for the drag (`plan_merged_project_move`). Every
/// group the merge made is here, whether or not a Space, a filter or hiding draws it: a drag is
/// computed against the whole order, so a project off screen keeps its place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergedProjectRow {
    /// The group id the list draws (an entry's first copy, or a user-made group's).
    pub group_id: String,
    /// The workspace project id of every copy the entry lists, the drawn one first; empty for a
    /// user-made group, which follows its project.
    pub project_ids: Vec<String>,
    /// A worktree's parent, as a workspace project id of the worktree's own computer.
    pub parent_project_id: Option<String>,
}

/// The ids an entry is ordered by: every copy's workspace project id, or the plan's own.
pub(super) fn plan_order_ids(plan: &GroupPlan, copies: Option<Vec<String>>) -> Vec<String> {
    if plan.kind == GroupKind::Subgroup {
        return Vec::new();
    }
    copies.unwrap_or_else(|| {
        ProjectKey::parse_sidebar_group_id(&plan.group_id)
            .map(|project| vec![project.to_workspace_project_id()])
            .unwrap_or_default()
    })
}

/// The workspace project id of a copy.
pub(super) fn workspace_id(machine: &MachineId, project_id: &str) -> String {
    ProjectKey {
        machine: machine.clone(),
        project_id: project_id.to_string(),
    }
    .to_workspace_project_id()
}

/// Whether a stored order names another computer's project, which only a drag in the one list
/// writes. An order without one is this computer's alone and the list keeps its default order.
pub fn order_names_another_computer(order: &[String]) -> bool {
    order.iter().any(|id| {
        ProjectKey::parse_workspace_project_id(id)
            .is_some_and(|project| !project.machine.is_local())
    })
}

/// Draws the plans in the saved order. A user-made group stays right after its project, and a
/// project the order does not name stays right after the one it follows now.
pub(super) fn sort_plans(
    plans: &mut Vec<GroupPlan>,
    ids: &mut Vec<Vec<String>>,
    stored: &[String],
) {
    if !order_names_another_computer(stored) || plans.len() != ids.len() {
        return;
    }
    let position: HashMap<&str, usize> = stored
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let order = sorted_indices(
        plans.iter().map(|plan| plan.kind == GroupKind::Subgroup),
        ids.iter().map(|ids| {
            ids.iter()
                .filter_map(|id| position.get(id.as_str()).copied())
                .min()
        }),
    );
    let mut taken_plans: Vec<Option<GroupPlan>> = plans.drain(..).map(Some).collect();
    let mut taken_ids: Vec<Option<Vec<String>>> = ids.drain(..).map(Some).collect();
    for index in order {
        plans.extend(taken_plans[index].take());
        ids.extend(taken_ids[index].take());
    }
}

/// The new order of the plans, from whether each is a user-made group and its rank in the saved
/// order. Pure, so the rule is pinned by a test.
fn sorted_indices(
    subgroup: impl Iterator<Item = bool>,
    rank: impl Iterator<Item = Option<usize>>,
) -> Vec<usize> {
    // A chain is a ranked entry with what travels behind it: its user-made groups and the entries
    // the order does not name. A chain before any ranked entry keeps the front.
    let mut chains: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
    for (index, (subgroup, rank)) in subgroup.zip(rank).enumerate() {
        match chains.last_mut() {
            Some(chain) if subgroup || rank.is_none() => chain.1.push(index),
            _ => chains.push((rank, vec![index])),
        }
    }
    chains.sort_by_key(|chain| chain.0);
    chains.into_iter().flat_map(|chain| chain.1).collect()
}

/// The rows the drag reads, from the plans in their drawn order.
pub(super) fn merged_rows(plans: &[GroupPlan], ids: &[Vec<String>]) -> Vec<MergedProjectRow> {
    plans
        .iter()
        .zip(ids)
        .map(|(plan, ids)| {
            let machine = match &plan.remote_machine {
                Some(remote) if remote.machine_id != LOCAL_MACHINE_ID => {
                    MachineId::Remote(remote.machine_id.clone())
                }
                _ => MachineId::Local,
            };
            MergedProjectRow {
                group_id: plan.group_id.clone(),
                project_ids: ids.clone(),
                parent_project_id: plan
                    .project
                    .as_ref()
                    .and_then(|project| project.worktree.as_ref())
                    .map(|worktree| workspace_id(&machine, &worktree.parent_project_id)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(entries: &[(bool, Option<usize>)]) -> Vec<usize> {
        sorted_indices(
            entries.iter().map(|entry| entry.0),
            entries.iter().map(|entry| entry.1),
        )
    }

    #[test]
    fn another_computers_project_can_lead_the_list() {
        // This computer's two projects, then one only another computer has, saved first.
        assert_eq!(
            order(&[(false, Some(1)), (false, Some(2)), (false, Some(0))]),
            vec![2, 0, 1]
        );
    }

    #[test]
    fn a_session_group_and_an_unnamed_project_travel_with_the_one_before() {
        // Project A (saved last) with its session group, a project the order does not name, then
        // project B (saved first).
        assert_eq!(
            order(&[
                (false, Some(5)),
                (true, None),
                (false, None),
                (false, Some(1))
            ]),
            vec![3, 0, 1, 2]
        );
    }

    #[test]
    fn unnamed_projects_before_any_named_one_keep_the_front() {
        assert_eq!(
            order(&[(false, None), (false, Some(3)), (false, Some(0))]),
            vec![0, 2, 1]
        );
    }

    #[test]
    fn only_a_remote_id_turns_the_saved_order_on() {
        assert!(!order_names_another_computer(&["P1".into(), "P2".into()]));
        assert!(order_names_another_computer(&[
            "P1".into(),
            workspace_id(&MachineId::Remote("arch".into()), "P9"),
        ]));
    }
}
