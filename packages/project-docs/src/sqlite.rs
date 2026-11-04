//! Reading a SQLite database for the Files view: its tables and views, one page of a table's rows,
//! or one page of a read-only query's answer. Shared by the desktop's own Files bridge (local
//! projects) and gxserver's `runProjectDocsAction` (remote projects), so both answer the same
//! `sqlite` action the same way.
//!
//! CDXC:Docs 2026-10-17 DECISION:
//! User (Q223) picked the "read-only browser" for SQLite databases: a table list, rows paged in the Files view's table, and a box for a SELECT query. The database opens read-only, so Files never writes to it; a query must be one read-only statement; ATTACH is refused, so a query cannot reach another database file; and every read gives up after a few seconds, so a long query cannot hold the database's shared lock against an app that is writing to it.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use rusqlite::{limits::Limit, types::ValueRef, Connection, OpenFlags};
use serde_json::{json, Map, Value};

/// Rows in one page when the request does not say.
pub const DEFAULT_PAGE_ROWS: u64 = 200;
const MAX_PAGE_ROWS: u64 = 1_000;
/// A cell's text is cut here so one huge value cannot swell the answer.
const MAX_CELL_CHARS: usize = 2_000;
/// The longest a read may run before it is interrupted.
const READ_TIME_LIMIT: Duration = Duration::from_secs(5);
/// How long a read waits for an app that is writing to finish.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_SQL_CHARS: usize = 20_000;

/// File extensions the Files view opens as a SQLite database.
pub const SQLITE_EXTENSIONS: &[&str] = &["db", "sqlite", "sqlite3", "db3"];

/// Answers the `sqlite` Files action for the database at `file`. `params` may name a `table` to
/// page through, or carry `sql` (a read-only query); with neither it lists the tables only.
/// `offset` and `limit` page the rows.
pub fn sqlite_action(file: &Path, params: &Map<String, Value>) -> Result<Value, String> {
    let connection = open_read_only(file)?;
    let tables = list_tables(&connection)?;
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_PAGE_ROWS)
        .clamp(1, MAX_PAGE_ROWS);
    let sql = params
        .get("sql")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|sql| !sql.is_empty());
    let table = params
        .get("table")
        .and_then(Value::as_str)
        .filter(|table| !table.is_empty());
    let mut answer = json!({ "tables": tables });
    if let Some(sql) = sql {
        if sql.chars().count() > MAX_SQL_CHARS {
            return Err("The query is too long.".to_string());
        }
        let page = run_page(&connection, sql, &[], offset, limit)?;
        merge(&mut answer, page);
    } else if let Some(table) = table {
        if !tables
            .iter()
            .any(|entry| entry["name"].as_str() == Some(table))
        {
            return Err(format!("There is no table named {table}."));
        }
        let quoted = quote_identifier(table);
        let page = run_page(
            &connection,
            &format!("SELECT * FROM {quoted} LIMIT ?1 OFFSET ?2"),
            &[limit as i64 + 1, offset as i64],
            0,
            limit,
        )?;
        merge(&mut answer, page);
        // Counting a very large table can be slow; past the time limit the count is left out.
        if let Ok(count) =
            connection.query_row(&format!("SELECT count(*) FROM {quoted}"), [], |row| {
                row.get::<_, i64>(0)
            })
        {
            answer["totalRows"] = json!(count);
        }
    }
    answer["offset"] = json!(offset);
    answer["limit"] = json!(limit);
    Ok(answer)
}

fn open_read_only(file: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        file,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("This database can't be opened: {error}"))?;
    let unguarded = |error: rusqlite::Error| format!("This database can't be opened: {error}");
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(unguarded)?;
    let _ = connection.busy_timeout(BUSY_TIMEOUT);
    let started = Instant::now();
    connection
        .progress_handler(10_000, Some(move || started.elapsed() > READ_TIME_LIMIT))
        .map_err(unguarded)?;
    // Reading the schema is the first thing that fails on a file that is not a database.
    connection
        .query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(()))
        .map_err(|error| match error.sqlite_error_code() {
            Some(rusqlite::ErrorCode::NotADatabase) => {
                "This file is not a SQLite database.".to_string()
            }
            _ => format!("This database can't be read: {error}"),
        })?;
    Ok(connection)
}

/// The database's tables and views, internal `sqlite_` tables left out.
fn list_tables(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name, type FROM sqlite_schema WHERE type IN ('table', 'view') \
             AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY type, name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "name": row.get::<_, String>(0)?,
                "kind": row.get::<_, String>(1)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// Runs `sql` (exactly one read-only statement), skips `offset` rows and returns the next
/// `limit`, with `hasMore` when another row follows.
fn run_page(
    connection: &Connection,
    sql: &str,
    params: &[i64],
    offset: u64,
    limit: u64,
) -> Result<Value, String> {
    use rusqlite::fallible_iterator::FallibleIterator as _;
    let mut statements = rusqlite::Batch::new(connection, sql);
    let mut statement = statements
        .next()
        .map_err(|error| prepare_error(&error))?
        .ok_or_else(|| "Type a query to run.".to_string())?;
    if statements
        .next()
        .map_err(|error| error.to_string())?
        .is_some()
    {
        return Err("Run one statement at a time.".to_string());
    }
    if !statement.readonly() {
        return Err("Files opens databases read-only: only queries that read can run.".to_string());
    }
    let columns: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut rows = statement
        .query(rusqlite::params_from_iter(params.iter()))
        .map_err(|error| read_error(&error))?;
    let mut out = Vec::new();
    let mut seen = 0u64;
    let mut has_more = false;
    while let Some(row) = rows.next().map_err(|error| read_error(&error))? {
        if seen < offset {
            seen += 1;
            continue;
        }
        if out.len() as u64 >= limit {
            has_more = true;
            break;
        }
        out.push(Value::Array(
            (0..columns.len())
                .map(|index| Value::String(row.get_ref(index).map(cell_text).unwrap_or_default()))
                .collect(),
        ));
    }
    Ok(json!({ "columns": columns, "rows": out, "hasMore": has_more }))
}

fn prepare_error(error: &rusqlite::Error) -> String {
    let text = error.to_string();
    if text.contains("too many attached databases") {
        "Files can't attach other databases.".to_string()
    } else {
        text
    }
}

fn read_error(error: &rusqlite::Error) -> String {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::OperationInterrupted) => {
            "The query took too long and was stopped.".to_string()
        }
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            "The database is busy: another app is writing to it. Try again.".to_string()
        }
        _ => error.to_string(),
    }
}

fn cell_text(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            if text.chars().count() > MAX_CELL_CHARS {
                let cut: String = text.chars().take(MAX_CELL_CHARS).collect();
                format!("{cut}…")
            } else {
                text.into_owned()
            }
        }
        ValueRef::Blob(bytes) => format!("BLOB ({} bytes)", bytes.len()),
    }
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn merge(into: &mut Value, from: Value) {
    if let (Some(into), Value::Object(from)) = (into.as_object_mut(), from) {
        into.extend(from);
    }
}
