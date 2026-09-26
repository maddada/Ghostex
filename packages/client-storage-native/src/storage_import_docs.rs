//! The Docs view's state moved once from the Docs page's browser storage into the native database,
//! when the native Docs view first opens.
//!
//! CDXC:Docs 2026-09-25 WHY:
//! The browser-era import (`storage_import.rs`) ran once on 2026-09-17, but the Docs page kept
//! writing its open files, unsaved drafts, selected file, files-list pin and formatting-bar fold
//! to browser storage after that. Native Docs reads those keys from the native database, so their
//! newer browser copies come across once more, with the same rule as the first import (the newer
//! record wins), before native Docs reads them. Drafts are protected user work.

use anyhow::{Context, Result, ensure};
use chromium_storage_indexeddb_core::{IdbKey, RecordValue};
use chromium_storage_localstorage_core::LocalStorageRecord;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::path::Path;

use super::storage_import::{latest_records, to_json, unwrap_values};

const MARKER: &str = "docsBrowserImportV1";
const RECORD_STORES: [&str; 3] = ["docsOpenFiles", "docsDrafts", "docsActiveFile"];
const PREFERENCE_KEYS: [&str; 2] = [
    "ghostex.manage.sidebarPinned",
    "ghostex.manage.formattingBarCollapsed",
];

/// Imports the Docs page's state from the browser `profile` once. Returns whether it ran.
pub fn import_docs_browser_state(connection: &Connection, profile: &Path) -> Result<bool> {
    let done: Option<String> = connection
        .query_row("SELECT value FROM metadata WHERE key=?1", [MARKER], |row| row.get(0))
        .optional()?;
    if done.is_some() {
        return Ok(false);
    }
    let mut preferences = Vec::new();
    let local = profile.join("Local Storage/leveldb");
    if local.exists() {
        for row in chromium_storage_localstorage_core::decode_records(&latest_records(&local)?) {
            if let LocalStorageRecord::Data {
                origin,
                script_key,
                value,
                deleted,
                ..
            } = row
            {
                if deleted || origin != "file://" || !PREFERENCE_KEYS.contains(&script_key.text.as_str()) {
                    continue;
                }
                ensure!(!script_key.lossy && !value.lossy, "A Docs preference could not be decoded exactly");
                preferences.push((script_key.text, value.text));
            }
        }
    }
    let mut records = Vec::new();
    let indexed = profile.join("IndexedDB/file__0.indexeddb.leveldb");
    if indexed.exists() {
        let mut raw = latest_records(&indexed)?;
        unwrap_values(&mut raw, &profile.join("IndexedDB/file__0.indexeddb.blob"))?;
        for row in chromium_storage_indexeddb_core::decode_records(&raw) {
            if row.deleted
                || row.database.as_deref() != Some("ghostex-client-storage")
                || row.object_store.as_deref() != Some("records")
            {
                continue;
            }
            let IdbKey::String(key) = &row.key else {
                continue;
            };
            let RecordValue::V8(value) = row.value else {
                continue;
            };
            let value = to_json(value).with_context(|| format!("Could not read Docs record {key}"))?;
            if value["store"].as_str().is_some_and(|store| RECORD_STORES.contains(&store))
                && value["key"].as_str() == Some(key.as_str())
                && value["raw"].is_string()
            {
                records.push(value);
            }
        }
    }
    let transaction = connection.unchecked_transaction()?;
    for (key, value) in preferences {
        transaction.execute(
            "INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    for row in records {
        let key = row["key"].as_str().unwrap_or_default();
        let previous: Option<String> = transaction
            .query_row("SELECT value FROM records WHERE key=?1", [key], |record| record.get(0))
            .optional()?;
        if let Some(previous) = previous {
            let previous: Value = serde_json::from_str(&previous)?;
            if previous["updatedAt"].as_f64().unwrap_or(0.0) > row["updatedAt"].as_f64().unwrap_or(0.0) {
                continue;
            }
        }
        transaction.execute(
            "INSERT INTO records VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET store=excluded.store,value=excluded.value",
            params![key, row["store"].as_str(), row.to_string()],
        )?;
    }
    super::storage_metadata::recompute_record_metadata(&transaction)?;
    transaction.execute(
        "INSERT INTO metadata VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![MARKER, json!(true).to_string()],
    )?;
    transaction.commit()?;
    Ok(true)
}
