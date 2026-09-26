//! Where the chat's client-storage rows live on the desktop: the one client-storage database.
//!
//! A catalog row on `local` lives in the `preferences` table as `(key, value)` with the raw string;
//! a row on `indexeddb` lives in `records` as `(key, store, value)` where `value` is the whole row
//! JSON. Both go through the connection pool `src/app/gx_store/sidebar_ui_storage.rs` owns, for the
//! reason written down there: a second pool would hold the write lock against the client-storage
//! service. The GPUI web build has its own file of this name, over the page's
//! `packages/client-storage`.

use crate::app::gx_store::{
    RecordRead, RecordStore, read_record_raw, remove_record, scan_record_raw, write_record,
};

use super::storage::{Backend, RecordBounds};

fn record_store(bounds: RecordBounds) -> RecordStore {
    RecordStore {
        id: bounds.id,
        version: bounds.version,
        max_entry_bytes: bounds.max_entry_bytes,
        max_bytes: bounds.max_bytes,
        max_entries: bounds.max_entries,
        max_age_ms: bounds.max_age_ms,
    }
}

/// One row's raw value, `None` when it is absent or expired.
pub(super) fn read(
    _store_id: &str,
    backend: Backend,
    name: &str,
    now_ms: i64,
) -> Result<Option<String>, &'static str> {
    match backend {
        Backend::Local => crate::app::gx_store::with_read_connection(|connection| {
            connection
                .query_row(
                    "SELECT value FROM preferences WHERE key=?1",
                    [name],
                    |row| row.get::<_, String>(0),
                )
                .map(Some)
                .or_else(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    _ => Err("query"),
                })
        }),
        Backend::Records(bounds) => match read_record_raw(record_store(bounds), name, now_ms)? {
            RecordRead::Payload(raw) => Ok(Some(raw)),
            RecordRead::Missing | RecordRead::Expired => Ok(None),
        },
    }
}

/// Every live row of one record store whose full key starts with `prefix`, as `(key, raw)`.
pub(super) fn scan(
    bounds: RecordBounds,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    scan_record_raw(record_store(bounds), prefix, now_ms)
}

/// Writes one row, or removes it when `value` is `None`.
pub(super) fn write(
    _store_id: &str,
    backend: Backend,
    name: &str,
    value: Option<&str>,
    now_ms: i64,
) -> Result<(), &'static str> {
    match backend {
        Backend::Local => crate::app::gx_store::with_write_connection(|connection| match value {
            Some(raw) => connection
                .execute(
                    "INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    rusqlite::params![name, raw],
                )
                .map(|_| ())
                .map_err(|_| "write"),
            None => connection
                .execute("DELETE FROM preferences WHERE key=?1", [name])
                .map(|_| ())
                .map_err(|_| "write"),
        }),
        Backend::Records(bounds) => match value {
            Some(raw) => write_record(record_store(bounds), name, raw, now_ms).map(|_| ()),
            // `removeItem`, which is a real DELETE. An emptied row is not the same as an absent one
            // on this table: `remove_record` says why, and what it costs the TypeScript reader.
            None => remove_record(name),
        },
    }
}
