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

use super::records::{list_open_threads, sql_error, thread_from_row, THREAD_COLUMNS};
use super::{SessionKey, ThreadRecord};
use crate::domain::DomainStateError;

/// The short columns of a resolved thread; any change to the thread changes one of them.
const FINGERPRINT_COLUMNS: &str = "projectId, sessionId, coordinatorProjectId, coordinatorSessionId, resolvedAt, observedWorking, reportedAt, reportedPromptKey, pendingMessageAt, closedWithCoordinatorAt, createdAt, updatedAt";

#[derive(Default)]
struct ResolvedThreads {
    fingerprints: Vec<(SessionKey, String)>,
    threads: Arc<Vec<ThreadRecord>>,
}

fn resolved_cache() -> &'static Mutex<ResolvedThreads> {
    static CACHE: OnceLock<Mutex<ResolvedThreads>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Every thread, oldest first, as `list_threads` returns them.
pub(crate) struct AllThreads {
    open: Vec<ThreadRecord>,
    resolved: Arc<Vec<ThreadRecord>>,
}

impl AllThreads {
    /// Open and resolved threads merged back into `createdAt` order.
    pub(crate) fn ordered(&self) -> Vec<&ThreadRecord> {
        let mut merged = Vec::with_capacity(self.open.len() + self.resolved.len());
        let (mut open, mut resolved) =
            (self.open.iter().peekable(), self.resolved.iter().peekable());
        loop {
            let next = match (open.peek(), resolved.peek()) {
                (Some(left), Some(right)) if right.created_at < left.created_at => resolved.next(),
                (Some(_), _) => open.next(),
                (None, Some(_)) => resolved.next(),
                (None, None) => break,
            };
            merged.extend(next);
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
    let open = list_open_threads(db)?;
    let resolved = read_resolved_threads(db)?;
    drop(snapshot);
    Ok(AllThreads { open, resolved })
}

fn read_resolved_threads(db: &Connection) -> Result<Arc<Vec<ThreadRecord>>, DomainStateError> {
    let fingerprints = {
        let mut statement = db
            .prepare_cached(&format!(
                "SELECT {FINGERPRINT_COLUMNS} FROM coordinator_threads WHERE resolvedAt IS NOT NULL ORDER BY createdAt"
            ))
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                let mut fingerprint = String::new();
                for index in 0..12 {
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
                Ok(((row.get(0)?, row.get(1)?), fingerprint))
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<(SessionKey, String)>, _>>()
            .map_err(sql_error)?
    };
    let Ok(mut cache) = resolved_cache().lock() else {
        return Ok(Arc::new(read_threads_by_key(db, &fingerprints)?));
    };
    if cache.fingerprints == fingerprints {
        return Ok(cache.threads.clone());
    }
    let known: HashMap<&SessionKey, (&String, &ThreadRecord)> = cache
        .fingerprints
        .iter()
        .zip(cache.threads.iter())
        .map(|((key, fingerprint), thread)| (key, (fingerprint, thread)))
        .collect();
    let mut threads = Vec::with_capacity(fingerprints.len());
    for (key, fingerprint) in &fingerprints {
        match known.get(key) {
            Some((known_fingerprint, thread)) if *known_fingerprint == fingerprint => {
                threads.push((*thread).clone());
            }
            _ => {
                // A thread reopened or removed since the short read is simply left out until the
                // next tick reads it again.
                if let Some(thread) = read_thread(db, key)?.filter(ThreadRecord::is_resolved) {
                    threads.push(thread);
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
    fingerprints: &[(SessionKey, String)],
) -> Result<Vec<ThreadRecord>, DomainStateError> {
    let mut threads = Vec::with_capacity(fingerprints.len());
    for (key, _) in fingerprints {
        threads.extend(read_thread(db, key)?.filter(ThreadRecord::is_resolved));
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
