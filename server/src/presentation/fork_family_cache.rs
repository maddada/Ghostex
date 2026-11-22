use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::{Connection, OptionalExtension};

use super::{SessionForkFacts, SessionForkFamilies};
use crate::domain::{sql_error, DomainRepository, DomainStateError};

const MAX_DATABASES: usize = 4;
const MAX_CACHED_SESSIONS: usize = 16_384;

struct CachedFamilies {
    database: String,
    server_id: String,
    revision: String,
    families: Arc<SessionForkFamilies>,
}

/// CDXC:StateSync 2026-09-13 WHY:
/// A presentation delta formerly extracted every session's JSON and rebuilt the fork graph, even for a title-only update.
/// Read the durable lineage token and rows in one SQLite snapshot, and never publish a caller's uncommitted graph into the shared cache.
pub(crate) fn read_session_fork_families(
    db: &Connection,
    server_id: &str,
) -> Result<Arc<SessionForkFamilies>, DomainStateError> {
    let database = db.path().filter(|path| !path.is_empty());
    if !db.is_autocommit() || database.is_none() {
        return Ok(Arc::new(SessionForkFamilies::build_from_facts(
            &DomainRepository::new(db, server_id)
                .map_session_fork_rows(|row| SessionForkFacts::from_session(&row))?,
        )));
    }
    static CACHE: OnceLock<Mutex<VecDeque<CachedFamilies>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    let revision: String = transaction
        .query_row(
            "SELECT value FROM metadata WHERE key = 'sessionForkRevision'",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    /*
    CDXC:StateSync 2026-09-18 WHY:
    Legacy registries carry the same sessionId under several projects, and the builder picks among such rows by updatedAt order, which a title write can change without touching the lineage token.
    The cache used to refuse those registries outright, so one with 50 duplicates rebuilt the whole graph from 9,000 rows on every delta (over half of gxserver's CPU). The duplicates' own order joins the key instead; it is a few rows to read and changes exactly when the pick could.
    */
    let database = database.unwrap();
    let revision = format!(
        "{revision}|{}",
        duplicate_session_order(&transaction, database, &revision)?
    );
    let lookup = || -> Result<Option<Arc<SessionForkFamilies>>, DomainStateError> {
        let mut cache = cache
            .lock()
            .map_err(|_| DomainStateError::corrupt_state("Session fork cache is poisoned."))?;
        Ok(cache
            .iter()
            .position(|entry| {
                entry.database == database
                    && entry.server_id == server_id
                    && entry.revision == revision
            })
            .map(|index| {
                let entry = cache.remove(index).unwrap();
                let families = entry.families.clone();
                cache.push_back(entry);
                families
            }))
    };
    if let Some(families) = lookup()? {
        transaction.commit().map_err(sql_error)?;
        return Ok(families);
    }
    /*
    CDXC:StateSync 2026-10-23 WHY:
    Reading every row's lineage and building the graph held about 45 MB on a registry of 10,900 rows (about 20 MB since the build keeps only `SessionForkFacts` per row), and right after a start every client's first snapshot and the first deltas all missed the cache at once and each built its own copy, which made up much of gxserver's 300 to 650 MB startup peak. One build runs at a time; a caller that waited finds the graph the build before it cached for the same token and uses it.
    */
    static BUILD: Mutex<()> = Mutex::new(());
    let _building = BUILD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(families) = lookup()? {
        transaction.commit().map_err(sql_error)?;
        return Ok(families);
    }
    let rows = DomainRepository::new(&transaction, server_id)
        .map_session_fork_rows(|row| SessionForkFacts::from_session(&row))?;
    let families = Arc::new(SessionForkFamilies::build_from_facts(&rows));
    transaction.commit().map_err(sql_error)?;
    let cacheable = rows.len() <= MAX_CACHED_SESSIONS;
    drop(rows);
    if cacheable {
        let mut cache = cache
            .lock()
            .map_err(|_| DomainStateError::corrupt_state("Session fork cache is poisoned."))?;
        cache.retain(|entry| entry.database != database || entry.server_id != server_id);
        if cache.len() >= MAX_DATABASES {
            cache.pop_front();
        }
        cache.push_back(CachedFamilies {
            database: database.to_owned(),
            server_id: server_id.to_owned(),
            revision,
            families: families.clone(),
        });
    }
    Ok(families)
}

/// One database's duplicated rows as of one lineage token.
struct CachedDuplicates {
    database: String,
    revision: String,
    /// `(projectId, sessionId)` of every row whose sessionId appears more than once.
    rows: Arc<Vec<(String, String)>>,
}

/// The rows whose sessionId appears more than once. Which rows those are can only change with an insert, a delete, or a projectId/sessionId update, and each of those moves `sessionForkRevision` (the `session_fork_*` triggers), so the set is cached under that token.
fn duplicated_session_rows(
    db: &Connection,
    database: &str,
    revision: &str,
) -> Result<Arc<Vec<(String, String)>>, DomainStateError> {
    static CACHE: OnceLock<Mutex<VecDeque<CachedDuplicates>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let cached = cache
        .lock()
        .map_err(|_| DomainStateError::corrupt_state("Session duplicate cache is poisoned."))?
        .iter()
        .find(|entry| entry.database == database && entry.revision == revision)
        .map(|entry| entry.rows.clone());
    if let Some(rows) = cached {
        return Ok(rows);
    }
    let mut statement = db
        .prepare_cached(
            "SELECT projectId, sessionId FROM sessions
             WHERE sessionId IN (SELECT sessionId FROM sessions GROUP BY sessionId HAVING count(*) > 1)",
        )
        .map_err(sql_error)?;
    let rows = Arc::new(
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?,
    );
    let mut cache = cache
        .lock()
        .map_err(|_| DomainStateError::corrupt_state("Session duplicate cache is poisoned."))?;
    cache.retain(|entry| entry.database != database);
    if cache.len() >= MAX_DATABASES {
        cache.pop_front();
    }
    cache.push_back(CachedDuplicates {
        database: database.to_owned(),
        revision: revision.to_owned(),
        rows: rows.clone(),
    });
    Ok(rows)
}

/// The ordered rows of every sessionId that appears more than once, which is what decides the builder's pick among them.
///
/// CDXC:StateSync 2026-10-23 WHY:
/// Finding the duplicates scanned the whole sessions table on every presentation delta and snapshot, and ordering them by `updatedAt` read every row's payload past its JSON columns: about half of each delta's cost on a registry of 10,000 rows. The duplicated rows are now cached under the lineage token (`duplicated_session_rows`), and only their own `updatedAt` is read on each call, by primary key, then sorted exactly as the old `ORDER BY updatedAt DESC, projectId ASC, sessionId ASC` did (both compare bytes).
fn duplicate_session_order(
    db: &Connection,
    database: &str,
    revision: &str,
) -> Result<String, DomainStateError> {
    let rows = duplicated_session_rows(db, database, revision)?;
    if rows.is_empty() {
        return Ok(String::new());
    }
    let mut statement = db
        .prepare_cached("SELECT updatedAt FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .map_err(sql_error)?;
    let mut ordered = Vec::with_capacity(rows.len());
    for (project_id, session_id) in rows.iter() {
        let updated_at = statement
            .query_row([project_id, session_id], |row| row.get::<_, String>(0))
            .optional()
            .map_err(sql_error)?;
        if let Some(updated_at) = updated_at {
            ordered.push((updated_at, project_id, session_id));
        }
    }
    ordered.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.cmp(right.2))
    });
    let mut order = String::new();
    for (_, project_id, session_id) in ordered {
        order.push_str(session_id);
        order.push('/');
        order.push_str(project_id);
        order.push(',');
    }
    Ok(order)
}
