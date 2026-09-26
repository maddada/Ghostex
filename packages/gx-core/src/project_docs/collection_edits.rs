//! The edits the project moves make to the collections document.
//!
//! Every one of these is a function of `packages/core-ui/project-collections.ts` and returns the
//! document it produced. None of them decides a refusal: the planners do that, with the reason at
//! the refusal, so that "this drop does nothing" and "this edit is a no-op" stay different things.
//!
//! **An edit that changes nothing still writes**, because the TypeScript's callers do not ask.
//! `moveProjectsToSidebarCollection` returns the SAME OBJECT for an empty id list or a target
//! collection that does not exist, and `saveNativeCollections` writes the key and posts the update
//! anyway. That identity is modelled rather than improved for the same reason the session moves
//! model theirs: a port that skipped the write would make one fewer push per drag, which the
//! parity gate saw while the TypeScript still ran. That TypeScript was frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/membership.ts` and `project-drag.ts` (see git history).
//!
//! SEE-ALSO: packages/core-ui/project-collections.ts.

use crate::sidebar_view::text::js_trim;
use crate::sidebar_view::{Collection, CollectionsState};

use super::collections::CollectionsDocument;

/// The palette a new collection takes its colour from, by `nextCollectionNumber - 1`.
const COLLECTION_COLORS: &[&str] = &[
    "#4f5663", "#808080", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4",
    "#8c9b45", "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

/// `moveProjectsToSidebarCollection`: take the projects out of wherever they are and, when a target
/// is named, put them at the END of it. `None` for the target is "out of every collection".
///
/// A collection left with no projects is DROPPED, which is how the last project leaving a folder
/// deletes it.
pub fn move_projects_to_collection(
    document: &CollectionsDocument,
    project_ids: &[String],
    collection_id: Option<&str>,
) -> CollectionsDocument {
    let mut unique: Vec<String> = Vec::new();
    for project_id in project_ids {
        // `js_trim`, not `str::trim`: JavaScript's `trim` leaves U+0085 and takes U+FEFF, and Rust's
        // does the opposite. A project id is not a place either character is expected, which is
        // exactly why a difference here would be found years later in one user's document.
        let trimmed = js_trim(project_id).to_string();
        if !unique.contains(&trimmed) {
            unique.push(trimmed);
        }
    }
    // `[...new Set(...)].filter(Boolean)`: the de-duplication happens BEFORE the empty ids are
    // dropped, so a list of two blanks is one entry and then none.
    unique.retain(|project_id| !project_id.is_empty());
    if unique.is_empty() {
        return document.clone();
    }
    if let Some(collection_id) = collection_id {
        if !document
            .state
            .collections
            .iter()
            .any(|collection| collection.collection_id == collection_id)
        {
            // A target that is not there leaves the document alone, rather than dropping the
            // projects out of the collections they are in.
            return document.clone();
        }
    }
    let collections = document
        .state
        .collections
        .iter()
        .map(|collection| {
            let mut next = collection.clone();
            next.project_ids
                .retain(|candidate| !unique.contains(candidate));
            if Some(next.collection_id.as_str()) == collection_id {
                next.project_ids.extend(unique.iter().cloned());
            }
            next
        })
        .filter(|collection| !collection.project_ids.is_empty())
        .collect();
    CollectionsDocument {
        state: CollectionsState { collections },
        next_collection_number: document.next_collection_number,
    }
}

/// `createSidebarProjectCollection`: a new folder holding one project, named and coloured after the
/// counter, which then advances.
///
/// The id carries a base-36 timestamp, so the host passes the clock in: the core has none, and a
/// collection id that differed between the two halves of a gate would make every later comparison
/// meaningless.
pub fn create_collection(
    document: &CollectionsDocument,
    project_id: &str,
    now_ms: i64,
) -> (String, CollectionsDocument) {
    let number = document.next_collection_number;
    let collection_id = format!("project-collection-{number}-{}", base36(now_ms));
    let mut collections: Vec<Collection> = document
        .state
        .collections
        .iter()
        .map(|collection| {
            let mut next = collection.clone();
            next.project_ids.retain(|candidate| candidate != project_id);
            next
        })
        .filter(|collection| !collection.project_ids.is_empty())
        .collect();
    // `(nextCollectionNumber - 1) % COLORS.length` on the JavaScript number, where a counter below
    // 1 would index from the end. The counter is clamped to 1 by every parse, so this cannot go
    // negative here; it is written as a clamp rather than a cast so it stays that way.
    let color_index = (number.max(1) - 1) as usize % COLLECTION_COLORS.len();
    collections.push(Collection {
        collection_id: collection_id.clone(),
        title: format!("Group {number}"),
        color: COLLECTION_COLORS[color_index].to_string(),
        project_ids: vec![project_id.to_string()],
    });
    (
        collection_id,
        CollectionsDocument {
            state: CollectionsState { collections },
            next_collection_number: number.saturating_add(1),
        },
    )
}

/// `reorderSidebarProjectCollections`: sort each folder's projects into the order the sidebar now
/// has them in.
///
/// A project the order does not name sorts AFTER one it does, which is the comparator's
/// `leftIndex === undefined ? (rightIndex === undefined ? 0 : 1) : -1`. That is a stable sort in
/// JavaScript, so ties keep their order, and this uses a stable sort for the same reason.
pub fn reorder_collection_projects(
    document: &CollectionsDocument,
    project_ids_in_order: &[String],
) -> CollectionsDocument {
    let index_of = |project_id: &String| -> Option<usize> {
        project_ids_in_order
            .iter()
            .position(|candidate| candidate == project_id)
    };
    let collections = document
        .state
        .collections
        .iter()
        .map(|collection| {
            let mut next = collection.clone();
            next.project_ids.sort_by(|left, right| {
                match (index_of(left), index_of(right)) {
                    (Some(left), Some(right)) => left.cmp(&right),
                    // `leftIndex === undefined ? (rightIndex === undefined ? 0 : 1) : -1`, which
                    // reads backwards from the usual: an id the order does NOT name is pushed
                    // after one it does.
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
            next
        })
        .collect();
    CollectionsDocument {
        state: CollectionsState { collections },
        next_collection_number: document.next_collection_number,
    }
}

/// `Date.now().toString(36)`.
pub(super) fn base36(value: i64) -> String {
    if value <= 0 {
        // `(0).toString(36)` is "0", and a negative clock is not a thing a host hands over; a
        // panic here would take a drag down for a clock that went backwards.
        return "0".to_string();
    }
    let mut digits = Vec::new();
    let mut remaining = value as u64;
    while remaining > 0 {
        let digit = (remaining % 36) as u32;
        digits.push(char::from_digit(digit, 36).unwrap_or('0'));
        remaining /= 36;
    }
    digits.iter().rev().collect()
}
