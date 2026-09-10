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
}

#[derive(Clone, Default)]
struct Parent {
    launches: HashMap<String, Launch>,
    children: HashMap<String, Child>,
    queued_notifications: HashMap<String, VecDeque<i64>>,
    started_at: Option<i64>,
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
            apply_turn(
                parent.children.entry(id.to_string()).or_default(),
                false,
                at,
            );
        }
    }
}

fn read_parent_record(parent: &mut Parent, record: &Value) {
    let Some(at) = timestamp(record) else {
        return;
    };
    if matches!(
        record
            .pointer("/attachment/hookName")
            .and_then(Value::as_str),
        Some("SessionStart:startup" | "SessionStart:resume" | "SessionStart:clear")
    ) {
        parent.started_at = Some(at);
    }
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
        }
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
    if record.get("type").and_then(Value::as_str) == Some("user")
        && (record.pointer("/origin/kind").and_then(Value::as_str) == Some("task-notification")
            || record.get("queueSkipAttachments").and_then(Value::as_bool) == Some(true)
            || record.get("promptSource").and_then(Value::as_str) == Some("queued"))
    {
        if let Some(content) = record.pointer("/message/content").and_then(Value::as_str) {
            notifications(parent, content, at, false);
        }
        if let Some(blocks) = blocks {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(content) = block.get("text").and_then(Value::as_str) {
                        notifications(parent, content, at, false);
                    }
                }
            }
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
    if let Some(previous) = previous.as_ref().filter(|entry| entry.version == version) {
        return Ok(previous.parent.clone());
    }
    let mut entry = previous
        .filter(|entry| {
            entry.version.identity == version.identity && entry.version.size < version.size
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
            read_parent_record(&mut entry.parent, &serde_json::from_slice::<Value>(&bytes)?);
        }
        entry.consumed_to += read as u64;
    }
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
            Some((id, child.launch, turn, child.launched_at, child.model))
        })
        .collect();
    if current.is_empty() {
        return Ok(None);
    }
    let (_, process_started) =
        crate::session_chat_fleet_process::current_process(session, "claude")?;
    current.retain(|(_, _, turn, _, _)| turn.timestamp >= process_started);
    current.sort_by(|a, b| {
        a.2.started_at
            .cmp(&b.2.started_at)
            .then_with(|| a.0.cmp(&b.0))
    });
    let now = chrono::Utc::now().timestamp_millis();
    let children = current
        .into_iter()
        .map(|(id, launch, turn, launched_at, model)| {
            let end = if turn.working { now } else { turn.timestamp };
            crate::session_chat_fleet_progress::ClaudeChild {
                launched_at,
                sampled_at: end,
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
