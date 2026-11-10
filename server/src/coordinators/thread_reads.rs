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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use rusqlite::{params, Connection, OptionalExtension};

use super::records::{sql_error, thread_from_row, THREAD_COLUMNS};
use super::{SessionKey, ThreadRecord};
use crate::domain::DomainStateError;

/// The short columns of a resolved thread; any change to the thread changes one of them. The
/// `rowid` breaks `createdAt` ties the way `list_threads`' single `ORDER BY createdAt` does (table
/// order), so threads started in the same millisecond keep their order when the two lists merge.
const FINGERPRINT_COLUMNS: &str = "projectId, sessionId, coordinatorProjectId, coordinatorSessionId, resolvedAt, observedWorking, reportedAt, reportedPromptKey, pendingMessageAt, closedWithCoordinatorAt, createdAt, updatedAt, rowid";

/// A thread and its `rowid`, for the merge order.
type Placed = (i64, ThreadRecord);

/// A resolved thread's key, its short columns as one text, and its `rowid`.
type Fingerprint = (SessionKey, String, i64);

#[derive(Default)]
struct ResolvedThreads {
    fingerprints: Vec<Fingerprint>,
    threads: Arc<Vec<Placed>>,
}

fn resolved_cache() -> &'static Mutex<ResolvedThreads> {
    static CACHE: OnceLock<Mutex<ResolvedThreads>> = OnceLock::new();
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
            .query_map([], |row| Ok((row.get(0)?, thread_from_row(row)?)))
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
                for index in 0..13 {
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
                Ok((
                    (row.get(0)?, row.get(1)?),
                    fingerprint,
                    row.get::<_, i64>(12)?,
                ))
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<Fingerprint>, _>>()
            .map_err(sql_error)?
    };
    let Ok(mut cache) = resolved_cache().lock() else {
        return Ok(Arc::new(read_threads_by_key(db, &fingerprints)?));
    };
    if cache.fingerprints == fingerprints {
        return Ok(cache.threads.clone());
    }
    let known: HashMap<&SessionKey, (&String, &Placed)> = cache
        .fingerprints
        .iter()
        .zip(cache.threads.iter())
        .map(|((key, fingerprint, _), placed)| (key, (fingerprint, placed)))
        .collect();
    let mut threads = Vec::with_capacity(fingerprints.len());
    for (key, fingerprint, rowid) in &fingerprints {
        match known.get(key) {
            Some((known_fingerprint, placed)) if *known_fingerprint == fingerprint => {
                threads.push((*placed).clone());
            }
            _ => {
                // A thread reopened or removed since the short read is simply left out until the
                // next tick reads it again.
                if let Some(thread) = read_thread(db, key)?.filter(ThreadRecord::is_resolved) {
                    threads.push((*rowid, thread));
                }
            }
        }
    }
    let threads = Arc::new(threads);
    // Kept only when every row was read as fingerprinted, so a skipped row is read again next tick.
    if threads.len() == fingerprints.len() {
        cache.fingerprints = fingerprints;
        cache.threads = threads.clone();
    } else {
        *cache = ResolvedThreads::default();
    }
    Ok(threads)
}

fn read_threads_by_key(
    db: &Connection,
    fingerprints: &[Fingerprint],
) -> Result<Vec<Placed>, DomainStateError> {
    let mut threads = Vec::with_capacity(fingerprints.len());
    for (key, _, rowid) in fingerprints {
        threads.extend(
            read_thread(db, key)?
                .filter(ThreadRecord::is_resolved)
                .map(|thread| (*rowid, thread)),
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
