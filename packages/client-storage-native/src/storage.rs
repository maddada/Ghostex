use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

/// The client-storage file, opened with its four tables in place.
pub(crate) struct Storage {
    pub(crate) database: Connection,
}

impl Storage {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let database = Connection::open(path)?;
        database.busy_timeout(std::time::Duration::from_secs(5))?;
        database.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS records (key TEXT PRIMARY KEY, store TEXT NOT NULL, value TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS records_by_store ON records(store);
            CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS legacy (database_name TEXT, store TEXT, value TEXT);")?;
        Ok(Self { database })
    }
}
