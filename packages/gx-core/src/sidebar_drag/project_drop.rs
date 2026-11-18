//! Where a project move lands, from the writes [`plan_project_move`] made for it.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/project_drop_landing.rs.
//!
//! [`plan_project_move`]: super::plan_project_move

use serde_json::{json, Value};

use crate::project_docs::CollectionsDocument;
use crate::sidebar_view::{OrderKind, ProjectDropLanding, SidebarView};

use super::project_move::{ProjectMovePlan, ProjectWrite};

/// The landing of the row `moveGroup`, `moveCollection` or `moveToCollection` drags, or `None` when
/// the plan does nothing (a refusal is an empty plan) or the row is not drawn.
pub fn project_drop_landing(
    view: &SidebarView,
    collections_before: &CollectionsDocument,
    plan: &ProjectMovePlan,
    moved_kind: &str,
    moved_id: &str,
) -> Option<ProjectDropLanding> {
    if plan.writes.is_empty() {
        return None;
    }
    let mut group_order = None;
    let mut collections = None;
    for write in &plan.writes {
        match write {
            ProjectWrite::GroupOrder { group_ids }
            | ProjectWrite::MergedGroupOrder { group_ids, .. } => {
                group_order = Some(group_ids.as_slice())
            }
            ProjectWrite::EditCollections { document } => collections = Some(&document.state),
            _ => {}
        }
    }
    view.preview_project_drop(
        &collections_before.state,
        group_order,
        collections,
        moved_kind,
        moved_id,
    )
}

/// The project drop as the plan performs it: a drop on any project of a worktree family (the
/// parent and the worktrees drawn after it) aims at the parent, before it when the dragged row
/// comes from below the family and after the family when it comes from above, the same rule a
/// coordinator's tree follows for sessions (`SidebarViewModel::tree_drop_target`). A drop made by
/// a member of the family itself, and every other drop, is returned as it is.
pub fn project_drop_command(view: &SidebarView, command: &Value) -> Value {
    let text = |key: &str| command.get(key).and_then(Value::as_str);
    let (moved, target_key) = match (text("type"), text("targetKind")) {
        (Some("moveGroup"), _) => (("group", text("groupId")), "targetGroupId"),
        (Some("moveCollection"), Some("group")) => (("collection", text("sourceId")), "targetId"),
        _ => return command.clone(),
    };
    let (Some(moved_id), Some(target_id)) = (moved.1, text(target_key)) else {
        return command.clone();
    };
    let project = |group_id: &str| {
        view.group(group_id)
            .and_then(|group| group.core.project_context.as_ref())
    };
    let Some(target) = project(target_id) else {
        return command.clone();
    };
    let family = target
        .worktree
        .as_ref()
        .map_or(target.project_id.as_str(), |worktree| {
            worktree.parent_project_id.as_str()
        });
    let in_family = |group_id: &str| {
        project(group_id).is_some_and(|project| {
            project.project_id == family
                || project
                    .worktree
                    .as_ref()
                    .is_some_and(|worktree| worktree.parent_project_id == family)
        })
    };
    let members = view
        .groups
        .iter()
        .filter(|group| in_family(&group.core.group_id))
        .count();
    let Some(parent) = view.groups.iter().find(|group| {
        project(&group.core.group_id).is_some_and(|project| project.project_id == family)
    }) else {
        return command.clone();
    };
    if members < 2 || (moved.0 == "group" && in_family(moved_id)) {
        return command.clone();
    }
    let from_above = drawn_above(view, moved.0, moved_id, &parent.core.group_id).unwrap_or(false);
    let mut command = command.clone();
    command[target_key] = json!(parent.core.group_id);
    command["position"] = json!(if from_above { "after" } else { "before" });
    command
}

/// The project drop of a dragged project (`group`) or collection held over a row drawn under
/// project `group_id`'s header (a session, a coordinator's thread, a section heading, the New
/// Session row): before that project when the dragged row comes from below it, after its whole
/// block when it comes from above. `None` over the dragged project's own rows or a row not drawn.
///
/// CDXC:Sidebar 2026-10-06 WHY:
/// Only a project's header was a target for a project drag, so the drop line went away over the rows under it and a release there moved nothing. Those rows aim at the slot next to their project the way a worktree family's rows aim at its parent (`project_drop_command` above) and a coordinator's threads aim at the coordinator for a session drop.
pub fn project_body_drop_command(
    view: &SidebarView,
    moved_kind: &str,
    moved_id: &str,
    group_id: &str,
) -> Option<Value> {
    if moved_kind == "group" && moved_id == group_id {
        return None;
    }
    let position = match drawn_above(view, moved_kind, moved_id, group_id)? {
        true => "after",
        false => "before",
    };
    match moved_kind {
        "group" => Some(json!({
            "type": "moveGroup",
            "groupId": moved_id,
            "targetGroupId": group_id,
            "position": position,
        })),
        "collection" => Some(json!({
            "type": "moveCollection",
            "sourceId": moved_id,
            "targetKind": "group",
            "targetId": group_id,
            "position": position,
        })),
        _ => None,
    }
}

/// Whether the dragged row is drawn above project `group_id`; `None` when either is not drawn.
fn drawn_above(
    view: &SidebarView,
    moved_kind: &str,
    moved_id: &str,
    group_id: &str,
) -> Option<bool> {
    // Every drawn row in order: a collection's row, then the projects inside it.
    let mut drawn: Vec<(&str, &str)> = Vec::new();
    for item in &view.order {
        match item.kind {
            OrderKind::Project => drawn.push(("group", &item.id)),
            OrderKind::Collection => {
                drawn.push(("collection", &item.id));
                if let Some(collection) = view
                    .collections
                    .iter()
                    .find(|collection| collection.collection_id == item.id)
                {
                    drawn.extend(collection.group_ids.iter().map(|id| ("group", id.as_str())));
                }
            }
        }
    }
    let index = |row: (&str, &str)| drawn.iter().position(|drawn| *drawn == row);
    Some(index((moved_kind, moved_id))? < index(("group", group_id))?)
}
