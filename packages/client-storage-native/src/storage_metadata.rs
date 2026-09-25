use anyhow::Result;
use rusqlite::{Connection, params};
use serde_json::{Value, json};

/// One store's rows as the `records` table holds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordStoreUsage {
    pub store: String,
    pub bytes: i64,
    pub entries: i64,
    /// The highest `revision` any of that store's rows carries.
    pub highest_revision: i64,
}

/// CDXC:Settings 2026-09-21 WHY:
/// The `records` table carries `metadata` rows beside it, and every writer of the table owes them:
/// `<storeId>` is that store's `{bytes, entries}` usage, `total` is the bytes of every indexeddb
/// row, and `revision` is the counter the client-storage service compares an event against before
/// it believes it. `applyDatabaseMutations`
/// (`packages/client-storage/adapters/database-transaction.ts`) keeps all three INCREMENTALLY, which
/// is correct for one writer in one process and not for two: a decrement missed by either side
/// drifts for the life of the installation, and the TypeScript side then refuses or over-admits
/// every later write of that store with nothing to say why. So a Rust writer recomputes them from
/// the table instead, which is self-correcting whatever the other side did. This function is that
/// recompute, in ONE place: the browser import wrote it first and the last-seen remote presentation
/// writer needs the same thing, and two copies of a bookkeeping rule is how one of them drifts.
///
/// Call it INSIDE the transaction that wrote the rows.
///
/// The scan is an aggregation rather than a Rust loop over every parsed row because it also runs
/// on the app's own write path, inside `BEGIN IMMEDIATE`, where the QuickJS service's writes are
/// waiting on the lock: at this user's scale the table is about 5,500 rows and 5.8 MB of JSON, a
/// few milliseconds in SQLite against tens in `serde_json`. A row whose value is not JSON fails the
/// statement, which is what the parsing loop did too.
///
/// SEE-ALSO: packages/client-storage/adapters/database-transaction.ts (the other writer),
/// packages/client-storage-native/src/storage_records.rs (the write path, which scans once and hands the
/// result on rather than scanning a second time).
pub fn recompute_record_metadata(connection: &Connection) -> Result<()> {
    apply_record_metadata(connection, &scan_record_usage(connection)?)
}

/// Every store's bytes, entries and highest revision, in one pass over the table.
///
/// CDXC:Settings 2026-09-21 WHY:
/// Grouped by the `store` COLUMN, which is what `records.byStore` asks and therefore what the rows
/// `admission` is given are selected by; the row's own `store` field is written from the same value
/// and the column is `NOT NULL`, so no group can be nameless. The first cut grouped by
/// `json_extract(value,'$.store')` behind a `COALESCE(...,'')`, which would have written a
/// `metadata['']` row out of any row whose JSON lost the field, and it disagreed with the bounds
/// check in the same transaction, which had always asked the column.
pub fn scan_record_usage(connection: &Connection) -> Result<Vec<RecordStoreUsage>> {
    let mut query = connection.prepare(
        "SELECT store,
                COALESCE(SUM(CAST(COALESCE(json_extract(value,'$.bytes'),0) AS INTEGER)),0),
                COUNT(*),
                COALESCE(MAX(CAST(COALESCE(json_extract(value,'$.revision'),0) AS INTEGER)),0)
         FROM records GROUP BY store",
    )?;
    let usage = query
        .query_map([], |row| {
            Ok(RecordStoreUsage {
                store: row.get(0)?,
                bytes: row.get(1)?,
                entries: row.get(2)?,
                highest_revision: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(usage)
}

/// Writes the `metadata` rows a scan implies.
///
/// CDXC:Settings 2026-09-21 WHY:
/// Two things this does that a plain loop over the scan does not, and both of them are what
/// `applyDatabaseMutations` leaves behind. A store whose LAST row was deleted produces no group, so
/// a loop would leave its old usage standing for ever while the service decrements it to zero: a
/// metadata row in the usage shape with no rows behind it is therefore written back to zero. And a
/// row that already holds exactly the computed value is not written at all, because this runs
/// inside `BEGIN IMMEDIATE` with the service's writes waiting on the lock and twenty-odd upserts of
/// values nobody changed is twenty-odd pages of WAL for nothing. Neither changes the end state.
pub fn apply_record_metadata(connection: &Connection, usage: &[RecordStoreUsage]) -> Result<()> {
    let mut total: i64 = 0;
    let mut revision: i64 = 0;
    let mut wanted: Vec<(String, String)> = Vec::with_capacity(usage.len() + 2);
    for store in usage {
        total += store.bytes;
        revision = revision.max(store.highest_revision);
        wanted.push((
            store.store.clone(),
            json!({"bytes": store.bytes, "entries": store.entries}).to_string(),
        ));
    }
    for empty in emptied_stores(connection, usage)? {
        wanted.push((empty, json!({"bytes": 0, "entries": 0}).to_string()));
    }
    wanted.push(("revision".to_string(), json!(revision).to_string()));
    wanted.push(("total".to_string(), json!(total).to_string()));
    for (key, value) in wanted {
        let held: Option<String> = connection
            .query_row("SELECT value FROM metadata WHERE key=?1", [&key], |row| {
                row.get(0)
            })
            .ok();
        if held.as_deref() == Some(value.as_str()) {
            continue;
        }
        connection.execute(
            "INSERT INTO metadata VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}

/// The metadata keys that carry a store's `{bytes, entries}` usage and have no rows behind them any
/// more.
///
/// The usage SHAPE is what says a key is a store's, rather than a list of catalog ids this crate
/// does not have: the same table also holds `revision` and `total` as bare numbers and
/// `browserImportV1` as a bare `true`, and none of those may be turned into a usage row.
fn emptied_stores(connection: &Connection, usage: &[RecordStoreUsage]) -> Result<Vec<String>> {
    let mut query = connection.prepare("SELECT key, value FROM metadata")?;
    let rows = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(key, value)| {
            if usage.iter().any(|store| &store.store == key) {
                return false;
            }
            serde_json::from_str::<Value>(value).is_ok_and(|held| {
                held.get("bytes").and_then(Value::as_i64).is_some()
                    && held.get("entries").and_then(Value::as_i64).is_some()
            })
        })
        .map(|(key, _)| key)
        .collect())
}
