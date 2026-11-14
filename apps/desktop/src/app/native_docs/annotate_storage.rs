//! What Files keeps for Annotate on HTML files: which files have it on, each file's notes (with
//! their pictures, under a size cap) and the settings every HTML file shares.
//!
//! SEE-ALSO: packages/client-storage/catalog.ts (`docsAnnotatedFiles`, `docsAnnotateNotes`,
//! `docsAnnotateSettings`), CDXC:Docs 2026-11-14 in apps/desktop/views/manage/preview/html-viewer.tsx.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::app::gx_store::{
    RecordRead, RecordStore, read_preference_value, read_record_raw, remove_record,
    write_client_document_value, write_record,
};

const MIB: i64 = 1024 * 1024;

/// The catalog's `disk` bounds.
const ANNOTATED_FILES: RecordStore = RecordStore {
    id: "docsAnnotatedFiles",
    version: 1,
    max_entry_bytes: 2 * MIB,
    max_bytes: 16 * MIB,
    max_entries: 2_000,
    max_age_ms: None,
};
/// `protectedDisk` with an 8 MiB entry: notes are user work and are never evicted.
const NOTES: RecordStore = RecordStore {
    id: "docsAnnotateNotes",
    version: 1,
    max_entry_bytes: 8 * MIB,
    max_bytes: 64 * MIB,
    max_entries: 2_000,
    max_age_ms: None,
};
/// A file's notes over this many bytes are kept without their pictures.
pub(crate) const NOTES_PICTURE_CAP_BYTES: usize = 6 * 1024 * 1024;

const ANNOTATED_FILES_PREFIX: &str = "ghostex.manage.annotatedFiles.";
const NOTES_PREFIX: &str = "ghostex.manage.annotateNotes.";
const SETTINGS_KEY: &str = "ghostex.manage.annotateSettings";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

fn read_raw(store: RecordStore, key: &str) -> Option<String> {
    match read_record_raw(store, key, now_ms()) {
        Ok(RecordRead::Payload(raw)) => Some(raw),
        _ => None,
    }
}

fn notes_key(project_id: &str, path: &str) -> String {
    format!("{NOTES_PREFIX}{project_id}\u{1f}{path}")
}

/// The project's HTML files with Annotate on.
pub(crate) fn read_annotated_files(project_id: &str) -> BTreeSet<String> {
    read_raw(
        ANNOTATED_FILES,
        &format!("{ANNOTATED_FILES_PREFIX}{project_id}"),
    )
    .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
    .unwrap_or_default()
    .into_iter()
    .collect()
}

pub(crate) fn write_annotated_files(project_id: &str, files: &BTreeSet<String>) {
    let key = format!("{ANNOTATED_FILES_PREFIX}{project_id}");
    let _ = if files.is_empty() {
        remove_record(&key).map(|_| ())
    } else {
        write_record(ANNOTATED_FILES, &key, &json!(files).to_string(), now_ms()).map(|_| ())
    };
}

/// A file's saved notes, as Agentation keeps them.
pub(crate) fn read_notes(project_id: &str, path: &str) -> Option<Value> {
    read_raw(NOTES, &notes_key(project_id, path))
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_array)
}

/// The notes as they are kept: without their pictures when they would pass the cap.
pub(crate) fn capped_notes(notes: Value) -> Value {
    let size = notes.to_string().len();
    if size <= NOTES_PICTURE_CAP_BYTES {
        return notes;
    }
    match notes {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|mut item| {
                    if let Some(object) = item.as_object_mut() {
                        object.remove("screenshot");
                    }
                    item
                })
                .collect(),
        ),
        other => other,
    }
}

pub(crate) fn write_notes(project_id: &str, path: &str, notes: &Value) {
    let key = notes_key(project_id, path);
    let empty = notes.as_array().is_none_or(Vec::is_empty);
    let _ = if empty {
        remove_record(&key).map(|_| ())
    } else {
        write_record(NOTES, &key, &notes.to_string(), now_ms()).map(|_| ())
    };
}

/// The settings every HTML file's Annotate shares (Agentation's own setting names).
pub(crate) fn read_settings() -> Value {
    read_preference_value(SETTINGS_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

pub(crate) fn write_settings(settings: &Value) {
    let _ = write_client_document_value(SETTINGS_KEY, Some(&settings.to_string()));
}
