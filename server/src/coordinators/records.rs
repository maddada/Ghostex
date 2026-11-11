//! The `coordinators` and `coordinator_threads` rows (migration 0041).

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::domain::DomainStateError;

/// A coordinator note never grows past this, so one careless paste cannot swallow the coordinator's memory.
pub const COORDINATOR_MEMORY_NOTE_MAX_CHARS: usize = 600;
/// Notes beyond this are refused; the coordinator is told to forget old ones first.
pub const COORDINATOR_MEMORY_MAX_NOTES: usize = 60;
/// Standing instructions share Claude's project-instructions budget.
pub const COORDINATOR_INSTRUCTIONS_MAX_CHARS: usize = 16_000;
pub const COORDINATOR_GOAL_MAX_CHARS: usize = 400;
/// The start of a thread's last report kept for the Threads panel and `coordinator status`.
pub const COORDINATOR_THREAD_REPORT_EXCERPT_MAX_CHARS: usize = 600;

pub type SessionKey = (String, String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorMemoryNote {
    pub text: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatorRecord {
    pub project_id: String,
    pub session_id: String,
    pub goal: String,
    pub instructions: String,
    pub memory: Vec<CoordinatorMemoryNote>,
    pub created_at: String,
    pub updated_at: String,
}

impl CoordinatorRecord {
    pub fn key(&self) -> SessionKey {
        (self.project_id.clone(), self.session_id.clone())
    }

    pub fn memory_value(&self) -> Value {
        Value::Array(
            self.memory
                .iter()
                .map(|note| json!({ "text": note.text, "createdAt": note.created_at }))
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRecord {
    pub project_id: String,
    pub session_id: String,
    pub coordinator_project_id: String,
    pub coordinator_session_id: String,
    pub task: String,
    pub resolved_at: Option<String>,
    /// The latest Mark Done, stamped by every resolve, even of a thread already done
    /// (`resolved_at` keeps the first one). `None` for a thread resolved before migration 0075.
    pub last_resolved_at: Option<String>,
    pub observed_working: bool,
    pub reported_at: Option<String>,
    pub reported_prompt_key: Option<String>,
    pub last_report: Option<String>,
    /// The start of a message its coordinator handed it that its transcript does not show yet.
    pub pending_message: Option<String>,
    /// When that message was sent.
    pub pending_message_at: Option<String>,
    /// That message's `sendRequestId`, when its sender gave one: what its confirmation names.
    pub pending_message_send_id: Option<String>,
    /// When closing its coordinator closed it too; restoring the coordinator restores it.
    pub closed_with_coordinator_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl ThreadRecord {
    pub fn key(&self) -> SessionKey {
        (self.project_id.clone(), self.session_id.clone())
    }

    pub fn coordinator_key(&self) -> SessionKey {
        (
            self.coordinator_project_id.clone(),
            self.coordinator_session_id.clone(),
        )
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved_at.is_some()
    }

    /// When the thread was last marked done: the stamp a message must not predate to reopen it.
    pub fn close_stamp(&self) -> Option<&str> {
        self.last_resolved_at
            .as_deref()
            .or(self.resolved_at.as_deref())
    }
}

pub(crate) fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite coordinator error: {error}"),
    }
}

fn parse_memory(text: &str) -> Vec<CoordinatorMemoryNote> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let text = entry.get("text")?.as_str()?.trim();
            (!text.is_empty()).then(|| CoordinatorMemoryNote {
                text: text.to_string(),
                created_at: entry
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}

fn coordinator_from_row(row: &Row<'_>) -> rusqlite::Result<CoordinatorRecord> {
    let memory: String = row.get("memoryJson")?;
    Ok(CoordinatorRecord {
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        goal: row.get("goal")?,
        instructions: row.get("instructions")?,
        memory: parse_memory(&memory),
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

pub(super) fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<ThreadRecord> {
    Ok(ThreadRecord {
        project_id: row.get("projectId")?,
        session_id: row.get("sessionId")?,
        coordinator_project_id: row.get("coordinatorProjectId")?,
        coordinator_session_id: row.get("coordinatorSessionId")?,
        task: row.get("task")?,
        resolved_at: row.get("resolvedAt")?,
        last_resolved_at: row.get("lastResolvedAt")?,
        observed_working: row.get::<_, i64>("observedWorking")? == 1,
        reported_at: row.get("reportedAt")?,
        reported_prompt_key: row.get("reportedPromptKey")?,
        last_report: row.get("lastReport")?,
        pending_message: row.get("pendingMessage")?,
        pending_message_at: row.get("pendingMessageAt")?,
        pending_message_send_id: row.get("pendingMessageSendId")?,
        closed_with_coordinator_at: row.get("closedWithCoordinatorAt")?,
        created_at: row.get("createdAt")?,
        updated_at: row.get("updatedAt")?,
    })
}

const COORDINATOR_COLUMNS: &str =
    "projectId, sessionId, goal, instructions, memoryJson, createdAt, updatedAt";
pub(super) const THREAD_COLUMNS: &str = "projectId, sessionId, coordinatorProjectId, coordinatorSessionId, task, resolvedAt, lastResolvedAt, observedWorking, reportedAt, reportedPromptKey, lastReport, pendingMessage, pendingMessageAt, pendingMessageSendId, closedWithCoordinatorAt, createdAt, updatedAt";

pub fn read_coordinator(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<CoordinatorRecord>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {COORDINATOR_COLUMNS} FROM coordinators WHERE projectId = ?1 AND sessionId = ?2"
        ),
        params![project_id, session_id],
        coordinator_from_row,
    )
    .optional()
    .map_err(sql_error)
}

pub fn list_coordinators(db: &Connection) -> Result<Vec<CoordinatorRecord>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COORDINATOR_COLUMNS} FROM coordinators ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], coordinator_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

pub fn insert_coordinator(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    goal: &str,
    instructions: &str,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        r#"
        INSERT INTO coordinators (projectId, sessionId, goal, instructions, memoryJson, createdAt, updatedAt)
        VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?5)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          goal = excluded.goal,
          instructions = excluded.instructions,
          updatedAt = excluded.updatedAt
        "#,
        params![project_id, session_id, goal, instructions, timestamp],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn write_coordinator(
    db: &Connection,
    record: &CoordinatorRecord,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinators
        SET goal = ?3, instructions = ?4, memoryJson = ?5, updatedAt = ?6
        WHERE projectId = ?1 AND sessionId = ?2
        "#,
        params![
            record.project_id,
            record.session_id,
            record.goal,
            record.instructions,
            record.memory_value().to_string(),
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn read_thread(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<ThreadRecord>, DomainStateError> {
    db.query_row(
        &format!(
            "SELECT {THREAD_COLUMNS} FROM coordinator_threads WHERE projectId = ?1 AND sessionId = ?2"
        ),
        params![project_id, session_id],
        thread_from_row,
    )
    .optional()
    .map_err(sql_error)
}

pub fn list_threads(db: &Connection) -> Result<Vec<ThreadRecord>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {THREAD_COLUMNS} FROM coordinator_threads ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], thread_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

/// The threads that are not resolved, oldest first: the ones the supervisor watches.
pub fn list_open_threads(db: &Connection) -> Result<Vec<ThreadRecord>, DomainStateError> {
    let mut statement = db
        .prepare_cached(&format!(
            "SELECT {THREAD_COLUMNS} FROM coordinator_threads WHERE resolvedAt IS NULL ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], thread_from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

pub fn list_threads_for(
    db: &Connection,
    coordinator_project_id: &str,
    coordinator_session_id: &str,
) -> Result<Vec<ThreadRecord>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {THREAD_COLUMNS} FROM coordinator_threads WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2 ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![coordinator_project_id, coordinator_session_id],
            thread_from_row,
        )
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

/// Links a session to a coordinator. A session already linked to the same coordinator keeps its
/// history (and gets the task only when it had none); linking it to another coordinator moves it
/// and reopens it. A session linked without a task was adopted as it is, so it counts as having
/// run already rather than as starting.
pub fn link_thread(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    coordinator_project_id: &str,
    coordinator_session_id: &str,
    task: &str,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        r#"
        INSERT INTO coordinator_threads (
          projectId, sessionId, coordinatorProjectId, coordinatorSessionId, task,
          resolvedAt, observedWorking, reportedAt, reportedPromptKey, lastReport, createdAt, updatedAt
        )
        VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, CASE WHEN ?5 = '' THEN ?6 ELSE NULL END, NULL, NULL, ?6, ?6)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          task = CASE WHEN coordinator_threads.task = '' THEN excluded.task ELSE coordinator_threads.task END,
          resolvedAt = CASE
            WHEN coordinator_threads.coordinatorProjectId = excluded.coordinatorProjectId
             AND coordinator_threads.coordinatorSessionId = excluded.coordinatorSessionId
            THEN coordinator_threads.resolvedAt ELSE NULL END,
          coordinatorProjectId = excluded.coordinatorProjectId,
          coordinatorSessionId = excluded.coordinatorSessionId,
          updatedAt = excluded.updatedAt
        "#,
        params![
            project_id,
            session_id,
            coordinator_project_id,
            coordinator_session_id,
            task,
            timestamp
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Removes a session's thread record, so it is an ordinary session again. Returns whether a record
/// existed.
pub fn unlink_thread(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<bool, DomainStateError> {
    let removed = db
        .execute(
            "DELETE FROM coordinator_threads WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        )
        .map_err(sql_error)?;
    Ok(removed > 0)
}

pub fn set_thread_resolved(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    resolved: bool,
) -> Result<bool, DomainStateError> {
    let timestamp = now_iso();
    let changed = db
        .execute(
            r#"
            UPDATE coordinator_threads
            SET resolvedAt = CASE WHEN ?3 THEN COALESCE(resolvedAt, ?4) ELSE NULL END,
                lastResolvedAt = CASE WHEN ?3 THEN ?4 ELSE lastResolvedAt END,
                observedWorking = CASE WHEN ?3 THEN 0 ELSE observedWorking END,
                updatedAt = ?4
            WHERE projectId = ?1 AND sessionId = ?2
            "#,
            params![project_id, session_id, resolved, timestamp],
        )
        .map_err(sql_error)?;
    Ok(changed > 0)
}

pub fn set_thread_observed_working(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET observedWorking = 1, updatedAt = ?3
        WHERE projectId = ?1 AND sessionId = ?2 AND observedWorking = 0
        "#,
        params![project_id, session_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Forgets that the thread was seen working, without a report: after its session stopped
/// unexpectedly and was restarted (session_self_heal.rs), its cut-off turn is not a finished one.
pub fn clear_thread_observed_working(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET observedWorking = 0, updatedAt = ?3
        WHERE projectId = ?1 AND sessionId = ?2 AND observedWorking = 1
        "#,
        params![project_id, session_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Records a delivered report: the finished turn (clears `observedWorking` and keeps the start of
/// the report) or the question that was relayed (its key, so it is relayed once).
pub fn record_thread_report(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    finished_report: Option<&str>,
    prompt_key: Option<&str>,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    match finished_report {
        Some(report) => {
            let excerpt: String = report
                .chars()
                .take(COORDINATOR_THREAD_REPORT_EXCERPT_MAX_CHARS)
                .collect();
            db.execute(
                r#"
                UPDATE coordinator_threads
                SET observedWorking = 0, reportedAt = ?3, lastReport = ?4, reportedPromptKey = NULL, updatedAt = ?3
                WHERE projectId = ?1 AND sessionId = ?2
                "#,
                params![project_id, session_id, timestamp, excerpt],
            )
            .map_err(sql_error)?;
        }
        None => {
            db.execute(
                r#"
                UPDATE coordinator_threads
                SET reportedPromptKey = ?3, updatedAt = ?4
                WHERE projectId = ?1 AND sessionId = ?2
                "#,
                params![project_id, session_id, prompt_key, timestamp],
            )
            .map_err(sql_error)?;
        }
    }
    Ok(())
}

/// Watches a message handed to the thread until its transcript records it; a newer message
/// replaces the one being watched. `send_id` is the message's `sendRequestId`, when known.
pub fn set_thread_pending_message(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    excerpt: &str,
    sent_at: &str,
    send_id: Option<&str>,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET pendingMessage = ?3, pendingMessageAt = ?4, pendingMessageSendId = ?6, updatedAt = ?5
        WHERE projectId = ?1 AND sessionId = ?2
        "#,
        params![project_id, session_id, excerpt, sent_at, now_iso(), send_id],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Stops watching whatever message the thread has pending: its sender saw it arrive.
pub fn drop_thread_pending_message(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET pendingMessage = NULL, pendingMessageAt = NULL, pendingMessageSendId = NULL, updatedAt = ?3
        WHERE projectId = ?1 AND sessionId = ?2 AND pendingMessageAt IS NOT NULL
        "#,
        params![project_id, session_id, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The watched message's sender saw it arrive: stops the watch if the watched message is the one
/// sent with `send_id`, first marking the thread seen working when it is open and has not been
/// reported on since that message was sent (a reply too fast for a supervisor tick to see is
/// reported through that mark). One conditional update, so a newer message watched meanwhile, or
/// a report recorded meanwhile, is never overwritten. Returns whether it matched.
///
/// CDXC:Coordinators 2026-11-10 WHY:
/// The confirmation used to drop whatever watch the thread had, without marking it seen working, so a reply too fast for a supervisor tick lost its report, and an older message's confirmation (a retried send) dropped a newer message's watch. It now names the send (`sendRequestId`) and changes only the watch of that send; a confirmation without one (an older CLI) changes nothing and the supervisor's own transcript check settles the watch. A matching start of the text is no identity: two messages can begin alike, or repeat.
pub fn acknowledge_thread_pending_message(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    send_id: &str,
) -> Result<bool, DomainStateError> {
    let changed = db
        .execute(
            r#"
            UPDATE coordinator_threads
            SET observedWorking = CASE
                  WHEN resolvedAt IS NULL AND (reportedAt IS NULL OR reportedAt < pendingMessageAt)
                  THEN 1 ELSE observedWorking END,
                pendingMessage = NULL, pendingMessageAt = NULL, pendingMessageSendId = NULL,
                updatedAt = ?4
            WHERE projectId = ?1 AND sessionId = ?2 AND pendingMessageSendId = ?3
            "#,
            params![project_id, session_id, send_id, now_iso()],
        )
        .map_err(sql_error)?;
    Ok(changed > 0)
}

/// The watched message sent at `sent_at` showed up in the thread's transcript: marks the thread
/// seen working (so a turn too short for a tick to see is still reported), only while it is open
/// and has not been reported on since that message was sent. One conditional update, so a report
/// recorded meanwhile is never undone.
///
/// CDXC:Coordinators 2026-11-11 FIX:
/// GitHub #554 (a comment of its author's): the supervisor's transcript check set `observedWorking` whenever it found the message, also after the turn it started had been reported, so the next tick reported that turn a second time (or said its final message could not be read). It now changes the mark under the same rule as the sender's confirmation (`acknowledge_thread_pending_message`).
pub fn mark_thread_working_for_message(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    sent_at: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET observedWorking = 1, updatedAt = ?4
        WHERE projectId = ?1 AND sessionId = ?2 AND observedWorking = 0
          AND resolvedAt IS NULL AND (reportedAt IS NULL OR reportedAt < ?3)
        "#,
        params![project_id, session_id, sent_at, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Stops watching the message sent at `sent_at`; a newer one sent meanwhile stays watched.
pub fn clear_thread_pending_message(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    sent_at: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET pendingMessage = NULL, pendingMessageAt = NULL, pendingMessageSendId = NULL, updatedAt = ?4
        WHERE projectId = ?1 AND sessionId = ?2 AND pendingMessageAt = ?3
        "#,
        params![project_id, session_id, sent_at, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}
