use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;

use super::resolve::text;
use crate::{
    domain::DomainRepository, sidebar_project_collections::read_sidebar_project_collections,
    sidebar_spaces::read_sidebar_spaces,
};

/// CDXC:Extensions 2026-09-09 DECISION:
/// User: selecting a Space makes the view available to every project in that Space.
/// Resolve current daemon-owned membership, including collection and parent-worktree inheritance, independently of the sidebar's selected filter.
/// SEE-ALSO: packages/shared/sidebar-spaces-other.ts owns the equivalent sidebar membership rule.
pub(super) fn matches(db: &Connection, server_id: &str, params: &Value) -> Result<bool> {
    let repository = DomainRepository::new(db, server_id);
    let Some(project) = repository.get_project(text(params, "projectId"))? else {
        return Ok(false);
    };
    let parent_id = text(&project["worktree"], "parentProjectId");
    let member_id = if parent_id.is_empty() {
        text(&project, "projectId")
    } else {
        parent_id
    };
    let collections = read_sidebar_project_collections(db)?;
    let collection_id = collections["collections"]
        .as_object()
        .into_iter()
        .flatten()
        .find(|(_, collection)| contains(&collection["projectIds"], member_id))
        .map(|(id, _)| id.as_str());
    let spaces = read_sidebar_spaces(db)?;
    let section = text(params, "spaceSectionKey");
    Ok(params["view"]["spaceRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|reference| {
            if text(reference, "sectionKey") != section {
                return false;
            }
            let space = &spaces["spaces"][text(reference, "spaceId")];
            match collection_id {
                Some(id) => contains(&space["memberCollectionIds"], id),
                None => contains(&space["memberProjectIds"], member_id),
            }
        }))
}
fn contains(value: &Value, id: &str) -> bool {
    value
        .as_array()
        .is_some_and(|ids| ids.iter().any(|value| value.as_str() == Some(id)))
}
