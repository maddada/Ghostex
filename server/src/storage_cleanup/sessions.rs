//! Which closed sessions have been idle long enough to drop from the database.

use std::time::Duration;

use rusqlite::{params, Connection};

use super::files::DAY;

/// CDXC:ServerDaemon 2026-11-03 DECISION:
/// User chose: a closed (stopped) session idle for 90 days is removed from the database, the way removing it from the closed sessions list does; pinned and favorite sessions are always kept. The agent's own transcript files are not touched.
pub(crate) const CLOSED_SESSION_MAX_IDLE: Duration = Duration::from_secs(90 * DAY.as_secs());

/// Up to `limit` `(projectId, sessionId)` pairs of closed, unpinned, unfavorited sessions last
/// updated before `cutoff_iso` (an RFC 3339 UTC time, the format `updatedAt` is stored in),
/// oldest first.
pub(crate) fn expired_closed_sessions(
    db: &Connection,
    cutoff_iso: &str,
    limit: usize,
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut statement = db.prepare(
        "SELECT projectId, sessionId FROM sessions
         WHERE lifecycleState = 'stopped'
           AND COALESCE(isPinned, 0) = 0
           AND COALESCE(isFavorite, 0) = 0
           AND updatedAt < ?1
         ORDER BY updatedAt ASC
         LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![cutoff_iso, limit as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect();
    rows
}
