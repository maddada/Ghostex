use std::fmt;
use std::fs;

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;

use crate::domain::normalize::normalize_settled_override;

pub(crate) type DomainResult<T> = std::result::Result<T, DomainStateError>;

#[derive(Debug, Clone)]
pub struct DomainStateError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectPathState {
    Available,
    Missing,
    NotDirectory,
    Unavailable,
}

impl ProjectPathState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Missing => "missing",
            Self::NotDirectory => "notDirectory",
            Self::Unavailable => "unavailable",
        }
    }
}

impl DomainStateError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    pub fn corrupt_state(message: impl Into<String>) -> Self {
        Self {
            code: "corruptState",
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

/*
CDXC:StateSync 2026-07-29-00:00:
The durable settle/snooze state of one session. `settled_override_at` stamps
when the current override was recorded and is server-internal: the lifecycle
sweep compares it against gxserver's meaningful-activity clock so real activity
resets any override without requiring an event log.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLifecycleFields {
    pub settled_at: Option<String>,
    pub settled_override: Option<String>,
    pub settled_override_at: Option<String>,
    pub snoozed_at: Option<String>,
    pub snoozed_until: Option<String>,
}

impl SessionLifecycleFields {
    pub fn from_session(session: &Value) -> Self {
        let text = |key: &str| {
            session
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        Self {
            settled_at: text("settledAt"),
            settled_override: normalize_settled_override(text("settledOverride").as_deref()),
            settled_override_at: text("settledOverrideAt"),
            snoozed_at: text("snoozedAt"),
            snoozed_until: text("snoozedUntil"),
        }
    }

    pub fn is_settled_override(&self) -> bool {
        self.settled_override.as_deref() == Some("settled")
    }

    pub fn is_active_override(&self) -> bool {
        self.settled_override.as_deref() == Some("active")
    }

    pub fn clear_settle(&mut self) {
        self.settled_at = None;
        self.settled_override = None;
        self.settled_override_at = None;
    }

    pub fn clear_snooze(&mut self) {
        self.snoozed_at = None;
        self.snoozed_until = None;
    }
}

impl fmt::Display for DomainStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for DomainStateError {}

/*
CDXC:RepoStructure 2026-06-14-22:12:
Phase 3 Rust must use the TypeScript SQLite tables as durable state instead of an in-memory compatibility stub. Preserve project/session IDs, JSON validation, corrupt-state errors, and camelCase response fields so sidebar inventory can opt into Rust without a client protocol change.
*/
pub struct DomainRepository<'a> {
    pub(crate) db: &'a Connection,
    pub(crate) server_id: String,
}

impl<'a> DomainRepository<'a> {
    pub fn new(db: &'a Connection, server_id: impl Into<String>) -> Self {
        Self {
            db,
            server_id: server_id.into(),
        }
    }

    /// The connection this repository reads and writes. Narrow accessor for the
    /// few callers that must run a query the repository does not model against
    /// the SAME connection instead of opening a second one — the chat prompt
    /// queue's Auto Sleep decline is one.
    pub fn connection(&self) -> &'a Connection {
        self.db
    }
}

pub(crate) fn project_path_state(project: &Value) -> ProjectPathState {
    let Some(path) = project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return ProjectPathState::Unavailable;
    };
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => ProjectPathState::Available,
        Ok(_) => ProjectPathState::NotDirectory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ProjectPathState::Missing,
        Err(_) => ProjectPathState::Unavailable,
    }
}

pub(crate) fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite domain-state error: {error}"),
    }
}

pub fn initialize_for_tests(db: &Connection) -> Result<()> {
    db.execute_batch("PRAGMA foreign_keys = ON;")
        .context("enable foreign keys")?;
    Ok(())
}
