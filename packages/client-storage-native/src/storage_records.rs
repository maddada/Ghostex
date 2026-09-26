//! Writing one row of the `records` table: the catalog's bounds, the row shape and the metadata
//! the table's writers owe, with no connection, pool or transaction of its own.
//!
//! CDXC:Settings 2026-09-21 WHY:
//! It lives in this crate rather than beside its caller in `apps/desktop` so that it can be DRIVEN.
//! The desktop door is a process-global pool pointed at the user's own state directory, so the
//! first cut of this logic could only be gated by copying its body into a scratch crate outside the
//! repository, which is the shape this port has repeatedly found a bug hiding in: the copy and the
//! original drift and the gate goes on passing against the copy. Everything here takes a
//! `&Connection` and the caller owns the transaction, so `examples/records_door.rs` runs the real
//! code against a COPY of a real database.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/records_storage.rs (the pool, the transaction and the
//! error vocabulary), packages/client-storage/adapters/database-transaction.ts (the other writer of
//! this table), packages/client-storage-native/src/storage_metadata.rs (the bookkeeping).

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::storage_metadata::{RecordStoreUsage, apply_record_metadata, scan_record_usage};

/// `STORAGE_BUDGETS.indexeddb` (`packages/client-storage/budgets.ts`), shared by every store on
/// that backend.
pub const MAX_BACKEND_BYTES: i64 = 128 * 1024 * 1024;

/// One store's row in the catalog, as far as a read or a write needs it.
#[derive(Clone, Copy, Debug)]
pub struct RecordStore {
    /// The catalog `id`, which is also the `store` column and the key of the store's `metadata`
    /// row.
    pub id: &'static str,
    /// The catalog `version`, stored on the row as `schemaVersion`.
    pub version: i64,
    pub max_entry_bytes: i64,
    pub max_bytes: i64,
    pub max_entries: i64,
    /// The catalog `maxAgeMs`, for a `cache` store that has one. `None` is a store that keeps its
    /// rows for ever.
    pub max_age_ms: Option<i64>,
}

/// What one write did. `Refused` names the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordWrite {
    Stored,
    /// The row already held exactly this payload at this schema version, so nothing was written.
    /// `applyDatabaseMutations` skips the same case, which is what keeps two writers of one key
    /// from flipping its `updatedAt` between them.
    Unchanged,
    Refused(&'static str),
}

/// The size the catalog measures a row by: UTF-16 code units of the key and the payload together,
/// two bytes each, which is `storageBytes` in `packages/client-storage/budgets.ts`.
pub fn storage_bytes(key: &str, raw: &str) -> usize {
    2 * (utf16_len(key) + utf16_len(raw))
}

/// The length JavaScript measures, which is code units rather than characters or bytes.
fn utf16_len(value: &str) -> usize {
    value.chars().map(char::len_utf16).sum()
}

/// Stores one record, with the catalog's four bounds and the table's bookkeeping.
///
/// The caller owns the transaction and must already be inside one: the bounds are measured against
/// the table as it stands and the row and its metadata have to land together.
///
/// CDXC:Settings 2026-09-21 WHY:
/// The order of the three steps is the whole cost model. The UNCHANGED comparison is first and is
/// one indexed lookup, because the common case is two writers agreeing on the same payload and it
/// must not pay for a scan. The bounds and the bookkeeping then share ONE pass over the table:
/// the second pass the first cut made was measured at 9.8 ms inside `BEGIN IMMEDIATE`, where the
/// other writer's transactions are waiting, and it asked the same question twice. What replaces it
/// is arithmetic rather than a guess: inside the transaction nothing else can touch the table, so
/// the table after the write is the table the scan saw with exactly one row replaced, and that is
/// what [`usage_after_write`] computes.
pub fn write_record(
    connection: &Connection,
    store: RecordStore,
    key: &str,
    raw: &str,
    now_ms: i64,
) -> Result<RecordWrite, &'static str> {
    let held: Option<(String, String)> = connection
        .query_row(
            "SELECT store, value FROM records WHERE key=?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "read")?;
    let held: Option<(String, Value)> =
        held.and_then(|(store, text)| Some((store, serde_json::from_str::<Value>(&text).ok()?)));
    let unchanged = held.as_ref().is_some_and(|(_, row)| {
        row.get("raw").and_then(Value::as_str) == Some(raw)
            && row.get("schemaVersion").and_then(Value::as_i64) == Some(store.version)
    });
    if unchanged {
        return Ok(RecordWrite::Unchanged);
    }
    let scan = scan_record_usage(connection).map_err(|_| "usage")?;
    let previous_store = held.as_ref().map(|(store, _)| store.clone());
    let previous_bytes = held.as_ref().map_or(0, |(_, row)| row_number(row, "bytes"));
    let usage = Usage::from_scan(&scan, store.id, previous_store.as_deref(), previous_bytes);
    let next_bytes = storage_bytes(key, raw) as i64;
    if let Some(bound) = usage.refuses(store, next_bytes) {
        return Ok(RecordWrite::Refused(bound));
    }
    // The counter `applyDatabaseMutations` bumps per mutation. Taken above BOTH the metadata row
    // and every row's own revision, so the bookkeeping below, which reports the highest row
    // revision, cannot move the stored counter backwards past a service that is ahead of its rows.
    let revision = usage.revision_floor.max(metadata_revision(connection)?) + 1;
    let row = json!({
        "key": key,
        "store": store.id,
        "raw": raw,
        "bytes": next_bytes,
        "updatedAt": now_ms,
        "revision": revision,
        "schemaVersion": store.version,
    });
    connection
        .execute(
            "INSERT INTO records VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET store=excluded.store,value=excluded.value",
            params![key, store.id, row.to_string()],
        )
        .map_err(|_| "write")?;
    let after = usage_after_write(
        scan,
        store.id,
        previous_store.as_deref(),
        previous_bytes,
        next_bytes,
        revision,
    );
    apply_record_metadata(connection, &after).map_err(|_| "metadata")?;
    Ok(RecordWrite::Stored)
}

/// The table after one row was written, from the scan taken before it.
///
/// Equal to a second scan because the write is inside the transaction that took the first one: one
/// row moved from `previous_store` to `store_id` and changed size, and nothing else could have.
/// `highest_revision` is left alone on the store the row LEFT even though a real scan could report
/// a lower one there, because the only thing read from it is the highest revision of the whole
/// table, and the revision just written is above every revision the scan saw.
fn usage_after_write(
    mut scan: Vec<RecordStoreUsage>,
    store_id: &str,
    previous_store: Option<&str>,
    previous_bytes: i64,
    next_bytes: i64,
    revision: i64,
) -> Vec<RecordStoreUsage> {
    if let Some(previous_store) = previous_store {
        if let Some(entry) = scan.iter_mut().find(|entry| entry.store == previous_store) {
            entry.bytes -= previous_bytes;
            entry.entries -= 1;
        }
    }
    match scan.iter_mut().find(|entry| entry.store == store_id) {
        Some(entry) => {
            entry.bytes += next_bytes;
            entry.entries += 1;
            entry.highest_revision = revision;
        }
        None => scan.push(RecordStoreUsage {
            store: store_id.to_string(),
            bytes: next_bytes,
            entries: 1,
            highest_revision: revision,
        }),
    }
    // A store whose last row this write took away produces no group in a real scan, and
    // `apply_record_metadata` is what puts its usage row back to zero.
    scan.retain(|entry| entry.entries > 0);
    scan
}

/// A number off a stored row, read the way SQLite's `CAST(... AS INTEGER)` reads it, so the
/// arithmetic above and the scan's SQL agree on a row whose field is a string or a float.
fn row_number(row: &Value, field: &str) -> i64 {
    match row.get(field) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0) as i64,
        Some(Value::String(text)) => text.parse::<f64>().unwrap_or(0.0) as i64,
        _ => 0,
    }
}

/// The sizes the four bounds are measured against.
struct Usage {
    /// Bytes and entries of this store, NOT counting the key being written.
    store_bytes: i64,
    store_entries: i64,
    /// Bytes of every row on the backend, the key being written included.
    backend_bytes: i64,
    /// What the key being written holds today.
    previous_bytes: i64,
    /// The highest revision any row carries.
    revision_floor: i64,
}

impl Usage {
    fn from_scan(
        scan: &[RecordStoreUsage],
        store_id: &str,
        previous_store: Option<&str>,
        previous_bytes: i64,
    ) -> Self {
        let own = scan.iter().find(|entry| entry.store == store_id);
        let key_is_in_this_store = previous_store == Some(store_id);
        Self {
            store_bytes: own.map_or(0, |entry| entry.bytes)
                - if key_is_in_this_store {
                    previous_bytes
                } else {
                    0
                },
            store_entries: own.map_or(0, |entry| entry.entries) - i64::from(key_is_in_this_store),
            backend_bytes: scan.iter().map(|entry| entry.bytes).sum(),
            previous_bytes,
            revision_floor: scan
                .iter()
                .map(|entry| entry.highest_revision)
                .max()
                .unwrap_or(0),
        }
    }

    /// The bound this value would break, or `None` when all four admit it. `admission`'s four, in
    /// its order and in its UTF-16 accounting.
    fn refuses(&self, store: RecordStore, next_bytes: i64) -> Option<&'static str> {
        if next_bytes > store.max_entry_bytes {
            return Some("entry");
        }
        if self.store_bytes + next_bytes > store.max_bytes {
            return Some("store");
        }
        if self.store_entries + 1 > store.max_entries {
            return Some("entries");
        }
        (self.backend_bytes - self.previous_bytes + next_bytes > MAX_BACKEND_BYTES)
            .then_some("backend")
    }
}

fn metadata_revision(connection: &Connection) -> Result<i64, &'static str> {
    let stored: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='revision'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "metadata")?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_i64())
        .unwrap_or(0))
}

/// What one read found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordRead {
    Missing,
    /// The row is there and the catalog says it is too old to be used. Told apart from `Missing`
    /// because the two mean different things to a caller: nothing was ever stored, against a copy
    /// that aged out.
    Expired,
    Payload(String),
}

/// The `raw` payload of one record, if the catalog still admits it.
///
/// A row whose JSON does not parse, or that carries no `raw` string, reads as absent rather than
/// as an error: the callers of this table are caches, and one damaged entry must not stop the
/// others being read.
///
/// CDXC:Settings 2026-09-21 WHY:
/// **The catalog's `maxAgeMs` is part of the read, not only of eviction.** `readManaged`
/// (`packages/client-storage/service.ts`) answers `null` for a `cache` entry older than its store's
/// `maxAgeMs` before it ever looks at the payload, so a Rust door that handed back a two-month-old
/// row would have drawn data the TypeScript reader of the same key had already stopped drawing. The
/// one thing NOT copied is the deletion `readManaged` then performs: a Rust door refuses where the
/// service evicted, and a read never deletes. The clock is the caller's, because the
/// store this feeds has none of its own.
pub fn read_record(
    connection: &Connection,
    store: RecordStore,
    key: &str,
    now_ms: i64,
) -> Result<RecordRead, &'static str> {
    let row: Option<String> = connection
        .query_row("SELECT value FROM records WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|_| "read")?;
    let Some(row) = row.and_then(|text| serde_json::from_str::<Value>(&text).ok()) else {
        return Ok(RecordRead::Missing);
    };
    let Some(raw) = row.get("raw").and_then(Value::as_str) else {
        return Ok(RecordRead::Missing);
    };
    if store
        .max_age_ms
        .is_some_and(|limit| now_ms - row_number(&row, "updatedAt") > limit)
    {
        return Ok(RecordRead::Expired);
    }
    Ok(RecordRead::Payload(raw.to_string()))
}
