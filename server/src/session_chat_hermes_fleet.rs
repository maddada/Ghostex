//! CDXC:SessionStatus 2026-09-28 WHY:
//! Hermes runs delegate_task subagents inside its own process, and a background batch hands the prompt back ("Keep chatting") while they work, so its hooks read idle. Its session store records each child (`source = 'subagent'` under `parent_session_id`) and closes it (`ended_at`) when it finishes, which is the same fleet evidence the Claude and Codex readers take from their own stores. A child whose activity stopped ten minutes ago is a crash leftover, not work.
//! SEE-ALSO: server/src/session_chat_fleet_status.rs (read_fleet), server/src/session_chat_subagent.rs (opening a child from the roster).

use anyhow::Context;
use serde_json::Value;

use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_hermes::{is_safe_hermes_session_id, open_hermes_state_db};

const STALE_CHILD_SECONDS: f64 = 600.0;

/// `Err` is unreadable evidence, so callers show the last roster as unavailable, with no running animation.
pub(crate) fn read_hermes_fleet(session: &Value) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(root_id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .filter(|id| is_safe_hermes_session_id(id))
    else {
        return Ok(None);
    };
    let connection =
        open_hermes_state_db(root_id).context("Hermes session store is unavailable")?;
    let now = chrono::Utc::now().timestamp() as f64;
    let mut query = connection.prepare(
        "SELECT s.id, s.started_at, s.model, \
                (SELECT m.content FROM messages m \
                 WHERE m.session_id = s.id AND m.role = 'user' ORDER BY m.id LIMIT 1) \
         FROM sessions s \
         WHERE s.parent_session_id = ?1 AND s.source = 'subagent' AND s.ended_at IS NULL \
           AND MAX(COALESCE(s.last_activity_at, 0), s.started_at, COALESCE( \
                 (SELECT MAX(m.timestamp) FROM messages m WHERE m.session_id = s.id), 0)) > ?2 \
         ORDER BY s.started_at, s.id",
    )?;
    let agents = query
        .query_map(
            rusqlite::params![root_id, now - STALE_CHILD_SECONDS],
            |row| {
                let started_at: f64 = row.get(1)?;
                Ok(SessionChatSubAgent {
                    id: Some(row.get(0)?),
                    started_at: (started_at * 1000.0) as i64,
                    working: true,
                    done: false,
                    name: "subagent".to_string(),
                    model: row.get(2)?,
                    effort: None,
                    task: row.get(3)?,
                    elapsed_seconds: Some((now - started_at).max(0.0) as u64),
                    tokens: None,
                    nested: None,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}

/// The title of one of `root_id`'s subagents, refusing any session that is not its child.
pub(crate) fn hermes_subagent_title(root_id: &str, child_id: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        is_safe_hermes_session_id(child_id),
        "Not a Hermes subagent id."
    );
    let connection =
        open_hermes_state_db(root_id).context("Hermes session store is unavailable")?;
    let title: Option<String> = connection
        .query_row(
            "SELECT title FROM sessions WHERE id = ?1 AND parent_session_id = ?2",
            rusqlite::params![child_id, root_id],
            |row| row.get(0),
        )
        .context("This subagent does not belong to this session.")?;
    Ok(title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| child_id.to_string()))
}
