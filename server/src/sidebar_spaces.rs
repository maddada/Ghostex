use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::sidebar_project_collections::read_sidebar_project_collections;

/*
CDXC:SidebarSpaces 2026-08-27:
A Space is a server-owned saved sidebar filter: a name, an icon id, a color, a
manual position, and the set of sidebar members it shows. Members are sidebar
project collections ("groups") and ungrouped projects; a collection or an
ungrouped project may belong to any number of Spaces. gxserver owns the
document so desktop, web, and mobile clients connected to the same daemon share
one Space set, and a remote daemon's Spaces stay that daemon's own.

Two invariants are enforced here rather than in each client:
  - A project inside a collection can never hold direct membership. It inherits
    its collection's Spaces, so any member project id that the collections
    document currently groups is dropped.
  - Member collection ids that no longer exist are dropped, because a
    collection disappears from the collections document as soon as it empties.
Worktree inheritance is a pure client concern and never stored. The built-in
"All Projects" view is a client-side constant and is never stored here either.

An empty Space is valid and is kept — unlike a project collection, a Space with
zero members still exists so the user can fill it later.
*/

const SIDEBAR_SPACES_METADATA_KEY: &str = "sidebarSpaces";
const MAX_SPACES: usize = 256;
const MAX_MEMBER_IDS_PER_LIST: usize = 512;
const MAX_ID_CHARS: usize = 256;
const MAX_NAME_CHARS: usize = 256;
const MAX_ICON_CHARS: usize = 256;

/// Mirrors SIDEBAR_PROJECT_COLLECTION_COLORS in packages/core-ui/project-collections.ts
/// so server-side fallback colors rotate exactly like the sidebar's other
/// user-colored overlay.
const SIDEBAR_SPACE_COLORS: [&str; 13] = [
    "#4f5663", "#808080", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4",
    "#8c9b45", "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

/// Icon ids come from SIDEBAR_COMMAND_ICON_IDS in
/// packages/shared/sidebar-command-icons.ts. The daemon deliberately does not
/// validate against that allowlist — that would pin the server to one client
/// build's icon pack — it only bounds the id and supplies this default when a
/// Space carries no usable icon.
const DEFAULT_SIDEBAR_SPACE_ICON: &str = "stack";

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
    let normalized = normalize_sidebar_spaces_state(state, &collections);
    write_sidebar_spaces_state(db, &normalized)?;
    Ok(normalized)
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
    collections_state: &Value,
) -> Result<Option<Value>, DomainStateError> {
    let previous = read_stored_sidebar_spaces_state(db)?;
    let normalized = normalize_sidebar_spaces_state(&previous, collections_state);
    if normalized == previous {
        return Ok(None);
    }
    write_sidebar_spaces_state(db, &normalized)?;
    Ok(Some(normalized))
}

pub fn normalize_sidebar_spaces_state(state: &Value, collections_state: &Value) -> Value {
    let collections = collections_state
        .get("collections")
        .and_then(Value::as_object);
    // Projects held by a collection inherit that collection's Spaces and can
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
    for space_id in ordered_ids {
        if spaces.len() >= MAX_SPACES {
            break;
        }
        let Some(space_state) = candidate_state_by_id.get(space_id.as_str()) else {
            continue;
        };
        let member_collection_ids =
            normalized_member_ids(space_state.get("memberCollectionIds"), |collection_id| {
                match collections {
                    Some(entries) => entries.contains_key(collection_id),
                    None => false,
                }
            });
        let member_project_ids = normalized_member_ids(space_state.get("memberProjectIds"), |id| {
            !grouped_project_ids.contains(id)
        });
        let name = trimmed_bounded_text(space_state.get("name"), MAX_NAME_CHARS)
            .unwrap_or_else(|| space_id.clone());
        let icon = trimmed_bounded_text(space_state.get("icon"), MAX_ICON_CHARS)
            .unwrap_or_else(|| DEFAULT_SIDEBAR_SPACE_ICON.to_string());
        let color = normalized_space_color(space_state.get("color"), spaces.len());
        spaces.insert(
            space_id.clone(),
            json!({
                "color": color,
                "icon": icon,
                "memberCollectionIds": member_collection_ids,
                "memberProjectIds": member_project_ids,
                "name": name,
                "spaceId": space_id,
            }),
        );
        order.push(space_id);
    }
    json!({
        "order": order,
        "spaces": spaces,
    })
}

fn normalized_member_ids(value: Option<&Value>, keep: impl Fn(&str) -> bool) -> Vec<String> {
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
