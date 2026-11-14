//! The exact shapes client storage holds for the sidebar's own state.
//!
//! CDXC:Sidebar 2026-09-20 WHY:
//! The keys, the envelope, the version number and every field name here are the ones
//! `collapse-state.ts`, `machine-tab-selection.ts` (deleted 2026-10-01) and `sidebar-hidden-items.ts` wrote before this
//! port, because an installation that upgrades must keep its collapsed groups, its Space, its
//! hidden items and its filters, and a build that predates the port must still read what this
//! writes. A write therefore keeps every field it does not own (`isReferenceChatsCollapsed`, the
//! per-Space session memory) exactly as it found it rather than re-deriving the object.
//!
//! SEE-ALSO: packages/core-ui/sidebar-app/collapse-state.ts (deleted 2026-10-01),
//! packages/core-ui/sidebar-app/machine-tab-selection.ts (deleted 2026-10-01), packages/core-ui/sidebar-hidden-items.ts (deleted 2026-10-01),
//! packages/core-ui/sidebar-app/project-session-section-model.ts (deleted 2026-10-01).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use crate::keys::encode_uri_component;
use crate::sidebar_view::{
    SectionCollapse, SidebarCollapseState, SidebarHiddenItems, SidebarMode, LOCAL_MACHINE_ID,
    MAX_RECENT_SPACE_SESSION_IDS,
};

/// The unscoped part of the collapse key; the window scope is appended.
pub const COLLAPSE_STORAGE_KEY: &str = "ghostex-sidebar-ui-collapse-state";
/// The unscoped part of the selected machine key.
pub const MACHINE_TAB_STORAGE_KEY: &str = "ghostex-sidebar-selected-machine-tab";
/// Hidden projects and collections; not scoped to a window.
pub const HIDDEN_ITEMS_STORAGE_KEY: &str = "ghostex.sidebar.hidden-items.v1";
/// The sidebar's own copy of the project collections.
pub const PROJECT_COLLECTIONS_STORAGE_KEY: &str = "ghostex.sidebar.projectCollections.v1";
/// The window scope the desktop app has always used.
pub const SIDEBAR_WINDOW_SCOPE_ID: &str = "main";
/// The envelope version a write stamps.
pub const COLLAPSE_STORAGE_VERSION: u64 = 3;

/// Versions a read accepts, as `SUPPORTED_SIDEBAR_UI_COLLAPSE_STORAGE_VERSIONS` does.
const SUPPORTED_VERSIONS: [u64; 2] = [2, 3];

/// `getSidebarUiCollapseStateStorageKey` and `getSidebarMachineTabStorageKey`: the unscoped key,
/// `:window:`, and the scope id escaped the way `encodeURIComponent` escapes it.
pub fn sidebar_window_storage_key(unscoped_key: &str, window_scope_id: &str) -> String {
    format!(
        "{unscoped_key}:window:{}",
        encode_uri_component(window_scope_id)
    )
}

/// `readSidebarSelectedMachineTabId`: a stored non-empty id, else this machine.
pub fn machine_tab_from_storage(raw: Option<&str>) -> String {
    raw.filter(|value| !value.is_empty())
        .unwrap_or(LOCAL_MACHINE_ID)
        .to_string()
}

/// `readSidebarUiCollapseState`: the scoped envelope when it parses and carries a supported
/// version, else the legacy unscoped payload, whose collapsed collections come from the separate
/// collections document instead.
///
/// An envelope that is present but damaged reads as the defaults, exactly as the TypeScript does,
/// rather than falling through to the legacy payload.
pub fn collapse_state_from_storage(
    scoped: Option<&str>,
    legacy: Option<&str>,
    legacy_collections: Option<&str>,
) -> SidebarCollapseState {
    if let Some(raw) = scoped {
        let Some(envelope) = parse_object(raw) else {
            return SidebarCollapseState::default();
        };
        let version = envelope.get("version").and_then(Value::as_u64);
        if !version.is_some_and(|version| SUPPORTED_VERSIONS.contains(&version)) {
            return SidebarCollapseState::default();
        }
        return normalize_collapse_state(envelope.get("state"));
    }
    let Some(raw) = legacy.and_then(parse_object) else {
        return SidebarCollapseState {
            collapsed_collections: legacy_collapsed_collections(legacy_collections),
            ..SidebarCollapseState::default()
        };
    };
    SidebarCollapseState {
        collapsed_collections: legacy_collapsed_collections(legacy_collections),
        ..normalize_collapse_state(Some(&Value::Object(raw)))
    }
}

/// The collapse envelope to write: the owned fields from `state`, every other field of a stored
/// envelope kept as it was, and the current version.
pub fn collapse_into_storage(state: &SidebarCollapseState, existing: Option<&str>) -> String {
    let mut object = existing.and_then(stored_state_object).unwrap_or_default();
    object.insert(
        "collapsedGroupsById".to_string(),
        flag_map(&state.collapsed_groups),
    );
    object.insert(
        "collapsedProjectCollectionsByKey".to_string(),
        flag_map(&state.collapsed_collections),
    );
    object.insert(
        "expandedSessionCardHoverActionsById".to_string(),
        flag_map(&state.expanded_hover_actions),
    );
    object.insert(
        "collapsedCoordinatorsById".to_string(),
        flag_map(&state.collapsed_coordinators),
    );
    object.insert(
        "expandedCoordinatorOlderThreadsById".to_string(),
        flag_map(&state.expanded_coordinator_older_threads),
    );
    object.insert(
        "collapsedProjectSessionSectionsById".to_string(),
        persisted_section_collapse(&state.section_collapse),
    );
    object.insert(
        "selectedSpaceIdBySectionKey".to_string(),
        Value::Object(
            state
                .selected_space_by_section
                .iter()
                .map(|(section, space_id)| (section.clone(), Value::String(space_id.clone())))
                .collect(),
        ),
    );
    object.insert(
        "recentSessionIdsBySpace".to_string(),
        recent_sessions_into_storage(&state.recent_sessions_by_space),
    );
    write_sidebar_mode(&mut object, state.sidebar_mode.clone());
    // The one field this state does not own travels untouched: normalizing it here would be this
    // writer quietly editing another writer's value, and the reader on either side already
    // normalizes what it finds. A first write spells out the default a reader would otherwise
    // normalize to, so the object an older build reads back is the one it would have written.
    object
        .entry("isReferenceChatsCollapsed".to_string())
        .or_insert(Value::Bool(false));
    json!({ "state": Value::Object(object), "version": COLLAPSE_STORAGE_VERSION }).to_string()
}

/// `readSidebarHiddenItems`: two lists of unique, non-empty strings; anything else reads as empty.
pub fn hidden_items_from_storage(raw: Option<&str>) -> SidebarHiddenItems {
    let Some(value) = raw.and_then(parse_object) else {
        return SidebarHiddenItems::default();
    };
    let list = |key: &str| -> Vec<String> {
        let mut unique: Vec<String> = Vec::new();
        for entry in value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(entry) = entry.as_str().filter(|entry| !entry.is_empty()) else {
                continue;
            };
            if !unique.iter().any(|seen| seen == entry) {
                unique.push(entry.to_string());
            }
        }
        unique
    };
    SidebarHiddenItems {
        group_ids: list("groupIds"),
        collection_keys: list("collectionKeys"),
    }
}

/// `writeSidebarHiddenItems`: the two lists, in the order the sidebar holds them.
pub fn hidden_items_into_storage(items: &SidebarHiddenItems) -> String {
    json!({
        "collectionKeys": items.collection_keys,
        "groupIds": items.group_ids,
    })
    .to_string()
}

/// The version a stored envelope names, when it names a number.
pub(super) fn stored_version(raw: &str) -> Option<u64> {
    parse_object(raw)?.get("version")?.as_u64()
}

/// The `state` object of a stored envelope, whatever version it names.
pub(super) fn stored_state_object(raw: &str) -> Option<Map<String, Value>> {
    match parse_object(raw)?.get("state") {
        Some(Value::Object(state)) => Some(state.clone()),
        _ => None,
    }
}

fn parse_object(raw: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(object)) => Some(object),
        _ => None,
    }
}

/// `normalizeSidebarUiCollapseState`, for the fields this state owns.
fn normalize_collapse_state(state: Option<&Value>) -> SidebarCollapseState {
    let Some(Value::Object(state)) = state else {
        return SidebarCollapseState::default();
    };
    SidebarCollapseState {
        collapsed_groups: flag_set(state.get("collapsedGroupsById")),
        collapsed_collections: flag_set(state.get("collapsedProjectCollectionsByKey")),
        expanded_hover_actions: flag_set(state.get("expandedSessionCardHoverActionsById")),
        collapsed_coordinators: flag_set(state.get("collapsedCoordinatorsById")),
        expanded_coordinator_older_threads: flag_set(
            state.get("expandedCoordinatorOlderThreadsById"),
        ),
        section_collapse: normalize_section_collapse(
            state.get("collapsedProjectSessionSectionsById"),
        ),
        selected_space_by_section: normalize_selected_spaces(
            state.get("selectedSpaceIdBySectionKey"),
        ),
        recent_sessions_by_space: normalize_recent_sessions(state.get("recentSessionIdsBySpace")),
        sidebar_mode: state
            .get(SIDEBAR_MODE_KEY)
            .and_then(Value::as_str)
            .and_then(SidebarMode::parse)
            .unwrap_or_default(),
    }
}

/// Where the collapse envelope keeps the sidebar mode, beside `selectedSpaceIdBySectionKey`.
const SIDEBAR_MODE_KEY: &str = "sidebarMode";

/// Projects is the absent key, so an envelope of a user who never opened Bots is byte-identical to
/// the one a build without Bots writes.
pub(super) fn write_sidebar_mode(object: &mut Map<String, Value>, mode: SidebarMode) {
    match mode {
        SidebarMode::Projects => {
            object.remove(SIDEBAR_MODE_KEY);
        }
        SidebarMode::Bots | SidebarMode::Extension(_) => {
            object.insert(SIDEBAR_MODE_KEY.to_string(), Value::from(mode.as_wire()));
        }
    }
}

/// `normalizeStoredRecentSessionIdsBySpace`: non-empty section key, non-empty Space id, a list of
/// unique non-empty ids cut to the cap. A section or a Space that normalizes to nothing is dropped
/// rather than stored empty, which is what keeps the written object equal to the one the sidebar
/// wrote before this port.
fn normalize_recent_sessions(
    value: Option<&Value>,
) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let Some(Value::Object(object)) = value else {
        return BTreeMap::new();
    };
    let mut sections = BTreeMap::new();
    for (section_key, spaces) in object {
        let Some(spaces) = spaces.as_object().filter(|_| !section_key.is_empty()) else {
            continue;
        };
        let mut by_space: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (space_id, session_ids) in spaces {
            let Some(session_ids) = session_ids.as_array().filter(|_| !space_id.is_empty()) else {
                continue;
            };
            let mut unique: Vec<String> = Vec::new();
            for session_id in session_ids {
                let Some(session_id) = session_id.as_str().filter(|id| !id.is_empty()) else {
                    continue;
                };
                if !unique.iter().any(|seen| seen == session_id) {
                    unique.push(session_id.to_string());
                }
            }
            unique.truncate(MAX_RECENT_SPACE_SESSION_IDS);
            if !unique.is_empty() {
                by_space.insert(space_id.clone(), unique);
            }
        }
        if !by_space.is_empty() {
            sections.insert(section_key.clone(), by_space);
        }
    }
    sections
}

/// The stored shape of the per-Space session memory.
pub(super) fn recent_sessions_into_storage(
    sections: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> Value {
    Value::Object(
        sections
            .iter()
            .map(|(section_key, by_space)| {
                (
                    section_key.clone(),
                    Value::Object(
                        by_space
                            .iter()
                            .map(|(space_id, session_ids)| (space_id.clone(), json!(session_ids)))
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}

/// `normalizeStoredCollapsedGroupsById`: only entries whose value is exactly `true` count.
fn flag_set(value: Option<&Value>) -> BTreeSet<String> {
    let Some(Value::Object(object)) = value else {
        return BTreeSet::new();
    };
    object
        .iter()
        .filter(|(_, flag)| **flag == Value::Bool(true))
        .map(|(key, _)| key.clone())
        .collect()
}

fn flag_map(keys: &BTreeSet<String>) -> Value {
    Value::Object(
        keys.iter()
            .map(|key| (key.clone(), Value::Bool(true)))
            .collect(),
    )
}

/// `normalizeProjectSessionSectionCollapseState`: only Pinned and Sessions are stored; every other
/// heading takes the default the sidebar starts with.
fn normalize_section_collapse(value: Option<&Value>) -> BTreeMap<String, SectionCollapse> {
    let Some(Value::Object(object)) = value else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(storage_id, state)| {
            let state = state.as_object()?;
            let flag = |key: &str| state.get(key) == Some(&Value::Bool(true));
            Some((
                storage_id.clone(),
                SectionCollapse {
                    pinned: flag("pinned"),
                    sessions: flag("sessions"),
                    ..SectionCollapse::default()
                },
            ))
        })
        .collect()
}

/// `persistedProjectSessionSectionCollapseState`: the two persisted headings, nothing else.
fn persisted_section_collapse(state: &BTreeMap<String, SectionCollapse>) -> Value {
    Value::Object(
        state
            .iter()
            .map(|(storage_id, sections)| {
                (
                    storage_id.clone(),
                    json!({ "pinned": sections.pinned, "sessions": sections.sessions }),
                )
            })
            .collect(),
    )
}

/// `normalizeStoredSelectedSpaceIdBySectionKey`: non-empty section key to non-empty Space id.
fn normalize_selected_spaces(value: Option<&Value>) -> BTreeMap<String, String> {
    let Some(Value::Object(object)) = value else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter(|(section_key, _)| !section_key.is_empty())
        .filter_map(|(section_key, space_id)| {
            let space_id = space_id.as_str().filter(|id| !id.is_empty())?;
            Some((section_key.clone(), space_id.to_string()))
        })
        .collect()
}

/// The collapsed collections a payload written before the scoped key existed carried: they lived
/// in the collections document itself, under the local section key.
fn legacy_collapsed_collections(raw: Option<&str>) -> BTreeSet<String> {
    let Some(document) = raw.and_then(parse_object) else {
        return BTreeSet::new();
    };
    document
        .get("collections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|collection| {
            let collection = collection.as_object()?;
            if collection.get("collapsed") != Some(&Value::Bool(true)) {
                return None;
            }
            let collection_id = collection.get("collectionId")?.as_str()?;
            Some(format!("{LOCAL_MACHINE_ID}:{collection_id}"))
        })
        .collect()
}
