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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::agent::{ALL_AGENTS, Agent};
use crate::favorites;
use crate::scan::answers::{
    TranscriptSource, discover_transcript_sources, extract_answers_from_file,
};
use crate::scan::{Meta, Record, RecordKind};

const SEARCH_DB_FILE_NAME: &str = "search_index.sqlite3";
const SCHEMA_VERSION: i64 = 1;

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

pub struct SearchStore {
    conn: Connection,
    db_path: PathBuf,
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

fn configure_and_init(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    let user_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if user_version == 0 {
        let _ = conn.pragma_update(None, "auto_vacuum", "INCREMENTAL");
        conn.execute_batch(SCHEMA_SQL)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    } else if user_version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}

impl SearchStore {
    pub fn db_path_for(cache_root: &Path) -> PathBuf {
        cache_root.join(SEARCH_DB_FILE_NAME)
    }

    /// Open (or cleanly recreate if corrupted/outdated) the rebuildable search store.
    pub fn open(cache_root: &Path) -> Option<Self> {
        let _ = fs::create_dir_all(cache_root);
        let db_path = Self::db_path_for(cache_root);
        if let Ok(conn) = Connection::open(&db_path) {
            if configure_and_init(&conn).is_ok() {
                return Some(Self { conn, db_path });
            }
        }
        remove_db_files(&db_path);
        let conn = Connection::open(&db_path).ok()?;
        configure_and_init(&conn).ok()?;
        Some(Self { conn, db_path })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Upsert user prompt records, skipping rows whose key and metadata are unchanged.
    pub fn sync_user_records(&mut self, records: &[Record]) -> usize {
        if records.is_empty() {
            return 0;
        }
        let Ok(tx) = self.conn.transaction() else {
            return 0;
        };
        let mut written = 0usize;
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
                return 0;
            };
            for rec in records {
                let item_key = format!(
                    "{:016x}",
                    favorites::key_for_kind(rec.agent.label(), rec.kind, &rec.text)
                );
                let meta_json =
                    serde_json::to_string(&rec.meta).unwrap_or_else(|_| "{}".to_string());
                if let Ok(changed) = stmt.execute(params![
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
                    written += changed;
                }
            }
        }
        let _ = tx.commit();
        written
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
        let Ok(tx) = self.conn.transaction() else {
            return 0;
        };
        let _ = tx.execute(
            "DELETE FROM search_items WHERE source_path = ?1 AND kind = 'agent_answer'",
            [&path_str],
        );
        let mut written = 0usize;
        {
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
                "#,
            ) else {
                return 0;
            };
            for rec in answers {
                let item_key = format!(
                    "{:016x}",
                    favorites::key_for_kind(rec.agent.label(), RecordKind::AgentAnswer, &rec.text)
                );
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
        let _ = tx.commit();
        written
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

    /// Build a safe FTS5 trigram query expression for queries where every token is >= 3 chars.
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

    /// Pull candidate records using FTS5 trigram search (for >= 3-char tokens) plus
    /// recent-row fallback so fuzzy subsequence matching stays identical to Find.
    pub fn query_candidates(
        &self,
        query: &str,
        kinds: &[RecordKind],
        agents: &[Agent],
        project: Option<&str>,
        max_candidates: usize,
    ) -> Vec<Record> {
        let mut results = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();

        let effective_kinds: Vec<&str> = if kinds.is_empty() {
            vec![RecordKind::User.as_str(), RecordKind::AgentAnswer.as_str()]
        } else {
            kinds.iter().map(|k| k.as_str()).collect()
        };

        let build_filter_clause =
            |params: &mut Vec<rusqlite::types::Value>, prefix: &str| -> String {
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
            };

        let trimmed = query.trim();
        if let Some(fts_expr) = Self::fts5_trigram_query(trimmed) {
            let mut params = vec![rusqlite::types::Value::Text(fts_expr)];
            let where_clause = build_filter_clause(&mut params, "s.");
            params.push(rusqlite::types::Value::Integer(max_candidates as i64));
            let sql = format!(
                "SELECT s.agent, s.kind, s.session_id, s.project, s.title, s.text, s.ts, s.interrupted, s.meta_json, s.item_key \
                 FROM search_items_fts f \
                 JOIN search_items s ON s.doc_id = f.rowid \
                 WHERE search_items_fts MATCH ? AND {where_clause} \
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
        }

        // For empty queries, very short queries (<3 chars), or sparse fuzzy acronym queries,
        // pull newest filtered rows so non-contiguous fuzzy matches still work.
        let need_recent_supplement = trimmed.is_empty()
            || Self::fts5_trigram_query(trimmed).is_none()
            || results.len() < 250;
        if need_recent_supplement {
            let remaining = max_candidates.saturating_sub(results.len());
            if remaining > 0 {
                let mut params = Vec::new();
                let where_clause = build_filter_clause(&mut params, "");
                params.push(rusqlite::types::Value::Integer(remaining as i64));
                let sql = format!(
                    "SELECT agent, kind, session_id, project, title, text, ts, interrupted, meta_json, item_key \
                     FROM search_items \
                     WHERE {where_clause} \
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
        self.query_candidates("", kinds, &[], None, limit)
    }
}
