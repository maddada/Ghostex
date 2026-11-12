//! How far an idle thread's watch got (idle_wait.rs), kept on the thread's row (migration 0077), so
//! a gxserver restart resumes its rounds instead of reporting, and nudging, the current one again.

use rusqlite::{params, Connection, OptionalExtension};

use super::records::sql_error;
use crate::domain::DomainStateError;

/// One idle stretch's progress: which stretch, where its rounds count from, and how many of them
/// its coordinator heard of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdleProgress {
    /// The stretch: the end of the turn its last report came from (`IdleWatch::since`), in ms.
    pub since: i64,
    /// Where its rounds count from: `since`, or the end of an outage after it, in ms.
    pub clock_from: i64,
    /// The rounds already reported.
    pub reported_rounds: u32,
}

impl IdleProgress {
    /// A stretch nothing happened in yet: what a thread's row with no saved progress stands for.
    pub fn fresh(since: i64) -> Self {
        Self {
            since,
            clock_from: since,
            reported_rounds: 0,
        }
    }
}

/// The idle progress saved on a thread's row, if any.
pub fn read_idle_progress(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<Option<IdleProgress>, DomainStateError> {
    db.query_row(
        r#"
        SELECT idleSinceMs, idleClockFromMs, idleReportedRounds FROM coordinator_threads
        WHERE projectId = ?1 AND sessionId = ?2 AND idleSinceMs IS NOT NULL
        "#,
        params![project_id, session_id],
        |row| {
            Ok(IdleProgress {
                since: row.get(0)?,
                clock_from: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                reported_rounds: row
                    .get::<_, Option<i64>>(2)?
                    .and_then(|rounds| u32::try_from(rounds).ok())
                    .unwrap_or(0),
            })
        },
    )
    .optional()
    .map_err(sql_error)
}

/// Saves a stretch's progress on the thread's row. It leaves `updatedAt` alone: the progress is
/// the supervisor's own bookkeeping, which no panel shows, and a resolved thread (the only kind
/// thread_reads.rs keeps in memory) has no idle watch.
pub fn save_idle_progress(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    progress: IdleProgress,
) -> Result<(), DomainStateError> {
    db.execute(
        r#"
        UPDATE coordinator_threads
        SET idleSinceMs = ?3, idleClockFromMs = ?4, idleReportedRounds = ?5
        WHERE projectId = ?1 AND sessionId = ?2
        "#,
        params![
            project_id,
            session_id,
            progress.since,
            progress.clock_from,
            i64::from(progress.reported_rounds)
        ],
    )
    .map_err(sql_error)?;
    Ok(())
}
