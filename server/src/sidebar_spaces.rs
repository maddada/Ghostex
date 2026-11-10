use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::sidebar_project_collections::read_sidebar_project_collections;

/*
CDXC:Spaces 2026-08-27:
A Space is a server-owned saved sidebar filter: a name, an icon id, a color, a
manual position, and the set of sidebar members it shows. Members are sidebar
project collections ("groups") and ungrouped projects; a collection or an
ungrouped project belongs to at most one Space. gxserver owns the
document so desktop, web, and mobile clients connected to the same daemon share
one Space set, and a remote daemon's Spaces stay that daemon's own.

Membership invariants are enforced here for every client:
  - Each member belongs to at most one Space; the first Space in sidebar order wins.
  - A project inside a collection can never hold direct membership. It inherits
    its collection's Space, so any member project id that the collections
    document currently groups is dropped.
  - Member collection ids that no longer exist are dropped, because a
    collection disappears from the collections document as soon as it empties.
Worktree inheritance is a pure client concern and never stored. The built-in
"Other" view is a client-side constant and is never stored here either.

An empty Space is valid and is kept — unlike a project collection, a Space with
zero members still exists so the user can fill it later.
*/

const SIDEBAR_SPACES_METADATA_KEY: &str = "sidebarSpaces";
const MAX_SPACES: usize = 256;
const MAX_MEMBER_IDS_PER_LIST: usize = 512;
const MAX_ID_CHARS: usize = 256;
const MAX_NAME_CHARS: usize = 256;
const MAX_ICON_CHARS: usize = 256;

/// Mirrors SIDEBAR_PROJECT_COLLECTION_COLORS in packages/core-ui/project-collections.ts (deleted 2026-10-01)
/// so server-side fallback colors rotate exactly like the sidebar's other
/// user-colored overlay.
const SIDEBAR_SPACE_COLORS: [&str; 13] = [
    "#4f5663", "#808080", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4",
    "#8c9b45", "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

/// Icon ids come from SIDEBAR_COMMAND_ICON_IDS in
/// packages/shared/sidebar-command-icons.ts (deleted 2026-10-01). The daemon deliberately does not
/// validate against that allowlist — that would pin the server to one client
/// build's icon pack — it only bounds the id and supplies this default when a
/// Space carries no usable icon.
const DEFAULT_SIDEBAR_SPACE_ICON: &str = "stack";

/// This machine's own Spaces switch (`sidebarSpacesEnabled`, Settings › Sidebar & Sessions ›
/// Spaces), as its settings file holds it.
///
/// CDXC:Spaces 2026-10-06 DECISION:
/// "so spaces is a setting per gxserver not per client app, but it's toggled in settings like it is now basically". The daemon publishes its own switch with its Spaces document (the presentation snapshot and every `sidebarSpacesChanged` frame), and a client viewing this machine shows Spaces exactly when it is on.
pub fn read_sidebar_spaces_enabled(paths: &crate::paths::GxserverPaths) -> bool {
    ghostex_settings_catalog::spaces::enabled_in_value(
        crate::session_lifecycle::read_sidebar_settings(paths).as_ref(),
    )
}

pub fn empty_sidebar_spaces_state() -> Value {
    json!({
        "order": [],
        "spaces": {},
    })
}

pub fn read_sidebar_spaces(db: &Connection) -> Result<Value, DomainStateError> {
    let collections = read_sidebar_project_collections(db)?;
    let stored = read_stored_sidebar_spaces_state(db)?;
    Ok(normalize_sidebar_spaces_state(&stored, &collections))
}

pub fn update_sidebar_spaces(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let state = params
        .get("state")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            DomainStateError::bad_request("Sidebar spaces update requires a state object.")
        })?;
    let collections = read_sidebar_project_collections(db)?;
    let previous = read_stored_sidebar_spaces_state(db)?;
    let state = carry_space_workspace_ids(state, &previous);
    let normalized = normalize_sidebar_spaces_state(&state, &collections);
    write_sidebar_spaces_state(db, &normalized)?;
    Ok(normalized)
}

/// A Space written without a `workspaceId` keeps the one it had.
///
/// CDXC:Workspaces 2026-10-09 WHY:
/// Clients that predate workspaces rewrite the whole Space document without the field; without
/// this carry every Space would fall back to the default workspace on their next edit. A Space
/// that never had one belongs to the default workspace.
fn carry_space_workspace_ids(state: &Value, previous: &Value) -> Value {
    let mut carried = state.clone();
    let Some(spaces) = carried.get_mut("spaces").and_then(Value::as_object_mut) else {
        return carried;
    };
    for (space_id, space) in spaces.iter_mut() {
        let Some(space) = space.as_object_mut() else {
            continue;
        };
        if space.contains_key("workspaceId") {
            continue;
        }
        if let Some(workspace_id) = previous
            .get("spaces")
            .and_then(|spaces| spaces.get(space_id.trim()))
            .and_then(|space| space.get("workspaceId"))
        {
            space.insert("workspaceId".to_string(), workspace_id.clone());
        }
    }
    carried
}

/// Moves every Space of a deleted workspace to the default workspace, and drops a project that
/// left for another workspace from the Spaces that list it directly. Returns the new document
/// when it changed.
pub fn rehome_sidebar_spaces_for_workspaces(
    db: &Connection,
    deleted_workspace_id: Option<&str>,
    moved_project_ids: &[String],
) -> Result<Option<Value>, DomainStateError> {
    let previous = read_stored_sidebar_spaces_state(db)?;
    let mut next = previous.clone();
    if let Some(spaces) = next.get_mut("spaces").and_then(Value::as_object_mut) {
        for space in spaces.values_mut() {
            let Some(space) = space.as_object_mut() else {
                continue;
            };
            if deleted_workspace_id.is_some()
                && space.get("workspaceId").and_then(Value::as_str) == deleted_workspace_id
            {
                space.remove("workspaceId");
            }
            if let Some(members) = space
                .get_mut("memberProjectIds")
                .and_then(Value::as_array_mut)
            {
                members.retain(|member| {
                    !member
                        .as_str()
                        .is_some_and(|id| moved_project_ids.iter().any(|moved| moved == id))
                });
            }
        }
    }
    let collections = read_sidebar_project_collections(db)?;
    let normalized = normalize_sidebar_spaces_state(&next, &collections);
    if normalized == previous {
        return Ok(None);
    }
    write_sidebar_spaces_state(db, &normalized)?;
    Ok(Some(normalized))
}

/// Re-apply the cross-document invariants after the collections document
/// changed, returning the new spaces state only when it actually moved.
///
/// Grouping a project must strip its direct Space memberships, and emptying a
/// collection removes it from the collections document, which must also remove
/// it from every Space that referenced it. The collections routes call this
/// while still holding the presentation event-sequence lock so both documents
/// and both broadcasts land as one ordered mutation.
pub fn prune_sidebar_spaces_for_collections(
    db: &Connection,
    previous_collections_state: &Value,
    collections_state: &Value,
) -> Result<Option<Value>, DomainStateError> {
    let previous = read_stored_sidebar_spaces_state(db)?;
    let carried = carry_sidebar_spaces_across_collections(
        &previous,
        previous_collections_state,
        collections_state,
    );
    let normalized = normalize_sidebar_spaces_state(&carried, collections_state);
    if normalized == previous {
        return Ok(None);
    }
    write_sidebar_spaces_state(db, &normalized)?;
    Ok(Some(normalized))
}

/// Keeps every project in the Space it was in across a collections change: a new collection that
/// no Space lists joins the Space its projects were in, and a project that leaves every collection
/// keeps its old collection's Space. Joining an existing collection still takes that collection's
/// Space. Runs before the normalizer, which then strips the grouped projects' own membership.
///
/// CDXC:Spaces 2026-09-28 SEE-ALSO:
/// packages/gx-core/src/project_docs/space_edits.rs (`keep_spaces_across_collection_edit`) holds
/// the user's decision and applies the same rule optimistically in the desktop and web sidebars.
/// Doing it here too covers `ghostex group-project`, remote machines, and a client whose Spaces
/// push lands before its collections push and was checked against the old collections.
fn carry_sidebar_spaces_across_collections(
    spaces_state: &Value,
    previous_collections_state: &Value,
    collections_state: &Value,
) -> Value {
    let previous_collections = collection_project_ids(previous_collections_state);
    let next_collections = collection_project_ids(collections_state);
    let Some(space_map) = spaces_state.get("spaces").and_then(Value::as_object) else {
        return spaces_state.clone();
    };
    let mut space_order: Vec<&str> = spaces_state
        .get("order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|space_id| space_map.contains_key(*space_id))
        .collect();
    for space_id in space_map.keys() {
        if !space_order.contains(&space_id.as_str()) {
            space_order.push(space_id);
        }
    }
    let space_listing = |field: &str, member_id: &str| -> Option<String> {
        space_order
            .iter()
            .find(|space_id| {
                space_map
                    .get(**space_id)
                    .and_then(|space| space.get(field))
                    .and_then(Value::as_array)
                    .is_some_and(|members| {
                        members
                            .iter()
                            .any(|member| member.as_str() == Some(member_id))
                    })
            })
            .map(|space_id| (*space_id).to_string())
    };
    let space_before = |project_id: &str| -> Option<String> {
        match previous_collections
            .iter()
            .find(|(_, project_ids)| project_ids.iter().any(|id| id == project_id))
        {
            Some((collection_id, _)) => space_listing("memberCollectionIds", collection_id),
            None => space_listing("memberProjectIds", project_id),
        }
    };
    let mut additions: Vec<(String, &'static str, String)> = Vec::new();
    for (collection_id, project_ids) in &next_collections {
        let is_new = !previous_collections
            .iter()
            .any(|(previous_id, _)| previous_id == collection_id);
        if !is_new || space_listing("memberCollectionIds", collection_id).is_some() {
            continue;
        }
        if let Some(space_id) = project_ids
            .iter()
            .find_map(|project_id| space_before(project_id))
        {
            additions.push((space_id, "memberCollectionIds", collection_id.clone()));
        }
    }
    for (collection_id, project_ids) in &previous_collections {
        for project_id in project_ids {
            let still_grouped = next_collections
                .iter()
                .any(|(_, next_ids)| next_ids.contains(project_id));
            if still_grouped || space_listing("memberProjectIds", project_id).is_some() {
                continue;
            }
            if let Some(space_id) = space_listing("memberCollectionIds", collection_id) {
                additions.push((space_id, "memberProjectIds", project_id.clone()));
            }
        }
    }
    let mut carried = spaces_state.clone();
    for (space_id, field, member_id) in additions {
        let Some(space) = carried
            .get_mut("spaces")
            .and_then(|spaces| spaces.get_mut(&space_id))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let members = space
            .entry(field)
            .or_insert_with(|| Value::Array(Vec::new()));
        if !members.is_array() {
            *members = Value::Array(Vec::new());
        }
        if let Some(members) = members.as_array_mut() {
            if !members
                .iter()
                .any(|member| member.as_str() == Some(member_id.as_str()))
            {
                members.push(Value::String(member_id));
            }
        }
    }
    carried
}

/// Each collection's id and project ids, in the order the collections document lists them.
fn collection_project_ids(collections_state: &Value) -> Vec<(String, Vec<String>)> {
    let Some(entries) = collections_state
        .get("collections")
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut ordered: Vec<&str> = collections_state
        .get("order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|collection_id| entries.contains_key(*collection_id))
        .collect();
    for collection_id in entries.keys() {
        if !ordered.contains(&collection_id.as_str()) {
            ordered.push(collection_id);
        }
    }
    ordered
        .into_iter()
        .map(|collection_id| {
            let project_ids = entries
                .get(collection_id)
                .and_then(|collection| collection.get("projectIds"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            (collection_id.to_string(), project_ids)
        })
        .collect()
}

/// CDXC:Spaces 2026-09-07 SEE-ALSO:
/// packages/core-ui/spaces.ts (deleted 2026-10-01) owns the one-Space-per-project decision and optimistic moves.
/// Normalize reads and writes in sidebar order so older clients and CLI payloads cannot retain duplicate memberships.
pub fn normalize_sidebar_spaces_state(state: &Value, collections_state: &Value) -> Value {
    let collections = collections_state
        .get("collections")
        .and_then(Value::as_object);
    // Projects held by a collection inherit that collection's Space and can
    // never carry direct membership.
    let mut grouped_project_ids = std::collections::HashSet::new();
    if let Some(entries) = collections {
        for collection_state in entries.values() {
            let Some(project_ids) = collection_state.get("projectIds").and_then(Value::as_array)
            else {
                continue;
            };
            for entry in project_ids {
                if let Some(project_id) = trimmed_bounded_text(Some(entry), MAX_ID_CHARS) {
                    grouped_project_ids.insert(project_id);
                }
            }
        }
    }

    // Candidate spaces keyed by trimmed space id; first occurrence wins.
    let mut candidates: Vec<(String, &Value)> = Vec::new();
    let mut candidate_ids = std::collections::HashSet::new();
    if let Some(entries) = state.get("spaces").and_then(Value::as_object) {
        for (space_id, space_state) in entries {
            let space_id = space_id.trim();
            if space_id.is_empty() || space_id.chars().count() > MAX_ID_CHARS {
                continue;
            }
            if !space_state.is_object() {
                continue;
            }
            if candidate_ids.insert(space_id.to_string()) {
                candidates.push((space_id.to_string(), space_state));
            }
        }
    }
    let candidate_state_by_id: std::collections::HashMap<&str, &Value> = candidates
        .iter()
        .map(|(id, state)| (id.as_str(), *state))
        .collect();
    // The explicit order array is authoritative; ids missing from it append in
    // stored map order so every kept space always has a position.
    let mut ordered_ids: Vec<String> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = state.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = trimmed_bounded_text(Some(entry), MAX_ID_CHARS) else {
                continue;
            };
            if candidate_state_by_id.contains_key(id.as_str()) && seen_order_ids.insert(id.clone())
            {
                ordered_ids.push(id);
            }
        }
    }
    for (id, _) in &candidates {
        if seen_order_ids.insert(id.clone()) {
            ordered_ids.push(id.clone());
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut spaces = Map::new();
    let mut assigned_collection_ids = std::collections::HashSet::new();
    let mut assigned_project_ids = std::collections::HashSet::new();
    for space_id in ordered_ids {
        if spaces.len() >= MAX_SPACES {
            break;
        }
        let Some(space_state) = candidate_state_by_id.get(space_id.as_str()) else {
            continue;
        };
        let member_collection_ids =
            normalized_member_ids(space_state.get("memberCollectionIds"), |collection_id| {
                collections.is_some_and(|entries| entries.contains_key(collection_id))
                    && assigned_collection_ids.insert(collection_id.to_string())
            });
        let member_project_ids = normalized_member_ids(space_state.get("memberProjectIds"), |id| {
            !grouped_project_ids.contains(id) && assigned_project_ids.insert(id.to_string())
        });
        let name = trimmed_bounded_text(space_state.get("name"), MAX_NAME_CHARS)
            .unwrap_or_else(|| space_id.clone());
        let icon = trimmed_bounded_text(space_state.get("icon"), MAX_ICON_CHARS)
            .unwrap_or_else(|| DEFAULT_SIDEBAR_SPACE_ICON.to_string());
        let color = normalized_space_color(space_state.get("color"), spaces.len());
        let mut space = json!({
            "color": color,
            "icon": icon,
            "memberCollectionIds": member_collection_ids,
            "memberProjectIds": member_project_ids,
            "name": name,
            "spaceId": space_id,
        });
        // CDXC:Workspaces 2026-10-09 WHY: absent means the default workspace, so the stored
        // document of an install that never made a second workspace stays byte-identical.
        if let Some(workspace_id) =
            trimmed_bounded_text(space_state.get("workspaceId"), MAX_ID_CHARS)
                .filter(|id| id != crate::workspaces::DEFAULT_WORKSPACE_ID)
        {
            space["workspaceId"] = Value::String(workspace_id);
        }
        spaces.insert(space_id.clone(), space);
        order.push(space_id);
    }
    json!({
        "order": order,
        "spaces": spaces,
    })
}

fn normalized_member_ids(value: Option<&Value>, mut keep: impl FnMut(&str) -> bool) -> Vec<String> {
    let mut member_ids: Vec<String> = Vec::new();
    let mut seen_member_ids = std::collections::HashSet::new();
    let Some(entries) = value.and_then(Value::as_array) else {
        return member_ids;
    };
    for entry in entries {
        if member_ids.len() >= MAX_MEMBER_IDS_PER_LIST {
            break;
        }
        let Some(member_id) = trimmed_bounded_text(Some(entry), MAX_ID_CHARS) else {
            continue;
        };
        if !keep(member_id.as_str()) {
            continue;
        }
        if seen_member_ids.insert(member_id.clone()) {
            member_ids.push(member_id);
        }
    }
    member_ids
}

fn read_stored_sidebar_spaces_state(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [SIDEBAR_SPACES_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite sidebar spaces error: {error}"),
        })?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(empty_sidebar_spaces_state))
}

fn write_sidebar_spaces_state(db: &Connection, state: &Value) -> Result<(), DomainStateError> {
    let serialized = serde_json::to_string(state).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Sidebar spaces serialization error: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![SIDEBAR_SPACES_METADATA_KEY, serialized, now_iso()],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite sidebar spaces error: {error}"),
    })?;
    Ok(())
}

fn normalized_space_color(value: Option<&Value>, fallback_index: usize) -> String {
    if let Some(color) = value.and_then(Value::as_str) {
        let color = color.trim();
        if is_valid_space_color(color) {
            return color.to_ascii_lowercase();
        }
    }
    SIDEBAR_SPACE_COLORS[fallback_index % SIDEBAR_SPACE_COLORS.len()].to_string()
}

fn is_valid_space_color(color: &str) -> bool {
    let mut chars = color.chars();
    chars.next() == Some('#')
        && color.len() == 7
        && chars.all(|character| character.is_ascii_hexdigit())
}

fn trimmed_bounded_text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() || text.chars().count() > max_chars {
        return None;
    }
    Some(text.to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
