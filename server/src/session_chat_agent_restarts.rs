//! When gxserver restarted a session's agent because its Native host died in the middle of a turn
//! (server/server/session_self_heal_runtime.rs), so the chat can mark the turn that was cut off.

use rusqlite::{params, Connection};

/// The chat shows at most this many restart marks per session (the newest).
const KEPT_RESTARTS: i64 = 50;

/// CDXC:SessionChat 2026-11-24 WHY: a Native host that dies mid-turn leaves nothing in the agent's own transcript (Claude records the killed tool's "Exit code 137" and stops), so the chat showed a turn that just ended at its tool call. gxserver records the restart time here and sends it as `agentRestartedAt`; the chat core draws "Stopped when the agent restarted" at that time (`with_agent_restart_rows` in packages/gx-chat-core/src/transcript/agent_restarts.rs). A table, not a runtimeSettings key, because reloads, account switches and launches rewrite runtimeSettings.
pub(crate) fn record_agent_restarted_mid_turn(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    at_ms: i64,
) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO session_agent_restarts (projectId, sessionId, restartedAtMs) VALUES (?1, ?2, ?3)",
        params![project_id, session_id, at_ms],
    )?;
    db.execute(
        "DELETE FROM session_agent_restarts WHERE projectId = ?1 AND sessionId = ?2 AND restartedAtMs < \
         COALESCE((SELECT restartedAtMs FROM session_agent_restarts WHERE projectId = ?1 AND sessionId = ?2 \
         ORDER BY restartedAtMs DESC LIMIT 1 OFFSET ?3), 0)",
        params![project_id, session_id, KEPT_RESTARTS - 1],
    )?;
    Ok(())
}

/// The session's recorded mid-turn restarts, oldest first (epoch ms).
pub(crate) fn read_agent_restarted_at(
    db: &Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<i64> {
    let Ok(mut statement) = db.prepare_cached(
        "SELECT restartedAtMs FROM session_agent_restarts WHERE projectId = ?1 AND sessionId = ?2 \
         ORDER BY restartedAtMs DESC LIMIT ?3",
    ) else {
        return Vec::new();
    };
    let mut times: Vec<i64> = statement
        .query_map(params![project_id, session_id, KEPT_RESTARTS], |row| {
            row.get(0)
        })
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default();
    times.reverse();
    times
}
