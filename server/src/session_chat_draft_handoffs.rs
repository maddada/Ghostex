//! Durable transfer receipts. Receipt acknowledgement never retires unsent text.
use crate::{domain::DomainStateError, session_chat_draft_versions::DraftVersion};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}
pub fn result(
    db: &Connection,
    project: &str,
    session: &str,
    id: &str,
) -> Result<Option<Value>, DomainStateError> {
    db.query_row("SELECT content,draftId,revision,state FROM session_chat_draft_handoffs WHERE id=?1 AND projectId=?2 AND sessionId=?3", params![id,project,session], |row| {
        Ok(json!({"content": row.get::<_,String>(0)?, "draftVersion": {"draftId":row.get::<_,String>(1)?,"revision":row.get::<_,i64>(2)?},"handoffId":id,"state":row.get::<_,String>(3)?,"transferred":true}))
    }).optional().map_err(sql_error)
}
pub fn stage(
    db: &Connection,
    project: &str,
    session: &str,
    id: &str,
    content: &str,
    version: &DraftVersion,
    direction: &str,
) -> Result<(), DomainStateError> {
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    crate::session_chat_draft_recovery::record(&transaction, project, session, content, version)?;
    transaction.execute("INSERT INTO session_chat_draft_handoffs(id,projectId,sessionId,content,draftId,revision,direction,updatedAt) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![id,project,session,content,version.draft_id,version.revision,direction,chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis,true)]).map_err(sql_error)?;
    if direction == "terminal" {
        transaction.execute("UPDATE session_chat_draft_handoffs SET state='redirected' WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision<=?4 AND direction='chat' AND state='pending'",params![project,session,version.draft_id,version.revision]).map_err(sql_error)?;
    }
    transaction.commit().map_err(sql_error)
}
pub fn placed(db: &Connection, id: &str) -> Result<(), DomainStateError> {
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE session_chat_draft_handoffs SET state='placed' WHERE id=?1",
            [id],
        )
        .map_err(sql_error)?;
    transaction.execute("UPDATE session_chat_drafts SET parked=1 WHERE EXISTS (SELECT 1 FROM session_chat_draft_handoffs h WHERE h.id=?1 AND h.projectId=session_chat_drafts.projectId AND h.sessionId=session_chat_drafts.sessionId AND h.draftId=session_chat_drafts.draftId AND h.revision=session_chat_drafts.revision)",[id]).map_err(sql_error)?;
    transaction.commit().map_err(sql_error)?;
    Ok(())
}
pub fn returned_version(
    db: &Connection,
    project: &str,
    session: &str,
    content: &str,
) -> Result<DraftVersion, DomainStateError> {
    let previous: Option<(String,i64,String)> = db.query_row(
        "SELECT h.draftId,h.revision,h.content FROM session_chat_draft_handoffs h LEFT JOIN session_chat_draft_versions v ON v.projectId=h.projectId AND v.sessionId=h.sessionId AND v.draftId=h.draftId WHERE h.projectId=?1 AND h.sessionId=?2 AND h.direction='terminal' AND h.state='placed' AND h.revision>COALESCE(v.consumed,0) ORDER BY h.updatedAt DESC LIMIT 1",
        params![project,session], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).optional().map_err(sql_error)?;
    Ok(match previous {
        Some((draft_id, revision, text)) => {
            let newest: i64 = db.query_row("SELECT COALESCE(MAX(revision),0) FROM session_chat_draft_versions WHERE projectId=?1 AND sessionId=?2 AND draftId=?3",params![project,session,draft_id], |row| row.get(0)).map_err(sql_error)?;
            DraftVersion {
                draft_id,
                revision: if text == content && newest <= revision {
                    revision
                } else {
                    newest.max(revision) + 1
                },
            }
        }
        None => DraftVersion {
            draft_id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
        },
    })
}

pub fn pending_chat(
    db: &Connection,
    project: &str,
    session: &str,
) -> Result<Option<Value>, DomainStateError> {
    let id: Option<String> = db.query_row("SELECT id FROM session_chat_draft_handoffs WHERE projectId=?1 AND sessionId=?2 AND direction='chat' AND state='pending' ORDER BY updatedAt LIMIT 1",params![project,session],|row| row.get(0)).optional().map_err(sql_error)?;
    match id {
        Some(id) => result(db, project, session, &id),
        None => Ok(None),
    }
}
