use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/// CDXC:Sessions 2026-09-11 WHY:
/// A custom session tag is one more value of the single `sessionTag` marker: the id is `custom-` plus a random token and the catalog (name, icon id, color, order) is a gxserver-owned per-daemon document, modeled on Spaces, so desktop, web, phone, and CLI on the same daemon resolve the same id to the same look and a remote daemon's tags stay that daemon's own.
/// The daemon bounds the icon id but never validates it against the client icon allowlist, for the same reason Spaces do not: that would pin the server to one client build's icon pack.
/// SEE-ALSO: packages/shared/session-tags.ts (`normalizeCustomSessionTagsState` mirrors `normalize_custom_session_tags_state` rule for rule), apps/mobile/app/src/contract/sessionTags.ts, server/src/domain/normalize/session.rs (`normalize_optional_session_tag` accepts the id shape).
const CUSTOM_SESSION_TAGS_METADATA_KEY: &str = "customSessionTags";
pub const CUSTOM_SESSION_TAG_ID_PREFIX: &str = "custom-";
const CUSTOM_SESSION_TAG_ID_MIN_TOKEN_CHARS: usize = 4;
const CUSTOM_SESSION_TAG_ID_MAX_TOKEN_CHARS: usize = 40;
const MAX_CUSTOM_SESSION_TAGS: usize = 64;
const MAX_NAME_CHARS: usize = 40;
const MAX_ICON_CHARS: usize = 64;
const DEFAULT_CUSTOM_SESSION_TAG_NAME: &str = "Tag";
const DEFAULT_CUSTOM_SESSION_TAG_ICON: &str = "sparkles";

/// Mirrors SESSION_TAG_COLOR_PRESETS in packages/shared/session-tags.ts so a
/// tag saved without a usable color rotates through the same presets the
/// picker offers.
const SESSION_TAG_COLOR_PRESETS: [&str; 13] = [
    "#f3cc5f", "#ff8b6b", "#f0c66e", "#4ee6b8", "#59d9ff", "#95d7f6", "#8fb8ff", "#d2a7ff",
    "#ff9ee7", "#ff5f73", "#a54646", "#d9dee6", "#8e949d",
];

pub fn empty_custom_session_tags_state() -> Value {
    json!({
        "order": [],
        "tags": {},
    })
}

/// `custom-` followed by 4..=40 ASCII lowercase letters or digits.
pub fn is_custom_session_tag_id(value: &str) -> bool {
    let Some(token) = value.strip_prefix(CUSTOM_SESSION_TAG_ID_PREFIX) else {
        return false;
    };
    (CUSTOM_SESSION_TAG_ID_MIN_TOKEN_CHARS..=CUSTOM_SESSION_TAG_ID_MAX_TOKEN_CHARS)
        .contains(&token.len())
        && token
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

pub fn read_custom_session_tags(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = read_stored_custom_session_tags_state(db)?;
    Ok(normalize_custom_session_tags_state(&stored))
}

pub fn update_custom_session_tags(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let state = params
        .get("state")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            DomainStateError::bad_request("Custom session tags update requires a state object.")
        })?;
    let normalized = normalize_custom_session_tags_state(state);
    write_custom_session_tags_state(db, &normalized)?;
    Ok(normalized)
}

/// Clear `sessionTag` on every session whose custom tag is no longer in the
/// catalog, returning the affected `(projectId, sessionId)` pairs so the
/// caller can publish a presentation delta for each one.
///
/// A custom tag never doubles as the legacy Favorite marker, so `isFavorite`
/// is reset alongside the tag exactly as a tag clear through
/// `/api/updateSession` would leave it.
pub fn clear_session_tags_missing_from_catalog(
    db: &Connection,
    state: &Value,
) -> Result<Vec<(String, String)>, DomainStateError> {
    let known_ids: std::collections::HashSet<&str> = state
        .get("tags")
        .and_then(Value::as_object)
        .map(|tags| tags.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut statement = db
        .prepare(
            "SELECT projectId, sessionId, sessionTag FROM sessions WHERE sessionTag LIKE 'custom-%'",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    let mut cleared = Vec::new();
    for (project_id, session_id, session_tag) in rows {
        if known_ids.contains(session_tag.as_str()) {
            continue;
        }
        db.execute(
            "UPDATE sessions SET sessionTag = NULL, isFavorite = 0, updatedAt = ?3 WHERE projectId = ?1 AND sessionId = ?2",
            rusqlite::params![project_id, session_id, now_iso()],
        )
        .map_err(sql_error)?;
        cleared.push((project_id, session_id));
    }
    Ok(cleared)
}

pub fn normalize_custom_session_tags_state(state: &Value) -> Value {
    // Candidate tags keyed by trimmed id; first occurrence wins.
    let mut candidates: Vec<(String, &Value)> = Vec::new();
    let mut candidate_ids = std::collections::HashSet::new();
    if let Some(entries) = state.get("tags").and_then(Value::as_object) {
        for (tag_id, tag_state) in entries {
            let tag_id = tag_id.trim();
            if !is_custom_session_tag_id(tag_id) || !tag_state.is_object() {
                continue;
            }
            if candidate_ids.insert(tag_id.to_string()) {
                candidates.push((tag_id.to_string(), tag_state));
            }
        }
    }
    let candidate_state_by_id: std::collections::HashMap<&str, &Value> = candidates
        .iter()
        .map(|(id, state)| (id.as_str(), *state))
        .collect();
    // The explicit order array is authoritative; ids missing from it append in
    // stored map order so every kept tag always has a position.
    let mut ordered_ids: Vec<String> = Vec::new();
    let mut seen_order_ids = std::collections::HashSet::new();
    if let Some(entries) = state.get("order").and_then(Value::as_array) {
        for entry in entries {
            let Some(id) = entry.as_str().map(str::trim) else {
                continue;
            };
            if candidate_state_by_id.contains_key(id) && seen_order_ids.insert(id.to_string()) {
                ordered_ids.push(id.to_string());
            }
        }
    }
    for (id, _) in &candidates {
        if seen_order_ids.insert(id.clone()) {
            ordered_ids.push(id.clone());
        }
    }

    let mut order: Vec<String> = Vec::new();
    let mut tags = Map::new();
    for tag_id in ordered_ids {
        if tags.len() >= MAX_CUSTOM_SESSION_TAGS {
            break;
        }
        let Some(tag_state) = candidate_state_by_id.get(tag_id.as_str()) else {
            continue;
        };
        let name = normalized_custom_session_tag_name(tag_state.get("name"));
        let icon = trimmed_bounded_text(tag_state.get("icon"), MAX_ICON_CHARS)
            .unwrap_or_else(|| DEFAULT_CUSTOM_SESSION_TAG_ICON.to_string());
        let color = normalized_session_tag_color(tag_state.get("color"), tags.len());
        tags.insert(
            tag_id.clone(),
            json!({
                "color": color,
                "icon": icon,
                "name": name,
                "tagId": tag_id,
            }),
        );
        order.push(tag_id);
    }
    json!({
        "order": order,
        "tags": tags,
    })
}

/// Trimmed, internal whitespace collapsed to one space, capped at 40 chars,
/// `"Tag"` when nothing is left; the same rule as
/// `normalizeCustomSessionTagName` in packages/shared/session-tags.ts.
fn normalized_custom_session_tag_name(value: Option<&Value>) -> String {
    let collapsed = value
        .and_then(Value::as_str)
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let name = if collapsed.is_empty() {
        DEFAULT_CUSTOM_SESSION_TAG_NAME.to_string()
    } else {
        collapsed
    };
    name.chars().take(MAX_NAME_CHARS).collect()
}

fn normalized_session_tag_color(value: Option<&Value>, fallback_index: usize) -> String {
    if let Some(color) = value.and_then(Value::as_str) {
        let color = color.trim();
        if is_valid_tag_color(color) {
            return color.to_ascii_lowercase();
        }
    }
    SESSION_TAG_COLOR_PRESETS[fallback_index % SESSION_TAG_COLOR_PRESETS.len()].to_string()
}

fn is_valid_tag_color(color: &str) -> bool {
    let mut chars = color.chars();
    chars.next() == Some('#')
        && color.len() == 7
        && chars.all(|character| character.is_ascii_hexdigit())
}

/// Trimmed and capped at `max_chars` characters (the TS side slices rather
/// than rejects, so an over-long icon id is truncated, not dropped).
fn trimmed_bounded_text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(max_chars).collect())
}

fn read_stored_custom_session_tags_state(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [CUSTOM_SESSION_TAGS_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(empty_custom_session_tags_state))
}

fn write_custom_session_tags_state(db: &Connection, state: &Value) -> Result<(), DomainStateError> {
    let serialized = serde_json::to_string(state).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Custom session tags serialization error: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![CUSTOM_SESSION_TAGS_METADATA_KEY, serialized, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite custom session tags error: {error}"),
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
