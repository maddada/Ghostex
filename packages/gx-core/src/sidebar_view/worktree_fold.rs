//! A worktree project's sessions listed under the project it is a worktree of.
//!
//! CDXC:Worktrees 2026-11-06 DECISION:
//! Reported by Brewed Bytes on Discord ("Worktrees are created as a new project in the sidebar"): "I tried chatting with worktrees and by default, it starts as a new project. My default assumption is that, this is just another session in the same project and shouldn't create a new project." The user answered that "a worktree is a folder and folders are projects in Ghostex, because imagine if you wanted to have multiple threads on 1 worktree ... Let me add a toggle for this in advanced for next version, will enable it by default ... let people pick", then: "implement this pls". Settings › Sidebar & Sessions › Advanced › Worktree sessions show in their project (`worktreeSessionsInProject`, on by default): the Project sidebar lists a worktree project's sessions (its session groups' too) among its parent project's own, each row naming its worktree's branch (`SessionRow::worktree`), and draws no entry for the worktree. Off, a worktree is its own entry under its parent, as before.
//! Only the list moves: the worktree stays a gxserver project, so its sessions still run in its folder (Files, Changes, Commit, diff counts and Delete Worktree read the worktree's project), several sessions share one worktree, and nothing is written, so switching the setting off brings every worktree entry back with its sessions, pins, tags and order. The parent's header menu lists the folded worktrees in a Delete Worktree submenu, as the Unified sidebar's repository header does. The Unified sidebar needs none of this: it already lists a worktree's sessions under its repository, and reads the worktree groups this would remove, so the fold runs only for the Project sidebar.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/merged.rs (the same fold for another machine's
//! worktrees), apps/desktop/src/app/native_sidebar/threads.rs (`worktree_badge`),
//! apps/mobile/app/src/contract/grouping.ts (the phone's Project list),
//! server/src/ghostex_cli/sessions/mobile_summary.rs (the setting the phone reads).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::groups::{FoldedWorktreePlan, GroupKind, GroupPlan, ProjectContextInput, RowRef};
use super::view::RowWorktree;

/// Rows already marked with their worktree, by the number of the row they were made from: a row
/// keeps its number when it is marked, so an unchanged row is not copied again.
pub(super) type MarkedRows = BTreeMap<u64, RowRef>;

/// Moves every worktree project plan (and its session groups) into its parent project's plan,
/// when the parent is in `plans`. Returns the group ids that are no longer drawn, each with the
/// group that now lists its sessions, so the focus follows them there.
pub(super) fn fold_worktrees(
    plans: &mut Vec<GroupPlan>,
    previous: &mut MarkedRows,
    marked: &mut MarkedRows,
) -> BTreeMap<String, String> {
    let mut folded_into: BTreeMap<String, String> = BTreeMap::new();
    let parent_of_project: BTreeMap<String, usize> = plans
        .iter()
        .enumerate()
        .filter(|(_, plan)| plan.kind == GroupKind::Project)
        .filter_map(|(index, plan)| {
            let project = plan.project.as_ref()?;
            (project.worktree.is_none() && project.bot_profile.is_none())
                .then(|| (project.project_id.clone(), index))
        })
        .collect();
    let mut dropped: BTreeSet<usize> = BTreeSet::new();
    let mut index = 0;
    while index < plans.len() {
        let subgroups = subgroups_after(plans, index);
        let parent = (plans[index].kind == GroupKind::Project)
            .then(|| plans[index].project.clone())
            .flatten()
            .and_then(|project| {
                let parent_id = &project.worktree.as_ref()?.parent_project_id;
                let parent = *parent_of_project.get(parent_id)?;
                // Another machine's parent is never this plan list's (one machine per list).
                (plans[parent]
                    .remote_machine
                    .as_ref()
                    .map(|remote| &remote.machine_id)
                    == plans[index]
                        .remote_machine
                        .as_ref()
                        .map(|remote| &remote.machine_id))
                .then_some((parent, project))
            });
        let Some((parent, project)) = parent else {
            index = subgroups.end;
            continue;
        };
        let worktree = row_worktree(&plans[index].group_id, &project);
        let parent_id = plans[parent].group_id.clone();
        for moved in std::iter::once(index).chain(subgroups.clone()) {
            let rows = std::mem::take(&mut plans[moved].rows);
            let rows = rows
                .into_iter()
                .map(|row| mark_row(row, &worktree, previous, marked));
            plans[parent].rows.extend(rows);
            folded_into.insert(plans[moved].group_id.clone(), parent_id.clone());
            dropped.insert(moved);
        }
        let folded = FoldedWorktreePlan {
            group_id: plans[index].group_id.clone(),
            project,
            remote_machine: plans[index].remote_machine.clone(),
        };
        plans[parent].folded_worktrees.push(folded);
        index = subgroups.end;
    }
    if !dropped.is_empty() {
        let mut index = 0;
        plans.retain(|_| {
            let keep = !dropped.contains(&index);
            index += 1;
            keep
        });
    }
    folded_into
}

/// The worktree a row of `project` (whose sidebar group is `group_id`) draws.
pub(super) fn row_worktree(group_id: &str, project: &ProjectContextInput) -> RowWorktree {
    let worktree = project.worktree.as_ref();
    RowWorktree {
        group_id: group_id.to_string(),
        name: worktree
            .map(|worktree| worktree.name.clone())
            .unwrap_or_default(),
        branch: worktree
            .map(|worktree| worktree.branch.clone())
            .unwrap_or_default(),
    }
}

/// `row` with its worktree on it, under the same number: the one made last time when that row
/// has not changed since.
pub(super) fn mark_row(
    row: RowRef,
    worktree: &RowWorktree,
    previous: &mut MarkedRows,
    marked: &mut MarkedRows,
) -> RowRef {
    if let Some(kept) = previous
        .remove(&row.id)
        .filter(|kept| kept.row.worktree.as_ref() == Some(worktree))
    {
        marked.insert(row.id, kept.clone());
        return kept;
    }
    let mut copy = (*row.row).clone();
    copy.worktree = Some(worktree.clone());
    let row = RowRef {
        id: row.id,
        row: Arc::new(copy),
    };
    marked.insert(row.id, row.clone());
    row
}

/// The session groups drawn right after the project plan at `index`, as indices.
fn subgroups_after(plans: &[GroupPlan], index: usize) -> std::ops::Range<usize> {
    let end = plans[index + 1..]
        .iter()
        .position(|plan| plan.kind != GroupKind::Subgroup)
        .map_or(plans.len(), |offset| index + 1 + offset);
    index + 1..end
}
