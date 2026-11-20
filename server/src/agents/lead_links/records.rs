//! The `lead_links` rows (migration 0084): which session reports to which lead, and the watcher's
//! bookkeeping for each link.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::coordinators::{now_iso, sql_error, SessionKey};
use crate::domain::DomainStateError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeadLink {
    pub project_id: String,
    pub session_id: String,
    pub lead_project_id: String,
    pub lead_session_id: String,
    /// When the lead last sent it a message whose turn is not reported yet.
    pub owed_since: Option<String>,
    pub observed_working: bool,
    pub reported_at: Option<String>,
    pub reported_prompt_key: Option<String>,
    pub reported_quiet_since: Option<i64>,
    pub created_at: String,
}

impl LeadLink {
    pub fn key(&self) -> SessionKey {
        (self.project_id.clone(), self.session_id.clone())
    }

    pub fn lead_key(&self) -> SessionKey {
        (self.lead_project_id.clone(), self.lead_session_id.clone())
    }
}

const COLUMNS: &str = "projectId, sessionId, leadProjectId, leadSessionId, owedSince, observedWorking, reportedAt, reportedPromptKey, reportedQuietSince, createdAt";

fn from_row(row: &Row<'_>) -> rusqlite::Result<LeadLink> {
    Ok(LeadLink {
        project_id: row.get(0)?,
        session_id: row.get(1)?,
        lead_project_id: row.get(2)?,
        lead_session_id: row.get(3)?,
        owed_since: row.get(4)?,
        observed_working: row.get::<_, i64>(5)? != 0,
        reported_at: row.get(6)?,
        reported_prompt_key: row.get(7)?,
        reported_quiet_since: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn query(
    db: &Connection,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<LeadLink>, DomainStateError> {
    let mut statement = db
        .prepare_cached(&format!(
            "SELECT {COLUMNS} FROM lead_links {filter} ORDER BY createdAt"
        ))
        .map_err(sql_error)?;
    let rows = statement
        .query_map(args, from_row)
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

pub fn list_lead_links(db: &Connection) -> Result<Vec<LeadLink>, DomainStateError> {
    query(db, "", [])
}

pub fn links_of_lead(
    db: &Connection,
    lead_project_id: &str,
    lead_session_id: &str,
) -> Result<Vec<LeadLink>, DomainStateError> {
    query(
        db,
        "WHERE leadProjectId = ?1 AND leadSessionId = ?2",
        params![lead_project_id, lead_session_id],
    )
}

pub fn read_lead_link(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<LeadLink>, DomainStateError> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM lead_links WHERE projectId = ?1 AND sessionId = ?2"),
        params![project_id, session_id],
        from_row,
    )
    .optional()
    .map_err(sql_error)
}

/// Whether the session leads at least one linked session.
pub fn is_lead(db: &Connection, project_id: &str, session_id: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM lead_links WHERE leadProjectId = ?1 AND leadSessionId = ?2 LIMIT 1",
        params![project_id, session_id],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

/// Links a session to a lead (moving it when it had another). `owed` says the lead handed it a
/// task with the link, so the turn that task starts is reported.
pub fn write_lead_link(
    db: &Connection,
    linked: &SessionKey,
    lead: &SessionKey,
    owed: bool,
) -> Result<(), DomainStateError> {
    let now = now_iso();
    let owed_since = owed.then(|| now.clone());
    db.execute(
        r#"
        INSERT INTO lead_links (
          projectId, sessionId, leadProjectId, leadSessionId, owedSince, observedWorking,
          reportedAt, reportedPromptKey, reportedQuietSince, createdAt, updatedAt
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, NULL, NULL, ?6, ?6)
        ON CONFLICT(projectId, sessionId) DO UPDATE SET
          leadProjectId = excluded.leadProjectId,
          leadSessionId = excluded.leadSessionId,
          owedSince = COALESCE(excluded.owedSince, lead_links.owedSince),
          updatedAt = excluded.updatedAt
        "#,
        params![linked.0, linked.1, lead.0, lead.1, owed_since, now],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Removes one link. Returns the lead it had, if any.
pub fn remove_lead_link(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<SessionKey>, DomainStateError> {
    let link = read_lead_link(db, project_id, session_id)?;
    if link.is_some() {
        db.execute(
            "DELETE FROM lead_links WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        )
        .map_err(sql_error)?;
    }
    Ok(link.map(|link| link.lead_key()))
}

/// Removes every link to a lead. Returns the sessions that were linked.
pub fn remove_links_of_lead(
    db: &Connection,
    lead_project_id: &str,
    lead_session_id: &str,
) -> Result<Vec<SessionKey>, DomainStateError> {
    let linked = links_of_lead(db, lead_project_id, lead_session_id)?
        .iter()
        .map(LeadLink::key)
        .collect::<Vec<_>>();
    db.execute(
        "DELETE FROM lead_links WHERE leadProjectId = ?1 AND leadSessionId = ?2",
        params![lead_project_id, lead_session_id],
    )
    .map_err(sql_error)?;
    Ok(linked)
}

/// The lead sent the linked session a message: the turn it starts is owed a report. A send while
/// one is already owed keeps the older stamp, so that turn's reply still counts.
pub fn mark_lead_send(db: &Connection, linked: &SessionKey) -> Result<bool, DomainStateError> {
    let changed = db
        .execute(
            "UPDATE lead_links SET owedSince = COALESCE(owedSince, ?3), updatedAt = ?3 WHERE projectId = ?1 AND sessionId = ?2",
            params![linked.0, linked.1, now_iso()],
        )
        .map_err(sql_error)?;
    Ok(changed > 0)
}

pub fn set_link_observed_working(
    db: &Connection,
    linked: &SessionKey,
    working: bool,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE lead_links SET observedWorking = ?3 WHERE projectId = ?1 AND sessionId = ?2",
        params![linked.0, linked.1, i64::from(working)],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// A finished turn was reported: nothing is owed until the lead sends again.
pub fn settle_finished(db: &Connection, linked: &SessionKey) -> Result<(), DomainStateError> {
    let now = now_iso();
    db.execute(
        "UPDATE lead_links SET owedSince = NULL, observedWorking = 0, reportedAt = ?3, reportedPromptKey = NULL, reportedQuietSince = NULL, updatedAt = ?3 WHERE projectId = ?1 AND sessionId = ?2",
        params![linked.0, linked.1, now],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn settle_waiting(
    db: &Connection,
    linked: &SessionKey,
    prompt_key: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE lead_links SET reportedPromptKey = ?3, updatedAt = ?4 WHERE projectId = ?1 AND sessionId = ?2",
        params![linked.0, linked.1, prompt_key, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}

pub fn settle_quiet(
    db: &Connection,
    linked: &SessionKey,
    quiet_since: i64,
) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE lead_links SET reportedQuietSince = ?3, updatedAt = ?4 WHERE projectId = ?1 AND sessionId = ?2",
        params![linked.0, linked.1, quiet_since, now_iso()],
    )
    .map_err(sql_error)?;
    Ok(())
}
