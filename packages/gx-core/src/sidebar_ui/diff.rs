//! What one burst of intents changed about the collapse state, and applying exactly that to the
//! stored envelope.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! A write carries the difference this state made and applies it to whatever is stored at that
//! moment, rather than serializing the whole object. That was first needed because the TypeScript
//! sidebar was a second writer of the same key; since M5 piece 7c it is not, and the reason the
//! difference stays is the one that outlives it: `isReferenceChatsCollapsed` belongs to the React
//! sidebar, and an envelope written by a BUILD THIS ONE CANNOT READ still has to keep its own
//! fields when a click lands on it. Supersedes the 2026-09-20 note, which named the TypeScript
//! sidebar's ownership of the per-Space session memory, the followed Space and the selection a
//! deleted Space leaves behind; all three are this state's now.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use super::persist::{
    collapse_into_storage, stored_state_object, stored_version, write_sidebar_mode,
    COLLAPSE_STORAGE_VERSION,
};
use crate::sidebar_view::{SectionCollapse, SidebarCollapseState, SidebarMode};

/// The keys one burst added and removed, per collapse field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarCollapseDiff {
    collapsed_groups: SetDiff,
    collapsed_collections: SetDiff,
    expanded_hover_actions: SetDiff,
    collapsed_coordinators: SetDiff,
    expanded_coordinator_older_threads: SetDiff,
    section_collapse: BTreeMap<String, Option<SectionCollapse>>,
    selected_space_by_section: BTreeMap<String, Option<String>>,
    /// Per SECTION, not per Space: `rememberSidebarSpaceSession` rebuilds a whole section's object
    /// and this writer is the only one that touches it, so the section is the unit that is
    /// replaced. A finer diff would buy nothing and would need a third level of Option to say
    /// "this Space's list was removed", which nothing produces.
    recent_sessions_by_space: BTreeMap<String, Option<BTreeMap<String, Vec<String>>>>,
    sidebar_mode: Option<SidebarMode>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SetDiff {
    added: BTreeSet<String>,
    removed: BTreeSet<String>,
}

impl SetDiff {
    fn between(base: &BTreeSet<String>, next: &BTreeSet<String>) -> Self {
        Self {
            added: next.difference(base).cloned().collect(),
            removed: base.difference(next).cloned().collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    fn apply(&self, stored: Option<&Value>) -> Value {
        let mut object = match stored {
            Some(Value::Object(object)) => object.clone(),
            _ => Map::new(),
        };
        for key in &self.removed {
            object.remove(key);
        }
        for key in &self.added {
            object.insert(key.clone(), Value::Bool(true));
        }
        Value::Object(object)
    }
}

impl SidebarCollapseDiff {
    /// What changed between the state that was read and the state that is held now.
    pub fn between(base: &SidebarCollapseState, next: &SidebarCollapseState) -> Self {
        Self {
            collapsed_groups: SetDiff::between(&base.collapsed_groups, &next.collapsed_groups),
            collapsed_collections: SetDiff::between(
                &base.collapsed_collections,
                &next.collapsed_collections,
            ),
            expanded_hover_actions: SetDiff::between(
                &base.expanded_hover_actions,
                &next.expanded_hover_actions,
            ),
            collapsed_coordinators: SetDiff::between(
                &base.collapsed_coordinators,
                &next.collapsed_coordinators,
            ),
            expanded_coordinator_older_threads: SetDiff::between(
                &base.expanded_coordinator_older_threads,
                &next.expanded_coordinator_older_threads,
            ),
            section_collapse: map_diff(&base.section_collapse, &next.section_collapse),
            selected_space_by_section: map_diff(
                &base.selected_space_by_section,
                &next.selected_space_by_section,
            ),
            recent_sessions_by_space: map_diff(
                &base.recent_sessions_by_space,
                &next.recent_sessions_by_space,
            ),
            sidebar_mode: (base.sidebar_mode != next.sidebar_mode)
                .then(|| next.sidebar_mode.clone()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.collapsed_groups.is_empty()
            && self.collapsed_collections.is_empty()
            && self.expanded_hover_actions.is_empty()
            && self.collapsed_coordinators.is_empty()
            && self.expanded_coordinator_older_threads.is_empty()
            && self.section_collapse.is_empty()
            && self.selected_space_by_section.is_empty()
            && self.recent_sessions_by_space.is_empty()
            && self.sidebar_mode.is_none()
    }

    /// The envelope to store: the stored one with this difference applied, stamped with the
    /// current version.
    ///
    /// Every envelope that carries a `state` object is carried forward, whatever version it names.
    /// A version this build reads (the one before it wrote `selectedSpaceIdBySectionKey`) must
    /// keep the fields this state does not own, exactly as `writeSidebarUiCollapseState` upgrades
    /// it; and a version from a newer build, which this one cannot read, is the payload where
    /// replacing the object would cost the most. Only a value that is not an object, or carries no
    /// `state` object at all, is replaced by one built from `fallback`, so a damaged payload does
    /// not keep a user's clicks from being saved.
    pub fn apply(&self, stored: Option<&str>, fallback: &SidebarCollapseState) -> String {
        let Some(mut object) = stored.and_then(stored_state_object) else {
            return collapse_into_storage(fallback, None);
        };
        // A newer build's envelope keeps its own version: carrying its fields forward and then
        // calling it version 3 would hand that build a payload labelled as a shape it is not.
        let version = stored
            .and_then(stored_version)
            .unwrap_or(COLLAPSE_STORAGE_VERSION)
            .max(COLLAPSE_STORAGE_VERSION);
        apply_set(&mut object, "collapsedGroupsById", &self.collapsed_groups);
        apply_set(
            &mut object,
            "collapsedProjectCollectionsByKey",
            &self.collapsed_collections,
        );
        apply_set(
            &mut object,
            "expandedSessionCardHoverActionsById",
            &self.expanded_hover_actions,
        );
        apply_set(
            &mut object,
            "collapsedCoordinatorsById",
            &self.collapsed_coordinators,
        );
        apply_set(
            &mut object,
            "expandedCoordinatorOlderThreadsById",
            &self.expanded_coordinator_older_threads,
        );
        if !self.section_collapse.is_empty() {
            let mut sections = match object.get("collapsedProjectSessionSectionsById") {
                Some(Value::Object(object)) => object.clone(),
                _ => Map::new(),
            };
            for (storage_id, state) in &self.section_collapse {
                match state {
                    // Only the two headings that are stored; the rest go back to their defaults on
                    // the next read, which is what the sidebar has always done.
                    Some(state) => {
                        sections.insert(
                            storage_id.clone(),
                            json!({ "pinned": state.pinned, "sessions": state.sessions }),
                        );
                    }
                    None => {
                        sections.remove(storage_id);
                    }
                }
            }
            object.insert(
                "collapsedProjectSessionSectionsById".to_string(),
                Value::Object(sections),
            );
        }
        if !self.selected_space_by_section.is_empty() {
            let mut spaces = match object.get("selectedSpaceIdBySectionKey") {
                Some(Value::Object(object)) => object.clone(),
                _ => Map::new(),
            };
            for (section_key, space_id) in &self.selected_space_by_section {
                match space_id {
                    Some(space_id) => {
                        spaces.insert(section_key.clone(), Value::String(space_id.clone()));
                    }
                    None => {
                        spaces.remove(section_key);
                    }
                }
            }
            object.insert(
                "selectedSpaceIdBySectionKey".to_string(),
                Value::Object(spaces),
            );
        }
        if !self.recent_sessions_by_space.is_empty() {
            let mut sections = match object.get("recentSessionIdsBySpace") {
                Some(Value::Object(object)) => object.clone(),
                _ => Map::new(),
            };
            for (section_key, by_space) in &self.recent_sessions_by_space {
                match by_space {
                    Some(by_space) => {
                        sections.insert(
                            section_key.clone(),
                            Value::Object(
                                by_space
                                    .iter()
                                    .map(|(space_id, session_ids)| {
                                        (space_id.clone(), json!(session_ids))
                                    })
                                    .collect(),
                            ),
                        );
                    }
                    None => {
                        sections.remove(section_key);
                    }
                }
            }
            object.insert(
                "recentSessionIdsBySpace".to_string(),
                Value::Object(sections),
            );
        }
        if let Some(mode) = &self.sidebar_mode {
            write_sidebar_mode(&mut object, mode.clone());
        }
        json!({ "state": Value::Object(object), "version": version }).to_string()
    }
}

fn apply_set(object: &mut Map<String, Value>, key: &str, diff: &SetDiff) {
    if diff.is_empty() {
        return;
    }
    let applied = diff.apply(object.get(key));
    object.insert(key.to_string(), applied);
}

fn map_diff<T: Clone + PartialEq>(
    base: &BTreeMap<String, T>,
    next: &BTreeMap<String, T>,
) -> BTreeMap<String, Option<T>> {
    let mut diff: BTreeMap<String, Option<T>> = BTreeMap::new();
    for (key, value) in next {
        if base.get(key) != Some(value) {
            diff.insert(key.clone(), Some(value.clone()));
        }
    }
    for key in base.keys() {
        if !next.contains_key(key) {
            diff.insert(key.clone(), None);
        }
    }
    diff
}
