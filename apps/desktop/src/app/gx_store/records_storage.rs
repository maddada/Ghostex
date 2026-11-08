//! The second table of the client-storage database: `records`, where an indexeddb-catalogued store
//! keeps its rows. This file is the CONNECTION half; the row shape, the bounds and the bookkeeping
//! are `packages/client-storage-native/src/storage_records.rs`, so a harness can drive them.
//!
//! CDXC:Settings 2026-09-21 WHY:
//! Every Rust storage door before this one writes the `preferences` table, which is `(key, value)`
//! with `value` the raw string, because every key it owns is catalogued on the `local` backend. A
//! store catalogued on **indexeddb** is a different shape in the same file: `records` is
//! `(key, store, value)` where `value` is the WHOLE ROW as JSON,
//! `{ key, store, raw, bytes, updatedAt, revision, schemaVersion }` (`applyDatabaseMutations` in
//! `packages/client-storage/adapters/database-transaction.ts`, `recordWrite` in
//! `packages/client-storage-native/src/storage.rs`), with `bytes` the UTF-16 accounting of the key and the
//! raw together and `schemaVersion` the catalog row's `version`. Beside it are the `metadata` rows
//! the table's writers all owe, which the pool's `RecordUsage` keeps exact (`storage_usage.rs`).
//!
//! **This door never deletes another store's rows.** For a `cache` store `admission` makes room by
//! deleting rows, its own store's expired and oldest first, and then other cache stores' when the
//! shared 128 MiB backend total is the bound that broke. The first half is the catalog's own rule
//! for a disposable store and this door applies it (`plan_eviction` in `storage_records.rs`, which
//! says why it stopped refusing there on 2026-10-23); the second half would let one feature's write
//! quietly delete another feature's cached data, a loss nobody could trace back to it, so a value
//! that only fits that way is REFUSED and counted instead.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_ui_storage.rs (the `preferences` door, whose
//! connection pool, busy timeout and error vocabulary this one borrows),
//! packages/client-storage-native/src/storage_records.rs (everything this file hands a connection to).

use rusqlite::Connection;

pub(crate) use ghostex_client_storage::{RecordRead, RecordStore, RecordWrite};

use super::sidebar_ui_storage::{with_read_connection, with_records_write_connection};

/// The `raw` payload of one record, if the catalog still admits it at `now_ms`.
pub(crate) fn read_record_raw(
    store: RecordStore,
    key: &str,
    now_ms: i64,
) -> Result<RecordRead, &'static str> {
    with_read_connection(|connection| {
        ghostex_client_storage::read_record(connection, store, key, now_ms)
    })
}

/// When one record was last written (`updatedAt`, epoch ms), whether or not the catalog still
/// admits its payload: an aged-out copy still says when it was written.
pub(crate) fn read_record_updated_at(store: RecordStore, key: &str) -> Option<i64> {
    with_read_connection(|connection| {
        let value: Option<String> = connection
            .query_row(
                "SELECT value FROM records WHERE store=?1 AND key=?2",
                rusqlite::params![store.id, key],
                |row| row.get(0),
            )
            .ok();
        Ok(value
            .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
            .and_then(|row| row.get("updatedAt").and_then(serde_json::Value::as_i64)))
    })
    .ok()
    .flatten()
}

/// Every live record of one store whose key starts with `prefix`, as `(key, raw)` pairs.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `Storage::call` had `recordScan` on the JavaScript side and this door had no equivalent, so the
/// Rust chat host could only read a record whose whole key it already knew. Two of the chat's own
/// records are keyed by a SCOPE it cannot know in advance (`<sessionKey>#<scope>` for the option
/// pills and the model-selection outbox, `packages/gx-chat-core/src/menus/option_storage.rs`), and
/// the draft save outbox is keyed by a revision, so a prefix scan is the only way to find them.
///
/// The scan is a RANGE over the primary key rather than a pattern, so it reads the rows it returns
/// and not the table: `key >= prefix AND key < <prefix with its last byte raised>`. The `store`
/// test is written `+store` so SQLite cannot answer it from `records_by_store`: given the choice it
/// took that index and walked every row of the store (1,569 `recovery` rows for one session's
/// prefix, about 0.9 ms against 0.05 ms for the key range). A prefix whose successor is not valid
/// UTF-8 (nothing the catalog produces) falls back to the store's own rows. An expired row is
/// skipped, exactly as [`read_record_raw`] skips it.
pub(crate) fn scan_record_raw(
    store: RecordStore,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    let upper = prefix_successor(prefix);
    with_read_connection(|connection| {
        let mut statement = match &upper {
            Some(_) => connection.prepare_cached(
                "SELECT key, value FROM records WHERE +store=?1 AND key>=?2 AND key<?3",
            ),
            None => connection.prepare_cached("SELECT key, value FROM records WHERE store=?1"),
        }
        .map_err(|_| "read")?;
        let bind: Vec<&dyn rusqlite::ToSql> = match &upper {
            Some(upper) => vec![&store.id, &prefix, upper],
            None => vec![&store.id],
        };
        let rows = statement
            .query_map(rusqlite::params_from_iter(bind), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "read")?;
        let mut found = Vec::new();
        for row in rows {
            let (key, value) = row.map_err(|_| "read")?;
            if !key.starts_with(prefix) {
                continue;
            }
            let Ok(row) = serde_json::from_str::<serde_json::Value>(&value) else {
                continue;
            };
            let Some(raw) = row.get("raw").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let updated_at = row
                .get("updatedAt")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default();
            if store
                .max_age_ms
                .is_some_and(|limit| now_ms - updated_at > limit)
            {
                continue;
            }
            found.push((key, raw.to_string()));
        }
        Ok(found)
    })
}

/// The first key that sorts after every key starting with `prefix`, under SQLite's BINARY
/// collation, or `None` when there is none this scan can express.
fn prefix_successor(prefix: &str) -> Option<String> {
    let mut bytes = prefix.as_bytes().to_vec();
    while let Some(last) = bytes.pop() {
        if last < 0xff {
            bytes.push(last + 1);
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

/// Stores one record in one immediate transaction, measured against the pool's record tallies.
///
/// A failed COMMIT returns an error, and an error is what makes the pool drop the connection and
/// the tallies with it: they were moved before the COMMIT and no longer describe the file.
pub(crate) fn write_record(
    store: RecordStore,
    key: &str,
    raw: &str,
    now_ms: i64,
) -> Result<RecordWrite, &'static str> {
    with_records_write_connection(|connection, usage| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| "begin")?;
        let result = usage.write(connection, store, key, raw, now_ms);
        match &result {
            Ok(RecordWrite::Stored) => {
                connection.execute_batch("COMMIT").map_err(|_| "commit")?;
            }
            // Nothing was written, so there is nothing to commit and a rollback is the shortest way
            // out of the immediate transaction the other writer is waiting on.
            _ => finish_without_writing(connection)?,
        }
        result
    })
}

/// Removes one record and moves the table's bookkeeping with it, in one immediate transaction.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// A real DELETE, because "write the empty string" is NOT a delete on this table and the other
/// reader of these rows proves it. `getItem` in `packages/client-storage/service.ts` answers
/// `entry.raw`, so an emptied row reads back as `""` rather than as absent, `managedKeys` still
/// lists it, and `SessionChatStorageIndex` (`packages/core-ui/chat/session-chat-storage-index.ts` (deleted 2026-10-01))
/// decodes every key of its namespace with `JSON.parse`, which THROWS on `""` and takes the whole
/// index down with it: one emptied `ghostex.sessionChat.outbox.` row would make `pendingDrafts`
/// return nothing for every session, so the TypeScript outbox would stop retrying every unsaved
/// draft on the computer. The store codecs say the same thing from the other side: `draftOutbox`
/// and `sentHistory` are object codecs and `writeManaged` refuses a value they cannot decode.
///
/// The metadata moves with the pool's record tallies, which rescan whenever anything else changed
/// the table (`storage_usage.rs`), so it stays exact without recounting the table per removal.
pub(crate) fn remove_record(key: &str) -> Result<(), &'static str> {
    with_records_write_connection(|connection, usage| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| "begin")?;
        match usage.remove(connection, key) {
            Ok(true) => connection.execute_batch("COMMIT").map_err(|_| "commit"),
            Ok(false) => finish_without_writing(connection),
            Err(error) => {
                finish_without_writing(connection)?;
                Err(error)
            }
        }
    })
}

/// Ends a transaction that wrote nothing.
///
/// CDXC:Settings 2026-09-21 WHY:
/// A failed ROLLBACK is reported rather than swallowed, and the reason is the pool rather than the
/// statement: `with_records_write_connection` keeps the connection unless the call returns an error, so a
/// swallowed failure would hand the next writer a connection that may still be inside `BEGIN
/// IMMEDIATE`, holding the database's write lock against the client-storage service until the app
/// quits. Turning it into an error drops the connection and the next call opens a fresh one, which
/// costs one retried write in a case that needs the statement itself to fail.
fn finish_without_writing(connection: &Connection) -> Result<(), &'static str> {
    connection
        .execute_batch("ROLLBACK")
        .map(|_| ())
        .map_err(|_| "rollback")
}
