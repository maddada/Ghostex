//! CDXC:SessionStatus 2026-11-06 WHY:
//! OMP (oh-my-pi) runs `task` subagents as background jobs inside its own process, so its lead reads idle while they work. Each child writes its own transcript beside the lead's (`<lead>/<name>.jsonl`, nested children under `<lead>/<name>/`), opens it with `session_init` (agent, model, assignment), writes `<name>.md` when it hands its result back and `session_exit` when it is disposed (print mode; the TUI parks a finished child without one); a killed child gets `<name>.jsonl.tombstone`. A revived child appends after its `session_exit`, so only the newest record decides. A child silent for ten minutes without an exit is a crash leftover, not work, as in the Hermes reader.
//! Bagheera's Discord request (2026-10-28), answered with the pill only: the rows do not open a transcript yet (packages/gx-chat-core/src/extras/agent_fleet.rs leaves OMP rows unlinked).
//! SEE-ALSO: oh-my-pi 18.13.2 packages/coding-agent/src/registry/persisted-agents.ts (registerPersistedSubagentsFromDir) and src/task/executor.ts (the `<id>.md` output artifact).

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};

const STALE_CHILD_MILLIS: i64 = 600_000;
/// `session_init` sits right after the header, as OMP's own roster expects (`MAX_METADATA_LINES`).
const HEAD_RECORDS: usize = 64;
/// Enough of a transcript's end to find its newest message past a large tool result.
const TAIL_BYTES: u64 = 256 * 1024;

/// `Err` is unreadable evidence, so callers show the last roster as unavailable, with no running animation.
pub(crate) fn read_omp_fleet(session: &Value) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(lead) = session
        .pointer("/runtimeSettings/agentSessionPath")
        .and_then(Value::as_str)
        .and_then(|path| path.strip_suffix(".jsonl"))
    else {
        return Ok(None);
    };
    // An OMP extension before v7 let a child's hooks rebind the row to the child's transcript
    // (`<lead>/<name>.jsonl`); climb back to the lead, whose folder is the parent's.
    let mut lead = Path::new(lead);
    while let Some(parent) = lead.parent().filter(|parent| {
        parent.file_name().is_some_and(|name| {
            parent
                .with_file_name(format!("{}.jsonl", name.to_string_lossy()))
                .is_file()
        })
    }) {
        lead = parent;
    }
    let now = chrono::Utc::now().timestamp_millis();
    let mut agents = children(lead, now, 0)?
        .into_iter()
        .map(|(agent, _)| agent)
        .collect::<Vec<_>>();
    agents.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}

/// The children in `dir`, each with how many of its own descendants are working.
fn children(dir: &Path, now: i64, depth: usize) -> anyhow::Result<Vec<(SessionChatSubAgent, u32)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut agents = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(id) = file_name
            .to_str()
            .and_then(|name| name.strip_suffix(".jsonl"))
            .filter(|id| !id.is_empty() && !id.starts_with('.') && !id.starts_with("__advisor"))
            .filter(|id| !id.contains(".bak"))
        else {
            continue;
        };
        let path = entry.path();
        if !entry.file_type()?.is_file() || dir.join(format!("{id}.jsonl.tombstone")).exists() {
            continue;
        }
        let output = modified_millis(&dir.join(format!("{id}.md")));
        let Some(child) = read_child(&path, id, output, now)? else {
            continue;
        };
        // Grandchildren fold into their parent's row, as Claude's `(+1)` does.
        let nested = if depth < 8 {
            children(&dir.join(id), now, depth + 1)?
                .iter()
                .map(|(agent, nested)| u32::from(agent.working) + nested)
                .sum()
        } else {
            0
        };
        let mut child = child;
        child.nested = (nested > 0).then_some(nested);
        agents.push((child, nested));
    }
    Ok(agents)
}

fn read_child(
    path: &Path,
    id: &str,
    output_written_at: Option<i64>,
    now: i64,
) -> anyhow::Result<Option<SessionChatSubAgent>> {
    let mut started_at = None;
    let mut model = None;
    let mut effort = None;
    let mut assignment = None;
    let mut initialized = false;
    for line in BufReader::new(File::open(path)?).lines().take(HEAD_RECORDS) {
        let Ok(record) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        match record["type"].as_str() {
            Some("session") => started_at = started_at.or_else(|| timestamp(&record)),
            Some("model_change") => model = record["model"].as_str().map(str::to_string),
            Some("thinking_level_change") => {
                effort = record["thinkingLevel"].as_str().map(str::to_string)
            }
            Some("session_init") => {
                initialized = true;
                started_at = timestamp(&record).or(started_at);
                model = record["resolvedModel"]
                    .as_str()
                    .map(str::to_string)
                    .or(model);
                assignment = record["task"].as_str().and_then(summarize_assignment);
            }
            _ => {}
        }
    }
    // A header without `session_init` is a spawn still claiming its name; OMP's roster skips it too.
    let (true, Some(started_at)) = (initialized, started_at) else {
        return Ok(None);
    };
    let tail = read_tail(path)?;
    let modified = modified_millis(path).unwrap_or(started_at);
    let last_activity = tail.last_activity.unwrap_or(modified).max(started_at);
    // The TUI parks a finished child without `session_exit`: its turn ended, and the result it
    // handed back (`<name>.md`) was written after its newest message.
    let working =
        !tail.exited && !tail.turn_ended && now.saturating_sub(modified) < STALE_CHILD_MILLIS;
    Ok(Some(SessionChatSubAgent {
        id: Some(id.to_string()),
        started_at,
        working,
        done: !working && output_written_at.is_some_and(|at| at >= last_activity),
        name: id.to_string(),
        model: tail.model.or(model),
        effort: tail.effort.or(effort),
        task: Some(match tail.action.or(assignment) {
            Some(action) => format!("{id}: {action}"),
            None => id.to_string(),
        }),
        elapsed_seconds: Some(
            (if working { now } else { last_activity })
                .saturating_sub(started_at)
                .max(0) as u64
                / 1000,
        ),
        tokens: None,
        nested: None,
    }))
}

#[derive(Default)]
struct Tail {
    /// The newest lifecycle record is `session_exit`, not a later message from a revived child.
    exited: bool,
    /// The newest message is an assistant reply that ended its turn rather than calling a tool.
    turn_ended: bool,
    /// When the newest message was written.
    last_activity: Option<i64>,
    /// What the child last did: its newest tool call's intent, or the start of its newest reply.
    action: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

fn read_tail(path: &Path) -> anyhow::Result<Tail> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines().rev().collect::<Vec<_>>();
    // A tail that starts mid-record drops its first, partial line.
    if start > 0 {
        lines.pop();
    }
    let mut tail = Tail::default();
    let mut lifecycle_seen = false;
    let mut message_seen = false;
    for line in lines {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match record["type"].as_str() {
            Some("custom") if record["customType"] == "session_exit" => {
                if !lifecycle_seen {
                    lifecycle_seen = true;
                    tail.exited = true;
                }
            }
            Some("message") => {
                if !message_seen {
                    message_seen = true;
                    tail.last_activity = timestamp(&record);
                    tail.turn_ended = !lifecycle_seen
                        && record["message"]["role"] == "assistant"
                        && record["message"]["stopReason"]
                            .as_str()
                            .is_some_and(|reason| reason != "toolUse");
                }
                lifecycle_seen = true;
                if tail.action.is_none() && record["message"]["role"] == "assistant" {
                    tail.action = action(&record["message"]);
                }
            }
            Some("model_change") if tail.model.is_none() => {
                tail.model = record["model"].as_str().map(str::to_string)
            }
            Some("thinking_level_change") if tail.effort.is_none() => {
                tail.effort = record["thinkingLevel"].as_str().map(str::to_string)
            }
            _ => {}
        }
        if message_seen && tail.action.is_some() && tail.model.is_some() && tail.effort.is_some() {
            break;
        }
    }
    Ok(tail)
}

/// The newest tool call's `i` (the intent OMP shows beside it), else the reply's first line.
fn action(message: &Value) -> Option<String> {
    let content = message["content"].as_array()?;
    content
        .iter()
        .rev()
        .filter(|part| part["type"] == "toolCall")
        .find_map(|part| part["arguments"]["i"].as_str())
        .or_else(|| {
            content
                .iter()
                .rev()
                .filter(|part| part["type"] == "text")
                .find_map(|part| part["text"].as_str())
                .and_then(|text| text.lines().find(|line| !line.trim().is_empty()))
        })
        .map(one_line)
        .filter(|action| !action.is_empty())
}

/// OMP's `summarizePersistedTask`: the `# Target` section when the assignment has one, else all of it.
fn summarize_assignment(task: &str) -> Option<String> {
    let lines = task.lines().collect::<Vec<_>>();
    let target = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("# target"))
        .map(|at| {
            lines[at + 1..]
                .iter()
                .take_while(|line| !line.trim_start().starts_with("# "))
                .copied()
                .collect::<Vec<_>>()
        })
        .filter(|target| target.iter().any(|line| !line.trim().is_empty()));
    let summary = one_line(&target.unwrap_or(lines).join(" "));
    (!summary.is_empty()).then_some(summary)
}

fn one_line(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(200) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

fn modified_millis(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as i64,
    )
}

fn timestamp(record: &Value) -> Option<i64> {
    record["timestamp"]
        .as_str()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis())
}
