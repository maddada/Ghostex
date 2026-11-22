//! Work-mode notices the person closed, remembered in gxserver's metadata table so a closed notice
//! stays closed in every window, after a restart, and in the web build.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

const DISMISSED_NOTICES_KEY: &str = "workModeDismissedNotices";

/// The notices a client may close.
pub(crate) const WORK_NOTICES: &[&str] = &["githubProjectsScope"];

fn read_dismissed(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
    Ok(db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [DISMISSED_NOTICES_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

pub(crate) fn work_notice_dismissed(db: &Connection, notice: &str) -> bool {
    read_dismissed(db)
        .ok()
        .is_some_and(|dismissed| dismissed.get(notice).is_some())
}

/// Remembers that the person closed `notice`.
pub(crate) fn dismiss_work_notice(db: &Connection, notice: &str) -> Result<(), DomainStateError> {
    if !WORK_NOTICES.contains(&notice) {
        return Err(DomainStateError::bad_request(format!(
            "Unknown notice \"{notice}\"."
        )));
    }
    let mut dismissed = read_dismissed(db)?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    dismissed.insert(notice.to_string(), json!(now));
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            DISMISSED_NOTICES_KEY,
            Value::Object(dismissed).to_string(),
            now
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite work notices error: {error}"),
    }
}
