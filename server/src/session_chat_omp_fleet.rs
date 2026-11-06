//! CDXC:SessionStatus 2026-11-06 WHY:
//! OMP (oh-my-pi) runs `task` subagents as background jobs inside its own process, so its lead reads idle while they work. Each child writes its own transcript beside the lead's (`<lead>/<name>.jsonl`, nested children under `<lead>/<name>/`), opens it with `session_init` (agent, model, assignment), writes `<name>.md` when it hands its result back and `session_exit` when it is disposed (print mode; the TUI parks a finished child without one); a killed child gets `<name>.jsonl.tombstone`. A revived child appends after its `session_exit`, so only the newest record decides. A child silent for ten minutes without an exit is a crash leftover, not work, as in the Hermes reader.
//! Bagheera's Discord request (2026-10-28). A row opens its child's transcript in the shared subagent viewer (server/src/session_chat_subagent.rs): the row's id is the child's name, `child_transcript` resolves it the way the roster lists it, and the viewer's model and finished state come from the same reading as the row's.
//! SEE-ALSO: oh-my-pi 18.13.2 packages/coding-agent/src/registry/persisted-agents.ts (registerPersistedSubagentsFromDir) and src/task/executor.ts (the `<id>.md` output artifact).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::session_chat::{SessionChatTurnLifecycle, SessionChatTurnLifecycleState};
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
        .and_then(|path| lead_folder(Path::new(path)))
    else {
        return Ok(None);
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut agents = children(&lead, now, 0)?
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

/// The folder the lead session at `transcript` keeps its subagents' transcripts in: its own path
/// without `.jsonl`.
pub(crate) fn lead_folder(transcript: &Path) -> Option<PathBuf> {
    let lead = transcript.to_str()?.strip_suffix(".jsonl")?;
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
    Some(lead.to_path_buf())
}

/// The transcript of the child named `id` in `folder`, under the rule the roster lists children
/// by: a regular `<id>.jsonl` file whose name is not hidden, an advisor's or a backup. `None` for
/// any other name, so a name that is not one file name on this system (a path, `..`) opens
/// nothing. A backslash is a separator only on Windows; elsewhere OMP keeps it in a task name.
pub(crate) fn child_transcript(folder: &Path, id: &str) -> io::Result<Option<PathBuf>> {
    let file_name = format!("{id}.jsonl");
    let mut parts = Path::new(&file_name).components();
    let one_name = matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(name)), None) if name == file_name.as_str()
    );
    if !one_name
        || id.is_empty()
        || id.starts_with('.')
        || id.starts_with("__advisor")
        || id.contains(".bak")
        || id.contains('\0')
    {
        return Ok(None);
    }
    let path = folder.join(file_name);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => Ok(metadata.is_file().then_some(path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
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
        else {
            continue;
        };
        let Some(path) = child_transcript(dir, id)? else {
            continue;
        };
        if killed(&path) {
            continue;
        }
        let Some(child) = read_child(&path, id, now)? else {
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

fn read_child(path: &Path, id: &str, now: i64) -> anyhow::Result<Option<SessionChatSubAgent>> {
    let head = read_head(path)?;
    // A header without `session_init` is a spawn still claiming its name; OMP's roster skips it too.
    let (true, Some(started_at)) = (head.initialized, head.started_at) else {
        return Ok(None);
    };
    let tail = read_tail(path)?;
    let run = Run::read(path, &tail, Some(started_at), now);
    Ok(Some(SessionChatSubAgent {
        id: Some(id.to_string()),
        started_at,
        working: run.working,
        done: run.handed_back,
        name: id.to_string(),
        model: tail.model.or(head.model),
        effort: tail.effort.or(head.effort),
        task: Some(match tail.action.or(head.assignment) {
            Some(action) => format!("{id}: {action}"),
            None => id.to_string(),
        }),
        elapsed_seconds: Some(
            (if run.working { now } else { run.last_activity })
                .saturating_sub(started_at)
                .max(0) as u64
                / 1000,
        ),
        tokens: None,
        nested: None,
    }))
}

/// The newest run of the child at `path` for the subagent viewer, working exactly while its
/// roster row is; a killed child, whose row is gone, has finished.
pub(crate) fn child_lifecycle(path: &Path) -> io::Result<SessionChatTurnLifecycle> {
    let head = read_head(path)?;
    let tail = read_tail(path)?;
    let run = Run::read(
        path,
        &tail,
        head.started_at,
        chrono::Utc::now().timestamp_millis(),
    );
    Ok(SessionChatTurnLifecycle {
        state: if run.working && !killed(path) {
            SessionChatTurnLifecycleState::Working
        } else {
            SessionChatTurnLifecycleState::Completed
        },
        turn_id: tail.newest_record.unwrap_or_default(),
        timestamp: Some(run.last_activity),
    })
}

/// The model and effort of the child at `path` as its roster row shows them.
pub(crate) fn child_model(path: &Path) -> io::Result<(Option<String>, Option<String>)> {
    let head = read_head(path)?;
    let tail = read_tail(path)?;
    Ok((tail.model.or(head.model), tail.effort.or(head.effort)))
}

/// OMP marks a killed child with `<name>.jsonl.tombstone`.
fn killed(path: &Path) -> bool {
    let mut tombstone = path.as_os_str().to_owned();
    tombstone.push(".tombstone");
    Path::new(&tombstone).exists()
}

/// What a child's transcript opens with: `session_init` (agent, model, assignment) right after
/// the header.
#[derive(Default)]
struct Head {
    initialized: bool,
    started_at: Option<i64>,
    model: Option<String>,
    effort: Option<String>,
    assignment: Option<String>,
}

fn read_head(path: &Path) -> io::Result<Head> {
    let mut head = Head::default();
    for line in BufReader::new(File::open(path)?).lines().take(HEAD_RECORDS) {
        let Ok(record) = serde_json::from_str::<Value>(&line?) else {
            continue;
        };
        match record["type"].as_str() {
            Some("session") => head.started_at = head.started_at.or_else(|| timestamp(&record)),
            Some("model_change") => head.model = record["model"].as_str().map(str::to_string),
            Some("thinking_level_change") => {
                head.effort = record["thinkingLevel"].as_str().map(str::to_string)
            }
            Some("session_init") => {
                head.initialized = true;
                head.started_at = timestamp(&record).or(head.started_at);
                head.model = record["resolvedModel"]
                    .as_str()
                    .map(str::to_string)
                    .or(head.model);
                head.assignment = record["task"].as_str().and_then(summarize_assignment);
            }
            _ => {}
        }
    }
    Ok(head)
}

/// Whether a child still works on its newest run, one rule for its roster row and its viewer.
struct Run {
    working: bool,
    /// The result it handed back (`<name>.md`) was written after its newest message.
    handed_back: bool,
    last_activity: i64,
}

impl Run {
    fn read(path: &Path, tail: &Tail, started_at: Option<i64>, now: i64) -> Self {
        let modified = modified_millis(path).or(started_at).unwrap_or_default();
        let last_activity = tail
            .last_activity
            .unwrap_or(modified)
            .max(started_at.unwrap_or_default());
        // The TUI parks a finished child without `session_exit`: its turn ended, or the result it
        // handed back was written after its newest message. A run that ends on its final `yield`
        // stops on that tool result with no closing reply, so only the result tells it ended.
        let handed_back =
            modified_millis(&path.with_extension("md")).is_some_and(|at| at >= last_activity);
        Self {
            working: !tail.exited
                && !tail.turn_ended
                && !handed_back
                && now.saturating_sub(modified) < STALE_CHILD_MILLIS,
            handed_back,
            last_activity,
        }
    }
}

#[derive(Default)]
struct Tail {
    /// The newest lifecycle record is `session_exit`, not a later message from a revived child.
    exited: bool,
    /// The newest message is an assistant reply that ended its turn rather than calling a tool.
    turn_ended: bool,
    /// When the newest message was written.
    last_activity: Option<i64>,
    /// The id of the newest message or `session_exit`, the record that decides the run.
    newest_record: Option<String>,
    /// What the child last did: its newest tool call's intent, or the start of its newest reply.
    action: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

fn read_tail(path: &Path) -> io::Result<Tail> {
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
                    tail.newest_record = record["id"].as_str().map(str::to_string);
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
                if !lifecycle_seen {
                    lifecycle_seen = true;
                    tail.newest_record = record["id"].as_str().map(str::to_string);
                }
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
