use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

type Key = (PathBuf, String, String);
static CACHE: OnceLock<Mutex<HashMap<Key, Value>>> = OnceLock::new();

/// CDXC:Docs 2026-09-11 WHY:
/// Git baselines are immutable for a repository, path and commit; reopening a document need not spawn cat-file again.
/// Only available baselines are retained, with a small bound because each one can contain up to 1 MB of text.
pub fn get(root: &Path, path: &str, commit: &str) -> Option<Value> {
    CACHE
        .get_or_init(Default::default)
        .lock()
        .ok()?
        .get(&(root.to_path_buf(), path.to_string(), commit.to_string()))
        .cloned()
}

pub fn insert(root: &Path, path: &str, commit: &str, value: &Value) {
    if value.get("available").and_then(Value::as_bool) != Some(true) {
        return;
    }
    let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() else {
        return;
    };
    if cache.len() >= 8 {
        cache.clear();
    }
    cache.insert(
        (root.to_path_buf(), path.to_string(), commit.to_string()),
        value.clone(),
    );
}
