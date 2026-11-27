//! CDXC:SessionStatus 2026-09-06 WHY:
//! Codex's `/subagents` display is history, not live terminal chrome. Its persisted spawn graph identifies descendants, while each child's newest turn lifecycle record distinguishes working from completed or interrupted.
//! SEE-ALSO: Codex codex-rs/state/src/runtime/threads.rs and codex-rs/rollout/src/policy.rs (a9896da3).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
    let process = crate::session_chat_fleet_process::current_process_with_files(session, "codex")?;
    // CDXC:SessionStatus 2026-09-13 WHY:
    // Process discovery already enumerates Codex's open rollouts. Reuse that bounded, two-second observation instead of spawning lsof again for the same fleet probe.
    let paths: HashSet<_> = match process.open_file_paths {
        Some(paths) => paths.iter().cloned().collect(),
        None => crate::zmx::process_open_file_paths(process.pid)
            .into_iter()
            .collect(),
    };
    anyhow::ensure!(
        paths.contains(root_rollout),
        "Codex process does not own the expected root rollout"
    );
    Ok((paths, process.started_at))
}

/// How recently a working child's rollout must have been written to count as live without the
/// current process holding it open.
const CHILD_ROLLOUT_FRESH_MS: i64 = 3 * 60 * 1000;

/// CDXC:SessionStatus 2026-11-28 WHY:
/// GitHub #791: after a Native Codex session was resumed (`acp-host --agent codex --resume`), 16 sub-agents were writing their rollouts (each updated within two minutes, `thread_source: subagent`, parent edges to the lead or to one of its sub-agents) while the Subagents roster read 0. Ownership by the current process's open files (`live_rollout_paths`) is how a crashed child's stale "working" turn is kept out (the ClipBook case above), but it also dropped children whose rollout the current process does not hold: ones an earlier process of the same session started and still runs, and any it has not opened. A rollout written in the last few minutes is being written by someone, so that is proof enough of live work; a crashed child stops writing and falls back to the ownership rule once the window passes.
fn being_written(rollout: &Path, now_ms: i64) -> bool {
    std::fs::metadata(rollout)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|modified| i64::try_from(modified.as_millis()).ok())
        .is_some_and(|modified| now_ms.saturating_sub(modified) <= CHILD_ROLLOUT_FRESH_MS)
}

/// The session a Codex state database read is recorded under (`crash_report::step`).
fn crash_subject(session: &Value) -> String {
    format!(
        "{}/{}",
        session
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        session
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
}

/// BLOCKING (one query of the Codex state database beside `rollout`). The conversation
/// `root_id`'s open, unarchived descendants in Codex's spawn graph: id, rollout path and source.
/// `walk_limit` stops the graph walk itself after that many descendants (`None` walks it all).
fn open_descendants(
    rollout: &Path,
    root_id: &str,
    walk_limit: Option<usize>,
) -> anyhow::Result<Vec<(String, String, String)>> {
    let home = rollout
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "sessions"))
        .and_then(Path::parent)
        .ok_or_else(|| anyhow::anyhow!("Codex rollout is outside its sessions directory"))?;
    let db = ghostex_find::agent_store::open_agent_store(&home.join("state_5.sqlite"))?;
    db.busy_timeout(std::time::Duration::from_millis(100))?;
    // `?2` bounds the walk itself, not only what it returns (a limit on the outer select would
    // still walk and sort the whole graph): the walk stops after that many descendants, and the
    // lead and each descendant it takes hand it at most that many children, so a lead or child
    // with thousands of open spawn edges is not read in full either. The children are picked by
    // row id, one lookup each, whatever indexes Codex's table has. A negative limit is none.
    let mut query = db.prepare(
        "WITH RECURSIVE descendants(id) AS (
             SELECT child_thread_id FROM (SELECT child_thread_id FROM thread_spawn_edges WHERE parent_thread_id = ?1 AND status = 'open' LIMIT ?2)
             UNION
             SELECT edge.child_thread_id FROM descendants JOIN thread_spawn_edges edge ON edge.rowid IN (SELECT rowid FROM thread_spawn_edges WHERE parent_thread_id = descendants.id AND status = 'open' LIMIT ?2)
             LIMIT ?2
         ) SELECT threads.id, threads.rollout_path, threads.source FROM descendants JOIN threads ON threads.id = descendants.id WHERE threads.archived = 0 ORDER BY threads.id"
    )?;
    let walk_limit = walk_limit.map_or(-1, |limit| i64::try_from(limit).unwrap_or(i64::MAX));
    let children = query
        .query_map(rusqlite::params![root_id, walk_limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(children)
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
    // Reads Codex's own SQLite database while Codex writes it, for one session at a time.
    let Some(_step) = crate::crash_report::step(&crash_subject(session), "codexFleetRead") else {
        anyhow::bail!("Reading this Codex session's subagents crashed gxserver before and is skipped on this build");
    };
    let rollout = Path::new(rollout);
    let children = open_descendants(rollout, root_id, None)?;
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
        if !being_written(Path::new(&path), now) {
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
            done: false,
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

/// BLOCKING (one query of Codex's state database). The rollout files of the session's open
/// subagents, whose file times the stall watch counts as the lead's output
/// (coordinators/subagent_output.rs); `rollout` is the lead's own. The spawn-graph walk stops
/// after `walk_limit` descendants.
pub(crate) fn open_child_rollouts(
    session: &Value,
    rollout: &Path,
    walk_limit: usize,
) -> anyhow::Result<Vec<PathBuf>> {
    let Some(root_id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
    else {
        return Ok(Vec::new());
    };
    let Some(_step) = crate::crash_report::step(&crash_subject(session), "codexFleetRead") else {
        anyhow::bail!("Reading this Codex session's subagents crashed gxserver before and is skipped on this build");
    };
    Ok(open_descendants(rollout, root_id, Some(walk_limit))?
        .into_iter()
        .map(|(_, path, _)| PathBuf::from(path))
        .collect())
}
