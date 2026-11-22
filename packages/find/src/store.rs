//! Rebuildable SQLite + FTS5 trigram search store for Find.
//!
//! CDXC:PromptSearch 2026-11-22 WHY:
//! Keeping search rows in a separate, rebuildable SQLite file under Ghostex's
//! cache directory eliminates the 90-second full history rescan and lets Find
//! index both user prompts and turn-ending agent answers across every agent
//! session (including sessions started outside Ghostex). Because the database
//! is purely derived state, deleting it or going back to an older Ghostex
//! release is always safe and simply triggers an incremental catch-up pass.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{
    Connection, ErrorCode, OptionalExtension, TransactionBehavior, params, params_from_iter,
};

use crate::agent::{ALL_AGENTS, Agent};
use crate::favorites;
use crate::scan::answers::{
    TranscriptSource, discover_transcript_sources, extract_answers_from_file,
};
use crate::scan::{Meta, Record, RecordKind};

const SEARCH_DB_FILE_NAME: &str = "search_index.sqlite3";
const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT_SECS: u64 = 10;
const USER_SYNC_BATCH_SIZE: usize = 500;
const MAX_SUBSEQUENCE_LIKE_CHARS: usize = 16;

/// Serializes in-process SQLite writers (foreground user sync batches,
/// foreground/background answer sync, debounced nudges, and schema init/recreate)
/// so concurrent daemon threads never fight over SQLite's single writer lock.
static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn write_lock() -> MutexGuard<'static, ()> {
    WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS indexed_files (
    path TEXT PRIMARY KEY,
    agent TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_items (
    doc_id INTEGER PRIMARY KEY,
    item_key TEXT NOT NULL UNIQUE,
    agent TEXT NOT NULL,
    kind TEXT NOT NULL,
    session_id TEXT NOT NULL,
    project TEXT NOT NULL,
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    ts INTEGER NOT NULL,
    interrupted INTEGER NOT NULL DEFAULT 0,
    meta_json TEXT NOT NULL,
    source_path TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_search_items_kind_ts
    ON search_items(kind, ts DESC);
CREATE INDEX IF NOT EXISTS idx_search_items_agent_kind_ts
    ON search_items(agent, kind, ts DESC);
CREATE INDEX IF NOT EXISTS idx_search_items_source_path
    ON search_items(source_path);

CREATE VIRTUAL TABLE IF NOT EXISTS search_items_fts USING fts5(
    text,
    title,
    content='search_items',
    content_rowid='doc_id',
    tokenize='trigram'
);

CREATE TRIGGER IF NOT EXISTS search_items_ai AFTER INSERT ON search_items BEGIN
    INSERT INTO search_items_fts(rowid, text, title)
    VALUES (new.doc_id, new.text, new.title);
END;

CREATE TRIGGER IF NOT EXISTS search_items_ad AFTER DELETE ON search_items BEGIN
    INSERT INTO search_items_fts(search_items_fts, rowid, text, title)
    VALUES ('delete', old.doc_id, old.text, old.title);
END;

CREATE TRIGGER IF NOT EXISTS search_items_au AFTER UPDATE ON search_items BEGIN
    INSERT INTO search_items_fts(search_items_fts, rowid, text, title)
    VALUES ('delete', old.doc_id, old.text, old.title);
    INSERT INTO search_items_fts(rowid, text, title)
    VALUES (new.doc_id, new.text, new.title);
END;
"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct AnswerSyncStats {
    pub files_indexed: usize,
    pub answers_written: usize,
    pub remaining_files: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserSyncStats {
    pub written: usize,
    pub complete: bool,
}

#[derive(Clone, Debug, Default)]
pub struct EmptyPageResult {
    pub matched: usize,
    pub records: Vec<Record>,
}

pub struct SearchStore {
    conn: Connection,
    db_path: PathBuf,
}

enum InitError {
    SchemaVersionMismatch,
    Corrupt,
    Other,
}

fn is_corruption_error(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if matches!(e.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

impl From<rusqlite::Error> for InitError {
    fn from(err: rusqlite::Error) -> Self {
        if is_corruption_error(&err) {
            Self::Corrupt
        } else {
            Self::Other
        }
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn remove_db_files(path: &Path) {
    let _ = fs::remove_file(path);
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let shm = PathBuf::from(format!("{}-shm", path.display()));
    let _ = fs::remove_file(wal);
    let _ = fs::remove_file(shm);
}

fn configure_and_init(conn: &mut Connection) -> Result<(), InitError> {
    conn.busy_timeout(Duration::from_secs(BUSY_TIMEOUT_SECS))?;
    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        conn.pragma_update(None, "journal_mode", "WAL")?;
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    let user_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version == SCHEMA_VERSION {
        return Ok(());
    }
    if user_version != 0 {
        return Err(InitError::SchemaVersionMismatch);
    }
    let _ = conn.pragma_update(None, "auto_vacuum", "INCREMENTAL");
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let tx_version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if tx_version == 0 {
        tx.execute_batch(SCHEMA_SQL)?;
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    } else if tx_version != SCHEMA_VERSION {
        return Err(InitError::SchemaVersionMismatch);
    }
    tx.commit()?;
    Ok(())
}

impl SearchStore {
    pub fn db_path_for(cache_root: &Path) -> PathBuf {
        cache_root.join(SEARCH_DB_FILE_NAME)
    }

    pub fn exists(cache_root: &Path) -> bool {
        Self::db_path_for(cache_root).is_file()
    }

    /// Open (or cleanly recreate if corrupted/outdated) the rebuildable search store.
    /// Never unlinks the database on transient lock/busy errors.
    pub fn open(cache_root: &Path) -> Option<Self> {
        let _ = fs::create_dir_all(cache_root);
        let db_path = Self::db_path_for(cache_root);
        let _guard = write_lock();
        let should_recreate = match Connection::open(&db_path) {
            Ok(mut conn) => match configure_and_init(&mut conn) {
                Ok(()) => return Some(Self { conn, db_path }),
                Err(InitError::Corrupt | InitError::SchemaVersionMismatch) => true,
                Err(InitError::Other) => return None,
            },
            Err(err) if is_corruption_error(&err) => true,
            Err(_) => return None,
        };
        if !should_recreate {
            return None;
        }
        remove_db_files(&db_path);
        let mut conn = Connection::open(&db_path).ok()?;
        configure_and_init(&mut conn).ok()?;
        Some(Self { conn, db_path })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Upsert user prompt records in bounded transactions so writers and readers
    /// never block for seconds, skipping rows whose key and metadata are unchanged.
    pub fn sync_user_records(&mut self, records: &[Record]) -> UserSyncStats {
        if records.is_empty() {
            return UserSyncStats {
                written: 0,
                complete: true,
            };
        }
        let mut written = 0usize;
        let mut complete = true;
        for chunk in records.chunks(USER_SYNC_BATCH_SIZE) {
            let _guard = write_lock();
            let Ok(tx) = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
            else {
                complete = false;
                break;
            };
            let mut batch_written = 0usize;
            {
                let Ok(mut stmt) = tx.prepare_cached(
                    r#"
                    INSERT INTO search_items (
                        item_key, agent, kind, session_id, project, title, text, ts, interrupted, meta_json, source_path
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    ON CONFLICT(item_key) DO UPDATE SET
                        session_id = CASE WHEN excluded.session_id <> '' THEN excluded.session_id ELSE search_items.session_id END,
                        project = CASE WHEN excluded.project <> '' THEN excluded.project ELSE search_items.project END,
                        title = CASE WHEN excluded.title <> '' THEN excluded.title ELSE search_items.title END,
                        ts = MAX(search_items.ts, excluded.ts),
                        meta_json = excluded.meta_json
                    WHERE
                        excluded.ts > search_items.ts
                        OR (excluded.title <> '' AND excluded.title <> search_items.title)
                        OR (excluded.project <> '' AND excluded.project <> search_items.project)
                        OR (excluded.session_id <> '' AND excluded.session_id <> search_items.session_id)
                    "#,
                ) else {
                    complete = false;
                    break;
                };
                for rec in chunk {
                    let item_key = format!(
                        "{:016x}",
                        favorites::key_for_kind(rec.agent.label(), rec.kind, &rec.text)
                    );
                    let meta_json =
                        serde_json::to_string(&rec.meta).unwrap_or_else(|_| "{}".to_string());
                    match stmt.execute(params![
                        item_key,
                        rec.agent.label(),
                        rec.kind.as_str(),
                        rec.session,
                        rec.project,
                        rec.title,
                        rec.text,
                        rec.ts,
                        i32::from(rec.interrupted),
                        meta_json,
                        "",
                    ]) {
                        Ok(changed) => batch_written += changed,
                        Err(_) => complete = false,
                    }
                }
            }
            if tx.commit().is_ok() {
                written += batch_written;
            } else {
                complete = false;
            }
        }
        UserSyncStats { written, complete }
    }

    fn file_is_current(&self, path_str: &str, size: u64, mtime_ns: i64) -> bool {
        self.conn
            .query_row(
                "SELECT size, mtime_ns FROM indexed_files WHERE path = ?1",
                [path_str],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .ok()
            .flatten()
            .is_some_and(|(s, m)| s == size as i64 && m == mtime_ns)
    }

    fn write_source_answers(&mut self, source: &TranscriptSource, answers: &[Record]) -> usize {
        let path_str = source.path.to_string_lossy().to_string();
        let now = now_unix();
        let prepared_answers: Vec<(String, &Record)> = answers
            .iter()
            .map(|rec| {
                let item_key = format!(
                    "{:016x}",
                    favorites::key_for_kind(rec.agent.label(), RecordKind::AgentAnswer, &rec.text)
                );
                (item_key, rec)
            })
            .collect();
        let new_keys: HashSet<&str> = prepared_answers
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();

        let _guard = write_lock();
        let Ok(tx) = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
        else {
            return 0;
        };
        if new_keys.is_empty() {
            let _ = tx.execute(
                "DELETE FROM search_items WHERE source_path = ?1 AND kind = 'agent_answer'",
                [&path_str],
            );
        } else {
            let mut stale_keys = Vec::new();
            if let Ok(mut select_stmt) = tx.prepare_cached(
                "SELECT item_key FROM search_items WHERE source_path = ?1 AND kind = 'agent_answer'",
            ) {
                if let Ok(rows) =
                    select_stmt.query_map([&path_str], |row| row.get::<_, String>(0))
                {
                    for key in rows.flatten() {
                        if !new_keys.contains(key.as_str()) {
                            stale_keys.push(key);
                        }
                    }
                }
            }
            if !stale_keys.is_empty() {
                if let Ok(mut delete_stmt) = tx.prepare_cached(
                    "DELETE FROM search_items WHERE source_path = ?1 AND kind = 'agent_answer' AND item_key = ?2",
                ) {
                    for key in &stale_keys {
                        let _ = delete_stmt.execute(params![&path_str, key]);
                    }
                }
            }
        }
        let mut written = 0usize;
        if !prepared_answers.is_empty() {
            let Ok(mut stmt) = tx.prepare_cached(
                r#"
                INSERT INTO search_items (
                    item_key, agent, kind, session_id, project, title, text, ts, interrupted, meta_json, source_path
                ) VALUES (?1, ?2, 'agent_answer', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(item_key) DO UPDATE SET
                    session_id = CASE WHEN excluded.session_id <> '' THEN excluded.session_id ELSE search_items.session_id END,
                    project = CASE WHEN excluded.project <> '' THEN excluded.project ELSE search_items.project END,
                    title = CASE WHEN excluded.title <> '' THEN excluded.title ELSE search_items.title END,
                    ts = MAX(search_items.ts, excluded.ts),
                    interrupted = excluded.interrupted,
                    source_path = excluded.source_path
                WHERE
                    excluded.ts > search_items.ts
                    OR excluded.interrupted <> search_items.interrupted
                    OR excluded.source_path <> search_items.source_path
                    OR (excluded.title <> '' AND excluded.title <> search_items.title)
                    OR (excluded.project <> '' AND excluded.project <> search_items.project)
                    OR (excluded.session_id <> '' AND excluded.session_id <> search_items.session_id)
                "#,
            ) else {
                return 0;
            };
            for (item_key, rec) in &prepared_answers {
                let meta_json =
                    serde_json::to_string(&rec.meta).unwrap_or_else(|_| "{}".to_string());
                if let Ok(changed) = stmt.execute(params![
                    item_key,
                    rec.agent.label(),
                    rec.session,
                    rec.project,
                    rec.title,
                    rec.text,
                    rec.ts,
                    i32::from(rec.interrupted),
                    meta_json,
                    path_str,
                ]) {
                    written += changed;
                }
            }
        }
        let _ = tx.execute(
            r#"
            INSERT INTO indexed_files (path, agent, size, mtime_ns, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(path) DO UPDATE SET
                agent = excluded.agent,
                size = excluded.size,
                mtime_ns = excluded.mtime_ns,
                updated_at = excluded.updated_at
            "#,
            params![
                path_str,
                source.agent.label(),
                source.size as i64,
                source.mtime_ns,
                now,
            ],
        );
        if tx.commit().is_ok() { written } else { 0 }
    }

    /// Incrementally index newest changed transcript files within `max_files` and `max_duration`.
    pub fn sync_answer_sources_budgeted(
        &mut self,
        home: &Path,
        max_files: usize,
        max_duration: Duration,
    ) -> AnswerSyncStats {
        let started = Instant::now();
        let sources = discover_transcript_sources(home);
        let mut stats = AnswerSyncStats::default();
        let mut pending_seen = 0usize;
        for source in sources {
            let path_str = source.path.to_string_lossy();
            if self.file_is_current(&path_str, source.size, source.mtime_ns) {
                continue;
            }
            if stats.files_indexed >= max_files || started.elapsed() >= max_duration {
                pending_seen += 1;
                continue;
            }
            let answers = extract_answers_from_file(&source);
            stats.answers_written += self.write_source_answers(&source, &answers);
            stats.files_indexed += 1;
        }
        stats.remaining_files = pending_seen;
        stats
    }

    /// Immediate single-file catch-up when a session turn ends (hook or ACP).
    pub fn index_single_transcript(
        &mut self,
        agent: Agent,
        path: &Path,
        session_id: Option<&str>,
        project: Option<&str>,
        title: Option<&str>,
    ) -> usize {
        let Ok(meta) = fs::metadata(path) else {
            return 0;
        };
        if !meta.is_file() || meta.len() == 0 {
            return 0;
        }
        let mtime_sec = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
            .unwrap_or(mtime_sec.saturating_mul(1_000_000_000));
        let path_str = path.to_string_lossy();
        if self.file_is_current(&path_str, meta.len(), mtime_ns) {
            return 0;
        }
        let source = TranscriptSource {
            agent,
            path: path.to_path_buf(),
            session_id: session_id.unwrap_or_default().to_string(),
            project: project.unwrap_or_default().to_string(),
            title: title.unwrap_or_default().to_string(),
            size: meta.len(),
            mtime_ns,
            mtime_sec,
        };
        let answers = extract_answers_from_file(&source);
        self.write_source_answers(&source, &answers)
    }

    /// Build a safe FTS5 trigram query expression for queries that contain at least one >= 3-char token.
    fn fts5_trigram_query(raw: &str) -> Option<String> {
        let tokens: Vec<String> = raw
            .split_whitespace()
            .filter(|t| t.chars().count() >= 3)
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect();
        if tokens.is_empty() {
            None
        } else {
            Some(tokens.join(" "))
        }
    }

    fn escape_like_literal(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '\\' | '%' | '_' => {
                    out.push('\\');
                    out.push(ch);
                }
                other => out.push(other),
            }
        }
        out
    }

    /// Ordered literal token LIKE pattern, e.g. `"go db"` -> `"%go%db%"`.
    fn like_tokens_pattern(raw: &str) -> Option<String> {
        if raw.is_empty() || raw.contains('\0') {
            return None;
        }
        let tokens: Vec<&str> = raw.split_whitespace().collect();
        if tokens.is_empty() {
            return Some(format!("%{}%", Self::escape_like_literal(raw)));
        }
        let mut pat = String::from("%");
        for tok in tokens {
            pat.push_str(&Self::escape_like_literal(tok));
            pat.push('%');
        }
        Some(pat)
    }

    /// Character-level subsequence LIKE pattern (e.g. `"rfp"` -> `"%r%f%p%"`)
    /// matching every possible fuzzy subsequence in `fuzzy::Matcher`.
    fn like_subsequence_pattern(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.contains('\0') {
            return None;
        }
        let chars: Vec<char> = trimmed.chars().collect();
        if chars.len() < 2 || chars.len() > MAX_SUBSEQUENCE_LIKE_CHARS {
            return None;
        }
        let mut pat = String::with_capacity(chars.len() * 2 + 1);
        pat.push('%');
        for ch in chars {
            match ch {
                '\\' | '%' | '_' => {
                    pat.push('\\');
                    pat.push(ch);
                }
                other => pat.push(other),
            }
            pat.push('%');
        }
        Some(pat)
    }

    /// Subsequence LIKE patterns for short (< 3 char) tokens alongside >= 3-char FTS5 tokens.
    fn short_token_like_patterns(raw: &str) -> Vec<String> {
        raw.split_whitespace()
            .filter(|t| {
                let len = t.chars().count();
                (1..3).contains(&len) && !t.contains('\0')
            })
            .take(3)
            .map(|tok| {
                let mut pat = String::with_capacity(tok.len() * 2 + 1);
                pat.push('%');
                for ch in tok.chars() {
                    match ch {
                        '\\' | '%' | '_' => {
                            pat.push('\\');
                            pat.push(ch);
                        }
                        other => pat.push(other),
                    }
                    pat.push('%');
                }
                pat
            })
            .collect()
    }

    fn build_filter_clause(
        kinds: &[RecordKind],
        agents: &[Agent],
        project: Option<&str>,
        params: &mut Vec<rusqlite::types::Value>,
        prefix: &str,
    ) -> String {
        let effective_kinds: Vec<&str> = if kinds.is_empty() {
            vec![RecordKind::User.as_str(), RecordKind::AgentAnswer.as_str()]
        } else {
            kinds.iter().map(|k| k.as_str()).collect()
        };

        let mut clauses = Vec::new();
        let placeholders = effective_kinds
            .iter()
            .map(|k| {
                params.push(rusqlite::types::Value::Text((*k).to_string()));
                "?".to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");
        clauses.push(format!("{prefix}kind IN ({placeholders})"));
        if !agents.is_empty() {
            let agent_ph = agents
                .iter()
                .map(|a| {
                    params.push(rusqlite::types::Value::Text(a.label().to_string()));
                    "?".to_string()
                })
                .collect::<Vec<_>>()
                .join(", ");
            clauses.push(format!("{prefix}agent IN ({agent_ph})"));
        }
        if let Some(proj) = project.filter(|p| !p.is_empty()) {
            params.push(rusqlite::types::Value::Text(proj.to_string()));
            clauses.push(format!("{prefix}project = ?"));
        }
        clauses.join(" AND ")
    }

    fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<Record>> {
        let agent_str: String = row.get(0)?;
        let kind_str: String = row.get(1)?;
        let session: String = row.get(2)?;
        let project: String = row.get(3)?;
        let title: String = row.get(4)?;
        let text: String = row.get(5)?;
        let ts: i64 = row.get(6)?;
        let interrupted: i32 = row.get(7)?;
        let meta_json: String = row.get(8)?;
        let Some(agent) = Agent::parse(&agent_str) else {
            return Ok(None);
        };
        let kind = RecordKind::parse(&kind_str).unwrap_or(RecordKind::User);
        let meta: Meta = serde_json::from_str(&meta_json).unwrap_or_default();
        Ok(Some(Record {
            agent,
            kind,
            interrupted: interrupted != 0,
            title,
            text,
            project,
            session,
            ts,
            meta,
        }))
    }

    /// Direct SQL pagination for empty queries: exact filtered `COUNT(*)` and
    /// reverse-chronological `LIMIT`/`OFFSET` with no candidate cap.
    pub fn query_empty_page(
        &self,
        kinds: &[RecordKind],
        agents: &[Agent],
        project: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> EmptyPageResult {
        let mut count_params = Vec::new();
        let count_where = Self::build_filter_clause(kinds, agents, project, &mut count_params, "");
        let count_sql = format!("SELECT COUNT(*) FROM search_items WHERE {count_where}");
        let matched = self
            .conn
            .query_row(&count_sql, params_from_iter(count_params.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0)
            .max(0) as usize;

        if matched == 0 || offset >= matched {
            return EmptyPageResult {
                matched,
                records: Vec::new(),
            };
        }

        let mut page_params = Vec::new();
        let page_where = Self::build_filter_clause(kinds, agents, project, &mut page_params, "");
        let sql = if limit == 0 {
            page_params.push(rusqlite::types::Value::Integer(offset as i64));
            format!(
                "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json \
                 FROM search_items \
                 WHERE {page_where} \
                 ORDER BY ts DESC, doc_id ASC \
                 LIMIT -1 OFFSET ?"
            )
        } else {
            page_params.push(rusqlite::types::Value::Integer(limit as i64));
            page_params.push(rusqlite::types::Value::Integer(offset as i64));
            format!(
                "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json \
                 FROM search_items \
                 WHERE {page_where} \
                 ORDER BY ts DESC, doc_id ASC \
                 LIMIT ? OFFSET ?"
            )
        };

        let mut records = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(&sql) {
            if let Ok(rows) =
                stmt.query_map(params_from_iter(page_params.iter()), Self::row_to_record)
            {
                for rec in rows.flatten().flatten() {
                    records.push(rec);
                }
            }
        }
        EmptyPageResult { matched, records }
    }

    /// Pull candidate records using FTS5 trigram search (for >= 3-char tokens),
    /// full-table `LIKE` search (for 1-2 char tokens), and subsequence `LIKE`
    /// fallback so older short/fuzzy matches remain reachable across large histories.
    pub fn query_candidates(
        &self,
        query: &str,
        kinds: &[RecordKind],
        agents: &[Agent],
        project: Option<&str>,
        max_candidates: usize,
    ) -> Vec<Record> {
        if query.is_empty() {
            return self
                .query_empty_page(kinds, agents, project, 0, max_candidates)
                .records;
        }

        let mut results = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();

        let trimmed = query.trim();
        let fts_expr = Self::fts5_trigram_query(trimmed);

        if let Some(ref expr) = fts_expr {
            let mut params = vec![rusqlite::types::Value::Text(expr.clone())];
            let where_clause = Self::build_filter_clause(kinds, agents, project, &mut params, "s.");
            let short_patterns = Self::short_token_like_patterns(trimmed);
            let mut extra_likes = String::new();
            for pat in short_patterns {
                params.push(rusqlite::types::Value::Text(pat));
                extra_likes.push_str(r" AND s.text LIKE ? ESCAPE '\'");
            }
            params.push(rusqlite::types::Value::Integer(max_candidates as i64));
            let sql = format!(
                "SELECT s.agent, s.kind, s.session_id, s.project, s.title, s.text, s.ts, s.interrupted, s.meta_json, s.item_key \
                 FROM search_items_fts f \
                 JOIN search_items s ON s.doc_id = f.rowid \
                 WHERE search_items_fts MATCH ? AND {where_clause}{extra_likes} \
                 ORDER BY s.ts DESC \
                 LIMIT ?"
            );
            if let Ok(mut stmt) = self.conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map(params_from_iter(params.iter()), |row| {
                    let key: String = row.get(9)?;
                    Ok((Self::row_to_record(row)?, key))
                }) {
                    for item in rows.flatten() {
                        if let (Some(rec), key) = item {
                            if seen_keys.insert(key) {
                                results.push(rec);
                            }
                        }
                    }
                }
            }
        } else if let Some(like_pat) = Self::like_tokens_pattern(query) {
            // Short queries (< 3-char tokens, e.g. "ci", "go", "db", "a", or whitespace):
            // scan the full index with SQL LIKE rather than stopping at the newest N rows.
            let mut params = Vec::new();
            let where_clause = Self::build_filter_clause(kinds, agents, project, &mut params, "");
            params.push(rusqlite::types::Value::Text(like_pat));
            params.push(rusqlite::types::Value::Integer(max_candidates as i64));
            let sql = format!(
                "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json, item_key \
                 FROM search_items \
                 WHERE {where_clause} AND text LIKE ? ESCAPE '\\' \
                 ORDER BY ts DESC \
                 LIMIT ?"
            );
            if let Ok(mut stmt) = self.conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map(params_from_iter(params.iter()), |row| {
                    let key: String = row.get(9)?;
                    Ok((Self::row_to_record(row)?, key))
                }) {
                    for item in rows.flatten() {
                        if let (Some(rec), key) = item {
                            if seen_keys.insert(key) {
                                results.push(rec);
                            }
                        }
                    }
                }
            }
        }

        // For sparse acronym / fuzzy queries with few literal hits, pull full-table
        // subsequence LIKE matches (for <= 16-char queries) so older fuzzy hits are not lost.
        // Single-character queries have no non-literal fuzzy matches beyond Stage 2.
        let single_char_query = trimmed.chars().count() <= 1;
        if !single_char_query && results.len() < 250 {
            let remaining = max_candidates.saturating_sub(results.len());
            if remaining > 0 {
                let mut params = Vec::new();
                let where_clause =
                    Self::build_filter_clause(kinds, agents, project, &mut params, "");
                let sql = if let Some(subseq_pat) = Self::like_subsequence_pattern(trimmed) {
                    params.push(rusqlite::types::Value::Text(subseq_pat));
                    params.push(rusqlite::types::Value::Integer(remaining as i64));
                    format!(
                        "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json, item_key \
                         FROM search_items \
                         WHERE {where_clause} AND text LIKE ? ESCAPE '\\' \
                         ORDER BY ts DESC \
                         LIMIT ?"
                    )
                } else {
                    params.push(rusqlite::types::Value::Integer(remaining as i64));
                    format!(
                        "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json, item_key \
                         FROM search_items \
                         WHERE {where_clause} \
                         ORDER BY ts DESC \
                         LIMIT ?"
                    )
                };
                if let Ok(mut stmt) = self.conn.prepare(&sql) {
                    if let Ok(rows) = stmt.query_map(params_from_iter(params.iter()), |row| {
                        let key: String = row.get(9)?;
                        Ok((Self::row_to_record(row)?, key))
                    }) {
                        for item in rows.flatten() {
                            if let (Some(rec), key) = item {
                                if seen_keys.insert(key) {
                                    results.push(rec);
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    pub fn find_by_key(&self, key_hex: &str) -> Option<Record> {
        self.conn
            .query_row(
                "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json \
                 FROM search_items WHERE item_key = ?1",
                [key_hex],
                Self::row_to_record,
            )
            .optional()
            .ok()
            .flatten()
            .flatten()
    }

    pub fn total_count(&self, kinds: &[RecordKind]) -> usize {
        let effective_kinds: Vec<&str> = if kinds.is_empty() {
            vec![RecordKind::User.as_str(), RecordKind::AgentAnswer.as_str()]
        } else {
            kinds.iter().map(|k| k.as_str()).collect()
        };
        let placeholders = vec!["?"; effective_kinds.len()].join(", ");
        let sql = format!("SELECT COUNT(*) FROM search_items WHERE kind IN ({placeholders})");
        let params: Vec<rusqlite::types::Value> = effective_kinds
            .into_iter()
            .map(|k| rusqlite::types::Value::Text(k.to_string()))
            .collect();
        self.conn
            .query_row(&sql, params_from_iter(params.iter()), |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0)
            .max(0) as usize
    }

    pub fn facets(&self, kinds: &[RecordKind]) -> (Vec<String>, Vec<Agent>) {
        let effective_kinds: Vec<&str> = if kinds.is_empty() {
            vec![RecordKind::User.as_str(), RecordKind::AgentAnswer.as_str()]
        } else {
            kinds.iter().map(|k| k.as_str()).collect()
        };
        let placeholders = vec!["?"; effective_kinds.len()].join(", ");
        let params: Vec<rusqlite::types::Value> = effective_kinds
            .iter()
            .map(|k| rusqlite::types::Value::Text((*k).to_string()))
            .collect();

        let mut projects = Vec::new();
        let proj_sql = format!(
            "SELECT project FROM search_items WHERE kind IN ({placeholders}) AND project <> '' \
             GROUP BY project ORDER BY MAX(ts) DESC LIMIT 400"
        );
        if let Ok(mut stmt) = self.conn.prepare(&proj_sql) {
            if let Ok(rows) = stmt.query_map(params_from_iter(params.iter()), |row| {
                row.get::<_, String>(0)
            }) {
                projects.extend(rows.flatten());
            }
        }

        let mut present_set = HashSet::new();
        let agent_sql =
            format!("SELECT DISTINCT agent FROM search_items WHERE kind IN ({placeholders})");
        if let Ok(mut stmt) = self.conn.prepare(&agent_sql) {
            if let Ok(rows) = stmt.query_map(params_from_iter(params.iter()), |row| {
                row.get::<_, String>(0)
            }) {
                for label in rows.flatten() {
                    if let Some(agent) = Agent::parse(&label) {
                        present_set.insert(agent);
                    }
                }
            }
        }
        let present_agents = ALL_AGENTS
            .into_iter()
            .filter(|a| present_set.contains(a))
            .collect();

        (projects, present_agents)
    }

    pub fn load_recent_records(&self, kinds: &[RecordKind], limit: usize) -> Vec<Record> {
        self.query_empty_page(kinds, &[], None, 0, limit).records
    }
}
