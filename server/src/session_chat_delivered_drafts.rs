//! CDXC:DelayedSend 2026-09-08 WHY:
//! Delayed delivery runs without a composer, so its exact text and delivery time must survive until a client imports it into sent history.
//! Draft retirement alone cannot reconstruct the sent text after newer edits or a closed client.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::domain::DomainStateError;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveredDraft {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub text: String,
    pub delivered_at: String,
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

pub fn record(
    db: &Connection,
    project: &str,
    session: &str,
    text: &str,
) -> Result<(), DomainStateError> {
    db.execute(
        "INSERT INTO session_chat_delivered_drafts(id,projectId,sessionId,content,deliveredAt) VALUES (?1,?2,?3,?4,?5)",
        params![uuid::Uuid::new_v4().to_string(), project, session, text, chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)],
    ).map_err(sql_error)?;
    db.execute(
        "DELETE FROM session_chat_delivered_drafts WHERE id NOT IN (SELECT id FROM session_chat_delivered_drafts ORDER BY deliveredAt DESC, id DESC LIMIT 50)",
        [],
    ).map_err(sql_error)?;
    Ok(())
}

pub fn read(
    db: &Connection,
    session: Option<(&str, &str)>,
) -> Result<Vec<DeliveredDraft>, DomainStateError> {
    let mut statement = db.prepare(
        "SELECT id,projectId,sessionId,content,deliveredAt FROM session_chat_delivered_drafts WHERE (?1 IS NULL OR (projectId=?1 AND sessionId=?2)) ORDER BY deliveredAt DESC,id DESC",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![session.map(|key| key.0), session.map(|key| key.1)],
            |row| {
                Ok(DeliveredDraft {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    text: row.get(3)?,
                    delivered_at: row.get(4)?,
                })
            },
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}
