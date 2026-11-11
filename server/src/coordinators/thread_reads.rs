//! Every coordinator thread for the panels, without reading every thread's text each tick.
//!
//! CDXC:Coordinators 2026-11-10 WHY: the supervisor's 2 second tick read all 1,104 thread rows
//! (3.9 MB of task and report text) twice, once to supervise and once for the panels: 195 of the
//! tick's 316 samples, about 5% of gxserver's busy time. Only 51 were open. The supervisor now
//! reads the open threads alone (`list_open_threads`). The panels still show resolved threads
//! (the board's done list), so those are kept in memory and read again only when one of them
//! changes, found by a query of their short columns: every write to a thread's task, report or
//! pending message also sets `updatedAt`, and the writes that do not (a restore moving threads to
//! another coordinator, clearing `closedWithCoordinatorAt`) change a short column the query reads.
//! The panels are still built after the tick's supervision, from a fresh read of the open threads,
//! so a thread's change reaches the board on the tick it happened (coordinator_runtime.rs `tick`).
//!
//! CDXC:Coordinators 2026-11-10 WHY (review): the kept threads are per database file, so two
//! databases in one process (parallel tests) never see each other's. Two text-only writes within
//! one millisecond would leave the short columns equal, so a resolved thread's text is reused only
//! once it was read at least [`SETTLE_MS`] after its own `updatedAt`: every thread write stamps the
//! current time, so no later write can carry that `updatedAt` (a clock set back could still).
//! Recently changed threads are simply read again on each tick until then. The threads are held as
//! `Arc`s that the panel's row cache shares (panel.rs `CachedRow`), which kept a full copy of every
//! thread before this cache existed, so their text is in memory once, as it was before.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use rusqlite::{params, Connection, OptionalExtension};

use super::records::{sql_error, thread_from_row, THREAD_COLUMNS};
use super::{SessionKey, ThreadRecord};
use crate::domain::DomainStateError;

/// The short columns of a resolved thread; any change to the thread changes one of them (main's
/// 0075 watch stamps included: their writers stamp `updatedAt` too, and are read here anyway). The
/// `rowid` breaks `createdAt` ties the way `list_threads`' single `ORDER BY createdAt` does (table
/// order), so threads started in the same millisecond keep their order when the two lists merge.
const FINGERPRINT_COLUMNS: &str = "projectId, sessionId, coordinatorProjectId, coordinatorSessionId, resolvedAt, observedWorking, reportedAt, reportedPromptKey, pendingMessageAt, closedWithCoordinatorAt, createdAt, updatedAt, rowid, lastResolvedAt, pendingMessageSendId";

/// A thread and its `rowid`, for the merge order.
type Placed = (i64, Arc<ThreadRecord>);

/// A resolved thread's key, its short columns as one text, its `rowid` and its `updatedAt` in ms.
type Fingerprint = (SessionKey, String, i64, Option<i64>);

/// How long after its `updatedAt` a resolved thread's text is read before it is kept.
const SETTLE_MS: i64 = 2_000;

/// Databases whose resolved threads are kept at once (tests open a few in one process).
const MAX_DATABASES: usize = 4;

#[derive(Default)]
struct ResolvedThreads {
    database: String,
    fingerprints: Vec<Fingerprint>,
    threads: Arc<Vec<Placed>>,
    /// Per thread, whether its text was read late enough to be reused (`SETTLE_MS`).
    settled: Vec<bool>,
}

fn resolved_cache() -> &'static Mutex<VecDeque<ResolvedThreads>> {
    static CACHE: OnceLock<Mutex<VecDeque<ResolvedThreads>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Every thread, oldest first, as `list_threads` returns them.
pub(crate) struct AllThreads {
    open: Vec<Placed>,
    resolved: Arc<Vec<Placed>>,
}

impl AllThreads {
    pub(crate) fn is_empty(&self) -> bool {
        self.open.is_empty() && self.resolved.is_empty()
    }

    /// Open and resolved threads merged back into `createdAt` order.
    pub(crate) fn ordered(&self) -> Vec<&ThreadRecord> {
        self.ordered_shared()
            .into_iter()
            .map(|thread| &**thread)
            .collect()
    }

    /// `ordered`, as the shared records, for a cache that keeps them (panel.rs).
    pub(crate) fn ordered_shared(&self) -> Vec<&Arc<ThreadRecord>> {
        let mut merged = Vec::with_capacity(self.open.len() + self.resolved.len());
        let (mut open, mut resolved) =
            (self.open.iter().peekable(), self.resolved.iter().peekable());
        loop {
            let next = match (open.peek(), resolved.peek()) {
                (Some(left), Some(right))
                    if (&right.1.created_at, right.0) < (&left.1.created_at, left.0) =>
                {
                    resolved.next()
                }
                (Some(_), _) => open.next(),
                (None, Some(_)) => resolved.next(),
                (None, None) => break,
            };
            merged.extend(next.map(|(_, thread)| thread));
        }
        merged
    }
}

/// Every thread: the open ones read now, the resolved ones from memory unless one changed.
pub(crate) fn read_all_threads(db: &Connection) -> Result<AllThreads, DomainStateError> {
    // One read transaction, so a thread resolved between the two reads is neither in both nor in
    // neither; a caller already inside a transaction reads within it.
    let snapshot = if db.is_autocommit() {
        Some(db.unchecked_transaction().map_err(sql_error)?)
    } else {
        None
    };
    let open = {
        let mut statement = db
            .prepare_cached(&format!(
                "SELECT rowid, {THREAD_COLUMNS} FROM coordinator_threads WHERE resolvedAt IS NULL ORDER BY createdAt, rowid"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, Arc::new(thread_from_row(row)?))))
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<Placed>, _>>()
            .map_err(sql_error)?
    };
    let resolved = read_resolved_threads(db)?;
    drop(snapshot);
    Ok(AllThreads { open, resolved })
}

fn read_resolved_threads(db: &Connection) -> Result<Arc<Vec<Placed>>, DomainStateError> {
    let fingerprints = {
        let mut statement = db
            .prepare_cached(&format!(
                "SELECT {FINGERPRINT_COLUMNS} FROM coordinator_threads WHERE resolvedAt IS NOT NULL ORDER BY createdAt, rowid"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                let mut fingerprint = String::new();
                for index in 0..15 {
                    match row.get_ref(index)? {
                        rusqlite::types::ValueRef::Null => fingerprint.push('\u{0}'),
                        rusqlite::types::ValueRef::Integer(value) => {
                            fingerprint.push_str(&value.to_string())
                        }
                        rusqlite::types::ValueRef::Real(value) => {
                            fingerprint.push_str(&value.to_string())
                        }
                        rusqlite::types::ValueRef::Text(bytes)
                        | rusqlite::types::ValueRef::Blob(bytes) => {
                            fingerprint.push_str(&String::from_utf8_lossy(bytes))
                        }
                    }
                    fingerprint.push('\u{1f}');
                }
                let updated_at = row
                    .get::<_, Option<String>>(11)?
                    .and_then(|text| chrono::DateTime::parse_from_rfc3339(&text).ok())
                    .map(|time| time.timestamp_millis());
                Ok((
                    (row.get(0)?, row.get(1)?),
                    fingerprint,
                    row.get::<_, i64>(12)?,
                    updated_at,
                ))
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<Fingerprint>, _>>()
            .map_err(sql_error)?
    };
    // An in-memory database has no file to key it by, and is read whole each time.
    let Some(database) = db
        .path()
        .filter(|path| !path.is_empty())
        .map(str::to_string)
    else {
        return Ok(Arc::new(read_threads_by_key(db, &fingerprints)?));
    };
    let Ok(mut caches) = resolved_cache().lock() else {
        return Ok(Arc::new(read_threads_by_key(db, &fingerprints)?));
    };
    let cache = match caches.iter().position(|entry| entry.database == database) {
        Some(index) => caches.remove(index).unwrap_or_default(),
        None => ResolvedThreads {
            database: database.clone(),
            ..Default::default()
        },
    };
    if cache.fingerprints == fingerprints && cache.settled.iter().all(|settled| *settled) {
        let threads = cache.threads.clone();
        caches.push_back(cache);
        return Ok(threads);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let known: HashMap<&SessionKey, (&String, &Placed)> = cache
        .fingerprints
        .iter()
        .zip(cache.threads.iter())
        .zip(cache.settled.iter())
        .filter(|(_, settled)| **settled)
        .map(|(((key, fingerprint, _, _), placed), _)| (key, (fingerprint, placed)))
        .collect();
    let mut threads = Vec::with_capacity(fingerprints.len());
    let mut settled = Vec::with_capacity(fingerprints.len());
    for (key, fingerprint, rowid, updated_at) in &fingerprints {
        match known.get(key) {
            Some((known_fingerprint, placed)) if *known_fingerprint == fingerprint => {
                threads.push((*placed).clone());
                settled.push(true);
            }
            _ => {
                // A thread reopened or removed since the short read is simply left out until the
                // next tick reads it again.
                if let Some(thread) = read_thread(db, key)?.filter(ThreadRecord::is_resolved) {
                    threads.push((*rowid, Arc::new(thread)));
                    settled.push(updated_at.is_some_and(|at| now - at >= SETTLE_MS));
                }
            }
        }
    }
    let threads = Arc::new(threads);
    // Kept only when every row was read as fingerprinted, so a skipped row is read again next tick.
    if threads.len() == fingerprints.len() {
        if caches.len() >= MAX_DATABASES {
            caches.pop_front();
        }
        caches.push_back(ResolvedThreads {
            database,
            fingerprints,
            threads: threads.clone(),
            settled,
        });
    }
    Ok(threads)
}

fn read_threads_by_key(
    db: &Connection,
    fingerprints: &[Fingerprint],
) -> Result<Vec<Placed>, DomainStateError> {
    let mut threads = Vec::with_capacity(fingerprints.len());
    for (key, _, rowid, _) in fingerprints {
        threads.extend(
            read_thread(db, key)?
                .filter(ThreadRecord::is_resolved)
                .map(|thread| (*rowid, Arc::new(thread))),
        );
    }
    Ok(threads)
}

fn read_thread(
    db: &Connection,
    key: &SessionKey,
) -> Result<Option<ThreadRecord>, DomainStateError> {
    db.prepare_cached(&format!(
        "SELECT {THREAD_COLUMNS} FROM coordinator_threads WHERE projectId = ?1 AND sessionId = ?2"
    ))
    .map_err(sql_error)?
    .query_row(params![key.0, key.1], thread_from_row)
    .optional()
    .map_err(sql_error)
}
