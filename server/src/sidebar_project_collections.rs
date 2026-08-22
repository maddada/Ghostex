use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::domain::DomainStateError;

/*
CDXC:SidebarProjectCollections 2026-07-18-00:00:
GPUI's colored "Group N" project collections used to live only in the desktop
sidebar's localStorage (`ghostex.sidebar.projectCollections.v1`), so React
Native Android could neither render nor edit the same grouped project list. gxserver
now owns a durable normalized copy of that overlay in the metadata table.
Every editor (GPUI and React Native Android) write-through-syncs the whole normalized
state here so all clients read one contract. Keep this metadata-only:
collection ids, titles, colors, project ids, and an explicit ordering array.
Expansion is client-local UI state and never enters gxserver.
*/

const SIDEBAR_PROJECT_COLLECTIONS_METADATA_KEY: &str = "sidebarProjectCollections";
const MAX_COLLECTIONS: usize = 256;
const MAX_PROJECT_IDS_PER_COLLECTION: usize = 512;
const MAX_ID_CHARS: usize = 256;
const MAX_TITLE_CHARS: usize = 256;
const MAX_NEXT_COLLECTION_NUMBER: i64 = 1_000_000;

/// Mirrors SIDEBAR_PROJECT_COLLECTION_COLORS in sidebar/project-collections.ts
/// so server-side fallback colors rotate exactly like the desktop client.
const SIDEBAR_PROJECT_COLLECTION_COLORS: [&str; 13] = [
    "#4f5663", "#808080", "#7c6df2", "#3aa675", "#d6873f", "#d75b72", "#3f8fc7", "#b36ad4",
    "#8c9b45", "#c95353", "#c4a23d", "#2f9b95", "#596fd1",
];

pub fn empty_sidebar_project_collections_state() -> Value {
    json!({
        "collections": {},
        "nextCollectionNumber": 1,
        "order": [],
    })
}

pub fn read_sidebar_project_collections(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [SIDEBAR_PROJECT_COLLECTIONS_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite sidebar project collections error: {error}"),
        })?;
    let parsed = stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(empty_sidebar_project_collections_state);
    Ok(normalize_sidebar_project_collections_state(&parsed))
}

pub fn update_sidebar_project_collections(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let state = params
        .get("state")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Sidebar project collections update requires a state object.",
            )
        })?;
    let normalized = normalize_sidebar_project_collections_state(state);
    let serialized = serde_json::to_string(&normalized).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Sidebar project collections serialization error: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            SIDEBAR_PROJECT_COLLECTIONS_METADATA_KEY,
            serialized,
            now_iso()
        ],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite sidebar project collections error: {error}"),
    })?;
    Ok(normalized)
}

/// Move one project into a collection selected by its human-readable title.
///
/// This is the daemon-owned primitive behind `ghostex group-project`. Keeping
/// the read/modify/write here makes the move atomic with respect to the
/// presentation mutation lock held by the HTTP route, instead of requiring CLI
/// callers to replace the entire collections document themselves.
pub fn assign_project_to_sidebar_collection(
    db: &Connection,
    project_id: &str,
    collection_title: &str,
) -> Result<Value, DomainStateError> {
    let project_id =
        trimmed_bounded_text(Some(&Value::String(project_id.to_string())), MAX_ID_CHARS)
            .ok_or_else(|| DomainStateError::bad_request("Project id must be non-empty."))?;
    let collection_title = trimmed_bounded_text(
        Some(&Value::String(collection_title.to_string())),
        MAX_TITLE_CHARS,
    )
    .ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Sidebar group title must be between 1 and {MAX_TITLE_CHARS} characters."
        ))
    })?;
    let mut state = read_sidebar_project_collections(db)?;
    let mut collections = state
        .get("collections")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut order = state
        .get("order")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let target_collection_id = order
        .iter()
        .filter_map(Value::as_str)
        .find(|collection_id| {
            collections
                .get(*collection_id)
                .and_then(|collection| collection.get("title"))
                .and_then(Value::as_str)
                .is_some_and(|title| title.eq_ignore_ascii_case(&collection_title))
        })
        .map(str::to_string)
        .or_else(|| {
            collections.iter().find_map(|(collection_id, collection)| {
                collection
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| title.eq_ignore_ascii_case(&collection_title))
                    .map(|_| collection_id.clone())
            })
        });

    // A project belongs to at most one collection. Remove it everywhere first,
    // including from the target, so re-running the command remains idempotent.
    for collection in collections.values_mut() {
        if let Some(project_ids) = collection
            .get_mut("projectIds")
            .and_then(Value::as_array_mut)
        {
            project_ids.retain(|candidate| candidate.as_str() != Some(project_id.as_str()));
        }
    }

    let target_collection_id = match target_collection_id {
        Some(collection_id) => collection_id,
        None => {
            let next_collection_number = state
                .get("nextCollectionNumber")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1 && *value <= MAX_NEXT_COLLECTION_NUMBER)
                .unwrap_or((collections.len() as i64) + 1);
            let collection_id = format!(
                "project-collection-{next_collection_number}-{}",
                Uuid::new_v4().simple()
            );
            let color_index = usize::try_from(next_collection_number.saturating_sub(1))
                .unwrap_or_default()
                % SIDEBAR_PROJECT_COLLECTION_COLORS.len();
            collections.insert(
                collection_id.clone(),
                json!({
                    "collectionId": collection_id,
                    "color": SIDEBAR_PROJECT_COLLECTION_COLORS[color_index],
                    "projectIds": [],
                    "title": collection_title,
                }),
            );
            order.push(Value::String(collection_id.clone()));
            state["nextCollectionNumber"] = Value::Number(
                next_collection_number
                    .saturating_add(1)
                    .min(MAX_NEXT_COLLECTION_NUMBER)
                    .into(),
            );
            collection_id
        }
    };
    let target_project_ids = collections
        .get_mut(&target_collection_id)
        .and_then(|collection| collection.get_mut("projectIds"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| DomainStateError {
            code: "internalError",
            message: "Sidebar group state lost its project list.".to_string(),
        })?;
    target_project_ids.push(Value::String(project_id));
    state["collections"] = Value::Object(collections);
    state["order"] = Value::Array(order);
    update_sidebar_project_collections(db, &Map::from_iter([("state".to_string(), state)]))
}

pub fn normalize_sidebar_project_collections_state(state: &Value) -> Value {
    // Candidate collections keyed by trimmed collection id; first occurrence wins.
    let mut candidates: Vec<(String, &Value)> = Vec::new();
    let mut candidate_ids = std::collections::HashSet::new();
    if let Some(entries) = state.get("collections").and_then(Value::as_object) {
        for (collection_id, collection_state) in entries {
            let collection_id = collection_id.trim();
            if collection_id.is_empty() || collection_id.chars().count() > MAX_ID_CHARS {
                continue;
            }
            if !collection_state.is_object() {
                continue;
            }
            if candidate_ids.insert(collection_id.to_string()) {
                candidates.push((collection_id.to_string(), collection_state));
            }
        }
    }
    let candidate_state_by_id: std::collections::HashMap<&str, &Value> = candidates
        .iter()
        .map(|(id, state)| (id.as_str(), *state))
        .collect();
    // The explicit order array is authoritative; ids missing from it append in
    // stored map order so every kept collection always has a position.
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
    // A project belongs to at most one collection; earlier-ordered collections win.
    let mut seen_project_ids = std::collections::HashSet::new();
    let mut order: Vec<String> = Vec::new();
    let mut collections = Map::new();
    for collection_id in ordered_ids {
        if collections.len() >= MAX_COLLECTIONS {
            break;
        }
        let Some(collection_state) = candidate_state_by_id.get(collection_id.as_str()) else {
            continue;
        };
        let mut project_ids: Vec<String> = Vec::new();
        if let Some(entries) = collection_state.get("projectIds").and_then(Value::as_array) {
            for entry in entries {
                if project_ids.len() >= MAX_PROJECT_IDS_PER_COLLECTION {
                    break;
                }
                let Some(project_id) = trimmed_bounded_text(Some(entry), MAX_ID_CHARS) else {
                    continue;
                };
                if seen_project_ids.insert(project_id.clone()) {
                    project_ids.push(project_id);
                }
            }
        }
        if project_ids.is_empty() {
            continue;
        }
        let title = trimmed_bounded_text(collection_state.get("title"), MAX_TITLE_CHARS)
            .unwrap_or_else(|| collection_id.clone());
        let color = normalized_collection_color(collection_state.get("color"), collections.len());
        collections.insert(
            collection_id.clone(),
            json!({
                "collectionId": collection_id,
                "color": color,
                "projectIds": project_ids,
                "title": title,
            }),
        );
        order.push(collection_id);
    }
    let next_collection_number = state
        .get("nextCollectionNumber")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1 && *value <= MAX_NEXT_COLLECTION_NUMBER)
        .unwrap_or((collections.len() as i64) + 1);
    json!({
        "collections": collections,
        "nextCollectionNumber": next_collection_number,
        "order": order,
    })
}

fn normalized_collection_color(value: Option<&Value>, fallback_index: usize) -> String {
    if let Some(color) = value.and_then(Value::as_str) {
        // Migrate the removed Transparent option to the new Dark Gray default.
        if color == "transparent" {
            return SIDEBAR_PROJECT_COLLECTION_COLORS[0].to_string();
        }
        if is_valid_collection_color(color) {
            return color.to_string();
        }
    }
    SIDEBAR_PROJECT_COLLECTION_COLORS[fallback_index % SIDEBAR_PROJECT_COLLECTION_COLORS.len()]
        .to_string()
}

fn is_valid_collection_color(color: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_round_trips_valid_state() {
        let state = json!({
            "collections": {
                "c1": {
                    "collectionId": "c1",
                    "color": "#7c6df2",
                    "projectIds": ["P1", "P2"],
                    "title": "Group 1",
                },
                "c2": {
                    "collectionId": "c2",
                    "color": "#4f5663",
                    "projectIds": ["P3"],
                    "title": "Group 2",
                },
            },
            "nextCollectionNumber": 3,
            "order": ["c2", "c1"],
        });
        let normalized = normalize_sidebar_project_collections_state(&state);
        assert_eq!(normalized, state);
        // Idempotent: normalizing the normalized state changes nothing.
        assert_eq!(
            normalize_sidebar_project_collections_state(&normalized),
            normalized
        );
    }

    #[test]
    fn normalize_migrates_removed_transparent_color_to_dark_gray() {
        let normalized = normalize_sidebar_project_collections_state(&json!({
            "collections": {
                "c1": {
                    "color": "transparent",
                    "projectIds": ["P1"],
                    "title": "Legacy group",
                },
            },
            "nextCollectionNumber": 2,
            "order": ["c1"],
        }));
        assert_eq!(normalized["collections"]["c1"]["color"], json!("#4f5663"));
    }

    #[test]
    fn normalize_repairs_invalid_state() {
        let state = json!({
            "collections": {
                "c1": {
                    "color": "not-a-color",
                    "projectIds": ["P1", " P1 ", "", "P2"],
                    "title": "  Keep  ",
                },
                "c2": {
                    // Duplicate project id loses to earlier-ordered c1, leaving
                    // this collection empty, so it is dropped entirely.
                    "projectIds": ["P1"],
                    "title": "Empty after dedup",
                },
                "c3": {
                    "collapsed": "yes",
                    "projectIds": ["P3"],
                },
                "  ": { "projectIds": ["P9"], "title": "Blank id" },
            },
            "nextCollectionNumber": 0,
            "order": ["c1", "ghost", "c1"],
        });
        let normalized = normalize_sidebar_project_collections_state(&state);
        assert_eq!(
            normalized,
            json!({
                "collections": {
                    "c1": {
                        "collectionId": "c1",
                        "color": "#4f5663",
                        "projectIds": ["P1", "P2"],
                        "title": "Keep",
                    },
                    "c3": {
                        "collectionId": "c3",
                        "color": "#808080",
                        "projectIds": ["P3"],
                        "title": "c3",
                    },
                },
                "nextCollectionNumber": 3,
                "order": ["c1", "c3"],
            })
        );
    }

    #[test]
    fn normalize_rejects_non_object_state() {
        assert_eq!(
            normalize_sidebar_project_collections_state(&json!(null)),
            empty_sidebar_project_collections_state()
        );
        assert_eq!(
            normalize_sidebar_project_collections_state(&json!({ "collections": [] })),
            empty_sidebar_project_collections_state()
        );
    }

    #[test]
    fn read_update_round_trip_persists_normalized_state() {
        let db = rusqlite::Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .expect("schema");
        assert_eq!(
            read_sidebar_project_collections(&db).expect("read empty"),
            empty_sidebar_project_collections_state()
        );
        let mut params = Map::new();
        params.insert(
            "state".to_string(),
            json!({
                "collections": {
                    "c1": { "projectIds": ["P1"], "title": "Group 1", "color": "#3aa675" },
                },
                "nextCollectionNumber": 2,
                "order": ["c1"],
            }),
        );
        let updated = update_sidebar_project_collections(&db, &params).expect("update");
        assert_eq!(
            updated,
            json!({
                "collections": {
                    "c1": {
                        "collectionId": "c1",
                        "color": "#3aa675",
                        "projectIds": ["P1"],
                        "title": "Group 1",
                    },
                },
                "nextCollectionNumber": 2,
                "order": ["c1"],
            })
        );
        assert_eq!(
            read_sidebar_project_collections(&db).expect("read back"),
            updated
        );
    }

    #[test]
    fn assign_moves_project_by_case_insensitive_title_and_creates_missing_group() {
        let db = rusqlite::Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .expect("schema");
        let mut params = Map::new();
        params.insert(
            "state".to_string(),
            json!({
                "collections": {
                    "personal": {
                        "collapsed": true,
                        "color": "transparent",
                        "projectIds": ["P1", "P2"],
                        "title": "Personal",
                    },
                    "shortpoint": {
                        "collapsed": false,
                        "color": "#3f8fc7",
                        "projectIds": ["P3"],
                        "title": "ShortPoint",
                    },
                },
                "nextCollectionNumber": 3,
                "order": ["personal", "shortpoint"],
            }),
        );
        update_sidebar_project_collections(&db, &params).expect("seed");

        let moved = assign_project_to_sidebar_collection(&db, "P1", "shortpoint")
            .expect("move to existing group");
        assert_eq!(
            moved["collections"]["personal"]["projectIds"],
            json!(["P2"])
        );
        assert_eq!(
            moved["collections"]["shortpoint"]["projectIds"],
            json!(["P3", "P1"])
        );

        let created = assign_project_to_sidebar_collection(&db, "P2", "Clients")
            .expect("create missing group");
        assert!(created["collections"].get("personal").is_none());
        let clients_id = created["order"]
            .as_array()
            .expect("order")
            .iter()
            .filter_map(Value::as_str)
            .find(|collection_id| {
                created["collections"][collection_id]["title"] == json!("Clients")
            })
            .expect("clients group");
        assert_eq!(
            created["collections"][clients_id]["projectIds"],
            json!(["P2"])
        );
        assert_eq!(created["nextCollectionNumber"], json!(4));
    }

    #[test]
    fn update_requires_state_object() {
        let db = rusqlite::Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .expect("schema");
        let mut params = Map::new();
        params.insert("state".to_string(), json!("not-an-object"));
        let error = update_sidebar_project_collections(&db, &params).expect_err("rejects");
        assert_eq!(error.code, "badRequest");
    }
}
