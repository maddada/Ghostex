//! `moveGroup` in the one list that holds every computer's projects (`sidebar_view/merged.rs`).
//!
//! SEE-ALSO: the DECISION in packages/gx-core/src/sidebar_view/merged_order.rs.
//!
//! CDXC:RemoteMachines 2026-11-18 WHY:
//! A drag there is computed against the one list's own order (`SidebarView::merged_project_rows`), not one computer's: the project dragged and the one it lands on may be on different computers. The order goes into this computer's project order (`ProjectWrite::MergedGroupOrder`). A Project Group (folder) joined or left by the drop is filed only when both projects are this computer's, through the same plan as before (`plan_project_move`): another computer's folders are that computer's document, and a reorder must not move a project out of its folder there.

use serde_json::Value;

use crate::core::Core;
use crate::project_docs::{
    move_projects_with_worktrees, CollectionsDocument, DropPosition, ProjectOrderItem,
    SpacesDocument,
};
use crate::sidebar_view::{MergedProjectRow, SidebarInputs};

use super::project_move::{plan_project_move, ProjectMovePlan, ProjectWrite};

/// What a `moveGroup` in the one list does, or `None` for any other command (the caller plans it
/// as before) and for a payload this cannot read.
pub fn plan_merged_project_move(
    core: &Core,
    inputs: &SidebarInputs,
    rows: &[MergedProjectRow],
    collections: &CollectionsDocument,
    spaces: Option<&SpacesDocument>,
    command: &Value,
    now_ms: i64,
) -> Option<ProjectMovePlan> {
    if command.get("type").and_then(Value::as_str) != Some("moveGroup") || rows.is_empty() {
        return None;
    }
    let group_id = command.get("groupId")?.as_str()?;
    let target_group_id = command.get("targetGroupId")?.as_str()?;
    let position = DropPosition::parse(command.get("position")?.as_str()?)?;
    let row = |id: &str| rows.iter().find(|row| row.group_id == id);
    let (Some(_), Some(_)) = (row(group_id), row(target_group_id)) else {
        return Some(ProjectMovePlan::refused("groupMissing"));
    };
    let group_ids =
        move_projects_with_worktrees(&order_items(rows), group_id, target_group_id, position)
            .into_iter()
            .map(|item| item.order_id)
            .collect::<Vec<_>>();
    let project_ids: Vec<String> = group_ids
        .iter()
        .filter_map(|id| row(id))
        .flat_map(|row| row.project_ids.iter().cloned())
        .collect();
    // This computer's folders, when both rows are this computer's (the old plan answers those
    // and refuses any other), without its order: the one list's order replaces it.
    let mut writes: Vec<ProjectWrite> =
        plan_project_move(core, inputs, collections, spaces, command, now_ms)
            .filter(|plan| plan.refusal.is_none())
            .map(|plan| {
                plan.writes
                    .into_iter()
                    .filter(|write| !matches!(write, ProjectWrite::GroupOrder { .. }))
                    .collect()
            })
            .unwrap_or_default();
    writes.push(ProjectWrite::MergedGroupOrder {
        group_ids,
        project_ids,
    });
    Some(ProjectMovePlan::of(writes))
}

/// The rows as `moveProjectsWithWorktrees` orders them: an entry by its drawn copy's id, a
/// worktree under the entry that lists its parent (on any computer), and a user-made group by its
/// own id, which keeps it out of every family.
fn order_items(rows: &[MergedProjectRow]) -> Vec<ProjectOrderItem> {
    let entry_of = |project_id: &str| {
        rows.iter()
            .find(|row| row.project_ids.iter().any(|id| id == project_id))
            .and_then(|row| row.project_ids.first().cloned())
    };
    rows.iter()
        .map(|row| ProjectOrderItem {
            order_id: row.group_id.clone(),
            project_id: row
                .project_ids
                .first()
                .cloned()
                .unwrap_or_else(|| row.group_id.clone()),
            parent_project_id: row
                .parent_project_id
                .as_deref()
                .map(|parent| entry_of(parent).unwrap_or_else(|| parent.to_string())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(group_id: &str, project_ids: &[&str]) -> MergedProjectRow {
        MergedProjectRow {
            group_id: group_id.to_string(),
            project_ids: project_ids.iter().map(|id| id.to_string()).collect(),
            parent_project_id: None,
        }
    }

    #[test]
    fn another_computers_worktree_follows_the_entry_of_its_parent() {
        let mut worktree = row("remote:arch:group:P5", &["remote:arch:project:P5"]);
        worktree.parent_project_id = Some("remote:arch:project:P4".to_string());
        let items = order_items(&[
            row("combined-project:P1", &["P1", "remote:arch:project:P4"]),
            worktree,
        ]);
        assert_eq!(items[1].parent_project_id.as_deref(), Some("P1"));
    }

    #[test]
    fn another_computers_project_moves_above_this_computers() {
        let rows = [
            row("combined-project:P1", &["P1"]),
            row("combined-project:P2", &["P2"]),
            row("remote:arch:group:P9", &["remote:arch:project:P9"]),
        ];
        let next: Vec<String> = move_projects_with_worktrees(
            &order_items(&rows),
            "remote:arch:group:P9",
            "combined-project:P1",
            DropPosition::parse("before").unwrap(),
        )
        .into_iter()
        .map(|item| item.order_id)
        .collect();
        assert_eq!(
            next,
            vec![
                "remote:arch:group:P9".to_string(),
                "combined-project:P1".to_string(),
                "combined-project:P2".to_string()
            ]
        );
    }
}
