//! Opening the client-storage database at app start: the tables, the browser-era import and the
//! migrations `initializeClientStorage` (`packages/client-storage/service.ts`) used to run inside
//! the QuickJS runtime before anything else could read the file.
//!
//! CDXC:Settings 2026-09-25 WHY:
//! Only `ServiceRuntime::new` created the tables, imported the browser-era profile and ran
//! `migrateStorage` and `upgradeStorageSchemas`. With the runtime being ported to Rust, a fresh
//! install would have had no tables and an upgraded one would have left its browser-era values
//! where no Rust door reads them. The desktop now calls [`initialize_client_storage`] once at
//! process start, before any storage door or the runtime opens the file. The TypeScript copy still
//! runs inside the runtime until it is deleted; every step here is idempotent against it (receipts,
//! `onlyIfAbsent`, compare-then-delete), so the second run finds nothing to do.
//!
//! SEE-ALSO: packages/client-storage/migration.ts (`migrateStorage`, `upgradeStorageSchemas`),
//! packages/client-storage/adapters/database-transaction.ts (the row shape a migration writes),
//! packages/client-storage-native/src/storage_import.rs (the browser-era import),
//! apps/desktop/src/app/gx_store/client_storage_init.rs (the caller).

use std::path::Path;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};

use crate::storage::Storage;
use crate::storage_catalog::{CatalogBackend, CatalogStore, catalog_store, definition_for_key};
use crate::storage_metadata::recompute_record_metadata;
use crate::storage_records::storage_bytes;

/// The `migrationReceipts` prefix (`storageCatalog.migrationReceipts.key`).
const RECEIPT_PREFIX: &str = "ghostex.storage.imported.";

/// What start-up did, in counts only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageInitReport {
    /// The browser-era profile was imported on this start.
    pub browser_import_ran: bool,
    /// `local` preferences whose catalog store is on the records backend, moved into `records`.
    pub preferences_migrated: usize,
    /// Rows of the two legacy IndexedDB tables written into `records`.
    pub legacy_rows_migrated: usize,
    /// Legacy rows whose destination already held a different value (`legacyStorageIssues`).
    pub legacy_rows_unmigrated: usize,
}

/// Creates the tables, imports the browser-era profile once, and runs the catalog migrations.
///
/// `legacy_profile` is the CEF profile directory (`<cef cache>/Default`) the browser-era app kept
/// its localStorage and IndexedDB in. An error leaves the file as it was: every step commits in its
/// own transaction and a failed step rolls back, as `initializeClientStorage` refused to start.
pub fn initialize_client_storage(
    database: &Path,
    legacy_profile: Option<&Path>,
    now_ms: i64,
) -> Result<StorageInitReport> {
    let mut storage = Storage::open(database)?;
    let mut report = StorageInitReport::default();
    if let Some(profile) = legacy_profile {
        report.browser_import_ran = crate::storage_import::import(&mut storage, profile)?;
    }
    let connection = &mut storage.database;
    report.preferences_migrated = migrate_preferences(connection, now_ms)?;
    let (migrated, unmigrated) = migrate_legacy_tables(connection, now_ms)?;
    report.legacy_rows_migrated = migrated;
    report.legacy_rows_unmigrated = unmigrated;
    check_schema_versions(connection)?;
    Ok(report)
}

/// `migrateStorage`, first half: a value the browser era kept in localStorage whose catalog store
/// is now on the records backend is copied into `records` (only where absent), one transaction and
/// one receipt per store, and the preference is deleted only once the copy reads back equal.
fn migrate_preferences(connection: &mut Connection, now_ms: i64) -> Result<usize> {
    let mut candidates: Vec<(&'static CatalogStore, String, String)> = Vec::new();
    {
        let mut query = connection.prepare("SELECT key, value FROM preferences")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, raw) = row?;
            if let Some(definition) = definition_for_key(&key) {
                if definition.backend == CatalogBackend::Records {
                    candidates.push((definition, key, raw));
                }
            }
        }
    }
    if candidates.is_empty() {
        return Ok(0);
    }
    let mut stores: Vec<&'static str> = Vec::new();
    for (definition, _, _) in &candidates {
        if !stores.contains(&definition.id) {
            stores.push(definition.id);
        }
    }
    for store in stores {
        let entries: Vec<(String, String)> = candidates
            .iter()
            .filter(|(definition, _, _)| definition.id == store)
            .map(|(_, key, raw)| (key.clone(), raw.clone()))
            .collect();
        commit_only_if_absent(
            connection,
            store,
            &entries,
            &format!("local:{store}"),
            now_ms,
        )?;
    }
    let transaction = connection.transaction()?;
    let mut moved = 0;
    for (_, key, raw) in &candidates {
        if record_raw(&transaction, key)?.as_deref() != Some(raw.as_str()) {
            continue;
        }
        transaction.execute(
            "DELETE FROM preferences WHERE key=?1 AND value=?2",
            params![key, raw],
        )?;
        moved += 1;
    }
    transaction.commit()?;
    Ok(moved)
}

/// One legacy IndexedDB table and where its rows go.
struct LegacyTable {
    database: &'static str,
    table: &'static str,
    store: &'static str,
}

const LEGACY_TABLES: [LegacyTable; 2] = [
    LegacyTable {
        database: "ghostex-draft-outbox",
        table: "drafts",
        store: "draftOutbox",
    },
    LegacyTable {
        database: "ghostex-session-chat-v1",
        table: "snapshots",
        store: "chatSnapshots",
    },
];

/// `migrateStorage`, second half: the draft outbox and the chat snapshots of the browser era's own
/// IndexedDB databases, which the import copied into the `legacy` table. The native `retire` was a
/// no-op, so the legacy rows stay where they are and the receipt is what stops a replay.
fn migrate_legacy_tables(connection: &mut Connection, now_ms: i64) -> Result<(usize, usize)> {
    let mut migrated = 0;
    let mut unmigrated = 0;
    for legacy in LEGACY_TABLES {
        let values: Vec<Value> = {
            let mut query = connection
                .prepare("SELECT value FROM legacy WHERE database_name=?1 AND store=?2")?;
            let rows = query.query_map(params![legacy.database, legacy.table], |row| {
                row.get::<_, String>(0)
            })?;
            rows.map(|text| Ok(serde_json::from_str::<Value>(&text?)?))
                .collect::<Result<_>>()?
        };
        if values.is_empty() {
            continue;
        }
        let entries: Vec<(String, String)> = values
            .iter()
            .map(|value| legacy_entry(legacy.store, value))
            .collect();
        commit_only_if_absent(
            connection,
            legacy.store,
            &entries,
            &format!("database:{}", legacy.database),
            now_ms,
        )?;
        for (key, raw) in &entries {
            if record_raw(connection, key)?.as_deref() == Some(raw.as_str()) {
                migrated += 1;
            } else {
                unmigrated += 1;
            }
        }
    }
    Ok((migrated, unmigrated))
}

/// The key and raw payload `migrateStorage` builds for one legacy row: the outbox drops its `id`
/// (which becomes the key), a snapshot keeps its whole value and is keyed by its `key`.
fn legacy_entry(store: &str, value: &Value) -> (String, String) {
    if store == "draftOutbox" {
        let id = js_template_value(&value["id"]);
        let without_id: Map<String, Value> = value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "id")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();
        (
            format!("ghostex.sessionChat.outbox.{id}"),
            js_stringify(&Value::Object(without_id)),
        )
    } else {
        (
            format!(
                "ghostex.sessionChat.snapshot.{}",
                js_template_value(&value["key"])
            ),
            js_stringify(value),
        )
    }
}

/// `commitDatabase(entries, true, importSource)` with every entry `onlyIfAbsent`: skipped whole
/// when the receipt exists, otherwise each absent key is written with no budget check (a migration
/// never refuses) and the receipt lands in the same transaction.
fn commit_only_if_absent(
    connection: &mut Connection,
    store_id: &str,
    entries: &[(String, String)],
    import_source: &str,
    now_ms: i64,
) -> Result<()> {
    let Some(store) = catalog_store(store_id) else {
        bail!("Unknown client-storage store {store_id}");
    };
    let receipts = catalog_store("migrationReceipts").expect("catalog lists migrationReceipts");
    let receipt_key = format!("{RECEIPT_PREFIX}{import_source}");
    let transaction =
        connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if record_raw(&transaction, &receipt_key)?.is_some() {
        transaction.commit()?;
        return Ok(());
    }
    let mut revision = metadata_revision(&transaction)?;
    for (key, raw) in entries {
        if record_raw(&transaction, key)?.is_some() {
            continue;
        }
        revision += 1;
        put_record(
            &transaction,
            store,
            key,
            raw,
            retained_at(store.id, raw).unwrap_or(now_ms),
            revision,
        )?;
    }
    revision += 1;
    put_record(&transaction, receipts, &receipt_key, "1", now_ms, revision)?;
    recompute_record_metadata(&transaction)?;
    transaction.commit()?;
    Ok(())
}

/// Writes one row in the shape `applyDatabaseMutations` writes it.
fn put_record(
    connection: &Connection,
    store: &CatalogStore,
    key: &str,
    raw: &str,
    updated_at: i64,
    revision: i64,
) -> Result<()> {
    let row = json!({
        "key": key,
        "store": store.id,
        "raw": raw,
        "bytes": storage_bytes(key, raw),
        "updatedAt": updated_at,
        "revision": revision,
        "schemaVersion": store.version,
    });
    connection.execute(
        "INSERT INTO records VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET store=excluded.store,value=excluded.value",
        params![key, store.id, row.to_string()],
    )?;
    Ok(())
}

/// `upgradeStorageSchemas`: every catalog store is at version 1 and none has an `upgrade`, so a row
/// written at any other version cannot be read and start-up must say so rather than erase it or
/// read it with the wrong meaning. The message names the stores only.
fn check_schema_versions(connection: &Connection) -> Result<()> {
    let mut query = connection.prepare(
        "SELECT DISTINCT store, CAST(COALESCE(json_extract(value,'$.schemaVersion'),1) AS INTEGER) FROM records",
    )?;
    let rows = query.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut unsupported = Vec::new();
    for row in rows {
        let (store, version) = row?;
        if let Some(definition) = catalog_store(&store) {
            if version != definition.version && !unsupported.contains(&store) {
                unsupported.push(store);
            }
        }
    }
    if !unsupported.is_empty() {
        bail!(
            "Client storage needs a schema upgrade that does not exist for: {}",
            unsupported.join(", ")
        );
    }
    Ok(())
}

fn record_raw(connection: &Connection, key: &str) -> Result<Option<String>> {
    let text: Option<String> = connection
        .query_row("SELECT value FROM records WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(text
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|row| row.get("raw").and_then(Value::as_str).map(str::to_string)))
}

fn metadata_revision(connection: &Connection) -> Result<i64> {
    let text: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='revision'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(text
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_f64())
        .map_or(0, |value| value as i64))
}

/// The catalog's `retainedAt` for the two stores that have one: `sentHistory` keeps the message's
/// `createdAt` (`Date.parse`), `chatSnapshots` its `savedAt` (`Number`). `None` is what the
/// TypeScript reads as not finite, which keeps `now`.
fn retained_at(store: &str, raw: &str) -> Option<i64> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let at = match store {
        "sentHistory" => parse_iso_ms(value.get("createdAt")?.as_str()?)?,
        "chatSnapshots" => match value.get("savedAt")? {
            Value::Number(number) => number.as_f64()?,
            Value::String(text) => text.trim().parse::<f64>().ok()?,
            _ => return None,
        },
        _ => return None,
    };
    at.is_finite().then_some(at as i64)
}

/// `Date.parse` for the ISO form `toISOString` writes (`YYYY-MM-DDTHH:MM:SS[.sss](Z|±HH:MM)`).
fn parse_iso_ms(text: &str) -> Option<f64> {
    let (date, time) = text.trim().split_once('T')?;
    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;
    let (clock, offset_minutes) = if let Some(clock) = time.strip_suffix('Z') {
        (clock, 0)
    } else if let Some(index) = time.rfind(['+', '-']) {
        let (clock, zone) = time.split_at(index);
        let sign = if zone.starts_with('-') { -1 } else { 1 };
        let (hours, minutes) = zone[1..].split_once(':')?;
        (
            clock,
            sign * (hours.parse::<i64>().ok()? * 60 + minutes.parse::<i64>().ok()?),
        )
    } else {
        return None;
    };
    let mut clock = clock.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let seconds: f64 = clock.next().unwrap_or("0").parse().ok()?;
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * m + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let seconds_of_day = hour * 3600 + minute * 60 - offset_minutes * 60;
    Some((days * 86_400 + seconds_of_day) as f64 * 1000.0 + (seconds * 1000.0).round())
}

/// A value inside a template literal (`${value.id}`): strings as they are, numbers in JavaScript's
/// spelling, everything else as JavaScript would print it.
fn js_template_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(_) => js_stringify(value),
        Value::Array(_) | Value::Object(_) => js_stringify(value),
    }
}

/// `JSON.stringify` for a parsed value. The one place serde's spelling differs is a number that
/// was stored as a float with no fraction: JavaScript writes `1`, not `1.0`, and large or tiny ones
/// with a JavaScript exponent. Key order is the value's own (the desktop builds serde_json with
/// `preserve_order`).
fn js_stringify(value: &Value) -> String {
    match value {
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                return number.to_string();
            }
            let float = number.as_f64().unwrap_or(0.0);
            if !float.is_finite() {
                return "null".to_string();
            }
            if float == 0.0 {
                return "0".to_string();
            }
            let magnitude = float.abs();
            if (1e-6..1e21).contains(&magnitude) {
                if float.fract() == 0.0 {
                    format!("{float:.0}")
                } else {
                    format!("{float}")
                }
            } else {
                let spelled = format!("{float:e}");
                match spelled.split_once('e') {
                    Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                        format!("{mantissa}e+{exponent}")
                    }
                    _ => spelled,
                }
            }
        }
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(js_stringify).collect::<Vec<_>>().join(",")
        ),
        Value::Object(object) => format!(
            "{{{}}}",
            object
                .iter()
                .map(|(key, value)| format!(
                    "{}:{}",
                    Value::String(key.clone()),
                    js_stringify(value)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => other.to_string(),
    }
}
