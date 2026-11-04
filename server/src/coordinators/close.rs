//! Closing a coordinator closes the threads open under it, and restoring it brings them back.

use rusqlite::{params, Connection};

use super::records::{list_threads_for, now_iso, sql_error, SessionKey, ThreadRecord};
use crate::domain::{DomainRepository, DomainStateError};
use crate::presentation::effective_lifecycle_state;

/// The threads of a coordinator that are open in the sidebar: not done, and with a session that is
/// running or asleep. These are the ones closing the coordinator closes, and the ones the close
/// confirmation counts.
pub fn threads_open_under(
    db: &Connection,
    repository: &DomainRepository<'_>,
    coordinator: &SessionKey,
) -> Result<Vec<ThreadRecord>, DomainStateError> {
    Ok(list_threads_for(db, &coordinator.0, &coordinator.1)?
        .into_iter()
        .filter(|thread| !thread.is_resolved() && thread.key() != *coordinator)
        .filter(|thread| {
            repository
                .get_session(&thread.project_id, &thread.session_id)
                .ok()
                .flatten()
                .is_some_and(|session| {
                    matches!(
                        effective_lifecycle_state(&session).as_str(),
                        "running" | "sleeping"
                    )
                })
        })
        .collect())
}

/// Marks a thread as closed by its coordinator's close, so the restore brings it back and the
/// supervisor leaves it alone until then.
pub fn mark_thread_closed_with_coordinator(
    db: &Connection,
    thread: &SessionKey,
) -> Result<(), DomainStateError> {
    let timestamp = now_iso();
    db.execute(
        r#"
        UPDATE coordinator_threads SET closedWithCoordinatorAt = ?3, updatedAt = ?3
        WHERE projectId = ?1 AND sessionId = ?2
        "#,
        params![thread.0, thread.1, timestamp],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// The threads the coordinator's last close took with it that are still not done: the ones its
/// restore brings back. The marks stay until `clear_threads_closed_with_coordinator`, so the
/// supervisor does not report them closed while the restore is still bringing them back.
pub fn threads_closed_with_coordinator(
    db: &Connection,
    coordinator: &SessionKey,
) -> Result<Vec<ThreadRecord>, DomainStateError> {
    Ok(list_threads_for(db, &coordinator.0, &coordinator.1)?
        .into_iter()
        .filter(|thread| thread.closed_with_coordinator_at.is_some() && !thread.is_resolved())
        .collect())
}

/// Clears every mark under the coordinator once its restore is done; a thread the restore could
/// not bring back is then reported to the coordinator as closed, like any other.
pub fn clear_threads_closed_with_coordinator(
    db: &Connection,
    coordinator: &SessionKey,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads SET closedWithCoordinatorAt = NULL
        WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2
          AND closedWithCoordinatorAt IS NOT NULL
        "#,
        params![coordinator.0, coordinator.1],
    )
    .map_err(sql_error)?;
    Ok(())
}
