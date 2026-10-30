//! CDXC:SessionStatus 2026-10-30 WHY:
//! OMP writes every subagent's transcript into the session's artifacts folder (`<session file minus .jsonl>/<id>.jsonl`; a child's own children go into a folder named like its id), and OMP's own Agent Hub lists a session's subagents by scanning those folders (`registerPersistedSubagentsFromDir` in OMP 18.13.3 `registry/persisted-agents.ts`). The roster is the list that rule gives, one row per subagent at any depth, rather than the parent's `task` tool records or the screen, so every subagent of the session appears whatever its role (`scout`, `reviewer`, `planner`, the user's own agent definitions). Each child's own transcript decides whether it is working.
//! SEE-ALSO: server/src/session_chat_fleet_status.rs (read_fleet), server/src/session_chat_fleet_transcript.rs (the OMP child turn), server/src/session_chat_subagent.rs (opening a child from the roster).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use crate::session_chat::{
    read_transcript_file_version, SessionChatTranscriptAgent, TranscriptFileVersion,
    MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES,
};
use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_fleet_transcript::{latest_child_turn, parse_time};

/// The leading records OMP reads a child's metadata from (`MAX_METADATA_LINES`).
const METADATA_RECORDS: usize = 64;
/// OMP's cap on the Hub's one-line task summary (`summarizePersistedTask`).
const TASK_SUMMARY_CHARS: usize = 1_000;

/// What one child transcript says about its child, read forward once and then only as it grows.
#[derive(Clone, Debug, Default)]
pub(crate) struct OmpChildRecords {
    /// The session header's time, else the `session_init` time.
    started_at: Option<i64>,
    /// `session_init.agent`: the agent definition the child runs (`scout`, `bounded-builder`).
    pub agent: Option<String>,
    task: Option<String>,
    /// A `session_init`, message or custom message among the leading records. OMP lists a file
    /// without one only when it is tombstoned: it is a stub whose spawn has not claimed it yet.
    opened: bool,
    records: usize,
    /// The latest `model_change`, `provider/id` as OMP writes it.
    pub model: Option<String>,
    /// The latest `thinking_level_change`.
    pub effort: Option<String>,
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// OMP's `summarizePersistedTask`: the assignment without its delegation preamble, only its
/// `# Target` section when it has one, on one line and bounded.
fn summarize_task(task: &str) -> Option<String> {
    const PREAMBLE: &str = "complete the assignment below,";
    const THOROUGHLY: &str = "thoroughly:";
    let body = task
        .get(..PREAMBLE.len())
        .filter(|head| head.eq_ignore_ascii_case(PREAMBLE))
        .map(|_| task[PREAMBLE.len()..].trim_start())
        .and_then(|rest| {
            rest.get(..THOROUGHLY.len())
                .filter(|head| head.eq_ignore_ascii_case(THOROUGHLY))
                .map(|_| rest[THOROUGHLY.len()..].trim_start())
        })
        .unwrap_or(task);
    let lines: Vec<&str> = body.lines().collect();
    let target: Vec<&str> = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("# target"))
        .map(|index| {
            lines[index + 1..]
                .iter()
                .take_while(|line| !line.trim_start().starts_with("# "))
                .copied()
                .collect()
        })
        .unwrap_or_default();
    let source = if target.is_empty() { lines } else { target };
    let summary = source
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!summary.is_empty()).then(|| summary.chars().take(TASK_SUMMARY_CHARS).collect())
}

fn first_text(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => blocks.iter().find_map(|block| {
            (block.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| text(block.get("text")))
                .flatten()
        }),
        _ => None,
    }
}

/// OMP's optional fixed-width title slot on a session file's first line (`parseTitleSlotLine`).
/// OMP's reader peels it off before it counts records.
fn is_title_slot(line: &[u8]) -> bool {
    let Ok(Value::Object(slot)) = serde_json::from_slice::<Value>(line) else {
        return false;
    };
    slot.get("type").and_then(Value::as_str) == Some("title")
        && slot.get("v").and_then(Value::as_f64) == Some(1.0)
        && ["title", "updatedAt", "pad"]
            .iter()
            .all(|key| slot.get(*key).is_some_and(Value::is_string))
        && slot
            .get("source")
            .is_none_or(|source| matches!(source.as_str(), Some("auto" | "user")))
}

fn observe(records: &mut OmpChildRecords, line: &[u8], first: bool) {
    if line.iter().all(u8::is_ascii_whitespace) || (first && is_title_slot(line)) {
        return;
    }
    let leading = records.records < METADATA_RECORDS;
    records.records += 1;
    let Ok(line) = std::str::from_utf8(line) else {
        return;
    };
    // Only these records carry anything the roster shows; the rest are skipped unparsed.
    if !(line.contains("\"model_change\"")
        || line.contains("\"thinking_level_change\"")
        || (records.started_at.is_none() && line.contains("\"type\":\"session\""))
        || (leading
            && (line.contains("\"session_init\"")
                || line.contains("\"type\":\"message\"")
                || line.contains("\"type\":\"custom_message\""))))
    {
        return;
    }
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let at = || {
        record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_time)
    };
    match record.get("type").and_then(Value::as_str) {
        Some("session") => records.started_at = records.started_at.or_else(at),
        Some("model_change") => records.model = text(record.get("model")),
        Some("thinking_level_change") => records.effort = text(record.get("thinkingLevel")),
        Some("session_init") if leading => {
            records.opened = true;
            records.started_at = records.started_at.or_else(at);
            records.agent = text(record.get("agent"));
            records.task = record
                .get("task")
                .and_then(Value::as_str)
                .and_then(summarize_task);
        }
        Some(kind @ ("message" | "custom_message")) if leading => {
            records.opened = true;
            if kind == "message"
                && records.task.is_none()
                && record.pointer("/message/role").and_then(Value::as_str) == Some("user")
            {
                records.task =
                    first_text(record.pointer("/message/content")).and_then(|t| summarize_task(&t));
            }
        }
        _ => {}
    }
}

/// What a forward reader keeps for one file between reads.
#[derive(Clone)]
struct Incremental<T> {
    version: TranscriptFileVersion,
    consumed_to: u64,
    state: T,
}

type IncrementalCache<T> = Mutex<HashMap<PathBuf, Incremental<T>>>;

/// `state` as of the file's last complete record, cached by file version: an unchanged file is not
/// read again, and a grown one only from where the last read stopped. `visit` gets each record and
/// whether it is the file's first line. The cache is cleared when it holds 4096 files.
fn read_incremental<T: Clone + Default>(
    cache: &IncrementalCache<T>,
    path: &Path,
    visit: fn(&mut T, &[u8], bool),
) -> io::Result<T> {
    let version = read_transcript_file_version(path)?;
    let previous = cache.lock().ok().and_then(|cache| cache.get(path).cloned());
    if let Some(previous) = previous.as_ref().filter(|entry| entry.version == version) {
        return Ok(previous.state.clone());
    }
    // OMP only appends, apart from the fixed-width title slot on line 1 it rewrites in place.
    let mut entry = previous
        .filter(|entry| {
            entry.version.identity == version.identity && entry.version.size <= version.size
        })
        .unwrap_or_else(|| Incremental {
            version: version.clone(),
            consumed_to: 0,
            state: T::default(),
        });
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(entry.consumed_to))?;
    let mut reader = BufReader::new(file.take(version.size.saturating_sub(entry.consumed_to)));
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = reader
            .by_ref()
            .take(MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES as u64 + 1)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        if read > MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OMP session record is too large",
            ));
        }
        // A partial final write is read again once it is complete.
        if bytes.last() != Some(&b'\n') {
            break;
        }
        visit(&mut entry.state, &bytes, entry.consumed_to == 0);
        entry.consumed_to += read as u64;
    }
    entry.version = version;
    let state = entry.state.clone();
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), entry);
    }
    Ok(state)
}

/// The child's records, read incrementally.
pub(crate) fn read_child_records(path: &Path) -> io::Result<OmpChildRecords> {
    static CACHE: LazyLock<IncrementalCache<OmpChildRecords>> = LazyLock::new(Mutex::default);
    read_incremental(&CACHE, path, observe)
}

/// The child id a valid vibe `spawn` lifecycle record claims (OMP's `persistedVibeChildIds` over
/// `parseLifecycleEvent`): the child file must be `<id>.jsonl`.
fn vibe_spawned_child(record: &Value) -> Option<&str> {
    if record.get("type").and_then(Value::as_str) != Some("custom")
        || record.get("customType").and_then(Value::as_str) != Some("vibe-session-lifecycle")
    {
        return None;
    }
    let data = record.get("data")?.as_object()?;
    let text = |key: &str| {
        data.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    let id = text("id")?;
    (data.get("version").and_then(Value::as_f64) == Some(1.0)
        && text("ownerId").is_some()
        && text("parentSessionId").is_some()
        && data.get("action").and_then(Value::as_str) == Some("spawn")
        && matches!(
            data.get("cli").and_then(Value::as_str),
            Some("fast" | "good")
        )
        && data.get("agent").is_some_and(Value::is_string)
        && data.get("createdAt").is_some_and(Value::is_number)
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && data.get("childSessionFile").and_then(Value::as_str)
            == Some(format!("{id}.jsonl").as_str()))
    .then_some(id)
}

fn observe_vibe(ids: &mut HashSet<String>, line: &[u8], _first: bool) {
    let Ok(line) = std::str::from_utf8(line) else {
        return;
    };
    // Only lifecycle records can claim a child; the rest are skipped unparsed.
    if !line.contains("\"vibe-session-lifecycle\"") {
        return;
    }
    if let Some(id) = serde_json::from_str::<Value>(line)
        .ok()
        .as_ref()
        .and_then(vibe_spawned_child)
    {
        ids.insert(id.to_string());
    }
}

/// The child ids OMP's vibe mode owns, from the session's own transcript, read incrementally.
fn vibe_child_ids(root: &Path) -> io::Result<HashSet<String>> {
    static CACHE: LazyLock<IncrementalCache<HashSet<String>>> = LazyLock::new(Mutex::default);
    match read_incremental(&CACHE, root, observe_vibe) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(HashSet::new()),
        ids => ids,
    }
}

/// CDXC:SessionStatus 2026-10-30 WHY:
/// OMP ends a subagent's run with its final `yield` and stops before another model call, so the child transcript ends on that tool result with no closing assistant reply, and a kept-alive child writes nothing more until it is parked. Read as a plain Pi turn, every finished child would stay working. The terminal `yield` result (OMP's `#isTerminalYieldToolResult`: a `yield` that is not an error and not an incremental section) and the `session_exit` record OMP writes when the child's session is disposed end the turn.
pub(crate) fn omp_child_turn_ended(record: &Value) -> bool {
    match record.get("type").and_then(Value::as_str) {
        Some("custom") => record.get("customType").and_then(Value::as_str) == Some("session_exit"),
        Some("message") => {
            let Some(message) = record.get("message") else {
                return false;
            };
            let incremental = message
                .get("details")
                .and_then(Value::as_object)
                .is_some_and(|details| {
                    details.get("status").and_then(Value::as_str) == Some("success")
                        && details
                            .get("type")
                            .and_then(Value::as_array)
                            .is_some_and(|kinds| {
                                !kinds.is_empty() && kinds.iter().all(Value::is_string)
                            })
                });
            message.get("role").and_then(Value::as_str) == Some("toolResult")
                && message.get("toolName").and_then(Value::as_str) == Some("yield")
                && message.get("isError") != Some(&Value::Bool(true))
                && !incremental
        }
        _ => false,
    }
}

struct Member {
    id: String,
    path: PathBuf,
    tombstoned: bool,
    records: OmpChildRecords,
}

/// The id a directory entry gives a subagent under OMP's Hub rule: a `*.jsonl` file without `.bak`
/// in its name that is not an advisor transcript (`__advisor.jsonl`, `__advisor.<slug>.jsonl`).
fn hub_id(name: &str) -> Option<&str> {
    let advisor =
        name == "__advisor.jsonl" || (name.starts_with("__advisor.") && name.ends_with(".jsonl"));
    name.strip_suffix(".jsonl")
        .filter(|_| !name.contains(".bak") && !advisor)
}

/// CDXC:SessionStatus 2026-10-30 WHY:
/// Every subagent OMP's Agent Hub lists for the session whose transcript is `root`, at any depth, as `registerPersistedSubagentsFromDir` registers them. Children the session's vibe mode spawned are the vibe runtime's, not the Hub's subagents, and are skipped with their folders. A file whose leading records hold no `session_init` or message is a spawn still being set up and is skipped unless a `<id>.jsonl.tombstone` marks it as killed; its folder is still searched. OMP's registry holds one subagent per id, so when two files would give the same id only the first counts; Ghostex walks each folder in name order, depth first, where OMP takes the order the folder lists.
fn hub_members(root: &Path) -> anyhow::Result<(PathBuf, Vec<Member>)> {
    let dir = root
        .to_str()
        .and_then(|root| root.strip_suffix(".jsonl"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("The main transcript is not an OMP session file."))?;
    let vibe = vibe_child_ids(root)?;
    let mut members = Vec::new();
    collect_members(&dir, &vibe, &mut HashSet::new(), &mut members)?;
    Ok((dir, members))
}

fn collect_members(
    dir: &Path,
    vibe: &HashSet<String>,
    ids: &mut HashSet<String>,
    members: &mut Vec<Member>,
) -> anyhow::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error.into()),
    };
    let mut files = Vec::new();
    let mut folders = HashSet::new();
    for entry in entries {
        let entry = entry?;
        let kind = entry.file_type()?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if kind.is_dir() {
            folders.insert(name);
        } else if kind.is_file() {
            files.push(name);
        }
    }
    files.sort();
    for name in &files {
        let Some(id) = hub_id(name).filter(|id| !vibe.contains(*id)) else {
            continue;
        };
        let path = dir.join(name);
        let mut tombstone = path.clone().into_os_string();
        tombstone.push(".tombstone");
        let tombstoned = match fs::metadata(&tombstone) {
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        // A later file with a claimed id is never read, as OMP checks its registry first.
        let records = if ids.contains(id) {
            None
        } else {
            match read_child_records(&path) {
                Ok(records) => Some(records),
                // Removed while the folder was being read.
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            }
        };
        if let Some(records) = records.filter(|records| records.opened || tombstoned) {
            ids.insert(id.to_string());
            members.push(Member {
                id: id.to_string(),
                path,
                tombstoned,
                records,
            });
        }
        if folders.contains(id) {
            collect_members(&dir.join(id), vibe, ids, members)?;
        }
    }
    Ok(())
}

/// `Err` is unreadable evidence, so callers show the last roster as unavailable, with no running animation.
pub(crate) fn read_omp_fleet(session: &Value) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(root) = session
        .pointer("/runtimeSettings/agentSessionPath")
        .and_then(Value::as_str)
        .filter(|path| path.ends_with(".jsonl"))
    else {
        return Ok(None);
    };
    omp_fleet(Path::new(root), || {
        Ok(crate::session_chat_fleet_process::current_process(session, "omp")?.1)
    })
}

/// The roster of the session whose transcript is `root`, one row per Hub member. Subagents run
/// inside the OMP process, so only a turn that moved after that process started can be live;
/// `process_started` is asked only when one is open. Rows are never folded into a parent, so
/// `nested` stays empty.
pub(crate) fn omp_fleet(
    root: &Path,
    process_started: impl FnOnce() -> anyhow::Result<i64>,
) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    let (_, members) = hub_members(root)?;
    if members.is_empty() {
        return Ok(None);
    }
    let rows = members
        .into_iter()
        .map(|member| {
            Ok((
                latest_child_turn(&member.path, SessionChatTranscriptAgent::Pi)?,
                member,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let open = rows
        .iter()
        .any(|(turn, member)| !member.tombstoned && turn.as_ref().is_some_and(|t| t.working));
    let started = if open { process_started()? } else { i64::MAX };
    let now = chrono::Utc::now().timestamp_millis();
    let mut agents: Vec<SessionChatSubAgent> = rows
        .into_iter()
        .map(|(turn, member)| {
            let working = !member.tombstoned
                && turn
                    .as_ref()
                    .is_some_and(|turn| turn.working && turn.timestamp >= started);
            let records = member.records;
            SessionChatSubAgent {
                elapsed_seconds: records.started_at.zip(turn.as_ref()).map(|(start, turn)| {
                    let end = if working { now } else { turn.timestamp };
                    end.saturating_sub(start).max(0) as u64 / 1000
                }),
                started_at: records.started_at.unwrap_or_default(),
                working,
                done: false,
                name: records.agent.unwrap_or_else(|| member.id.clone()),
                id: Some(member.id),
                model: records.model,
                effort: records.effort,
                task: records.task,
                tokens: None,
                nested: None,
            }
        })
        .collect();
    agents.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(Some(SessionChatAgentFleet::new(agents)))
}

/// One roster row's transcript and agent definition. Only an id the roster's own scan produced
/// opens anything, and only at the path that scan found.
pub(crate) fn omp_child_transcript(
    root: &Path,
    id: &str,
) -> anyhow::Result<(PathBuf, Option<String>)> {
    let mut parts = Path::new(id).components();
    anyhow::ensure!(
        !id.contains(['/', '\\', '\0'])
            && matches!(
                (parts.next(), parts.next()),
                (Some(Component::Normal(_)), None)
            ),
        "Not an OMP subagent id."
    );
    let unavailable =
        || anyhow::anyhow!("This subagent's transcript is not available in this conversation.");
    let (dir, members) = hub_members(root)?;
    let member = members
        .into_iter()
        .find(|member| member.id == id)
        .ok_or_else(unavailable)?;
    if !fs::canonicalize(&member.path)?.starts_with(fs::canonicalize(&dir)?) {
        return Err(unavailable());
    }
    Ok((member.path, member.records.agent))
}

#[cfg(test)]
#[path = "session_chat_omp_fleet_tests.rs"]
mod tests;
