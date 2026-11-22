use std::{
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::apply_wal_durability;

/// The busy wait every borrow gets unless it asks for its own: spent in turn (`fair_busy`). A pooled connection is reset to it on each borrow, because a caller may have changed it for its own statements.
pub(super) const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

/// Open connections kept for reuse. Further concurrent borrowers open their own connection, which is closed when they drop it.
const POOLED_CONNECTIONS_MAX_IDLE: usize = 6;

/// Room for every hot statement the repositories prepare with `prepare_cached`.
const POOLED_STATEMENT_CACHE_CAPACITY: usize = 64;

struct ConnectionPool {
    path: PathBuf,
    idle: Vec<Connection>,
}

/// One pool, for the state.db this process opened last: gxserver and the CLI only ever open one, and a test that moves to a new temporary database simply replaces it.
static POOL: Mutex<Option<ConnectionPool>> = Mutex::new(None);

/// A state.db connection borrowed from the pool. It derefs to `Connection`, so it is used exactly like one, and goes back to the pool when dropped.
///
/// CDXC:ServerDaemon 2026-10-23 WHY:
/// Every storage call used to open its own connection: a file open, three PRAGMA round trips, a fresh parse of the whole schema on the first statement and a cold page cache, paid several times a second by the background tasks and once per open chat per second by the chat follower's live-state read. Connections now return to a small pool when the borrower drops them, so a call reuses an open connection whose schema, page cache and cached statements are already warm. A connection that comes back inside an open transaction is closed instead of pooled, and each borrow resets the busy timeout, the only per-connection setting callers change.
pub struct GxserverDatabase {
    connection: Option<Connection>,
    path: PathBuf,
}

impl Deref for GxserverDatabase {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("pooled connection is present until drop")
    }
}

impl DerefMut for GxserverDatabase {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection
            .as_mut()
            .expect("pooled connection is present until drop")
    }
}

impl Drop for GxserverDatabase {
    fn drop(&mut self) {
        // A turn in the busy queue this thread still holds ends with its connection.
        super::fair_busy::release_current_thread();
        let Some(connection) = self.connection.take() else {
            return;
        };
        if !connection.is_autocommit() {
            return;
        }
        let path = std::mem::take(&mut self.path);
        let mut pool = POOL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(current) = pool.as_mut().filter(|current| current.path == path) {
            if current.idle.len() < POOLED_CONNECTIONS_MAX_IDLE {
                current.idle.push(connection);
            }
            return;
        }
        // Closing the previous database's connections can wait for the lock to be released.
        let replaced = pool.replace(ConnectionPool {
            path,
            idle: vec![connection],
        });
        drop(pool);
        drop(replaced);
    }
}

/// Borrows a pooled connection to `path`, or opens a new one with the state.db PRAGMAs when none is idle.
pub(super) fn borrow_gxserver_database(
    path: &Path,
    busy_timeout: Option<Duration>,
) -> Result<GxserverDatabase> {
    let pooled = {
        let mut pool = POOL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        pool.as_mut()
            .filter(|pool| pool.path == path)
            .and_then(|pool| pool.idle.pop())
    };
    // rusqlite 0.41+: installing a hook or busy handler needs `&mut Connection`.
    let mut connection = match pooled {
        Some(connection) => connection,
        None => {
            let mut connection = open_unpooled_gxserver_database(path, busy_timeout)?;
            connection.set_prepared_statement_cache_capacity(POOLED_STATEMENT_CACHE_CAPACITY);
            // A write that commits or rolls back ends its thread's turn in the busy queue.
            connection.commit_hook(Some(|| {
                super::fair_busy::release_current_thread();
                false
            }))?;
            connection.rollback_hook(Some(super::fair_busy::release_current_thread))?;
            connection
        }
    };
    // The default wait is taken in turn (`fair_busy`); a caller's own timeout keeps SQLite's.
    match busy_timeout {
        Some(busy_timeout) => connection.busy_timeout(busy_timeout)?,
        None => connection.busy_handler(Some(super::fair_busy::busy))?,
    }
    Ok(GxserverDatabase {
        connection: Some(connection),
        path: path.to_path_buf(),
    })
}

/// A plain connection with the state.db PRAGMAs. A busy timeout, when given, is applied before the PRAGMAs so they wait for a concurrent writer as well.
pub(super) fn open_unpooled_gxserver_database(
    path: &Path,
    busy_timeout: Option<Duration>,
) -> Result<Connection> {
    let db = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    if let Some(busy_timeout) = busy_timeout {
        db.busy_timeout(busy_timeout)?;
    }
    db.pragma_update(None, "foreign_keys", "ON")?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    apply_wal_durability(&db)?;
    Ok(db)
}
