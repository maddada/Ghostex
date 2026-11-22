//! The `gbots_problems` table (migration 0067): one row per problem a GBot's chat hit that Chief of
//! Staff hears about. Ids are short and readable (`P7`), so Chief of Staff can write
//! `ghostex gbots problem retry P7` straight from the report.

use rusqlite::{Connection, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::coordinators::now_iso;
use crate::domain::DomainStateError;

/// The chat's agent did not come up (a start error, a sign-in, every account at its limit).
pub(crate) const COULD_NOT_START: &str = "couldNotStart";
/// Its terminal kept ending under it and Ghostex's restarts gave up (session_self_heal.rs).
pub(crate) const FAILED: &str = "failed";
/// Working with no new output for `STALL_MS`.
pub(crate) const STALLED: &str = "stalled";
/// A routine run that failed.
pub(crate) const ROUTINE_FAILED: &str = "routineFailed";
/// A healthy chat waiting on something only a person or Chief of Staff can lift: a question or
/// approval (`source: prompt`) or a flag the GBot raised itself (`source: flag`).
pub(crate) const NEEDS_YOU: &str = "needsYou";

/// A row's detail is cut to this many characters.
pub(crate) const DETAIL_MAX_CHARS: usize = 240;
/// Chief of Staff's retries and re-briefs of one problem.
pub(crate) const MAX_ATTEMPTS: i64 = 2;

/// Statuses: `open` (reported, nothing done yet), `retrying` (Ghostex or Chief of Staff retried or
/// re-briefed it; it closes on its own once the chat works again), `handed` (Chief of Staff told
/// the user: theirs now), `closed`.
pub(crate) const LIVE_STATUSES: [&str; 3] = ["open", "retrying", "handed"];

#[derive(Clone, Debug, Default)]
pub(crate) struct Problem {
    pub problem_id: String,
    pub bot_id: String,
    pub chat_session_key: Option<String>,
    pub automation_id: Option<String>,
    pub kind: String,
    pub source: Option<String>,
    pub detail: String,
    pub user_only: bool,
    pub prompt_key: Option<String>,
    pub attempts: i64,
    pub ghostex_retried: bool,
    pub status: String,
    pub reported_to: Option<String>,
    pub reported_at: Option<String>,
    pub report_due_at: Option<String>,
    pub report_tries: i64,
    pub retried_at: Option<String>,
    pub closed_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Problem {
    pub fn is_live(&self) -> bool {
        LIVE_STATUSES.contains(&self.status.as_str())
    }

    pub fn is_escalation(&self) -> bool {
        self.kind == NEEDS_YOU
    }

    /// The chat's project and session ids.
    pub fn chat(&self) -> Option<(&str, &str)> {
        self.chat_session_key.as_deref()?.split_once(':')
    }
}

const COLUMNS: &str = "problemId, botId, chatSessionKey, automationId, kind, source, detail, \
     userOnly, promptKey, attempts, ghostexRetried, status, reportedTo, reportedAt, reportDueAt, \
     reportTries, retriedAt, closedBy, createdAt, updatedAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Problem> {
    Ok(Problem {
        problem_id: row.get(0)?,
        bot_id: row.get(1)?,
        chat_session_key: row.get(2)?,
        automation_id: row.get(3)?,
        kind: row.get(4)?,
        source: row.get(5)?,
        detail: row.get(6)?,
        user_only: row.get::<_, i64>(7)? != 0,
        prompt_key: row.get(8)?,
        attempts: row.get(9)?,
        ghostex_retried: row.get::<_, i64>(10)? != 0,
        status: row.get(11)?,
        reported_to: row.get(12)?,
        reported_at: row.get(13)?,
        report_due_at: row.get(14)?,
        report_tries: row.get(15)?,
        retried_at: row.get(16)?,
        closed_by: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// `detail` within `DETAIL_MAX_CHARS`, on one line.
pub(crate) fn short_detail(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= DETAIL_MAX_CHARS {
        return line;
    }
    let cut: String = line.chars().take(DETAIL_MAX_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// A new row, not yet stored: `open`, numbered by [`insert_problem`].
pub(crate) fn new_problem(bot_id: &str, chat: Option<&str>, kind: &str, detail: &str) -> Problem {
    let now = now_iso();
    Problem {
        bot_id: bot_id.to_string(),
        chat_session_key: chat.map(str::to_string),
        kind: kind.to_string(),
        detail: short_detail(detail),
        status: "open".to_string(),
        created_at: now.clone(),
        updated_at: now,
        ..Problem::default()
    }
}

/// Stores a new row under the next readable id (`P` and one more than the highest so far), which
/// it writes into `problem`.
pub(crate) fn insert_problem(
    db: &Connection,
    problem: &mut Problem,
) -> Result<(), DomainStateError> {
    let highest: Option<i64> = db
        .query_row(
            "SELECT MAX(CAST(SUBSTR(problemId, 2) AS INTEGER)) FROM gbots_problems \
             WHERE problemId GLOB 'P[0-9]*'",
            [],
            |row| row.get(0),
        )
        .map_err(sql)?;
    problem.problem_id = format!("P{}", highest.unwrap_or(0) + 1);
    db.execute(
        &format!(
            "INSERT INTO gbots_problems ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"
        ),
        rusqlite::params![
            problem.problem_id,
            problem.bot_id,
            problem.chat_session_key,
            problem.automation_id,
            problem.kind,
            problem.source,
            problem.detail,
            problem.user_only as i64,
            problem.prompt_key,
            problem.attempts,
            problem.ghostex_retried as i64,
            problem.status,
            problem.reported_to,
            problem.reported_at,
            problem.report_due_at,
            problem.report_tries,
            problem.retried_at,
            problem.closed_by,
            problem.created_at,
            problem.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// Writes back everything that changes after a row is made.
pub(crate) fn save_problem(db: &Connection, problem: &mut Problem) -> Result<(), DomainStateError> {
    problem.updated_at = now_iso();
    db.execute(
        "UPDATE gbots_problems SET kind = ?2, source = ?3, detail = ?4, userOnly = ?5, \
         promptKey = ?6, attempts = ?7, ghostexRetried = ?8, status = ?9, reportedTo = ?10, \
         reportedAt = ?11, reportDueAt = ?12, reportTries = ?13, retriedAt = ?14, closedBy = ?15, \
         updatedAt = ?16 WHERE problemId = ?1",
        rusqlite::params![
            problem.problem_id,
            problem.kind,
            problem.source,
            problem.detail,
            problem.user_only as i64,
            problem.prompt_key,
            problem.attempts,
            problem.ghostex_retried as i64,
            problem.status,
            problem.reported_to,
            problem.reported_at,
            problem.report_due_at,
            problem.report_tries,
            problem.retried_at,
            problem.closed_by,
            problem.updated_at,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// A row by id, any case (`p7` finds `P7`).
pub(crate) fn read_problem(
    db: &Connection,
    problem_id: &str,
) -> Result<Option<Problem>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM gbots_problems WHERE problemId = ?1 COLLATE NOCASE"),
        [problem_id.trim()],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// The chat's live problem of one family (an escalation, or any other), newest first.
pub(crate) fn live_problem_for_chat(
    db: &Connection,
    chat: &str,
    escalation: bool,
) -> Result<Option<Problem>, DomainStateError> {
    let comparison = if escalation { "=" } else { "<>" };
    db.query_row(
        &format!(
            "SELECT {COLUMNS} FROM gbots_problems WHERE chatSessionKey = ?1 \
             AND kind {comparison} '{NEEDS_YOU}' AND status IN ('open', 'retrying', 'handed') \
             ORDER BY createdAt DESC LIMIT 1"
        ),
        [chat],
        from_row,
    )
    .optional()
    .map_err(sql)
}

/// Live rows of a routine with no chat of their own (a run that never opened one).
pub(crate) fn live_routine_problems(
    db: &Connection,
    automation_id: &str,
) -> Result<Vec<Problem>, DomainStateError> {
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_problems WHERE automationId = ?1 \
             AND status IN ('open', 'retrying', 'handed')"
        ))
        .map_err(sql)?;
    let rows = statement
        .query_map([automation_id], from_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows)
}

/// Every live row (the watcher's), or the newest `limit` rows of any status.
pub(crate) fn list_problems(
    db: &Connection,
    live_only: bool,
    limit: usize,
) -> Result<Vec<Problem>, DomainStateError> {
    let filter = if live_only {
        "WHERE status IN ('open', 'retrying', 'handed')"
    } else {
        ""
    };
    let mut statement = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM gbots_problems {filter} ORDER BY createdAt DESC LIMIT {limit}"
        ))
        .map_err(sql)?;
    let rows = statement
        .query_map([], from_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(rows)
}

/// How a row reads in `/api/gbots` answers.
pub(crate) fn problem_summary(
    problem: &Problem,
    bot_name: Option<&str>,
    chat_ref: Option<&str>,
) -> Value {
    json!({
        "problemId": problem.problem_id,
        "botId": problem.bot_id,
        "botName": bot_name,
        "chat": chat_ref,
        "automationId": problem.automation_id,
        "kind": problem.kind,
        "source": problem.source,
        "detail": problem.detail,
        "userOnly": problem.user_only,
        "attempts": problem.attempts,
        "maxAttempts": MAX_ATTEMPTS,
        "ghostexRetried": problem.ghostex_retried,
        "status": problem.status,
        "reportedTo": problem.reported_to,
        "reportedAt": problem.reported_at,
        "closedBy": problem.closed_by,
        "createdAt": problem.created_at,
        "updatedAt": problem.updated_at,
    })
}
