//! What a working lead's subagents write, for the stall watch (stall.rs): a lead that handed its
//! work to subagents and waits on them writes nothing to its own transcript, so their transcript
//! writes count as its output.
//!
//! CDXC:Coordinators 2026-11-24 WHY:
//! The user kept getting the phone's "Not moving: it may be stuck" and a red dot for OMP sessions that were only waiting on their subagent while it worked: the quiet clock counted only the lead's own transcript and screen. A subagent's write to its own transcript now restarts its lead's clock exactly as a row of the lead's own does. That a subagent exists, is marked running or is counted on the screen does not count, so a subagent that is itself stuck lets its lead turn red one round after its last write.
//! Covered are the agents whose subagents write a transcript file of their own that gxserver can locate: OMP (`<lead>/<name>.jsonl`, a child's own subagents under `<lead>/<name>/`, session_chat_omp_fleet.rs), Claude (`<lead>/subagents/agent-<id>.jsonl` with the launch metadata that sets a subagent apart from Claude's internal prompt-suggestion transcripts, and workflow agents under `subagents/workflows/<run>/`, session_chat_claude_fleet.rs) and Codex (the rollouts of the open spawn graph in its state database, session_chat_codex_fleet.rs). OpenCode and Hermes keep their subagents' output in their own databases, and the transcript gxserver reads for them is a copy it writes itself, so their leads keep the lead-only clock.
//! Only file times are read (and the small launch metadata of a newly written Claude transcript), at most `SCAN_LIMIT` entries per look, and only once the lead itself has gone quiet.

use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::session_chat::SessionChatTranscriptAgent;

/// Directory entries (or Codex child rollouts) one look examines at most, nested folders included.
const SCAN_LIMIT: usize = 256;
/// How many levels of an OMP child's own subagents are followed, as deep as OMP's roster reads.
const OMP_NESTING: usize = 8;
/// The most of a Claude subagent's launch metadata read to tell it from an internal transcript.
const CLAUDE_META_MAX_BYTES: u64 = 64 * 1024;

/// BLOCKING (transcript path resolution, file stats and, per look, at most one query of Codex's
/// state database). The newest write after `after` (ms) to a transcript of one of the session's
/// subagents; `path` caches the lead's transcript as the watch resolved it.
pub(super) fn newest_subagent_write(
    session: &Value,
    path: &mut Option<PathBuf>,
    after: i64,
) -> Option<i64> {
    let agent = super::delivery::resolve_thread_transcript(session, path)?;
    newest_write_of(session, agent, path.as_deref()?, after)
}

/// `newest_subagent_write` for the lead transcript `lead` that `agent` writes.
fn newest_write_of(
    session: &Value,
    agent: SessionChatTranscriptAgent,
    lead: &Path,
    after: i64,
) -> Option<i64> {
    let mut budget = SCAN_LIMIT;
    let newest = match agent {
        SessionChatTranscriptAgent::Pi
            if crate::session_chat_follower::session_chat_display_agent_for_session(session)
                .as_deref()
                == Some("omp") =>
        {
            omp_newest(
                &crate::session_chat_omp_fleet::lead_folder(lead)?,
                OMP_NESTING,
                &mut budget,
            )
        }
        SessionChatTranscriptAgent::Claude => claude_newest(
            &lead.with_extension("").join("subagents"),
            after,
            &mut budget,
        ),
        SessionChatTranscriptAgent::Codex => {
            crate::session_chat_codex_fleet::open_child_rollouts(session, lead)
                .ok()?
                .iter()
                .take(budget)
                .filter_map(|rollout| std::fs::metadata(rollout).ok())
                .filter_map(|metadata| modified_ms(&metadata))
                .max()
        }
        _ => None,
    };
    newest.filter(|written| *written > after)
}

/// The newest child transcript write in OMP's `folder` and, up to `depth` levels down, in its
/// children's own folders.
fn omp_newest(folder: &Path, depth: usize, budget: &mut usize) -> Option<i64> {
    let mut newest = None;
    let mut nested = Vec::new();
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".jsonl"))
            .filter(|id| crate::session_chat_omp_fleet::is_child_name(id))
        else {
            continue;
        };
        let Some(metadata) = entry.metadata().ok().filter(Metadata::is_file) else {
            continue;
        };
        newest = newest.max(modified_ms(&metadata));
        if depth > 0 {
            nested.push(folder.join(id));
        }
    }
    for child in nested {
        newest = newest.max(omp_newest(&child, depth - 1, budget));
    }
    newest
}

/// The newest write to a Claude subagent transcript in `subagents`: an `agent-<id>.jsonl` whose
/// launch metadata names the tool call that started it (only those written after `after` are
/// checked), or a workflow agent's under `workflows/<run>/`.
fn claude_newest(subagents: &Path, after: i64, budget: &mut usize) -> Option<i64> {
    let mut written = Vec::new();
    for entry in std::fs::read_dir(subagents).ok()?.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        if !entry.file_name().to_str().is_some_and(is_claude_agent_file) {
            continue;
        }
        if let Some(at) = entry
            .metadata()
            .ok()
            .filter(Metadata::is_file)
            .and_then(|metadata| modified_ms(&metadata))
            .filter(|at| *at > after)
        {
            written.push((at, entry.path()));
        }
    }
    written.sort_unstable_by(|left, right| right.0.cmp(&left.0));
    let launched = written
        .into_iter()
        .find(|(_, transcript)| launched_by_tool_call(transcript))
        .map(|(at, _)| at);
    launched.max(claude_workflow_newest(&subagents.join("workflows"), budget))
}

/// The newest write to a workflow agent's transcript under Claude's `workflows/<run>/`.
fn claude_workflow_newest(workflows: &Path, budget: &mut usize) -> Option<i64> {
    let mut newest = None;
    for run in std::fs::read_dir(workflows).ok()?.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Ok(files) = std::fs::read_dir(run.path()) else {
            continue;
        };
        for file in files.flatten() {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            if file.file_name().to_str().is_some_and(is_claude_agent_file) {
                newest = newest.max(
                    file.metadata()
                        .ok()
                        .filter(Metadata::is_file)
                        .and_then(|metadata| modified_ms(&metadata)),
                );
            }
        }
    }
    newest
}

/// `agent-<id>.jsonl`, with an id Claude's subagent viewer accepts.
fn is_claude_agent_file(name: &str) -> bool {
    name.strip_prefix("agent-")
        .and_then(|name| name.strip_suffix(".jsonl"))
        .is_some_and(|id| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

/// Whether a Claude subagent transcript's `.meta.json` names the tool call that launched it.
fn launched_by_tool_call(transcript: &Path) -> bool {
    let Ok(file) = File::open(transcript.with_extension("meta.json")) else {
        return false;
    };
    let mut bytes = Vec::new();
    file.take(CLAUDE_META_MAX_BYTES)
        .read_to_end(&mut bytes)
        .is_ok()
        && serde_json::from_slice::<Value>(&bytes)
            .is_ok_and(|metadata| metadata.get("toolUseId").is_some())
}

fn modified_ms(metadata: &Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    const MIN: i64 = 60_000;
    const T0: i64 = 1_795_000_000_000;

    /// Writes `text` to `path` with its file time at `at` (ms).
    fn write_at(path: &Path, text: &str, at: i64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_millis(at as u64))
            .unwrap();
    }

    #[test]
    fn omp_counts_nested_children_and_skips_what_is_no_child() {
        let temp = tempfile::tempdir().unwrap();
        let lead = temp
            .path()
            .join("sessions/2026-11-24T00-00-00-000Z_lead.jsonl");
        write_at(&lead, "{}\n", T0);
        let folder = lead.with_extension("");
        write_at(&folder.join("Worker.jsonl"), "{}\n", T0 + 2 * MIN);
        // The worker's own subagent, the one still writing.
        write_at(
            &folder.join("Worker/Grandchild.jsonl"),
            "{}\n",
            T0 + 7 * MIN,
        );
        // Not subagent transcripts: the handed-back result, an advisor's and a backup.
        write_at(&folder.join("Worker.md"), "done", T0 + 8 * MIN);
        write_at(&folder.join("__advisor.jsonl"), "{}\n", T0 + 9 * MIN);
        write_at(&folder.join("Worker.bak.jsonl"), "{}\n", T0 + 9 * MIN);
        let session = json!({ "launchSettings": { "icon": "omp" } });
        let agent = SessionChatTranscriptAgent::Pi;
        assert_eq!(
            newest_write_of(&session, agent, &lead, T0),
            Some(T0 + 7 * MIN)
        );
        assert_eq!(newest_write_of(&session, agent, &lead, T0 + 7 * MIN), None);
        // A plain Pi session keeps no subagents beside its transcript.
        let pi = json!({ "launchSettings": { "icon": "pi" } });
        assert_eq!(newest_write_of(&pi, agent, &lead, T0), None);
    }

    #[test]
    fn claude_counts_launched_subagents_and_workflow_agents_only() {
        let temp = tempfile::tempdir().unwrap();
        let lead = temp.path().join("projects/-work/0b1c.jsonl");
        write_at(&lead, "{}\n", T0);
        let subagents = temp.path().join("projects/-work/0b1c/subagents");
        write_at(&subagents.join("agent-a1.jsonl"), "{}\n", T0 + 3 * MIN);
        write_at(
            &subagents.join("agent-a1.meta.json"),
            r#"{"agentType":"general-purpose","toolUseId":"toolu_1"}"#,
            T0 + MIN,
        );
        // Claude's internal prompt-suggestion transcript: no launch metadata.
        write_at(
            &subagents.join("agent-aprompt_suggestion-1.jsonl"),
            "{}\n",
            T0 + 9 * MIN,
        );
        let session = json!({});
        let agent = SessionChatTranscriptAgent::Claude;
        assert_eq!(
            newest_write_of(&session, agent, &lead, T0),
            Some(T0 + 3 * MIN)
        );
        write_at(
            &subagents.join("workflows/run-1/agent-w1.jsonl"),
            "{}\n",
            T0 + 6 * MIN,
        );
        assert_eq!(
            newest_write_of(&session, agent, &lead, T0),
            Some(T0 + 6 * MIN)
        );
        assert_eq!(newest_write_of(&session, agent, &lead, T0 + 6 * MIN), None);
    }

    #[test]
    fn codex_counts_the_rollouts_of_its_open_spawn_graph() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = temp.path().join("sessions/2026/11/24");
        let lead = sessions.join("rollout-lead.jsonl");
        write_at(&lead, "{}\n", T0);
        let open = sessions.join("rollout-open.jsonl");
        let grandchild = sessions.join("rollout-grandchild.jsonl");
        let closed = sessions.join("rollout-closed.jsonl");
        let other = sessions.join("rollout-other.jsonl");
        write_at(&open, "{}\n", T0 + 2 * MIN);
        write_at(&grandchild, "{}\n", T0 + 4 * MIN);
        write_at(&closed, "{}\n", T0 + 9 * MIN);
        write_at(&other, "{}\n", T0 + 9 * MIN);
        let db = rusqlite::Connection::open(temp.path().join("state_5.sqlite")).unwrap();
        db.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, source TEXT, archived INTEGER);
             CREATE TABLE thread_spawn_edges (parent_thread_id TEXT, child_thread_id TEXT, status TEXT);",
        )
        .unwrap();
        for (id, path, parent, status) in [
            ("open", &open, "lead", "open"),
            ("grandchild", &grandchild, "open", "open"),
            ("closed", &closed, "lead", "closed"),
            ("other", &other, "someone-else", "open"),
        ] {
            db.execute(
                "INSERT INTO threads VALUES (?1, ?2, '{}', 0)",
                (id, path.to_str().unwrap()),
            )
            .unwrap();
            db.execute(
                "INSERT INTO thread_spawn_edges VALUES (?1, ?2, ?3)",
                (parent, id, status),
            )
            .unwrap();
        }
        drop(db);
        let session = json!({
            "projectId": "p-codex-subagents",
            "sessionId": "s-codex-subagents",
            "runtimeSettings": { "agentSessionId": "lead" },
        });
        let agent = SessionChatTranscriptAgent::Codex;
        assert_eq!(
            newest_write_of(&session, agent, &lead, T0),
            Some(T0 + 4 * MIN)
        );
    }
}
