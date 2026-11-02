//! CDXC:SessionStatus 2026-09-10 WHY:
//! Claude persists async launch IDs, terminal task notifications and child turn boundaries. The current terminal roster controls idle-child visibility; persisted lifecycle evidence controls working state, so retained footer rows cannot resurrect completed work.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{
    read_transcript_file_version, SessionChatTranscriptAgent, TranscriptFileVersion,
    MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES,
};
use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_fleet_transcript::{latest_child_turn, parse_time, timestamp, ChildTurn};

#[derive(Clone, Default)]
struct Launch {
    name: Option<String>,
    task: Option<String>,
    at: i64,
}

#[derive(Clone, Default)]
struct Child {
    launch: Launch,
    turn: Option<ChildTurn>,
    launched_at: Option<i64>,
    model: crate::session_chat_subagent::model::SubagentModel,
    /// A finished agent of a workflow that is still running, which the footer still counts.
    resident: bool,
}

#[derive(Clone, Default)]
struct Parent {
    launches: HashMap<String, Launch>,
    children: HashMap<String, Child>,
    queued_notifications: HashMap<String, VecDeque<i64>>,
    started_at: Option<i64>,
    workflows: crate::session_chat_claude_workflows::WorkflowRuns,
    background: crate::session_chat_claude_background::ClaudeBackgroundLaunches,
    /// TaskStop calls awaiting their result: tool use id to the `task_id` the lead passed.
    stop_requests: HashMap<String, Option<String>>,
    /// Agent-team teammates by name and by `name@team`, to their agent id.
    teammates: HashMap<String, String>,
    /// How far the process starts Ghostex recorded for this conversation are applied.
    starts: crate::hookless_activity::ProcessStartCursor,
}

#[derive(Clone)]
struct ParentCacheEntry {
    version: TranscriptFileVersion,
    consumed_to: u64,
    parent: Parent,
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "killed" | "stopped" | "cancelled" | "canceled"
    )
}

fn apply_turn(child: &mut Child, working: bool, at: i64) {
    if child.turn.as_ref().is_some_and(|turn| turn.timestamp > at) {
        return;
    }
    let started_at = child
        .turn
        .as_ref()
        .filter(|turn| turn.working && working)
        .map_or(at, |turn| turn.started_at);
    child.turn = Some(ChildTurn {
        working,
        timestamp: at,
        started_at,
    });
}

fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.split_once(&format!("<{name}>"))?
        .1
        .split_once(&format!("</{name}>"))
        .map(|(value, _)| value.trim())
}

fn notifications(parent: &mut Parent, content: &str, at: i64, queued: bool) {
    // A real user can quote a notification. Only harness envelopes are lifecycle evidence.
    if !content.trim_start().starts_with("<task-notification>") {
        return;
    }
    for part in content.split("<task-notification>").skip(1) {
        let Some((body, _)) = part.split_once("</task-notification>") else {
            continue;
        };
        let (Some(id), Some(status)) = (tag(body, "task-id"), tag(body, "status")) else {
            continue;
        };
        if terminal_status(status) {
            parent.background.finish(id);
            // Delivery can follow a resumed turn. Match each queued envelope, not task IDs,
            // because the same child may finish repeatedly, and retain its original event time.
            let key = body.split_whitespace().collect::<Vec<_>>().join(" ");
            let at = if queued {
                parent
                    .queued_notifications
                    .entry(key)
                    .or_default()
                    .push_back(at);
                at
            } else {
                let original = parent
                    .queued_notifications
                    .get_mut(&key)
                    .and_then(VecDeque::pop_front);
                if parent
                    .queued_notifications
                    .get(&key)
                    .is_some_and(VecDeque::is_empty)
                {
                    parent.queued_notifications.remove(&key);
                }
                original.unwrap_or(at)
            };
            if parent.workflows.finish(id, at, status) {
                continue;
            }
            apply_turn(
                parent.children.entry(id.to_string()).or_default(),
                false,
                at,
            );
        }
    }
}

/// CDXC:SessionStatus 2026-10-19 WHY:
/// Claude sends no task-notification when an agent-team teammate (`in_process_teammate`) is stopped, and the teammate's own transcript just ends mid-turn, so its last persisted turn reads as working forever (GitHub #259). The lead's transcript holds the real signal: the successful TaskStop result, and the "N background agents were stopped by the user: "…"" notice that Claude's stop-all-agents writes, which names each agent by its description. Settle the child at that moment; a later turn of its own still wins.
fn settle_stopped(parent: &mut Parent, id: &str, at: i64) {
    if let Some(child) = parent.children.get_mut(id) {
        apply_turn(child, false, at);
    }
}

/// The child a successful TaskStop result names: its agent id, the name or `name@team` the lead
/// passed, or else the one child launched with the stopped task's description.
fn stopped_child(parent: &Parent, request: Option<&str>, result: &Value) -> Option<String> {
    let known = |id: &str| parent.children.contains_key(id).then(|| id.to_string());
    result
        .get("task_id")
        .and_then(Value::as_str)
        .and_then(known)
        .or_else(|| request.and_then(known))
        .or_else(|| request.and_then(|name| parent.teammates.get(name).cloned()))
        .or_else(|| {
            let command = result.get("command").and_then(Value::as_str)?;
            let mut matches = parent
                .children
                .iter()
                .filter(|(_, child)| child.launch.task.as_deref() == Some(command));
            let (id, _) = matches.next()?;
            matches.next().is_none().then(|| id.clone())
        })
}

/// Every agent a "… stopped by the user: "a", "b"" notice names, by launch description.
fn stopped_by_user(parent: &mut Parent, content: &str, at: i64) {
    let Some((_, names)) = content.split_once("stopped by the user") else {
        return;
    };
    let descriptions: Vec<&str> = names.split('"').skip(1).step_by(2).collect();
    let ids: Vec<String> = parent
        .children
        .iter()
        .filter(|(_, child)| {
            child
                .launch
                .task
                .as_deref()
                .is_some_and(|task| descriptions.contains(&task))
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in ids {
        settle_stopped(parent, &id, at);
    }
}

/// A Claude process took the conversation over at `start.at`: children before it are not this
/// process's, and a new process has none of the old one's shells.
fn process_started(parent: &mut Parent, start: crate::hookless_activity::ClaudeProcessStart) {
    parent.started_at = Some(parent.started_at.map_or(start.at, |at| at.max(start.at)));
    if start.source.new_process() {
        parent.background.process_started();
    }
}

/// Applies the recorded process starts due by `upto`, in transcript order.
fn apply_process_starts(
    parent: &mut Parent,
    starts: &[crate::hookless_activity::ClaudeProcessStart],
    upto: i64,
) {
    for start in parent.starts.due(starts, upto) {
        process_started(parent, start);
    }
}

/// CDXC:SessionStatus 2026-11-02 DECISION:
/// User, asked how to close the gaps that keep Claude's hooks installed: "Everything, including Windows". A teammate whose last answer was written with `stop_reason: null` stayed "running" unless the `TeammateIdle` hook line ended its turn (GitHub #259). With no hooks, the lead's transcript still says it: Claude delivers each idle teammate's `{"type":"idle_notification","from":<name>,"timestamp":…}` to the lead inside a `<teammate-message>`, as a user row, a queued command or an enqueued prompt. The teammate's turn ends at that timestamp; a later turn of its own still wins.
fn teammate_idle(parent: &mut Parent, content: &str, at: i64) {
    for part in content.split("<teammate-message").skip(1) {
        let Some((head, rest)) = part.split_once('>') else {
            continue;
        };
        let Some((body, _)) = rest.split_once("</teammate-message>") else {
            continue;
        };
        let Ok(message) = serde_json::from_str::<Value>(body.trim()) else {
            continue;
        };
        if message.get("type").and_then(Value::as_str) != Some("idle_notification") {
            continue;
        }
        let name = text(message.get("from")).or_else(|| {
            head.split_once("teammate_id=\"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name.to_string())
        });
        let Some(id) = name.and_then(|name| parent.teammates.get(&name).cloned()) else {
            continue;
        };
        let at = message
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_time)
            .unwrap_or(at);
        settle_stopped(parent, &id, at);
    }
}

/// Every `<teammate-message>` a lead's record carries, wherever Claude put it.
fn teammate_messages(parent: &mut Parent, record: &Value, at: i64) {
    let mut contents: Vec<&str> = Vec::new();
    match record.get("type").and_then(Value::as_str) {
        Some("user") => match record.pointer("/message/content") {
            Some(Value::String(content)) => contents.push(content),
            Some(Value::Array(blocks)) => contents.extend(
                blocks
                    .iter()
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|block| block.get("text").and_then(Value::as_str)),
            ),
            _ => {}
        },
        Some("queue-operation") => contents.extend(record.get("content").and_then(Value::as_str)),
        Some("attachment") => {
            contents.extend(record.pointer("/attachment/prompt").and_then(Value::as_str))
        }
        _ => {}
    }
    for content in contents {
        if content.contains("idle_notification") {
            teammate_idle(parent, content, at);
        }
    }
}

fn read_parent_record(parent: &mut Parent, record: &Value) {
    let Some(at) = timestamp(record) else {
        return;
    };
    // The hook line an old transcript holds; the observer's own record of the same start arrives
    // through `apply_process_starts`.
    if crate::hookless_activity::claude_hook_line_start(record).is_some() {
        parent.started_at = Some(at);
    }
    parent.background.observe(record, at);
    teammate_messages(parent, record, at);
    let blocks = record.pointer("/message/content").and_then(Value::as_array);
    if let Some(blocks) = blocks {
        for block in blocks {
            if block.get("type").and_then(Value::as_str) == Some("tool_use")
                && matches!(
                    block.get("name").and_then(Value::as_str),
                    Some("Agent" | "Task")
                )
            {
                if let Some(id) = text(block.get("id")) {
                    parent.launches.insert(
                        id,
                        Launch {
                            at,
                            name: text(block.pointer("/input/name"))
                                .or_else(|| text(block.pointer("/input/subagent_type"))),
                            task: text(block.pointer("/input/description")),
                        },
                    );
                }
            }
            if block.get("type").and_then(Value::as_str) == Some("tool_use")
                && block.get("name").and_then(Value::as_str) == Some("Workflow")
            {
                if let (Some(id), Some(input)) = (text(block.get("id")), block.get("input")) {
                    parent.workflows.call(&id, input);
                }
            }
            if block.get("type").and_then(Value::as_str) == Some("tool_use")
                && block.get("name").and_then(Value::as_str) == Some("TaskStop")
            {
                if let Some(id) = text(block.get("id")) {
                    let task = text(block.pointer("/input/task_id"))
                        .or_else(|| text(block.pointer("/input/shell_id")));
                    parent.stop_requests.insert(id, task);
                }
            }
        }
    }
    if let Some(result) = record.get("toolUseResult") {
        let call_id = blocks.and_then(|blocks| {
            blocks
                .iter()
                .find_map(|block| block.get("tool_use_id").and_then(Value::as_str))
        });
        parent.workflows.launch(result, at, call_id);
    }
    if let Some(id) = text(record.pointer("/toolUseResult/agentId")) {
        let launch = blocks
            .and_then(|blocks| {
                blocks.iter().find_map(|block| {
                    block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .and_then(|id| parent.launches.get(id))
                })
            })
            .cloned();
        let child = parent.children.entry(id).or_default();
        child.launched_at.get_or_insert(at);
        if let Some(launch) = launch {
            child.launch = launch;
        }
        child.launch.task =
            text(record.pointer("/toolUseResult/description")).or(child.launch.task.take());
        if let Some(status) = record
            .pointer("/toolUseResult/status")
            .and_then(Value::as_str)
        {
            if status == "async_launched" || terminal_status(status) {
                apply_turn(child, status == "async_launched", at);
            }
        }
    }
    if record
        .pointer("/toolUseResult/status")
        .and_then(Value::as_str)
        == Some("teammate_spawned")
    {
        if let Some(id) = text(record.pointer("/toolUseResult/agentId")) {
            for key in ["/toolUseResult/name", "/toolUseResult/teammate_id"] {
                if let Some(name) = text(record.pointer(key)) {
                    parent.teammates.insert(name, id.clone());
                }
            }
        }
    }
    if let (Some(result), Some(blocks)) = (
        record
            .get("toolUseResult")
            .filter(|result| result.get("task_type").is_some()),
        blocks,
    ) {
        let request = blocks.iter().find_map(|block| {
            block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .and_then(|id| parent.stop_requests.remove(id))
        });
        if let Some(request) = request {
            if let Some(id) = stopped_child(parent, request.as_deref(), result) {
                settle_stopped(parent, &id, at);
            }
        }
    }
    if record.get("type").and_then(Value::as_str) == Some("user")
        && (record.pointer("/origin/kind").and_then(Value::as_str) == Some("task-notification")
            || record.get("queueSkipAttachments").and_then(Value::as_bool) == Some(true)
            || record.get("promptSource").and_then(Value::as_str) == Some("queued"))
    {
        let harness =
            record.pointer("/origin/kind").and_then(Value::as_str) == Some("task-notification");
        if let Some(content) = record.pointer("/message/content").and_then(Value::as_str) {
            notifications(parent, content, at, false);
            if harness {
                stopped_by_user(parent, content, at);
            }
        }
        if let Some(blocks) = blocks {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(content) = block.get("text").and_then(Value::as_str) {
                        notifications(parent, content, at, false);
                        if harness {
                            stopped_by_user(parent, content, at);
                        }
                    }
                }
            }
        }
    }
    // A notice the lead absorbs mid-turn is written only as this attachment, never as a user row.
    if record.pointer("/attachment/type").and_then(Value::as_str) == Some("queued_command")
        && record
            .pointer("/attachment/origin/kind")
            .and_then(Value::as_str)
            == Some("task-notification")
    {
        if let Some(content) = record.pointer("/attachment/prompt").and_then(Value::as_str) {
            stopped_by_user(parent, content, at);
        }
    }
    // Completion is known when Claude queues the notification, before the lead consumes it.
    if record.get("type").and_then(Value::as_str) == Some("queue-operation")
        && record.get("operation").and_then(Value::as_str) == Some("enqueue")
    {
        if let Some(content) = record.get("content").and_then(Value::as_str) {
            notifications(parent, content, at, true);
        }
    }
}

fn read_parent(path: &Path) -> anyhow::Result<Parent> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, ParentCacheEntry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    let previous = cache.lock().ok().and_then(|cache| cache.get(path).cloned());
    // Read even when the file is unchanged: a process may have started on it since.
    let starts = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(crate::hookless_activity::read_claude_process_starts)
        .unwrap_or_default();
    let mut entry = previous
        .filter(|entry| {
            entry.version == version
                || (entry.version.identity == version.identity && entry.version.size < version.size)
        })
        .unwrap_or_else(|| ParentCacheEntry {
            version: version.clone(),
            consumed_to: 0,
            parent: Parent::default(),
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
        anyhow::ensure!(
            read <= MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES,
            "Subagent parent record is too large"
        );
        if bytes.last() != Some(&b'\n') {
            break;
        }
        if !bytes.iter().all(u8::is_ascii_whitespace) {
            let record = serde_json::from_slice::<Value>(&bytes)?;
            if let Some(at) = timestamp(&record) {
                apply_process_starts(&mut entry.parent, &starts, at);
            }
            read_parent_record(&mut entry.parent, &record);
        }
        entry.consumed_to += read as u64;
    }
    // Starts after the newest record: the process has written nothing since.
    apply_process_starts(&mut entry.parent, &starts, i64::MAX);
    entry.version = version;
    let parent = entry.parent.clone();
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), entry);
    }
    Ok(parent)
}

/// The Workflow launches the parent transcript recorded, for the chat's workflow cards.
pub(crate) fn read_claude_workflow_runs(
    path: &Path,
) -> anyhow::Result<crate::session_chat_claude_workflows::WorkflowRuns> {
    Ok(read_parent(path)?.workflows)
}

/// The background shells and monitors still running in this session, at most `count` (the footer's).
pub(crate) fn read_claude_background_tasks(
    session: &Value,
    count: u64,
) -> anyhow::Result<Vec<crate::session_chat_terminal_activity::SessionChatBackgroundTask>> {
    let Some(path) = session
        .pointer("/runtimeSettings/agentSessionPath")
        .and_then(Value::as_str)
    else {
        return Ok(Vec::new());
    };
    let parent = read_parent(Path::new(path))?;
    let boundary = session
        .pointer("/runtimeSettings/sessionChatClaudeStartedAt")
        .and_then(Value::as_str)
        .and_then(parse_time)
        .unwrap_or(0);
    Ok(parent.background.running(count, boundary))
}

pub(crate) fn read_claude_fleet(
    session: &Value,
    screen: Option<&str>,
) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(path) = session
        .pointer("/runtimeSettings/agentSessionPath")
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let root = Path::new(path);
    let mut parent = read_parent(root)?;
    let directory = root.with_extension("").join("subagents");
    let files = match fs::read_dir(&directory) {
        Ok(files) => Some(files),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    for entry in files.into_iter().flatten() {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(id) = filename
            .to_str()
            .and_then(|s| s.strip_prefix("agent-"))
            .and_then(|s| s.strip_suffix(".jsonl"))
        else {
            continue;
        };
        if !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            continue;
        }
        let metadata = match File::open(entry.path().with_extension("meta.json")) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(64 * 1024).read_to_end(&mut bytes)?;
                Some(serde_json::from_slice::<Value>(&bytes)?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        // The directory also contains internal prompt-suggestion transcripts, without launch metadata.
        if !parent.children.contains_key(id)
            && metadata.as_ref().and_then(|m| m.get("toolUseId")).is_none()
        {
            continue;
        }
        let launch = metadata
            .as_ref()
            .and_then(|m| m.get("toolUseId"))
            .and_then(Value::as_str)
            .and_then(|id| parent.launches.get(id))
            .cloned();
        let child = parent.children.entry(id.to_string()).or_default();
        child.model = crate::session_chat_subagent::model::read(
            &entry.path(),
            SessionChatTranscriptAgent::Claude,
        )?;
        if let Some(launch) = launch {
            child.launched_at.get_or_insert(launch.at);
            child.launch = launch;
        }
        if let Some(meta) = metadata {
            child.launch.name = text(meta.get("agentType")).or(child.launch.name.take());
            child.launch.task = child
                .launch
                .task
                .take()
                .or_else(|| text(meta.get("description")));
        }
        if let Some(turn) = latest_child_turn(&entry.path(), SessionChatTranscriptAgent::Claude)? {
            if child
                .turn
                .as_ref()
                .is_none_or(|old| turn.timestamp >= old.timestamp)
            {
                child.turn = Some(turn);
            }
        }
    }
    for agent in parent.workflows.running_agents(&directory)? {
        let child = parent.children.entry(agent.id).or_default();
        child.model = crate::session_chat_subagent::model::read(
            &agent.path,
            SessionChatTranscriptAgent::Claude,
        )?;
        child.launched_at = agent.launched_at;
        child.launch = Launch {
            name: Some(agent.run.name),
            task: agent.label,
            at: agent.launched_at.unwrap_or(agent.run.launched_at),
        };
        child.turn = latest_child_turn(&agent.path, SessionChatTranscriptAgent::Claude)?;
        // An agent the resumed run answered from its cache belongs to an earlier launch.
        child.resident = child
            .turn
            .as_ref()
            .is_some_and(|turn| turn.timestamp >= agent.run.launched_at);
    }
    let hook_started = session
        .pointer("/runtimeSettings/sessionChatClaudeStartedAt")
        .and_then(Value::as_str)
        .and_then(parse_time);
    let boundary = parent
        .started_at
        .into_iter()
        .chain(hook_started)
        .max()
        .unwrap_or(0);
    let mut current: Vec<_> = parent
        .children
        .into_iter()
        .filter_map(|(id, child)| {
            let turn = child.turn.filter(|turn| turn.timestamp >= boundary)?;
            Some((
                id,
                child.launch,
                turn,
                child.launched_at,
                child.model,
                child.resident,
            ))
        })
        .collect();
    if current.is_empty() {
        return Ok(None);
    }
    let (_, process_started) =
        crate::session_chat_fleet_process::current_process(session, "claude")?;
    current.retain(|(_, _, turn, _, _, _)| turn.timestamp >= process_started);
    current.sort_by(|a, b| {
        a.2.started_at
            .cmp(&b.2.started_at)
            .then_with(|| a.0.cmp(&b.0))
    });
    let now = chrono::Utc::now().timestamp_millis();
    let children = current
        .into_iter()
        .map(|(id, launch, turn, launched_at, model, resident)| {
            let end = if turn.working { now } else { turn.timestamp };
            crate::session_chat_fleet_progress::ClaudeChild {
                launched_at,
                sampled_at: end,
                resident,
                agent: SessionChatSubAgent {
                    id: Some(id.clone()),
                    started_at: turn.started_at,
                    working: turn.working,
                    name: launch.name.unwrap_or(id),
                    model: model.model,
                    effort: model.effort,
                    task: launch.task,
                    elapsed_seconds: Some(
                        end.saturating_sub(launched_at.unwrap_or(turn.started_at))
                            .max(0) as u64
                            / 1000,
                    ),
                    tokens: None,
                    nested: None,
                },
            }
        })
        .collect::<Vec<_>>();
    let agents = crate::session_chat_fleet_progress::reconcile(children, screen);
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hookless_activity::{ClaudeProcessStart, ProcessStartCursor};
    use serde_json::json;

    fn at(second: u32) -> String {
        format!("2026-11-02T10:00:{second:02}.000Z")
    }

    fn ms(second: u32) -> i64 {
        parse_time(&at(second)).unwrap()
    }

    /// Reads records in order with the process starts Ghostex recorded, as `read_parent` does.
    fn read(records: &[Value], starts: &[ClaudeProcessStart]) -> Parent {
        let mut parent = Parent {
            starts: ProcessStartCursor::default(),
            ..Parent::default()
        };
        for record in records {
            if let Some(at) = timestamp(record) {
                apply_process_starts(&mut parent, starts, at);
            }
            read_parent_record(&mut parent, record);
        }
        apply_process_starts(&mut parent, starts, i64::MAX);
        parent
    }

    fn spawn_teammate(second: u32) -> [Value; 2] {
        [
            json!({"type": "assistant", "timestamp": at(second), "message": {"content": [{"type": "tool_use", "id": "toolu_q", "name": "Agent", "input": {"name": "quick", "description": "capsule test quick"}}]}}),
            json!({"type": "user", "timestamp": at(second + 1), "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_q"}]}, "toolUseResult": {"status": "teammate_spawned", "agentId": "aquick-1f17", "name": "quick", "teammate_id": "quick@session-8145b258"}}),
        ]
    }

    /// GitHub #259 with no hooks: the teammate's own file ends on an answer with a null stop
    /// reason and no `TeammateIdle` line; the lead's idle notification ends its turn.
    #[test]
    fn a_teammates_idle_notification_ends_its_turn_without_hooks() {
        let [launch, spawned] = spawn_teammate(1);
        let idle = json!({"type": "user", "timestamp": at(9), "message": {"content": format!(
            "Another Claude session sent a message:\n<teammate-message teammate_id=\"quick\" color=\"green\">\n{{\"type\":\"idle_notification\",\"from\":\"quick\",\"timestamp\":\"{}\",\"idleReason\":\"available\",\"result\":\"done\"}}\n</teammate-message>",
            at(8)
        )}});
        let parent = read(&[launch.clone(), spawned.clone()], &[]);
        assert!(parent.children["aquick-1f17"].turn.is_none());
        let parent = read(&[launch.clone(), spawned.clone(), idle], &[]);
        let turn = parent.children["aquick-1f17"].turn.clone().unwrap();
        assert!(!turn.working);
        assert_eq!(turn.timestamp, ms(8));

        // Queued while the lead was busy: the enqueued prompt carries it.
        let queued = json!({"type": "queue-operation", "operation": "enqueue", "timestamp": at(9), "content": format!(
            "<teammate-message teammate_id=\"quick\">\n{{\"type\":\"idle_notification\",\"from\":\"quick\",\"timestamp\":\"{}\"}}\n</teammate-message>",
            at(7)
        )});
        let parent = read(&[launch, spawned, queued], &[]);
        assert_eq!(
            parent.children["aquick-1f17"]
                .turn
                .as_ref()
                .map(|turn| (turn.working, turn.timestamp)),
            Some((false, ms(7)))
        );
    }

    #[test]
    fn a_recorded_process_start_resets_shells_like_the_hook_line_did() {
        let shell = |second: u32, id: &str, task: &str| {
            [
                json!({"type": "assistant", "timestamp": at(second), "message": {"content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "sleep 600", "description": task, "run_in_background": true}}]}}),
                json!({"type": "user", "timestamp": at(second + 1), "message": {"content": [{"type": "tool_result", "tool_use_id": id}]}, "toolUseResult": {"backgroundTaskId": task}}),
            ]
        };
        let [a, b] = shell(1, "toolu_a", "old-shell");
        let [c, d] = shell(20, "toolu_c", "new-shell");
        let records = [a, b, c, d];
        let titles = |parent: &Parent| {
            parent
                .background
                .running(10, 0)
                .into_iter()
                .map(|task| task.title)
                .collect::<Vec<_>>()
        };
        // No hook lines and no recorded start: both shells read as running.
        assert_eq!(titles(&read(&records, &[])), vec!["old-shell", "new-shell"]);
        // Claude was resumed between the two launches: only the new one is.
        let resumed = ClaudeProcessStart {
            at: ms(10),
            source: crate::hookless_activity::ClaudeStartSource::Resume,
        };
        let parent = read(&records, &[resumed]);
        assert_eq!(titles(&parent), vec!["new-shell"]);
        assert_eq!(parent.started_at, Some(ms(10)));
        // A start after the newest record (the resumed process wrote nothing yet) still applies.
        let later = ClaudeProcessStart {
            at: ms(40),
            ..resumed
        };
        assert!(titles(&read(&records, &[resumed, later])).is_empty());
        // A /clear keeps the process and its shells, but still bounds the children.
        let cleared = ClaudeProcessStart {
            at: ms(10),
            source: crate::hookless_activity::ClaudeStartSource::Clear,
        };
        let parent = read(&records, &[cleared]);
        assert_eq!(titles(&parent), vec!["old-shell", "new-shell"]);
        assert_eq!(parent.started_at, Some(ms(10)));
        // The hook line of an old transcript still resets them.
        let hook = json!({"type": "attachment", "timestamp": at(10), "attachment": {"type": "hook_success", "hookEvent": "SessionStart", "hookName": "SessionStart:resume"}});
        let mut with_hook = records.to_vec();
        with_hook.insert(2, hook);
        assert_eq!(titles(&read(&with_hook, &[])), vec!["new-shell"]);
    }
}
