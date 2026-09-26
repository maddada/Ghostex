use super::storage::Storage;
use anyhow::{Context, Result, anyhow, bail, ensure};
use chromium_storage_indexeddb_core::{IdbKey, RecordValue, V8Value};
use chromium_storage_localstorage_core::LocalStorageRecord;
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{collections::HashMap, fs, path::Path};

/// CDXC:Drafts 2026-09-17 WHY: Native chat must retain browser-era draft ownership, recovery records and preferences without starting a hidden browser to read them. Import only the app's file origin, resolve deletion tombstones before decoding, and leave the source intact until the strict native transaction commits.
/// `Ok(true)` when this call imported, `Ok(false)` when an earlier start already had.
pub(crate) fn import(storage: &mut Storage, profile: &Path) -> Result<bool> {
    let imported: Option<String> = storage
        .database
        .query_row(
            "SELECT value FROM metadata WHERE key='browserImportV1'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if imported.is_some() {
        return Ok(false);
    }
    let local = profile.join("Local Storage/leveldb");
    let mut preferences = Vec::new();
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
                if deleted || origin != "file://" {
                    continue;
                }
                ensure!(
                    !script_key.lossy && !value.lossy,
                    "An existing app preference could not be decoded exactly"
                );
                preferences.push((script_key.text, value.text));
            }
        }
    }
    let indexed = profile.join("IndexedDB/file__0.indexeddb.leveldb");
    let mut records = Vec::new();
    let mut legacy = Vec::new();
    if indexed.exists() {
        let mut raw = latest_records(&indexed)?;
        unwrap_values(&mut raw, &profile.join("IndexedDB/file__0.indexeddb.blob"))?;
        for row in chromium_storage_indexeddb_core::decode_records(&raw) {
            if row.deleted {
                continue;
            }
            let database = row.database.as_deref().unwrap_or_default();
            let table = row.object_store.as_deref().unwrap_or_default();
            if !matches!(
                (database, table),
                ("ghostex-client-storage", "records")
                    | ("ghostex-draft-outbox", "drafts")
                    | ("ghostex-session-chat-v1", "snapshots")
            ) {
                continue;
            }
            let key = match &row.key {
                IdbKey::String(key) => key.as_str(),
                _ => "",
            };
            let value = match row.value {
                RecordValue::V8(value) => to_json(value)?,
                RecordValue::Undecoded { error, .. } => {
                    bail!("Could not import existing app storage {database}/{table}/{key}: {error}")
                }
            };
            if database == "ghostex-client-storage" {
                ensure!(
                    value["key"].as_str() == Some(key)
                        && value["raw"].is_string()
                        && value["store"].is_string(),
                    "Invalid existing app storage record: {key}"
                );
                records.push(value);
            } else {
                legacy.push((database.to_owned(), table.to_owned(), value));
            }
        }
    }
    let transaction = storage.database.transaction()?;
    for (key, value) in preferences {
        transaction.execute("INSERT INTO preferences VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
    }
    for row in records {
        let key = row["key"].as_str().unwrap();
        let previous: Option<String> = transaction
            .query_row("SELECT value FROM records WHERE key=?1", [key], |record| {
                record.get(0)
            })
            .optional()?;
        if let Some(previous) = previous {
            let previous: Value = serde_json::from_str(&previous)?;
            if previous["updatedAt"].as_f64().unwrap_or(0.0)
                > row["updatedAt"].as_f64().unwrap_or(0.0)
            {
                continue;
            }
        }
        transaction.execute("INSERT INTO records VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET store=excluded.store,value=excluded.value", params![key, row["store"].as_str(), row.to_string()])?;
    }
    for (database, table, value) in legacy {
        transaction.execute(
            "INSERT INTO legacy VALUES (?1,?2,?3)",
            params![database, table, value.to_string()],
        )?;
    }
    super::storage_metadata::recompute_record_metadata(&transaction)?;
    transaction.execute(
        "INSERT INTO metadata VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params!["browserImportV1", json!(true).to_string()],
    )?;
    transaction.commit()?;
    Ok(true)
}

pub(crate) fn latest_records(path: &Path) -> Result<Vec<leveldb_core::Record>> {
    let mut latest: HashMap<Vec<u8>, leveldb_core::Record> = HashMap::new();
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        let records = match path.extension().and_then(|ext| ext.to_str()) {
            Some("ldb" | "sst") => leveldb_core::parse_table_bytes(&fs::read(&path)?, &path)?,
            Some("log") => leveldb_core::parse_log_bytes(&fs::read(&path)?, &path)?,
            _ => continue,
        };
        for record in records {
            if latest
                .get(&record.key)
                .is_none_or(|old| record.seq > old.seq)
            {
                latest.insert(record.key.clone(), record);
            }
        }
    }
    Ok(latest.into_values().filter(|row| !row.deleted).collect())
}

fn varint(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let byte = *bytes
            .get(*offset)
            .ok_or_else(|| anyhow!("Truncated storage integer"))?;
        *offset += 1;
        value |= ((byte & 127) as u64) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    bail!("Invalid storage integer")
}
fn prefix(key: &[u8]) -> Option<(usize, u64, usize)> {
    let first = *key.first()?;
    let database_len = (first >> 5) as usize + 1;
    let store_len = ((first >> 2) & 7) as usize + 1;
    let index_len = (first & 3) as usize + 1;
    let index_offset = 1 + database_len + store_len;
    let database = key
        .get(1..1 + database_len)?
        .iter()
        .enumerate()
        .fold(0, |v, (i, b)| v | ((*b as u64) << (i * 8)));
    Some((index_offset, database, index_len))
}
pub(crate) fn unwrap_values(records: &mut [leveldb_core::Record], blobs: &Path) -> Result<()> {
    let externals: HashMap<Vec<u8>, Vec<u8>> = records
        .iter()
        .filter(|row| {
            prefix(&row.key)
                .is_some_and(|(offset, _, len)| len == 1 && row.key.get(offset) == Some(&3))
        })
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect();
    for row in records {
        let Some((index_offset, database, index_len)) = prefix(&row.key) else {
            continue;
        };
        if index_len != 1 || row.key.get(index_offset) != Some(&1) {
            continue;
        }
        let mut offset = 0;
        varint(&row.value, &mut offset)?;
        let mut body = row.value[offset..].to_vec();
        if body.starts_with(&[255, 17, 1]) {
            let mut pointer = 3;
            let size = varint(&body, &mut pointer)?;
            let selected = varint(&body, &mut pointer)?;
            let mut key = row.key.clone();
            key[index_offset] = 3;
            let metadata = externals
                .get(&key)
                .ok_or_else(|| anyhow!("Missing external app storage metadata"))?;
            let mut cursor = 0;
            let mut number = 0;
            for index in 0..=selected {
                let kind = *metadata
                    .get(cursor)
                    .ok_or_else(|| anyhow!("Missing external app storage value"))?;
                cursor += 1;
                ensure!(kind <= 1, "Unsupported external app storage kind");
                number = varint(metadata, &mut cursor)?;
                let units = varint(metadata, &mut cursor)? as usize;
                cursor += units * 2;
                let _ = varint(metadata, &mut cursor)?;
                if kind == 1 {
                    let units = varint(metadata, &mut cursor)? as usize;
                    cursor += units * 2;
                    let _ = varint(metadata, &mut cursor)?;
                }
                if index == selected {
                    break;
                }
            }
            let file = blobs
                .join(format!("{database:x}"))
                .join(format!("{:02x}", (number >> 8) & 255))
                .join(format!("{number:x}"));
            body = fs::read(&file).context("Read existing app storage blob")?;
            ensure!(
                body.len() as u64 == size,
                "Existing app storage blob has changed size"
            );
        }
        if body.starts_with(&[255, 17, 2]) {
            body = snap::raw::Decoder::new().decompress_vec(&body[3..])?;
        }
        row.value.truncate(offset);
        row.value.extend(body);
    }
    Ok(())
}
pub(crate) fn to_json(value: V8Value) -> Result<Value> {
    Ok(match value {
        V8Value::Undefined | V8Value::Null | V8Value::Hole => Value::Null,
        V8Value::Bool(value) | V8Value::BooleanObject(value) => json!(value),
        V8Value::Int(value) => json!(value),
        V8Value::Double(value) | V8Value::NumberObject(value) => json!(value),
        V8Value::String(value) | V8Value::StringObject(value) => json!(value),
        V8Value::Array(values) => {
            Value::Array(values.into_iter().map(to_json).collect::<Result<_>>()?)
        }
        V8Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, to_json(value)?)))
                .collect::<Result<_>>()?,
        ),
        _ => bail!("Existing app storage contains a non-JSON value"),
    })
}
