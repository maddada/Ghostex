//! A coordinator's threads that live in another project of the same computer, listed under the
//! coordinator in its own project's group.
//!
//! CDXC:Coordinators 2026-11-16 WHY:
//! A thread is linked by session (`ghostex coordinator link`, `start-thread --project-id` or `--path`, `agents send --main-coordinator --if-none session`), and the linked session keeps running in its own project, but the Project sidebar nested threads only among the rows of one group, so a thread from another project stayed a plain row in that project while the CLI said it was the coordinator's (reported by the Kiwimake coordinator after linking a madda session). Its row now moves into the group that draws its coordinator, as a thread on another computer already does (`remote_thread_rows` in model.rs) and as the Unified sidebar's tree already nests across projects (`unified_sidebar/tree.rs`). Only the list moves: the session still runs in its own project's folder, and its menu, Files and Changes act on that project. A thread in a worktree of the coordinator's own project is left to `worktree_fold.rs` and the Worktree sessions show in their project setting, and Chief of Staff's sessions in other projects stay in those projects.
//! SEE-ALSO: apps/mobile/app/src/contract/coordinatorTree.ts (the phone's twin nesting).

use std::collections::HashMap;

use crate::keys::SessionKey;

use super::groups::{FocusKey, GroupKind, GroupPlan};

/// Moves each same-computer thread whose coordinator another plan draws into that plan, and
/// points the focus at the coordinator's group when the focused session was one of them.
pub(super) fn fold_cross_project_threads(plans: &mut [GroupPlan], focus: &mut FocusKey) {
    let coordinators: HashMap<SessionKey, usize> = plans
        .iter()
        .enumerate()
        .filter(|(_, plan)| matches!(plan.kind, GroupKind::Project | GroupKind::Subgroup))
        .flat_map(|(index, plan)| {
            plan.rows
                .iter()
                .filter(|row| row.row.is_coordinator && !row.row.is_chief)
                .filter_map(move |row| Some((row.row.key.clone()?, index)))
        })
        .collect();
    if coordinators.is_empty() {
        return;
    }
    // Worktree project -> the project it is a worktree of, whether its plan is still drawn or was
    // already folded into its parent.
    let worktree_parent: HashMap<&str, &str> = plans
        .iter()
        .flat_map(|plan| {
            plan.project
                .iter()
                .chain(plan.folded_worktrees.iter().map(|folded| &folded.project))
        })
        .filter_map(|project| {
            let worktree = project.worktree.as_ref()?;
            Some((
                project.project_id.as_str(),
                worktree.parent_project_id.as_str(),
            ))
        })
        .collect();
    let mut moves: Vec<(usize, usize, usize)> = Vec::new();
    for (from, plan) in plans.iter().enumerate() {
        for (at, row) in plan.rows.iter().enumerate() {
            let (Some(key), Some(parent)) = (&row.row.key, &row.row.coordinator_parent) else {
                continue;
            };
            // Another computer's thread is already drawn under its coordinator.
            if key.machine != parent.machine || key.project_id == parent.project_id {
                continue;
            }
            if worktree_parent.get(key.project_id.as_str()) == Some(&parent.project_id.as_str()) {
                continue;
            }
            let Some(&to) = coordinators.get(parent) else {
                continue;
            };
            if to != from {
                moves.push((from, at, to));
            }
        }
    }
    if moves.is_empty() {
        return;
    }
    let mut moved_focus_to: Option<String> = None;
    // Taken out last row first, so the earlier positions of the same plan stay valid, then added
    // in the order their groups listed them.
    let mut taken = Vec::with_capacity(moves.len());
    for &(from, at, to) in moves.iter().rev() {
        let row = plans[from].rows.remove(at);
        if let Some(key) = row.row.key.as_ref() {
            if focus.focused_session_id.as_deref() == Some(key.session_id.as_str())
                && focus.active_group_id.as_deref() == Some(plans[from].group_id.as_str())
            {
                moved_focus_to = Some(plans[to].group_id.clone());
            }
        }
        taken.push((to, row));
    }
    for (to, row) in taken.into_iter().rev() {
        plans[to].rows.push(row);
    }
    if let Some(group_id) = moved_focus_to {
        focus.active_group_id = Some(group_id);
    }
}
