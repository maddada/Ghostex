//! CDXC:Drafts 2026-10-23 DECISION:
//! User: preserve unsent text across view transfers, keep independent recovery history, and never expire active unsent drafts. Supersedes the 2026-09-09 entry: the user chose to prune the recovery history of closed (stopped) and deleted sessions and to cap it at 20 checkpoints per draft; the newest unsent checkpoint of a live session is never deleted.
//! A blank composer or a successful terminal paste is not evidence of submission. Only consumed revisions, a closed or deleted session, and the per-draft cap (`prune`) retire recovery records.

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

/// Checkpoints kept per draft; the newest are kept, so the newest unsent one always stays.
pub const MAX_CHECKPOINTS_PER_DRAFT: i64 = 20;

/// Retires the recovery history the user chose not to keep (see the DECISION above): every checkpoint of a session that is closed (`stopped`) or no longer exists, and every checkpoint of a draft beyond its 20 newest revisions. A live session's newest unsent checkpoint is never touched: the unsent revisions of a draft are always its highest (`consume_in` deletes the consumed ones), and the cap keeps the highest. Each rule is checked with a read first, so a pass with nothing to prune never takes the write lock. Returns the number of checkpoints deleted.
///
/// CDXC:Drafts 2026-10-23 WHY:
/// Nothing ever retired the history of a closed session or a long-edited draft, so the table reached 7,500 rows and 18 MB (one draft held 1,825 checkpoints), most of it for sessions nobody can reopen. The startup pass and the session sweep run this (server/src/server/background_tasks.rs). The session's current draft (`session_chat_drafts`), its revisions and its handoff receipts are untouched: `returned_version` and `require_saved` read those tables, and `record` only compares against the checkpoints that remain.
pub fn prune(db: &Connection) -> Result<usize, DomainStateError> {
    const CLOSED: &str = "NOT EXISTS (SELECT 1 FROM sessions s
        WHERE s.projectId = session_chat_draft_recovery.projectId
          AND s.sessionId = session_chat_draft_recovery.sessionId
          AND s.lifecycleState <> 'stopped')";
    let mut deleted = 0;
    let closed: bool = db
        .query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM session_chat_draft_recovery WHERE {CLOSED})"),
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if closed {
        deleted += db
            .execute(
                &format!("DELETE FROM session_chat_draft_recovery WHERE {CLOSED}"),
                [],
            )
            .map_err(sql_error)?;
    }
    let over_cap: bool = db
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM session_chat_draft_recovery GROUP BY projectId, sessionId, draftId HAVING count(*) > ?1)",
            [MAX_CHECKPOINTS_PER_DRAFT],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if over_cap {
        deleted += db
            .execute(
                "DELETE FROM session_chat_draft_recovery WHERE rowid IN (
                   SELECT rowid FROM (
                     SELECT rowid, row_number() OVER (
                       PARTITION BY projectId, sessionId, draftId ORDER BY revision DESC
                     ) AS position
                     FROM session_chat_draft_recovery
                   ) WHERE position > ?1
                 )",
                [MAX_CHECKPOINTS_PER_DRAFT],
            )
            .map_err(sql_error)?;
    }
    Ok(deleted)
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
