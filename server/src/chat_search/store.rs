//! The chat search database: its own SQLite file in gxserver's state folder, so the index can be
//! dropped and rebuilt at any time and its writes never contend with `state.db`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};

use crate::paths::GxserverPaths;

/// Bumped when what the index stores changes; a file of another version is deleted and rebuilt.
const SCHEMA_VERSION: i64 = 2;
const FILE_NAME: &str = "chat-search.sqlite3";

pub(super) fn database_file(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join(FILE_NAME)
}

/// Opens the index at `file` for queries; the indexer creates its tables.
pub(super) fn open_reader(file: &Path) -> rusqlite::Result<Connection> {
    open_raw(file)
}

/// Opens (creating or rebuilding when its version is old) the index at `file`, for the indexer.
pub(super) fn open(file: &Path) -> rusqlite::Result<Connection> {
    let db = open_raw(file)?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == SCHEMA_VERSION {
        return Ok(db);
    }
    if version != 0 {
        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", file.display()));
        }
        let db = open_raw(file)?;
        create(&db)?;
        return Ok(db);
    }
    create(&db)?;
    Ok(db)
}

fn open_raw(file: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = Connection::open(file)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;",
    )?;
    Ok(db)
}

/// CDXC:Sessions 2026-11-27 WHY:
/// A trigram FTS5 index matches any 3+ character piece of a word (`sear` finds "research") in
/// either case and with or without accents, without scanning messages, which is what makes search
/// as you type fast on every chat. It keeps no positions (`detail = none`): with them it was about
/// 4.7 times the indexed words, about a gigabyte for a heavy user. Without them it cannot match a
/// phrase, so a query asks for a word's trigrams and checks the candidates' words itself
/// (query.rs). It is contentless (`content = ''`): the words live once, in `messages`, which
/// builds the snippet; `contentless_delete` lets a rewritten transcript drop its rows. Keyed by
/// transcript file, not session, because a restored or reopened session reads the same file as
/// the one it came from, and each file is indexed once.
fn create(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(&format!(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS transcripts (
           id INTEGER PRIMARY KEY,
           path TEXT NOT NULL UNIQUE,
           agent TEXT NOT NULL,
           identity TEXT NOT NULL DEFAULT '',
           readTo INTEGER NOT NULL DEFAULT 0,
           boundary TEXT NOT NULL DEFAULT '',
           size INTEGER NOT NULL DEFAULT 0,
           mtimeMs INTEGER NOT NULL DEFAULT 0,
           lastMessageAtMs INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS sessions (
           projectId TEXT NOT NULL,
           sessionId TEXT NOT NULL,
           updatedAt TEXT NOT NULL,
           transcriptId INTEGER,
           PRIMARY KEY (projectId, sessionId)
         );
         CREATE INDEX IF NOT EXISTS sessions_transcript ON sessions(transcriptId);
         CREATE TABLE IF NOT EXISTS messages (
           rowid INTEGER PRIMARY KEY,
           transcriptId INTEGER NOT NULL,
           messageId TEXT NOT NULL,
           role INTEGER NOT NULL,
           atMs INTEGER NOT NULL DEFAULT 0,
           text TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS messages_transcript ON messages(transcriptId, messageId);
         CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
           text,
           content = '',
           contentless_delete = 1,
           detail = none,
           tokenize = 'trigram remove_diacritics 1'
         );
         PRAGMA user_version = {SCHEMA_VERSION};
         COMMIT;"
    ))
}

/// One transcript file and how far the index has read it.
#[derive(Clone, Debug, Default)]
pub(super) struct TranscriptRow {
    pub identity: String,
    pub read_to: u64,
    pub boundary: String,
    pub size: u64,
    pub mtime_ms: i64,
}

pub(super) fn transcript(db: &Connection, path: &str) -> rusqlite::Result<Option<TranscriptRow>> {
    db.query_row(
        "SELECT identity, readTo, boundary, size, mtimeMs FROM transcripts WHERE path = ?1",
        [path],
        |row| {
            Ok(TranscriptRow {
                identity: row.get(0)?,
                read_to: row.get::<_, i64>(1)?.max(0) as u64,
                boundary: row.get(2)?,
                size: row.get::<_, i64>(3)?.max(0) as u64,
                mtime_ms: row.get(4)?,
            })
        },
    )
    .optional()
}

pub(super) fn insert_transcript(db: &Connection, path: &str, agent: &str) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO transcripts (path, agent) VALUES (?1, ?2)
         ON CONFLICT(path) DO UPDATE SET agent = excluded.agent",
        params![path, agent],
    )?;
    db.query_row(
        "SELECT id FROM transcripts WHERE path = ?1",
        [path],
        |row| row.get(0),
    )
}

/// Drops every message of a transcript (it was rewritten, or no session reads it any more).
pub(super) fn clear_messages(db: &Connection, transcript_id: i64) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM messages_fts WHERE rowid IN (SELECT rowid FROM messages WHERE transcriptId = ?1)",
        [transcript_id],
    )?;
    db.execute(
        "DELETE FROM messages WHERE transcriptId = ?1",
        [transcript_id],
    )?;
    Ok(())
}

pub(super) fn insert_message(
    db: &Connection,
    transcript_id: i64,
    message_id: &str,
    role: i64,
    at_ms: i64,
    text: &str,
) -> rusqlite::Result<()> {
    // A row the agent rewrote under its id (Codex's growing thoughts) keeps only its last words.
    let earlier: Option<i64> = db
        .prepare_cached("SELECT rowid FROM messages WHERE transcriptId = ?1 AND messageId = ?2")?
        .query_row(params![transcript_id, message_id], |row| row.get(0))
        .optional()?;
    if let Some(rowid) = earlier {
        db.prepare_cached("DELETE FROM messages_fts WHERE rowid = ?1")?
            .execute([rowid])?;
        db.prepare_cached("DELETE FROM messages WHERE rowid = ?1")?
            .execute([rowid])?;
    }
    db.prepare_cached(
        "INSERT INTO messages (transcriptId, messageId, role, atMs, text) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?
    .execute(params![transcript_id, message_id, role, at_ms, text])?;
    let rowid = db.last_insert_rowid();
    db.prepare_cached("INSERT INTO messages_fts (rowid, text) VALUES (?1, ?2)")?
        .execute(params![rowid, text])?;
    Ok(())
}

pub(super) fn save_progress(
    db: &Connection,
    transcript_id: i64,
    row: &TranscriptRow,
    last_message_at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE transcripts SET identity = ?2, readTo = ?3, boundary = ?4, size = ?5, mtimeMs = ?6,
           lastMessageAtMs = max(lastMessageAtMs, ?7)
         WHERE id = ?1",
        params![
            transcript_id,
            row.identity,
            row.read_to as i64,
            row.boundary,
            row.size as i64,
            row.mtime_ms,
            last_message_at_ms,
        ],
    )?;
    Ok(())
}

/// Transcripts no session reads any more (the sessions were deleted), with their messages.
pub(super) fn drop_orphans(db: &Connection) -> rusqlite::Result<usize> {
    let orphans: Vec<i64> = db
        .prepare(
            "SELECT id FROM transcripts
             WHERE id NOT IN (SELECT transcriptId FROM sessions WHERE transcriptId IS NOT NULL)",
        )?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in &orphans {
        clear_messages(db, *id)?;
        db.execute("DELETE FROM transcripts WHERE id = ?1", [id])?;
    }
    Ok(orphans.len())
}
