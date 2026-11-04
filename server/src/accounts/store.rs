use super::model::{Provider, Registry};
use crate::domain::DomainStateError;
use rusqlite::{Connection, OptionalExtension};
use std::sync::RwLock;
const KEY: &str = "agents.accounts.v1";
/// The logins the user turned off, as of the last registry read or write, so usage discovery
/// (which has no database) can skip them.
static USER_DISABLED: RwLock<Vec<(Provider, String)>> = RwLock::new(Vec::new());
/// The logins a session reported expired (login_expired.rs), kept the same way for usage discovery.
static LOGIN_EXPIRED: RwLock<Vec<(Provider, String, String)>> = RwLock::new(Vec::new());
static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether the registry was read since gxserver started (so `user_disabled` is known).
pub(crate) fn loaded() -> bool {
    LOADED.load(std::sync::atomic::Ordering::Relaxed)
}
fn remember_user_disabled(registry: &Registry) {
    let next = registry
        .accounts
        .iter()
        .filter(|account| account.user_disabled)
        .map(|account| (account.provider, account.selector.clone()))
        .collect();
    *USER_DISABLED.write().unwrap_or_else(|e| e.into_inner()) = next;
    *LOGIN_EXPIRED.write().unwrap_or_else(|e| e.into_inner()) = registry
        .accounts
        .iter()
        .filter_map(|account| {
            let at = account.login_expired_at.clone()?;
            Some((account.provider, account.selector.clone(), at))
        })
        .collect();
    LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
}
/// Whether the user turned off the saved login in `provider`'s slot `selector`.
pub(crate) fn user_disabled(provider: Provider, selector: &str) -> bool {
    USER_DISABLED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|(p, s)| *p == provider && s == selector)
}
/// When a session reported that the saved login in `provider`'s slot `selector` expired.
pub(crate) fn login_expired(provider: Provider, selector: &str) -> Option<String> {
    LOGIN_EXPIRED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(p, s, _)| *p == provider && s == selector)
        .map(|(_, _, at)| at.clone())
}
pub(crate) fn read(db: &Connection) -> Result<Registry, DomainStateError> {
    let raw: Option<String> = db
        .query_row("SELECT value FROM metadata WHERE key=?1", [KEY], |r| {
            r.get(0)
        })
        .optional()
        .map_err(error)?;
    let mut registry = raw
        .map(|s| serde_json::from_str(&s).map_err(error))
        .unwrap_or_else(|| Ok(Registry::default()))?;
    super::codex_identity_migration::apply(db, &mut registry);
    remember_user_disabled(&registry);
    Ok(registry)
}
pub(crate) fn write(db: &Connection, value: &Registry) -> Result<(), DomainStateError> {
    remember_user_disabled(value);
    db.execute("INSERT INTO metadata (key,value,updatedAt) VALUES (?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updatedAt=excluded.updatedAt",rusqlite::params![KEY,serde_json::to_string(value).map_err(error)?,chrono::Utc::now().to_rfc3339()]).map_err(error)?;
    Ok(())
}
pub(crate) fn error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("Account operation failed: {error}"),
    }
}
