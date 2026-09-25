//! Drives the `records` door against a COPY of a client-storage database and checks every rule it
//! owes the client-storage service.
//!
//! Usage: `cargo run --example records_door -- [client-storage.sqlite3]`
//!
//! Without a path it builds a small table of its own. With one it COPIES that file to a scratch
//! path and works there, so the real database is never opened for writing: a door that writes the
//! user's own state directory is a door no gate may point at.
//!
//! This is tooling, not a test suite. It prints what it checked and exits non-zero on the first
//! rule that does not hold.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use ghostex_client_storage::{RecordRead, RecordStore, RecordWrite, read_record, write_record};
use rusqlite::Connection;
use serde_json::{Value, json};

const STORE: RecordStore = RecordStore {
    id: "remotePresentations",
    version: 1,
    max_entry_bytes: 8 * 1024 * 1024,
    max_bytes: 24 * 1024 * 1024,
    max_entries: 32,
    max_age_ms: Some(30 * 24 * 60 * 60 * 1_000),
};
const KEY: &str = "ghostex-gpui-remote-last-seen-presentations:machine:records-door";
const NOW: i64 = 1_800_000_000_000;

fn main() -> ExitCode {
    let scratch = std::env::temp_dir().join(format!("records-door-{}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&scratch);
    let source = std::env::args().nth(1);
    match prepare(&scratch, source.as_deref()) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("cannot prepare the scratch database: {error}");
            return ExitCode::from(2);
        }
    }
    let mut checks = Checks::default();
    let result = run(&scratch, &mut checks);
    let _ = std::fs::remove_file(&scratch);
    let _ = std::fs::remove_file(format!("{}-wal", scratch.display()));
    let _ = std::fs::remove_file(format!("{}-shm", scratch.display()));
    if let Err(error) = result {
        println!("\nthe harness stopped: {error}");
        return ExitCode::FAILURE;
    }
    println!("\n{} checks, {} failed", checks.total, checks.failed);
    if checks.total == 0 {
        println!("nothing was checked, which is a failure of the harness");
        return ExitCode::FAILURE;
    }
    if checks.failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Copies the source database, or builds a small one with two stores and a few rows.
fn prepare(scratch: &PathBuf, source: Option<&str>) -> Result<(), String> {
    if let Some(source) = source {
        std::fs::copy(source, scratch).map_err(|error| error.to_string())?;
        println!("working on a copy of {source}");
        return Ok(());
    }
    let connection = Connection::open(scratch).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "CREATE TABLE records (key TEXT PRIMARY KEY, store TEXT NOT NULL, value TEXT NOT NULL);
             CREATE INDEX records_by_store ON records(store);
             CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|error| error.to_string())?;
    // The usage rows are counted here rather than by the code under test, so the harness starts
    // from a table whose bookkeeping is already right and every later move is the door's.
    let mut total = 0;
    for (index, store) in [("drafts", 3), ("notices", 2)] {
        let mut bytes = 0;
        for row in 0..store {
            let key = format!("{index}:{row}");
            let raw = format!("{{\"n\":{row}}}");
            let size = 2 * (key.chars().count() + raw.chars().count()) as i64;
            bytes += size;
            total += size;
            let value = json!({
                "key": key, "store": index, "raw": raw, "bytes": size,
                "updatedAt": NOW - 1_000, "revision": 10 + row, "schemaVersion": 1,
            });
            connection
                .execute(
                    "INSERT INTO records VALUES (?1,?2,?3)",
                    rusqlite::params![key, index, value.to_string()],
                )
                .map_err(|error| error.to_string())?;
        }
        connection
            .execute(
                "INSERT INTO metadata VALUES (?1,?2)",
                rusqlite::params![index, json!({"bytes": bytes, "entries": store}).to_string()],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "INSERT INTO metadata VALUES ('total',?1)",
            [total.to_string()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "INSERT INTO metadata VALUES ('revision','14');
             INSERT INTO metadata VALUES ('browserImportV1','true');",
        )
        .map_err(|error| error.to_string())?;
    println!("working on a database built here");
    Ok(())
}

#[derive(Default)]
struct Checks {
    total: u64,
    failed: u64,
}

impl Checks {
    fn check(&mut self, label: &str, ok: bool) {
        self.total += 1;
        if !ok {
            self.failed += 1;
        }
        println!("{label:<56} {}", if ok { "ok" } else { "FAILED" });
    }
}

fn run(scratch: &PathBuf, checks: &mut Checks) -> Result<(), String> {
    let connection = Connection::open(scratch).map_err(|error| error.to_string())?;
    connection
        .busy_timeout(std::time::Duration::from_millis(500))
        .map_err(|error| error.to_string())?;
    let before = snapshot(&connection)?;

    // A fresh write.
    let small = json!({"rows": ["a", "b"], "sidebarOrder": 0}).to_string();
    let first = commit(&connection, &small, NOW)?;
    checks.check("a fresh payload is stored", first == RecordWrite::Stored);
    let after_first = snapshot(&connection)?;
    checks.check(
        "the row carries the catalog's schema version",
        row(&connection, KEY)?
            .get("schemaVersion")
            .and_then(Value::as_i64)
            == Some(STORE.version),
    );
    checks.check(
        "bytes is the UTF-16 accounting, not the raw length",
        row(&connection, KEY)?.get("bytes").and_then(Value::as_i64)
            == Some(2 * (KEY.chars().count() + small.chars().count()) as i64),
    );
    checks.check(
        "every metadata row equals a hand recount",
        after_first.metadata == recount(&connection)?,
    );
    checks.check(
        "the counter moved up by one",
        after_first.revision == before.revision.max(before.highest_row_revision) + 1,
    );
    checks.check(
        "no other store's usage moved",
        other_stores(&before.metadata, STORE.id) == other_stores(&after_first.metadata, STORE.id),
    );
    checks.check(
        "nothing wrote a nameless metadata row",
        !after_first.metadata.contains_key(""),
    );
    checks.check(
        "browserImportV1 was left alone",
        before.metadata.get("browserImportV1") == after_first.metadata.get("browserImportV1"),
    );

    // The identical write.
    let again = commit(&connection, &small, NOW + 5_000)?;
    let after_again = snapshot(&connection)?;
    checks.check(
        "an identical payload is Unchanged",
        again == RecordWrite::Unchanged,
    );
    checks.check(
        "an Unchanged write touches nothing at all",
        after_again == after_first,
    );

    // A replacement.
    let bigger = json!({"rows": ["a", "b", "c"], "sidebarOrder": 1}).to_string();
    let replaced = commit(&connection, &bigger, NOW + 10_000)?;
    let after_replaced = snapshot(&connection)?;
    checks.check(
        "a different payload is stored",
        replaced == RecordWrite::Stored,
    );
    checks.check(
        "every metadata row still equals a hand recount",
        after_replaced.metadata == recount(&connection)?,
    );
    checks.check(
        "the entry count did not double",
        usage(&after_replaced.metadata, STORE.id).1 == usage(&after_first.metadata, STORE.id).1,
    );
    // The load-bearing line in `write_record`: the bookkeeping reports the highest ROW revision,
    // which is below a service that has counted mutations the rows do not carry. The write takes
    // its number above both, so the counter can only ever move up. This check and the next one fail
    // if `max(revision_floor, metadata_revision)` becomes the row revision alone. The metadata-only
    // variant is not told apart: on every consistent database the counter is already at or above
    // every row, and the row floor is there for a counter row that is missing or damaged.
    checks.check(
        "the counter never moved backwards",
        after_replaced.revision > after_first.revision && after_first.revision > before.revision,
    );

    // The service is ahead of its rows, which is the real shape of the database at any moment.
    connection
        .execute(
            "INSERT INTO metadata VALUES ('revision',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [(after_replaced.revision + 5_000).to_string()],
        )
        .map_err(|error| error.to_string())?;
    let ahead = snapshot(&connection)?;
    let third = json!({"rows": ["a"], "sidebarOrder": 2}).to_string();
    commit(&connection, &third, NOW + 20_000)?;
    let after_ahead = snapshot(&connection)?;
    checks.check(
        "a service ahead of its rows is not overwritten downwards",
        after_ahead.revision == ahead.revision + 1,
    );

    // The read, and the catalog's age limit.
    checks.check(
        "a fresh row reads back",
        read_record(&connection, STORE, KEY, NOW + 20_000).map_err(str::to_string)?
            == RecordRead::Payload(third.clone()),
    );
    let limit = STORE.max_age_ms.unwrap_or_default();
    checks.check(
        "a row one millisecond inside the age limit still reads",
        read_record(&connection, STORE, KEY, NOW + 20_000 + limit).map_err(str::to_string)?
            == RecordRead::Payload(third.clone()),
    );
    checks.check(
        "a row past the catalog's maxAgeMs is refused",
        read_record(&connection, STORE, KEY, NOW + 20_001 + limit).map_err(str::to_string)?
            == RecordRead::Expired,
    );
    checks.check(
        "an expired read leaves the row where it is",
        row(&connection, KEY).is_ok(),
    );
    checks.check(
        "a key with no row reads as missing",
        read_record(&connection, STORE, "nothing:here", NOW).map_err(str::to_string)?
            == RecordRead::Missing,
    );

    // The bounds.
    let huge = "x".repeat(STORE.max_entry_bytes as usize);
    checks.check(
        "an oversized entry is refused, not evicted around",
        commit(&connection, &huge, NOW + 30_000)? == RecordWrite::Refused("entry"),
    );
    let after_refusal = snapshot(&connection)?;
    checks.check("a refusal writes nothing", after_refusal == after_ahead);
    fill_store(&connection, STORE.max_entries)?;
    let crowded = json!({"rows": ["z"]}).to_string();
    checks.check(
        "the entry-count bound refuses a new key",
        write_in_transaction(
            &connection,
            "…:machine:one-too-many",
            &crowded,
            NOW + 40_000,
        )? == RecordWrite::Refused("entries"),
    );
    checks.check(
        "a key already in the store is still admitted at the bound",
        commit(
            &connection,
            &json!({"rows": ["y"]}).to_string(),
            NOW + 41_000,
        )? == RecordWrite::Stored,
    );

    // An emptied store.
    connection
        .execute("DELETE FROM records WHERE store='notices'", [])
        .map_err(|error| error.to_string())?;
    commit(
        &connection,
        &json!({"rows": ["w"]}).to_string(),
        NOW + 50_000,
    )?;
    let after_empty = snapshot(&connection)?;
    checks.check(
        "a store emptied of every row goes to zero rather than stale",
        usage(&after_empty.metadata, "notices") == (0, 0),
    );
    checks.check(
        "and the totals agree with a hand recount",
        after_empty.metadata == recount(&connection)?,
    );
    Ok(())
}

/// One write, in the transaction the desktop door wraps it in.
fn commit(connection: &Connection, raw: &str, now_ms: i64) -> Result<RecordWrite, String> {
    write_in_transaction(connection, KEY, raw, now_ms)
}

fn write_in_transaction(
    connection: &Connection,
    key: &str,
    raw: &str,
    now_ms: i64,
) -> Result<RecordWrite, String> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = write_record(connection, STORE, key, raw, now_ms);
    match &result {
        Ok(RecordWrite::Stored) => connection.execute_batch("COMMIT"),
        _ => connection.execute_batch("ROLLBACK"),
    }
    .map_err(|error| error.to_string())?;
    result.map_err(str::to_string)
}

/// Fills the store up to its entry bound.
fn fill_store(connection: &Connection, entries: i64) -> Result<(), String> {
    let held: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM records WHERE store=?1",
            [STORE.id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    for index in held..entries {
        let key = format!("…:machine:filler-{index}");
        let raw = json!({"filler": index}).to_string();
        write_in_transaction(connection, &key, &raw, NOW + 35_000 + index)?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    metadata: BTreeMap<String, String>,
    revision: i64,
    highest_row_revision: i64,
}

fn snapshot(connection: &Connection) -> Result<Snapshot, String> {
    let mut query = connection
        .prepare("SELECT key, value FROM metadata")
        .map_err(|error| error.to_string())?;
    let metadata: BTreeMap<String, String> = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error: rusqlite::Error| error.to_string())?;
    let revision = metadata
        .get("revision")
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(0);
    let highest_row_revision = connection
        .query_row(
            "SELECT COALESCE(MAX(CAST(COALESCE(json_extract(value,'$.revision'),0) AS INTEGER)),0) FROM records",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(Snapshot {
        metadata,
        revision,
        highest_row_revision,
    })
}

/// The metadata the table implies, counted in Rust rather than by the door's own SQL.
fn recount(connection: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut query = connection
        .prepare("SELECT store, value FROM records")
        .map_err(|error| error.to_string())?;
    let rows: Vec<(String, Value)> = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .map(|row| {
            row.map_err(|error| error.to_string())
                .and_then(|(store, text)| {
                    serde_json::from_str::<Value>(&text)
                        .map(|value| (store, value))
                        .map_err(|error| error.to_string())
                })
        })
        .collect::<Result<_, _>>()?;
    let mut counted: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut total = 0;
    for (store, value) in &rows {
        let bytes = value.get("bytes").and_then(Value::as_i64).unwrap_or(0);
        let entry = counted.entry(store.clone()).or_insert((0, 0));
        entry.0 += bytes;
        entry.1 += 1;
        total += bytes;
    }
    // Every usage-shaped metadata row the table no longer has rows for reads as zero, which is what
    // `applyDatabaseMutations` decrements it to.
    let held = snapshot(connection)?.metadata;
    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in &held {
        let usage_shaped = serde_json::from_str::<Value>(value).is_ok_and(|held| {
            held.get("bytes").and_then(Value::as_i64).is_some()
                && held.get("entries").and_then(Value::as_i64).is_some()
        });
        if usage_shaped && !counted.contains_key(key) {
            expected.insert(key.clone(), json!({"bytes": 0, "entries": 0}).to_string());
        } else if !usage_shaped {
            expected.insert(key.clone(), value.clone());
        }
    }
    for (store, (bytes, entries)) in counted {
        expected.insert(
            store,
            json!({"bytes": bytes, "entries": entries}).to_string(),
        );
    }
    expected.insert("total".to_string(), json!(total).to_string());
    // The stored counter is the service's own and only ever rises, so a recount cannot predict it;
    // it is checked on its own above.
    expected.insert(
        "revision".to_string(),
        held.get("revision").cloned().unwrap_or_default(),
    );
    Ok(expected)
}

fn usage(metadata: &BTreeMap<String, String>, store: &str) -> (i64, i64) {
    metadata
        .get(store)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .map(|value| {
            (
                value.get("bytes").and_then(Value::as_i64).unwrap_or(-1),
                value.get("entries").and_then(Value::as_i64).unwrap_or(-1),
            )
        })
        .unwrap_or((-1, -1))
}

fn other_stores(metadata: &BTreeMap<String, String>, store: &str) -> BTreeMap<String, String> {
    metadata
        .iter()
        .filter(|(key, _)| {
            key.as_str() != store && key.as_str() != "total" && key.as_str() != "revision"
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn row(connection: &Connection, key: &str) -> Result<Value, String> {
    let text: String = connection
        .query_row("SELECT value FROM records WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}
