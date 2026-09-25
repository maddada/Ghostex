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
//! the table's writers all owe, which `apply_record_metadata` rebuilds rather than adjusts.
//!
//! **This door refuses where `admission` evicts, on purpose.** For a `cache` store `admission`
//! makes room by deleting rows, its own store's oldest first and then other cache stores' when the
//! shared 128 MiB backend total is the bound that broke. A sidebar door that quietly deleted
//! another feature's cached data would be a loss nobody could trace back to it, so all four bounds
//! are checked and a value that breaks one is REFUSED and counted. What that costs is written down
//! where it is paid: the one store this door writes holds one row per remote machine against a
//! 32-entry, 24 MiB budget, so its own bound needs more than thirty machines to reach, and the
//! shared one is the client-storage service's to manage as it always has been.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_ui_storage.rs (the `preferences` door, whose
//! connection pool, busy timeout and error vocabulary this one borrows),
//! packages/client-storage-native/src/storage_records.rs (everything this file hands a connection to).

use rusqlite::Connection;

pub(crate) use ghostex_client_storage::{RecordRead, RecordStore, RecordWrite};

use super::sidebar_ui_storage::{with_read_connection, with_write_connection};

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

/// Every live record of one store whose key starts with `prefix`, as `(key, raw)` pairs.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// `Storage::call` has `recordScan` on the JavaScript side and this door had no equivalent, so the
/// Rust chat host could only read a record whose whole key it already knew. Two of the chat's own
/// records are keyed by a SCOPE it cannot know in advance (`<sessionKey>#<scope>` for the option
/// pills and the model-selection outbox, `storedSessionChatOptionKeys` in
/// `packages/core-ui/chat/session-chat-session-options.ts`), and the draft save outbox is keyed by
/// a revision, so a prefix scan is the only way to find them.
///
/// The scan is a RANGE over the primary key rather than a pattern, so it reads the rows it returns
/// and not the table: `key >= prefix AND key < <prefix with its last byte raised>`. A prefix whose
/// successor is not valid UTF-8 (nothing the catalog produces) falls back to the store's own rows.
/// An expired row is skipped, exactly as [`read_record_raw`] skips it.
pub(crate) fn scan_record_raw(
    store: RecordStore,
    prefix: &str,
    now_ms: i64,
) -> Result<Vec<(String, String)>, &'static str> {
    let upper = prefix_successor(prefix);
    with_read_connection(|connection| {
        let mut statement = match &upper {
            Some(_) => connection
                .prepare("SELECT key, value FROM records WHERE store=?1 AND key>=?2 AND key<?3"),
            None => connection.prepare("SELECT key, value FROM records WHERE store=?1"),
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

/// Stores one record in one immediate transaction.
pub(crate) fn write_record(
    store: RecordStore,
    key: &str,
    raw: &str,
    now_ms: i64,
) -> Result<RecordWrite, &'static str> {
    with_write_connection(|connection| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| "begin")?;
        let result = ghostex_client_storage::write_record(connection, store, key, raw, now_ms);
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

/// Removes one record and rebuilds the table's bookkeeping, in one immediate transaction.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// A real DELETE, because "write the empty string" is NOT a delete on this table and the other
/// reader of these rows proves it. `getItem` in `packages/client-storage/service.ts` answers
/// `entry.raw`, so an emptied row reads back as `""` rather than as absent, `managedKeys` still
/// lists it, and `SessionChatStorageIndex` (`packages/core-ui/chat/session-chat-storage-index.ts`)
/// decodes every key of its namespace with `JSON.parse`, which THROWS on `""` and takes the whole
/// index down with it: one emptied `ghostex.sessionChat.outbox.` row would make `pendingDrafts`
/// return nothing for every session, so the TypeScript brain would stop retrying every unsaved
/// draft on the computer. The store codecs say the same thing from the other side: `draftOutbox`
/// and `sentHistory` are object codecs and `writeManaged` refuses a value they cannot decode.
///
/// The metadata is recomputed rather than adjusted, for the reason `storage_metadata.rs` gives:
/// two writers keeping one counter incrementally drift for the life of the installation.
pub(crate) fn remove_record(key: &str) -> Result<(), &'static str> {
    with_write_connection(|connection| {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|_| "begin")?;
        let removed = match connection.execute("DELETE FROM records WHERE key=?1", [key]) {
            Ok(removed) => removed,
            Err(_) => {
                finish_without_writing(connection)?;
                return Err("write");
            }
        };
        if removed == 0 {
            return finish_without_writing(connection);
        }
        if ghostex_client_storage::recompute_record_metadata(connection).is_err() {
            finish_without_writing(connection)?;
            return Err("metadata");
        }
        connection.execute_batch("COMMIT").map_err(|_| "commit")
    })
}

/// Ends a transaction that wrote nothing.
///
/// CDXC:Settings 2026-09-21 WHY:
/// A failed ROLLBACK is reported rather than swallowed, and the reason is the pool rather than the
/// statement: `with_write_connection` keeps the connection unless the call returns an error, so a
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
