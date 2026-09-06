use super::model::Registry;
use crate::domain::DomainStateError;
use rusqlite::{Connection, OptionalExtension};
const KEY: &str = "agents.accounts.v1";
pub(crate) fn read(db: &Connection) -> Result<Registry, DomainStateError> {
    let raw: Option<String> = db
        .query_row("SELECT value FROM metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    raw.map(|s| serde_json::from_str(&s).map_err(error))
        .unwrap_or_else(|| Ok(Registry::default()))
}
pub(crate) fn write(db: &Connection, value: &Registry) -> Result<(), DomainStateError> {
    db.execute("INSERT INTO metadata (key,value,updatedAt) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt",rusqlite::params![KEY,serde_json::to_string(value).map_err(error)?,chrono::Utc::now().to_rfc3339()]).map_err(error)?;
    Ok(())
}
pub(crate) fn error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Account operation failed: {error}"),
    }
}
