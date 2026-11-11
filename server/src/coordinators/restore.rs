//! A closed session restored from Previous Sessions keeps its place among coordinators.

use rusqlite::{params, Connection, Transaction, TransactionBehavior};

use super::records::{now_iso, sql_error, SessionKey};
use crate::domain::DomainStateError;

/// Moves the coordinator records of a closed session onto the session that restored it: its own
/// coordinator record (goal, instructions, memory), the threads it leads, its link when it was
/// a thread itself, and the record of the guide it last read (playbook.rs `carry_guide_records`). Nothing moves when the restored session already has a record of its own.
///
/// CDXC:Coordinators 2026-10-16 WHY: Restore makes a new session and deletes the closed one, and the coordinator tables are keyed by session with no foreign key, so a restored coordinator came back as a plain session while its record and every thread link stayed on the deleted id: the sidebar showed no crown and no threads, and thread reports had nowhere to go.
/// SEE-ALSO: server/src/previous_session_restore.rs (what the restore carries over), server/src/server/previous_session_restore_http.rs (which calls this).
pub fn carry_coordinator_to_restored_session(
    db: &Connection,
    from: &SessionKey,
    to: &SessionKey,
) -> Result<(), DomainStateError> {
    if from == to {
        return Ok(());
    }
    let transaction =
        Transaction::new_unchecked(db, TransactionBehavior::Immediate).map_err(sql_error)?;
    let timestamp = now_iso();
    transaction
        .execute(
            r#"
            UPDATE coordinators SET projectId = ?3, sessionId = ?4, updatedAt = ?5
            WHERE projectId = ?1 AND sessionId = ?2
              AND NOT EXISTS (SELECT 1 FROM coordinators WHERE projectId = ?3 AND sessionId = ?4)
            "#,
            params![from.0, from.1, to.0, to.1, timestamp],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            r#"
            UPDATE coordinator_threads SET coordinatorProjectId = ?3, coordinatorSessionId = ?4
            WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2
            "#,
            params![from.0, from.1, to.0, to.1],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            r#"
            UPDATE coordinator_threads SET projectId = ?3, sessionId = ?4, updatedAt = ?5
            WHERE projectId = ?1 AND sessionId = ?2
              AND NOT EXISTS (SELECT 1 FROM coordinator_threads WHERE projectId = ?3 AND sessionId = ?4)
            "#,
            params![from.0, from.1, to.0, to.1, timestamp],
        )
        .map_err(sql_error)?;
    super::playbook::carry_guide_records(&transaction, from, to)?;
    transaction.commit().map_err(sql_error)
}
