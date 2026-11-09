//! Where a GBot's chats run, as stored: `gbots_bots.computer` and the columns migration 0070
//! added beside it, plus `gbots_remote_chats`, the chats it has on another computer.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// Where a GBot's chats run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// This computer (the default, and where its folder always is).
    This,
    /// A connected computer, by its server id.
    Computer(String),
    /// One of the user's Cloud Boxes, by its box id.
    CloudBox(String),
}

impl Place {
    pub(crate) fn parse(value: &str) -> Self {
        let value = value.trim();
        if let Some(id) = value.strip_prefix("computer:").filter(|id| !id.is_empty()) {
            return Self::Computer(id.to_string());
        }
        if let Some(id) = value.strip_prefix("cloudbox:").filter(|id| !id.is_empty()) {
            return Self::CloudBox(id.to_string());
        }
        Self::This
    }

    pub(crate) fn wire(&self) -> String {
        match self {
            Self::This => "this".to_string(),
            Self::Computer(id) => format!("computer:{id}"),
            Self::CloudBox(id) => format!("cloudbox:{id}"),
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::This => "this",
            Self::Computer(_) => "computer",
            Self::CloudBox(_) => "cloudBox",
        }
    }
}

/// A GBot's computer as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BotComputer {
    pub place: Place,
    /// The computer's or box's name when it was picked, shown while it is away.
    pub name: Option<String>,
    /// The folder its chats work in on that computer (`~/GBots/<name>` unless picked).
    pub folder: Option<String>,
    /// It may use that computer's screen and apps (cua-driver, SpaceO).
    pub computer_use: bool,
}

impl BotComputer {
    pub(crate) fn this() -> Self {
        Self {
            place: Place::This,
            name: None,
            folder: None,
            computer_use: false,
        }
    }

    /// The name to show: "This computer", else the stored name, else the id.
    pub(crate) fn label(&self) -> String {
        match &self.place {
            Place::This => "This computer".to_string(),
            Place::Computer(id) | Place::CloudBox(id) => self
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| id.clone()),
        }
    }

    pub(crate) fn summary(&self) -> Value {
        json!({
            "kind": self.place.kind(),
            "computer": self.place.wire(),
            "name": self.label(),
            "folder": self.folder,
            "computerUse": self.computer_use,
        })
    }
}

fn from_row(row: &Row<'_>) -> rusqlite::Result<(String, BotComputer)> {
    Ok((
        row.get(0)?,
        BotComputer {
            place: Place::parse(&row.get::<_, String>(1)?),
            name: row.get(2)?,
            folder: row.get(3)?,
            computer_use: row.get::<_, i64>(4)? != 0,
        },
    ))
}

const COLUMNS: &str = "botId, computer, computerName, computerFolder, computerUse";

pub(crate) fn read(db: &Connection, bot_id: &str) -> Result<BotComputer, DomainStateError> {
    Ok(db
        .query_row(
            &format!("SELECT {COLUMNS} FROM gbots_bots WHERE botId = ?1"),
            [bot_id],
            from_row,
        )
        .optional()
        .map_err(sql)?
        .map(|(_, computer)| computer)
        .unwrap_or_else(BotComputer::this))
}

/// Every bot's computer, by bot id.
pub(crate) fn all(db: &Connection) -> Result<HashMap<String, BotComputer>, DomainStateError> {
    let mut statement = db
        .prepare(&format!("SELECT {COLUMNS} FROM gbots_bots"))
        .map_err(sql)?;
    let rows = statement.query_map([], from_row).map_err(sql)?;
    rows.collect::<Result<HashMap<_, _>, _>>().map_err(sql)
}

pub(crate) fn write(
    db: &Connection,
    bot_id: &str,
    computer: &BotComputer,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_bots SET computer = ?2, computerName = ?3, computerFolder = ?4, computerUse = ?5, updatedAt = ?6 WHERE botId = ?1",
        params![
            bot_id,
            computer.place.wire(),
            computer.name,
            computer.folder,
            computer.computer_use as i64,
            now_iso()
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// A chat of a GBot that runs on another computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteChat {
    pub server_id: String,
    pub project_id: String,
    pub session_id: String,
    pub bot_id: String,
    pub opened_by: String,
    pub purpose: String,
    /// The sender's chat (`projectId:sessionId`, or `user`) for chats `ask` opened.
    pub from_key: Option<String>,
    pub standing: bool,
    pub title: String,
    pub created_at: String,
}

const CHAT_COLUMNS: &str =
    "serverId, projectId, sessionId, botId, openedBy, purpose, fromKey, standing, title, createdAt";

fn chat_from_row(row: &Row<'_>) -> rusqlite::Result<RemoteChat> {
    Ok(RemoteChat {
        server_id: row.get(0)?,
        project_id: row.get(1)?,
        session_id: row.get(2)?,
        bot_id: row.get(3)?,
        opened_by: row.get(4)?,
        purpose: row.get(5)?,
        from_key: row.get(6)?,
        standing: row.get::<_, i64>(7)? != 0,
        title: row.get(8)?,
        created_at: row.get(9)?,
    })
}

pub(crate) fn insert_remote_chat(
    db: &Connection,
    chat: &RemoteChat,
) -> Result<(), DomainStateError> {
    db.execute(
        "INSERT OR REPLACE INTO gbots_remote_chats (serverId, projectId, sessionId, botId, openedBy, purpose, fromKey, standing, title, createdAt, closedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
        params![
            chat.server_id,
            chat.project_id,
            chat.session_id,
            chat.bot_id,
            chat.opened_by,
            chat.purpose,
            chat.from_key,
            chat.standing as i64,
            chat.title,
            chat.created_at
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// The open chat a session of computer `server_id` is, if it is one.
pub(crate) fn find_remote_chat(
    db: &Connection,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Result<Option<RemoteChat>, DomainStateError> {
    db.query_row(
        &format!("SELECT {CHAT_COLUMNS} FROM gbots_remote_chats WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3 AND closedAt IS NULL"),
        params![server_id, project_id, session_id],
        chat_from_row,
    )
    .optional()
    .map_err(sql)
}

/// A bot's open chats on other computers, newest first; every bot's with `None`.
pub(crate) fn list_remote_chats(
    db: &Connection,
    bot_id: Option<&str>,
) -> Result<Vec<RemoteChat>, DomainStateError> {
    let mut statement = db
        .prepare(&format!("SELECT {CHAT_COLUMNS} FROM gbots_remote_chats WHERE closedAt IS NULL AND (?1 IS NULL OR botId = ?1) ORDER BY createdAt DESC"))
        .map_err(sql)?;
    let rows = statement
        .query_map(params![bot_id], chat_from_row)
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub(crate) fn close_remote_chat(
    db: &Connection,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    db.execute(
        "UPDATE gbots_remote_chats SET closedAt = ?4 WHERE serverId = ?1 AND projectId = ?2 AND sessionId = ?3 AND closedAt IS NULL",
        params![server_id, project_id, session_id, now_iso()],
    )
    .map(|changed| changed > 0)
    .map_err(sql)
}

/// Every open chat of a bot on another computer closes here when the bot is removed.
pub(crate) fn close_bot_remote_chats(
    db: &Connection,
    bot_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_remote_chats SET closedAt = ?2 WHERE botId = ?1 AND closedAt IS NULL",
        params![bot_id, now_iso()],
    )
    .map_err(sql)?;
    Ok(())
}
