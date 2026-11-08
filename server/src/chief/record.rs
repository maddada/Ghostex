//! Which session is this computer's Chief: one row in the `metadata` table, written only by
//! gxserver when it created that session (`session.rs`). No route takes a session id to name as
//! Chief, so no caller can point the head role at a session of its own choosing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::coordinators::{now_iso, SessionKey};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

const RECORD_KEY: &str = "chiefSession";

/// This computer's Chief session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChiefRecord {
    pub project_id: String,
    pub session_id: String,
    /// The launcher it runs on (the first-open pick, or Change Agent's).
    pub agent_id: String,
    pub created_at: String,
}

impl ChiefRecord {
    pub fn key(&self) -> SessionKey {
        (self.project_id.clone(), self.session_id.clone())
    }
}

/// The saved record; `None` before the first open, or when it can't be read.
pub(crate) fn read_record(db: &Connection) -> Option<ChiefRecord> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [RECORD_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|text| serde_json::from_str(&text).ok())
}

/// Saves the record and refreshes the head role's cache for this database.
pub(super) fn write_record(
    db: &Connection,
    paths: &GxserverPaths,
    record: &ChiefRecord,
) -> Result<(), DomainStateError> {
    let text = serde_json::to_string(record).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save Chief of Staff's record: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![RECORD_KEY, text, now_iso()],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite Chief of Staff error: {error}"),
    })?;
    remember(paths, Some(record.key()));
    Ok(())
}

/// The Chief records read so far, per state database, so the auth gate's check on every session
/// request costs a map lookup. Only `write_record` changes a record, and it updates this.
fn cache() -> &'static Mutex<HashMap<PathBuf, Option<SessionKey>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<SessionKey>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn remember(paths: &GxserverPaths, key: Option<SessionKey>) {
    cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(paths.state_db_file.clone(), key);
}

/// This computer's Chief session, if it has one.
pub(crate) fn chief_key(paths: &GxserverPaths) -> Option<SessionKey> {
    if let Some(known) = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&paths.state_db_file)
    {
        return known.clone();
    }
    let db = crate::storage::open_gxserver_database(paths).ok()?;
    let key = read_record(&db).map(|record| record.key());
    remember(paths, key.clone());
    key
}
