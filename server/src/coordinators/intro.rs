//! Whether coordinators offer the "How to use coordinators" tips: one flag for every coordinator
//! this gxserver runs, set by "Don't show again" (`/api/coordinatorBoard` op `hideIntro`) and sent
//! to each coordinator's chat as `coordinatorThreads.introHidden` (panel.rs), so every window and
//! the phone drop the pill together. The tips and the user's decision are in gx-chat-core
//! extras/coordinator_intro.rs.

use rusqlite::{Connection, OptionalExtension};

use super::records::{now_iso, sql_error};
use crate::domain::DomainStateError;

const KEY: &str = "coordinatorIntroHidden";

/// Whether the user turned the tips off; a read error leaves them on.
pub fn intro_hidden(db: &Connection) -> bool {
    db.query_row("SELECT value FROM metadata WHERE key = ?1", [KEY], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .is_some_and(|value| value == "true")
}

/// "Don't show again": turns the tips off for every coordinator.
pub fn hide_intro(db: &Connection) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, 'true', ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![KEY, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tips_show_until_turned_off() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        assert!(!intro_hidden(&db));
        hide_intro(&db).unwrap();
        assert!(intro_hidden(&db));
        hide_intro(&db).unwrap();
        assert!(intro_hidden(&db));
    }
}
