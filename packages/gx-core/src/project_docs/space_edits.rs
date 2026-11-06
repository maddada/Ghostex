//! The edits the project moves make to the Spaces document.
//!
//! None of these re-sanitizes. `withToggledMember` and `reorderSidebarSpaces` build the next state
//! by hand and hand it straight to the host, so a member id this client cannot resolve survives
//! until the daemon prunes it, which is what `packages/core-ui/spaces.ts` (deleted 2026-10-01) says clients must
//! tolerate. Re-sanitizing here would silently drop a member the server still holds and the next
//! echo would put it back, which is an oscillation with no guard to stop it.
//!
//! Ported from the sidebar page's `moveSpace` and `moveToSpace` arms (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/reorder.ts` and `project-drag.ts`; see git history).
//!
//! SEE-ALSO: packages/core-ui/spaces.ts (deleted 2026-10-01).

use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::SpacesState;

use super::collections::CollectionsDocument;
use super::spaces::SpacesDocument;

/// Which list of a Space a member belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceMemberKind {
    Collection,
    Project,
}

/// `toggleSpaceCollectionMembership` / `toggleSpaceProjectMembership`, which are one function with
/// the field chosen by the member's kind.
///
/// The toggle is not local to one Space: the member is removed from EVERY other Space's list in the
/// same pass, which is the "each project belongs to at most one Space" decision
/// (`CDXC:Spaces 2026-09-07 DECISION`) enforced on the way in rather than by the sanitizer.
pub fn toggle_space_member(
    document: &SpacesDocument,
    space_id: &str,
    kind: SpaceMemberKind,
    member_id: &str,
) -> SpacesDocument {
    // `js_trim`, not `str::trim`: `withToggledMember` trims with JavaScript's rules, which keep
    // U+0085 and take U+FEFF where Rust's do the opposite. The twin of the same line in
    // `move_projects_to_collection`, fixed with it rather than after it.
    let trimmed = js_trim(member_id);
    if trimmed.is_empty() || !document.state.spaces.contains_key(space_id) {
        return document.clone();
    }
    let mut state = SpacesState {
        order: document.state.order.clone(),
        spaces: document.state.spaces.clone(),
    };
    for (id, space) in state.spaces.iter_mut() {
        let list = match kind {
            SpaceMemberKind::Collection => &mut space.member_collection_ids,
            SpaceMemberKind::Project => &mut space.member_project_ids,
        };
        if id != space_id {
            list.retain(|candidate| candidate != trimmed);
            continue;
        }
        match list.iter().any(|candidate| candidate == trimmed) {
            true => list.retain(|candidate| candidate != trimmed),
            false => list.push(trimmed.to_string()),
        }
    }
    SpacesDocument { state }
}

/// The `moveToSpace` rewrite, which is NOT the toggle: the members land at the FRONT of the target
/// Space's list and are removed from every other, and a drop onto `other` removes them from all of
/// them without adding them anywhere.
///
/// Written out rather than expressed as a loop of toggles because it is not one: a member already
/// in the target Space would be toggled OUT by that loop, so dragging a project onto the Space it
/// is already in would remove it.
pub fn move_members_to_space(
    document: &SpacesDocument,
    space_id: &str,
    kind: SpaceMemberKind,
    member_ids: &[String],
) -> SpacesDocument {
    let mut state = SpacesState {
        order: document.state.order.clone(),
        spaces: document.state.spaces.clone(),
    };
    for (id, space) in state.spaces.iter_mut() {
        let list = match kind {
            SpaceMemberKind::Collection => &mut space.member_collection_ids,
            SpaceMemberKind::Project => &mut space.member_project_ids,
        };
        let kept: Vec<String> = list
            .iter()
            .filter(|candidate| !member_ids.contains(candidate))
            .cloned()
            .collect();
        *list = match id == space_id {
            true => member_ids.iter().cloned().chain(kept).collect(),
            false => kept,
        };
    }
    SpacesDocument { state }
}

/// The Spaces document after a Project Groups edit, with every project kept in the Space it was in;
/// `None` when nothing had to move.
///
/// CDXC:Spaces 2026-09-28 DECISION:
/// User: Add to Group > New Group always puts the new group in the current Space, and no
/// action may move a project out of its Space unless the user moves it to another one. A grouped
/// project takes its Space from its Project Group and gxserver strips its own membership, so every
/// groups edit carries the Space across: a new group that no Space lists joins the Space its
/// projects were in, and a project that leaves every group keeps its old group's Space. Joining a
/// group that already exists still takes that group's Space, because the user picked the group.
///
/// Nothing is removed here. A new group's projects keep their own membership until gxserver strips
/// it, so whichever of the two documents reaches the daemon first, the project is never left in no
/// Space at all.
///
/// SEE-ALSO: server/src/sidebar_spaces.rs (`carry_sidebar_spaces_across_collections`), the same
/// rule for every client's and the CLI's groups writes.
pub fn keep_spaces_across_collection_edit(
    spaces: &SpacesDocument,
    before: &CollectionsDocument,
    after: &CollectionsDocument,
) -> Option<SpacesDocument> {
    let state = &spaces.state;
    let collection_before = |project_id: &str| {
        before
            .state
            .collections
            .iter()
            .find(|collection| collection.project_ids.iter().any(|id| id == project_id))
            .map(|collection| collection.collection_id.as_str())
    };
    let space_listing = |state: &SpacesState, kind: SpaceMemberKind, member_id: &str| {
        state
            .order
            .iter()
            .find(|space_id| {
                state.spaces.get(*space_id).is_some_and(|space| {
                    let list = match kind {
                        SpaceMemberKind::Collection => &space.member_collection_ids,
                        SpaceMemberKind::Project => &space.member_project_ids,
                    };
                    list.iter().any(|candidate| candidate == member_id)
                })
            })
            .cloned()
    };
    let space_before = |project_id: &str| match collection_before(project_id) {
        Some(collection_id) => space_listing(state, SpaceMemberKind::Collection, collection_id),
        None => space_listing(state, SpaceMemberKind::Project, project_id),
    };
    let mut additions: Vec<(String, SpaceMemberKind, String)> = Vec::new();
    for collection in &after.state.collections {
        let is_new = !before
            .state
            .collections
            .iter()
            .any(|candidate| candidate.collection_id == collection.collection_id);
        if !is_new
            || space_listing(
                state,
                SpaceMemberKind::Collection,
                &collection.collection_id,
            )
            .is_some()
        {
            continue;
        }
        if let Some(space_id) = collection
            .project_ids
            .iter()
            .find_map(|project_id| space_before(project_id))
        {
            additions.push((
                space_id,
                SpaceMemberKind::Collection,
                collection.collection_id.clone(),
            ));
        }
    }
    for collection in &before.state.collections {
        for project_id in &collection.project_ids {
            let still_grouped = after
                .state
                .collections
                .iter()
                .any(|candidate| candidate.project_ids.contains(project_id));
            if still_grouped || space_listing(state, SpaceMemberKind::Project, project_id).is_some()
            {
                continue;
            }
            if let Some(space_id) = space_listing(
                state,
                SpaceMemberKind::Collection,
                &collection.collection_id,
            ) {
                additions.push((space_id, SpaceMemberKind::Project, project_id.clone()));
            }
        }
    }
    if additions.is_empty() {
        return None;
    }
    let mut next = state.clone();
    for (space_id, kind, member_id) in additions {
        let Some(space) = next.spaces.get_mut(&space_id) else {
            continue;
        };
        let list = match kind {
            SpaceMemberKind::Collection => &mut space.member_collection_ids,
            SpaceMemberKind::Project => &mut space.member_project_ids,
        };
        if !list.contains(&member_id) {
            list.push(member_id);
        }
    }
    Some(SpacesDocument { state: next })
}

/// `reorderSidebarSpaces`: the named ids first, in the order given, then whatever the stored order
/// still holds. An id naming a Space that is gone is dropped.
pub fn reorder_spaces(document: &SpacesDocument, ordered_ids: &[String]) -> SpacesDocument {
    let mut order: Vec<String> = Vec::new();
    for space_id in ordered_ids
        .iter()
        .chain(document.state.order.iter())
        .cloned()
    {
        if document.state.spaces.contains_key(&space_id) && !order.contains(&space_id) {
            order.push(space_id);
        }
    }
    SpacesDocument {
        state: SpacesState {
            order,
            spaces: document.state.spaces.clone(),
        },
    }
}

/// `applySidebarSpaceRowReorder`: project a reorder of the VISIBLE Space buttons back onto the full
/// order.
///
/// The visible buttons occupy a set of positions in the stored order and promotion can make that
/// set non-contiguous, so the reordered visible ids are written back into exactly those positions
/// and every overflowed Space keeps its slot.
pub fn apply_space_row_reorder(
    ordered_space_ids: &[String],
    visible_space_ids: &[String],
    reordered_visible_space_ids: &[String],
) -> Vec<String> {
    let mut next: Vec<String> = ordered_space_ids.to_vec();
    let mut visible_index = 0usize;
    for slot in next.iter_mut() {
        if !visible_space_ids.contains(slot) {
            continue;
        }
        let replacement = reordered_visible_space_ids.get(visible_index).cloned();
        visible_index += 1;
        // `if (nextSpaceId)`: a shorter reordered list leaves the remaining slots as they were
        // rather than blanking them.
        if let Some(replacement) = replacement {
            *slot = replacement;
        }
    }
    next
}
