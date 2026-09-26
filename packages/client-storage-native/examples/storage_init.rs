//! Drives `initialize_client_storage` against scratch databases and checks what start-up owes:
//! a fresh file gets its tables, browser-era values move into `records` exactly once, a second
//! start finds nothing to do, and the metadata matches the rows.
//!
//! Usage: `cargo run --example storage_init -- [client-storage.sqlite3]`
//!
//! With a path it COPIES that database to a scratch file and runs start-up there, so the real file
//! is never opened for writing. Tooling, not a test suite: prints what it checked, exits non-zero
//! on the first rule that does not hold.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ghostex_client_storage::{StorageInitReport, initialize_client_storage, scan_record_usage};
use rusqlite::Connection;
use serde_json::Value;

const NOW: i64 = 1_800_000_000_000;

fn main() -> ExitCode {
    let mut failures = 0;
    let mut check = |ok: bool, what: &str| {
        println!("{} {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            failures += 1;
        }
    };
    let scratch = |name: &str| -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "storage-init-{name}-{}.sqlite3",
            std::process::id()
        ));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        path
    };

    // 1. A fresh install.
    let fresh = scratch("fresh");
    let first = initialize_client_storage(&fresh, None, NOW);
    check(
        first
            .as_ref()
            .is_ok_and(|report| *report == StorageInitReport::default()),
        "fresh: start-up succeeds and has nothing to migrate",
    );
    let tables = table_names(&fresh);
    check(
        ["records", "metadata", "preferences", "legacy"]
            .iter()
            .all(|table| tables.contains(&table.to_string())),
        "fresh: the four tables exist",
    );
    check(
        initialize_client_storage(&fresh, None, NOW).is_ok(),
        "fresh: a second start succeeds",
    );

    // 2. Browser-era values waiting to move.
    let upgraded = scratch("upgraded");
    initialize_client_storage(&upgraded, None, NOW).expect("create");
    {
        let connection = Connection::open(&upgraded).expect("open");
        for (key, value) in [
            (
                "ghostex.sessionChat.draft.alpha",
                r#"{"text":"draft","revision":2}"#,
            ),
            (
                "ghostex.sessionChat.sent.alpha",
                r#"{"createdAt":"2026-01-02T03:04:05.678Z","text":"sent"}"#,
            ),
            ("ghostex.sessionChat.codeWrap", "true"),
        ] {
            connection
                .execute("INSERT INTO preferences VALUES (?1,?2)", [key, value])
                .expect("preference");
        }
        for (database, table, value) in [
            (
                "ghostex-draft-outbox",
                "drafts",
                r#"{"id":"d1","text":"queued","weight":1.0,"tiny":1e-7}"#,
            ),
            (
                "ghostex-session-chat-v1",
                "snapshots",
                r#"{"key":"s1","savedAt":1700000000000,"turns":[]}"#,
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO legacy VALUES (?1,?2,?3)",
                    [database, table, value],
                )
                .expect("legacy");
        }
    }
    let report = initialize_client_storage(&upgraded, None, NOW).expect("migrate");
    check(
        report.preferences_migrated == 2,
        "upgraded: the two records-backed preferences moved",
    );
    check(
        report.legacy_rows_migrated == 2 && report.legacy_rows_unmigrated == 0,
        "upgraded: both legacy rows landed",
    );
    let connection = Connection::open(&upgraded).expect("open");
    check(
        preference(&connection, "ghostex.sessionChat.codeWrap").as_deref() == Some("true"),
        "upgraded: a local preference stays a preference",
    );
    check(
        preference(&connection, "ghostex.sessionChat.draft.alpha").is_none(),
        "upgraded: a moved preference is deleted",
    );
    check(
        record(&connection, "ghostex.sessionChat.draft.alpha").is_some_and(|row| {
            row["raw"] == r#"{"text":"draft","revision":2}"#
                && row["store"] == "drafts"
                && row["updatedAt"] == NOW
        }),
        "upgraded: the draft row is the preference's raw, now-stamped",
    );
    check(
        record(&connection, "ghostex.sessionChat.sent.alpha")
            .is_some_and(|row| row["updatedAt"] == 1_767_323_045_678_i64),
        "upgraded: sentHistory keeps createdAt as updatedAt (Date.parse)",
    );
    // `JSON.stringify` of the parsed row without `id` (checked with bun): {"text":"queued","weight":1,"tiny":1e-7}
    check(
        record(&connection, "ghostex.sessionChat.outbox.d1")
            .is_some_and(|row| row["raw"] == r#"{"text":"queued","weight":1,"tiny":1e-7}"#),
        "upgraded: the outbox row drops its id and spells numbers as JavaScript does",
    );
    check(
        record(&connection, "ghostex.sessionChat.snapshot.s1")
            .is_some_and(|row| row["updatedAt"] == 1_700_000_000_000_i64),
        "upgraded: a snapshot keeps savedAt as updatedAt",
    );
    for receipt in [
        "local:drafts",
        "local:sentHistory",
        "database:ghostex-draft-outbox",
        "database:ghostex-session-chat-v1",
    ] {
        check(
            record(&connection, &format!("ghostex.storage.imported.{receipt}"))
                .is_some_and(|row| row["raw"] == "1"),
            &format!("upgraded: receipt {receipt}"),
        );
    }
    check(
        metadata_matches_rows(&connection),
        "upgraded: metadata equals a fresh scan of the rows",
    );
    let again = initialize_client_storage(&upgraded, None, NOW + 1).expect("again");
    check(
        again.preferences_migrated == 0 && again.legacy_rows_unmigrated == 0,
        "upgraded: a second start moves nothing",
    );
    check(
        record(&connection, "ghostex.sessionChat.outbox.d1")
            .is_some_and(|row| row["updatedAt"] == NOW),
        "upgraded: a second start rewrites nothing",
    );

    // 3. An unsupported schema version stops start-up without touching the row.
    {
        connection
            .execute("UPDATE records SET value=json_set(value,'$.schemaVersion',2) WHERE key='ghostex.sessionChat.draft.alpha'", [])
            .expect("bump");
    }
    let refused = initialize_client_storage(&upgraded, None, NOW);
    check(
        refused
            .as_ref()
            .is_err_and(|error| error.to_string().contains("drafts")),
        "schema: a version with no upgrade is an error naming the store",
    );

    // 4. A copy of a real database.
    if let Some(source) = std::env::args().nth(1) {
        let copy = scratch("copy");
        std::fs::copy(&source, &copy).expect("copy");
        match initialize_client_storage(&copy, None, NOW) {
            Ok(report) => {
                println!("copy: {report:?}");
                let connection = Connection::open(&copy).expect("open");
                check(
                    metadata_matches_rows(&connection),
                    "copy: metadata equals a fresh scan of the rows",
                );
            }
            Err(error) => check(false, &format!("copy: start-up failed: {error}")),
        }
    }

    println!("{failures} failures");
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn table_names(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).expect("open");
    let mut query = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .expect("query");
    query
        .query_map([], |row| row.get(0))
        .expect("rows")
        .map(Result::unwrap)
        .collect()
}

fn preference(connection: &Connection, key: &str) -> Option<String> {
    connection
        .query_row("SELECT value FROM preferences WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .ok()
}

fn record(connection: &Connection, key: &str) -> Option<Value> {
    let text: String = connection
        .query_row("SELECT value FROM records WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .ok()?;
    serde_json::from_str(&text).ok()
}

fn metadata_matches_rows(connection: &Connection) -> bool {
    let usage = scan_record_usage(connection).expect("scan");
    let total: i64 = usage.iter().map(|store| store.bytes).sum();
    let stored_total: Option<String> = connection
        .query_row("SELECT value FROM metadata WHERE key='total'", [], |row| {
            row.get(0)
        })
        .ok();
    usage.iter().all(|store| {
        let held: Option<String> = connection
            .query_row(
                "SELECT value FROM metadata WHERE key=?1",
                [&store.store],
                |row| row.get(0),
            )
            .ok();
        held.and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .is_some_and(|value| value["bytes"] == store.bytes && value["entries"] == store.entries)
    }) && stored_total.as_deref() == Some(total.to_string().as_str())
}
