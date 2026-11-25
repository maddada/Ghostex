//! A coordinator's threads that sit in another group of the same computer (another project, a
//! worktree project listed on its own, or another session group), listed under the coordinator in
//! its own group.
//!
//! CDXC:Coordinators 2026-11-16 WHY:
//! A thread is linked by session (`ghostex coordinator link`, `start-thread --project-id` or `--path`, `agents send --main-coordinator --if-none session`), and the linked session keeps running in its own project, but the Project sidebar nested threads only among the rows of one group, so a thread from another project stayed a plain row in that project while the CLI said it was the coordinator's (reported by the Kiwimake coordinator after linking a madda session). Its row now moves into the group that draws its coordinator, as a thread on another computer already does (`remote_thread_rows` in model.rs) and as the Unified sidebar's tree already nests across projects (`unified_sidebar/tree.rs`). Only the list moves: the session still runs in its own project's folder, and its menu, Files and Changes act on that project. Chief of Staff's sessions in other projects stay in those projects.
//! CDXC:Coordinators 2026-11-25 SEE-ALSO: the user's decision in server/src/coordinators/thread_shelves.rs ("i want threads that are under a coordinator to not be able to be away from it. they always appear below it"). A thread in a worktree project listed on its own, or in another session group of the coordinator's project, moves under its coordinator too (it was left in its own group before); a thread already in its coordinator's group (a worktree folded into its project by Worktree sessions show in their project) stays where it is.
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
    let mut moves: Vec<(usize, usize, usize)> = Vec::new();
    for (from, plan) in plans.iter().enumerate() {
        for (at, row) in plan.rows.iter().enumerate() {
            let (Some(key), Some(parent)) = (&row.row.key, &row.row.coordinator_parent) else {
                continue;
            };
            // Another computer's thread is already drawn under its coordinator.
            if key.machine != parent.machine {
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
        // A row a session group and its project both list is drawn once, under its coordinator.
        let listed = plans[to]
            .rows
            .iter()
            .any(|drawn| drawn.row.sidebar_session_id == row.row.sidebar_session_id);
        if !listed {
            plans[to].rows.push(row);
        }
    }
    if let Some(group_id) = moved_focus_to {
        focus.active_group_id = Some(group_id);
    }
}
