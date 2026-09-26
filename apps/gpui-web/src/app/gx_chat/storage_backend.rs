//! The desktop's `gx_chat/storage_backend.rs` for a browser page: the page's own
//! `packages/client-storage`, which `www/src/main.js` hydrates before the app starts and exposes as
//! `globalThis.ghostexChatStorage`.
//!
//! The catalog there is the one `storage.rs` mirrors, so the same keys land in the same stores
//! (localStorage for a `local` row, IndexedDB for an `indexeddb` one) and the page's client storage
//! applies each store's bounds, retention and durability itself. A draft the page's TypeScript chat
//! wrote before 2026-09-25 is read back unchanged.

use wasm_bindgen::prelude::*;

use super::storage::{Backend, RecordBounds};

#[wasm_bindgen(inline_js = r#"
export function chat_storage_get(store, key) {
  const value = globalThis.ghostexChatStorage.get(store, key);
  return value === undefined ? null : value;
}
export function chat_storage_set(store, key, raw) {
  globalThis.ghostexChatStorage.set(store, key, raw);
}
export function chat_storage_remove(store, key) {
  globalThis.ghostexChatStorage.remove(store, key);
}
export function chat_storage_keys(store, prefix) {
  return globalThis.ghostexChatStorage.keys(store, prefix);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    fn chat_storage_get(store: &str, key: &str) -> Result<Option<String>, JsValue>;
    #[wasm_bindgen(catch)]
    fn chat_storage_set(store: &str, key: &str, raw: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn chat_storage_remove(store: &str, key: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(catch)]
    fn chat_storage_keys(store: &str, prefix: &str) -> Result<Vec<String>, JsValue>;
}

/// One row's raw value, `None` when it is absent or expired.
pub(super) fn read(
    store_id: &str,
    _backend: Backend,
    name: &str,
    _now_ms: i64,
) -> Result<Option<String>, &'static str> {
    chat_storage_get(store_id, name).map_err(|_| "query")
}

/// Every live row of one record store whose full key starts with `prefix`, as `(key, raw)`.
pub(super) fn scan(
    bounds: RecordBounds,
    prefix: &str,
    _now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    let keys = chat_storage_keys(bounds.id, prefix).map_err(|_| "read")?;
    Ok(keys
        .into_iter()
        .filter_map(|key| {
            let raw = chat_storage_get(bounds.id, &key).ok().flatten()?;
            Some((key, raw))
        })
        .collect())
}

/// Writes one row, or removes it when `value` is `None`. A write the store's bounds refuse throws
/// in the page and is answered as a refusal here, never truncated.
pub(super) fn write(
    store_id: &str,
    _backend: Backend,
    name: &str,
    value: Option<&str>,
    _now_ms: i64,
) -> Result<(), &'static str> {
    match value {
        Some(raw) => chat_storage_set(store_id, name, raw).map_err(|_| "write"),
        None => chat_storage_remove(store_id, name).map_err(|_| "write"),
    }
}
