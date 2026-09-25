//! Starred models.
//!
//! Port of `packages/shared/session-chat-controller/model-favorites.ts` and
//! `toggleModelMenuFavorite` from `session-chat-presentation/model-menu.ts`.
//!
//! Starred models belong to the person, not to a session or a renderer, so every chat reads and
//! writes the one list and a star set in one shows in all. **The stored record is user
//! data**: `ghostex.model-favorites` holds a JSON array of `provider:value` strings and nothing
//! else, and an unreadable list is an empty one, never an error.

use serde_json::Value;

use crate::event::StorageKey;

/// The store this list lives in; the host owns the `ghostex.model-favorites` prefix.
pub const MODEL_FAVORITES_STORE: &str = "modelFavorites";

/// The record's key, a singleton with no per-session suffix.
pub fn model_favorites_key() -> StorageKey {
    StorageKey {
        store: MODEL_FAVORITES_STORE.to_string(),
        suffix: String::new(),
    }
}

/// `modelFavorites()`: the saved list, read leniently.
///
/// `JSON.parse(raw ?? '[]')` followed by a string filter, so a record that is not an array, or
/// that holds non-strings, degrades to the strings it does hold rather than throwing.
pub fn parse_model_favorites(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let Ok(stored) = serde_json::from_str::<Value>(raw) else {
        // An unreadable list is an empty one.
        return Vec::new();
    };
    match stored {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                Value::String(value) => Some(value),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The text written back, which must stay exactly `JSON.stringify(list)`.
pub fn serialize_model_favorites(favorites: &[String]) -> String {
    serde_json::to_string(favorites).unwrap_or_else(|_| "[]".to_string())
}

/// `toggleModelMenuFavorite`: starred rows are appended, unstarred ones removed in place.
pub fn toggle_model_favorite(favorites: &[String], key: &str) -> Vec<String> {
    if favorites.iter().any(|item| item == key) {
        favorites
            .iter()
            .filter(|item| *item != key)
            .cloned()
            .collect()
    } else {
        let mut next = favorites.to_vec();
        next.push(key.to_string());
        next
    }
}
