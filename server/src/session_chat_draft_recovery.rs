//! CDXC:Drafts 2026-09-09 DECISION:
//! User: preserve unsent text across view transfers, keep independent recovery history, and never expire active unsent drafts.
//! A blank composer or a successful terminal paste is not evidence of submission. Only consumed revisions retire recovery records.

use crate::{domain::DomainStateError, session_chat_draft_versions::DraftVersion};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryDraft {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub content: String,
    pub updated_at: String,
    pub version: DraftVersion,
}

/// Save checkpoints before destructive edits, while coalescing uninterrupted appends.
pub fn record(
    db: &Connection,
    project: &str,
    session: &str,
    text: &str,
    version: &DraftVersion,
) -> Result<(), DomainStateError> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    db.execute(
        "UPDATE session_chat_draft_recovery SET checkpoint=1 WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision<?4 AND substr(?5,1,length(content))<>content",
        params![project, session, version.draft_id, version.revision, text],
    ).map_err(sql_error)?;
    if !text.is_empty() {
        db.execute(
            "INSERT OR IGNORE INTO session_chat_draft_recovery(projectId,sessionId,draftId,revision,content,updatedAt) VALUES (?1,?2,?3,?4,?5,?6)",
            params![project, session, version.draft_id, version.revision, text, now],
        ).map_err(sql_error)?;
        db.execute(
            "DELETE FROM session_chat_draft_recovery WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision<?4 AND checkpoint=0 AND substr(?5,1,length(content))=content",
            params![project, session, version.draft_id, version.revision, text],
        ).map_err(sql_error)?;
    }
    Ok(())
}

pub fn read(db: &Connection) -> Result<Vec<RecoveryDraft>, DomainStateError> {
    let mut statement = db.prepare(
        "SELECT r.projectId,r.sessionId,r.draftId,r.revision,r.content,r.updatedAt FROM session_chat_draft_recovery r LEFT JOIN session_chat_draft_versions v ON v.projectId=r.projectId AND v.sessionId=r.sessionId AND v.draftId=r.draftId WHERE r.revision>COALESCE(v.consumed,0) ORDER BY r.updatedAt DESC",
    ).map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let project: String = row.get(0)?;
            let session: String = row.get(1)?;
            let draft: String = row.get(2)?;
            let revision: i64 = row.get(3)?;
            Ok(RecoveryDraft {
                id: format!("{project}:{session}:{draft}:{revision}"),
                project_id: project,
                session_id: session,
                version: DraftVersion {
                    draft_id: draft,
                    revision,
                },
                content: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

pub fn acknowledge(
    db: &Connection,
    project: &str,
    session: &str,
    id: &str,
) -> Result<(), DomainStateError> {
    db.execute("UPDATE session_chat_draft_handoffs SET state='received' WHERE projectId=?1 AND sessionId=?2 AND id=?3", params![project, session, id]).map_err(sql_error)?;
    Ok(())
}
