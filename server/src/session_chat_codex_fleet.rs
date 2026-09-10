//! CDXC:SessionStatus 2026-09-06 WHY:
//! Codex's `/subagents` display is history, not live terminal chrome. Its persisted spawn graph identifies descendants, while each child's newest turn lifecycle record distinguishes working from completed or interrupted.
//! SEE-ALSO: Codex codex-rs/state/src/runtime/threads.rs and codex-rs/rollout/src/policy.rs (a9896da3).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::session_chat::SessionChatTranscriptAgent;
use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_fleet_transcript::latest_child_turn;

/// CDXC:SessionStatus 2026-09-07 WHY:
/// ClipBook's crashed /root/ui child retained an open spawn edge and task_started after Codex resumed without it; the startup hook was absent, so its old turn kept the session working forever.
/// Codex keeps loaded threads' rollouts open for append, including idle children. Require current process ownership before reading a historical child as live work, and verify the root rollout so an unreadable or mismatched process never declares the fleet idle.
fn live_rollout_paths(
    session: &Value,
    root_rollout: &Path,
) -> anyhow::Result<(HashSet<PathBuf>, i64)> {
    let (pid, started_at) = crate::session_chat_fleet_process::current_process(session, "codex")?;
    let paths: HashSet<_> = crate::zmx::process_open_file_paths(pid)
        .into_iter()
        .collect();
    anyhow::ensure!(
        paths.contains(root_rollout),
        "Codex process does not own the expected root rollout"
    );
    Ok((paths, started_at))
}

/// `Err` is unreadable evidence, so callers show the last roster as unavailable, with no running animation.
pub(crate) fn read_codex_fleet(session: &Value) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(root_id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let Some(rollout) = session
        .pointer("/runtimeSettings/agentSessionPath")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let rollout = Path::new(rollout);
    let home = rollout
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "sessions"))
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("Codex rollout is outside its sessions directory"))?;
    let db = Connection::open_with_flags(
        home.join("state_5.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    let mut query = db.prepare(
        "WITH RECURSIVE descendants(id) AS (
             SELECT child_thread_id FROM thread_spawn_edges WHERE parent_thread_id = ?1 AND status = 'open'
             UNION
             SELECT edge.child_thread_id FROM thread_spawn_edges edge JOIN descendants ON edge.parent_thread_id = descendants.id WHERE edge.status = 'open'
         ) SELECT threads.id, threads.rollout_path, threads.source FROM descendants JOIN threads ON threads.id = descendants.id WHERE threads.archived = 0 ORDER BY threads.id"
    )?;
    let children = query
        .query_map([root_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let now = chrono::Utc::now().timestamp_millis();
    let process_started = session
        .pointer("/runtimeSettings/sessionChatCodexStartedAt")
        .and_then(Value::as_str)
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis());
    let mut agents = Vec::new();
    let mut live_paths = None;
    for (id, path, source) in children {
        let Some(lifecycle) =
            latest_child_turn(Path::new(&path), SessionChatTranscriptAgent::Codex)?
        else {
            continue;
        };
        if !lifecycle.working {
            continue;
        }
        if process_started.is_some_and(|started| lifecycle.timestamp < started) {
            continue;
        }
        if live_paths.is_none() {
            live_paths = Some(live_rollout_paths(session, rollout)?);
        }
        let (paths, started_at) = live_paths.as_ref().unwrap();
        if lifecycle.timestamp < *started_at || !paths.contains(Path::new(&path)) {
            continue;
        }
        let source: Value = serde_json::from_str(&source)?;
        let spawn = source.pointer("/subagent/thread_spawn");
        let name = spawn
            .and_then(|spawn| {
                spawn
                    .get("agent_path")
                    .and_then(Value::as_str)
                    .or_else(|| spawn.get("agent_nickname").and_then(Value::as_str))
            })
            .unwrap_or(&id)
            .to_string();
        let model = crate::session_chat_subagent::model::read(
            Path::new(&path),
            SessionChatTranscriptAgent::Codex,
        )?;
        agents.push(SessionChatSubAgent {
            id: Some(id),
            started_at: lifecycle.started_at,
            working: true,
            name,
            model: model.model,
            effort: model.effort,
            task: None,
            elapsed_seconds: Some(now.saturating_sub(lifecycle.started_at).max(0) as u64 / 1000),
            tokens: None,
            nested: None,
        });
    }
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}
