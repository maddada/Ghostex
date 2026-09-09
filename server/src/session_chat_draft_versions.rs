//! CDXC:Drafts 2026-09-06 DECISION:
//! User: use durable draft identities, increasing edit revisions, and consumed records so older copies cannot return after sending, while subsequent unsent messages survive.
//! Text comparisons cannot identify a draft after the user deletes or replaces part of it.

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{domain::DomainStateError, session_chat_queue::SessionChatDraft};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftVersion {
    pub draft_id: String,
    pub revision: i64,
}

pub fn parse(params: &Map<String, Value>) -> Result<Option<DraftVersion>, DomainStateError> {
    let Some(value) = params.get("draftVersion") else {
        return Ok(None);
    };
    let version: DraftVersion = serde_json::from_value(value.clone())
        .map_err(|_| DomainStateError::bad_request("Invalid draftVersion."))?;
    if version.draft_id.trim().is_empty()
        || version.draft_id.len() > 128
        || !(1..=9_007_199_254_740_991).contains(&version.revision)
    {
        return Err(DomainStateError::bad_request(
            "Invalid draft identity or revision.",
        ));
    }
    Ok(Some(version))
}

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

pub fn read(
    db: &Connection,
    project: &str,
    session: &str,
) -> Result<Option<SessionChatDraft>, DomainStateError> {
    let mut draft = db.query_row(
        "SELECT content, originClientId, updatedAt, draftId, revision, parked FROM session_chat_drafts WHERE projectId=?1 AND sessionId=?2",
        params![project, session], |row| {
            let id: Option<String> = row.get(3)?;
            Ok(SessionChatDraft {
                parked: row.get(5)?,
                content: row.get(0)?, origin_client_id: row.get(1)?, updated_at: row.get(2)?,
                version: id.map(|draft_id| Ok::<_, rusqlite::Error>(DraftVersion { draft_id, revision: row.get(4)? })).transpose()?,
                consumed_drafts: Vec::new(),
                delivered_drafts: Vec::new(),
            })
        },
    ).optional().map_err(sql_error)?;
    if let Some(draft) = draft.as_mut() {
        draft.delivered_drafts =
            crate::session_chat_delivered_drafts::read(db, Some((project, session)))?;
        let mut statement = db.prepare("SELECT draftId, consumed FROM session_chat_draft_versions WHERE projectId=?1 AND sessionId=?2 AND consumed>0 ORDER BY draftId").map_err(sql_error)?;
        draft.consumed_drafts = statement
            .query_map(params![project, session], |row| {
                Ok(DraftVersion {
                    draft_id: row.get(0)?,
                    revision: row.get(1)?,
                })
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
    }
    Ok(draft)
}

pub fn save(
    db: &Connection,
    project: &str,
    session: &str,
    client: &str,
    content: &str,
    version: &DraftVersion,
) -> Result<SessionChatDraft, DomainStateError> {
    if content.len() > crate::zmx::GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(
            "Draft exceeds the message size limit.",
        ));
    }
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    let current: Option<(i64, String, i64)> = transaction.query_row(
        "SELECT revision, content, consumed FROM session_chat_draft_versions WHERE projectId=?1 AND sessionId=?2 AND draftId=?3",
        params![project, session, version.draft_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(sql_error)?;
    if current
        .as_ref()
        .is_some_and(|(revision, previous, consumed)| {
            *revision == version.revision && previous != content && *consumed < version.revision
        })
    {
        use sha2::{Digest, Sha256};
        let identity = format!(
            "conflict-{:x}",
            Sha256::digest(
                format!("{}:{}:{}", version.draft_id, version.revision, content).as_bytes()
            )
        );
        crate::session_chat_draft_recovery::record(
            &transaction,
            project,
            session,
            content,
            &DraftVersion {
                draft_id: identity,
                revision: 1,
            },
        )?;
        transaction.commit().map_err(sql_error)?;
        return Err(DomainStateError::bad_request("Two editors changed the same draft revision. Both versions are in Recovered; edit the input to continue syncing."));
    }
    if current
        .as_ref()
        .is_none_or(|(_, _, consumed)| *consumed < version.revision)
    {
        crate::session_chat_draft_recovery::record(
            &transaction,
            project,
            session,
            content,
            version,
        )?;
    }
    let obsolete = current.as_ref().is_some_and(|(revision, _, consumed)| {
        *consumed >= version.revision || *revision > version.revision
    });
    if !obsolete
        && !current
            .as_ref()
            .is_some_and(|(revision, _, _)| *revision == version.revision)
    {
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        transaction.execute(
            "INSERT INTO session_chat_draft_versions(projectId,sessionId,draftId,revision,content,originClientId,updatedAt) VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(projectId,sessionId,draftId) DO UPDATE SET revision=excluded.revision,content=excluded.content,originClientId=excluded.originClientId,updatedAt=excluded.updatedAt",
            params![project, session, version.draft_id, version.revision, content, client, now],
        ).map_err(sql_error)?;
        transaction.execute(
            "INSERT INTO session_chat_drafts(projectId,sessionId,content,originClientId,updatedAt,draftId,revision) VALUES (?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(projectId,sessionId) DO UPDATE SET content=excluded.content,originClientId=excluded.originClientId,updatedAt=excluded.updatedAt,draftId=excluded.draftId,revision=excluded.revision,parked=0",
            params![project, session, content, client, now, version.draft_id, version.revision],
        ).map_err(sql_error)?;
    }
    if client == "gxserver-draft-handoff" && !obsolete {
        transaction.execute("UPDATE session_chat_drafts SET parked=0 WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision=?4",params![project,session,version.draft_id,version.revision]).map_err(sql_error)?;
    }
    let result = read(&transaction, project, session)?
        .ok_or_else(|| DomainStateError::bad_request("Draft state is missing."))?;
    transaction.commit().map_err(sql_error)?;
    Ok(result)
}

pub fn require_saved(
    db: &Connection,
    project: &str,
    session: &str,
    text: &str,
    version: &DraftVersion,
) -> Result<(), DomainStateError> {
    let saved: Option<(i64, String, i64)> = db.query_row(
        "SELECT revision, content, consumed FROM session_chat_draft_versions WHERE projectId=?1 AND sessionId=?2 AND draftId=?3",
        params![project, session, version.draft_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(sql_error)?;
    let valid = saved.is_some_and(|(revision, content, consumed)| {
        revision == version.revision && consumed < revision && content.trim() == text.trim()
    });
    if !valid {
        return Err(DomainStateError::bad_request(
            "The submitted draft revision is no longer available. Your text has not been sent.",
        ));
    }
    Ok(())
}

pub fn consume(
    db: &Connection,
    project: &str,
    session: &str,
    version: &DraftVersion,
) -> Result<(), DomainStateError> {
    let transaction = db.unchecked_transaction().map_err(sql_error)?;
    consume_in(&transaction, project, session, version)?;
    transaction.commit().map_err(sql_error)?;
    Ok(())
}

pub fn consume_in(
    transaction: &Connection,
    project: &str,
    session: &str,
    version: &DraftVersion,
) -> Result<(), DomainStateError> {
    transaction.execute("UPDATE session_chat_draft_handoffs SET state='consumed' WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision<=?4",params![project,session,version.draft_id,version.revision]).map_err(sql_error)?;
    transaction.execute(
        "UPDATE session_chat_draft_versions SET consumed=MAX(consumed,?4),content=CASE WHEN revision<=?4 THEN '' ELSE content END WHERE projectId=?1 AND sessionId=?2 AND draftId=?3",
        params![project, session, version.draft_id, version.revision],
    ).map_err(sql_error)?;
    transaction.execute(
        "UPDATE session_chat_drafts SET content='',originClientId='gxserver-chat-send',updatedAt=?4 WHERE projectId=?1 AND sessionId=?2 AND draftId=?3 AND revision<=?5",
        params![project, session, version.draft_id, Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true), version.revision],
    ).map_err(sql_error)?;
    Ok(())
}
