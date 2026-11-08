//! The `gbots_assignments` table: one row per piece of work handed to a GBot, or question asked of
//! it, with `ghostex gbots ask` (assign.rs). The chat it runs in is linked to the sender as a
//! coordinator thread, so the report travels the thread way; this row adds what a thread does not
//! know: work or question, the chain it belongs to, and how often it was sent back for rework.
//!
//! Ids are short and readable (`A12`), so a sender can write `--rework A12` from its own report.

use rusqlite::{Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

/// How deep a chain of hand-offs may go: Chief of Staff → a lead → its member → … (the first
/// hand-off is depth 1).
pub(crate) const MAX_DEPTH: i64 = 4;
/// Hand-offs one chain may make in all, reworks not counted.
pub(crate) const MAX_PER_ROOT: i64 = 24;
/// Times one assignment may be sent back.
pub(crate) const MAX_REWORKS: i64 = 2;

/// `fromSessionKey` of work the user handed over from a terminal, phone or computer: nobody's chat
/// gets the report, the user reads the bot's chat.
pub(crate) const FROM_USER: &str = "user";

/// Statuses: `open` (handed over, no report yet), `reworking` (sent back, no report yet),
/// `reported` (its report went to the sender), `answered` (a question's answer went to the sender;
/// the chat was resolved), `closed` (its chat went away before it reported).
pub(crate) const OPEN_STATUSES: [&str; 2] = ["open", "reworking"];

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    pub assignment_id: String,
    pub root_id: String,
    pub parent_id: Option<String>,
    pub depth: i64,
    pub from_session_key: String,
    pub bot_id: String,
    pub chat_session_key: String,
    pub mode: String,
    pub brief: String,
    pub done_when: String,
    pub reworks: i64,
    pub status: String,
    pub created_at: String,
    pub settled_at: Option<String>,
}

impl Assignment {
    pub fn is_open(&self) -> bool {
        OPEN_STATUSES.contains(&self.status.as_str())
    }

    pub fn is_consult(&self) -> bool {
        self.mode == "consult"
    }
}

const COLUMNS: &str = "assignmentId, rootId, parentId, depth, fromSessionKey, botId, \
     chatSessionKey, mode, brief, doneWhen, reworks, status, createdAt, settledAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Assignment> {
    Ok(Assignment {
        assignment_id: row.get(0)?,
        root_id: row.get(1)?,
        parent_id: row.get(2)?,
        depth: row.get(3)?,
        from_session_key: row.get(4)?,
        bot_id: row.get(5)?,
        chat_session_key: row.get(6)?,
        mode: row.get(7)?,
        brief: row.get(8)?,
        done_when: row.get(9)?,
        reworks: row.get(10)?,
        status: row.get(11)?,
        created_at: row.get(12)?,
        settled_at: row.get(13)?,
    })
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// `projectId:sessionId`, how assignments name a chat.
pub(crate) fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
}

/// The next readable id: `A` and one more than the highest number used so far.
pub(crate) fn next_assignment_id(db: &Connection) -> Result<String, DomainStateError> {
    let highest: Option<i64> = db
        .query_row(
            "SELECT MAX(CAST(SUBSTR(assignmentId, 2) AS INTEGER)) FROM gbots_assignments \
             WHERE assignmentId GLOB 'A[0-9]*'",
            [],
            |row| row.get(0),
        )
        .map_err(sql)?;
    Ok(format!("A{}", highest.unwrap_or(0) + 1))
}

pub(crate) fn insert_assignment(
    db: &Connection,
    assignment: &Assignment,
) -> Result<(), DomainStateError> {
    db.execute(
        &format!(
            "INSERT INTO gbots_assignments ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
        ),
        rusqlite::params![
            assignment.assignment_id,
            assignment.root_id,
            assignment.parent_id,
            assignment.depth,
            assignment.from_session_key,
            assignment.bot_id,
            assignment.chat_session_key,
            assignment.mode,
            assignment.brief,
            assignment.done_when,
            assignment.reworks,
            assignment.status,
            assignment.created_at,
            assignment.settled_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// An assignment by id, any case (`a12` finds `A12`).
pub(crate) fn read_assignment(
    db: &Connection,
    assignment_id: &str,
) -> Result<Option<Assignment>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_assignments WHERE assignmentId = ?1 COLLATE NOCASE"),
        [assignment_id.trim()],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The oldest assignment still waiting for its report in `chat` (a chat takes its work in order).
pub(crate) fn open_assignment_in_chat(
    db: &Connection,
    chat: &str,
) -> Result<Option<Assignment>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {COLUMNS} FROM gbots_assignments WHERE chatSessionKey = ?1 \
             AND status IN ('open', 'reworking') ORDER BY createdAt, assignmentId LIMIT 1"
        ),
        [chat],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The newest assignment `chat` is working on or reported last: the hand-off a new `ask` from that
/// chat continues.
pub(crate) fn latest_assignment_in_chat(
    db: &Connection,
    chat: &str,
) -> Result<Option<Assignment>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {COLUMNS} FROM gbots_assignments WHERE chatSessionKey = ?1 \
             ORDER BY createdAt DESC, assignmentId DESC LIMIT 1"
        ),
        [chat],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// How many assignments a chain has made.
pub(crate) fn count_in_root(db: &Connection, root_id: &str) -> Result<i64, DomainStateError> {
    db.query_row(
        "SELECT COUNT(*) FROM gbots_assignments WHERE rootId = ?1",
        [root_id],
        |row| row.get(0),
    )
    .map_err(sql)
}

/// Assignments, newest first: all of them, or those one sender handed out.
pub(crate) fn list_assignments(
    db: &Connection,
    from: Option<&str>,
    limit: usize,
) -> Result<Vec<Assignment>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_assignments WHERE (?1 IS NULL OR fromSessionKey = ?1) \
             ORDER BY createdAt DESC, assignmentId DESC LIMIT ?2"
        ))
        .map_err(sql)?;
    let rows = statement
        .query_map(rusqlite::params![from, limit as i64], from_row)
        .map_err(sql)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql)
}

pub(crate) fn set_status(
    db: &Connection,
    assignment_id: &str,
    status: &str,
) -> Result<(), DomainStateError> {
    let settled = (!OPEN_STATUSES.contains(&status)).then(now_iso);
    db.execute(
        "UPDATE gbots_assignments SET status = ?2, settledAt = ?3 WHERE assignmentId = ?1",
        rusqlite::params![assignment_id, status, settled],
    )
    .map_err(sql)?;
    Ok(())
}

/// Sends `assignment` back once more.
pub(crate) fn mark_rework(db: &Connection, assignment_id: &str) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_assignments SET status = 'reworking', reworks = reworks + 1, \
         settledAt = NULL WHERE assignmentId = ?1",
        [assignment_id],
    )
    .map_err(sql)?;
    Ok(())
}

/// Every chain member above `assignment`, nearest first.
pub(crate) fn ancestors(
    db: &Connection,
    assignment: &Assignment,
) -> Result<Vec<Assignment>, DomainStateError> {
    let mut chain = Vec::new();
    let mut next = assignment.parent_id.clone();
    while let Some(id) = next {
        let Some(parent) = read_assignment(db, &id)? else {
            break;
        };
        next = parent.parent_id.clone();
        chain.push(parent);
        if chain.len() > MAX_DEPTH as usize + 2 {
            break;
        }
    }
    Ok(chain)
}

/// An assignment as `/api/gbots` answers it.
pub(crate) fn assignment_summary(assignment: &Assignment, bot_name: Option<&str>) -> Value {
    json!({
        "assignmentId": assignment.assignment_id,
        "rootId": assignment.root_id,
        "parentId": assignment.parent_id,
        "depth": assignment.depth,
        "from": assignment.from_session_key,
        "botId": assignment.bot_id,
        "botName": bot_name,
        "chat": assignment.chat_session_key,
        "mode": assignment.mode,
        "brief": assignment.brief,
        "doneWhen": assignment.done_when,
        "reworks": assignment.reworks,
        "status": assignment.status,
        "createdAt": assignment.created_at,
        "settledAt": assignment.settled_at,
    })
}
