//! CDXC:SessionChat 2026-10-22 WHY:
//! The chat's workflow card (Claude Code's Workflow tool) needs each run's phases, agents, statuses, durations and output previews, and none of that is in the lead's transcript: it records the call, the launch acknowledgement (`taskType: "local_workflow"`, run id) and the terminal notification only. Claude writes the rest beside the transcript. While a run is going: `subagents/workflows/<runId>/journal.jsonl` (`started` with agent id, label and phase, `result` with the answer), each agent's own `agent-<id>.jsonl` (its first record is its prompt and start) and `agent-<id>.meta.json` (label, phase, model). When it ends: `workflows/<runId>.json`, the finished run with every agent's state, start, duration and previews. This reads those files and publishes one entry per Workflow call as the `workflows` side state, on the same disk-backed cadence as the task list; the chat core turns it into the card.
//!
//! SEE-ALSO: `packages/gx-chat-core/src/transcript/workflow_card.rs` (the card),
//! `server/src/session_chat_claude_workflows.rs` (the launches, also read by the Subagents strip).

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};

use crate::session_chat::{read_transcript_file_version, MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES};
use crate::session_chat_claude_workflows::WorkflowLaunch;
use crate::session_chat_fleet_transcript::timestamp;

/// The newest launches a session publishes; older cards keep their plain tool row.
const MAX_WORKFLOW_CARDS: usize = 24;
/// The longest preview line an agent row carries.
const MAX_PREVIEW_CHARS: usize = 240;
/// The largest run file or journal read whole.
const MAX_RUN_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCRIPT_BYTES: u64 = 64 * 1024;

/// Every Workflow call's run, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatWorkflows {
    pub runs: Vec<Value>,
}

impl SessionChatWorkflows {
    pub fn to_value(&self) -> Value {
        json!({ "runs": self.runs })
    }
}

/// Change test for a value that can also disappear: an omitted field on a frame means cleared.
pub fn same_session_chat_workflows(
    current: Option<&SessionChatWorkflows>,
    published: Option<&SessionChatWorkflows>,
) -> bool {
    current == published
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

/// The first line that says something, whitespace collapsed and cut to one row's worth.
fn preview(value: &str) -> Option<String> {
    let line = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "```")?;
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if line.chars().count() > MAX_PREVIEW_CHARS {
        let cut: String = line.chars().take(MAX_PREVIEW_CHARS).collect();
        format!("{}\u{2026}", cut.trim_end())
    } else {
        line
    })
}

fn read_bounded(path: &Path, limit: u64) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn modified_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// A JS string literal starting at `at` (`'…'`, `"…"` or `` `…` ``): its text and the index after it.
fn js_string(source: &str, at: usize) -> Option<(String, usize)> {
    let quote = source[at..].chars().next()?;
    if !matches!(quote, '\'' | '"' | '`') {
        return None;
    }
    let mut out = String::new();
    let mut chars = source[at + 1..].char_indices();
    while let Some((offset, character)) = chars.next() {
        match character {
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    out.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
            }
            c if c == quote => return Some((out, at + 1 + offset + 1)),
            c => out.push(c),
        }
    }
    None
}

/// The value of `key: '…'` after `from` and before `until`.
fn meta_string(source: &str, key: &str, from: usize, until: usize) -> Option<String> {
    let mut cursor = from;
    while let Some(found) = source[cursor..until].find(key) {
        let start = cursor + found;
        cursor = start + key.len();
        let before = source[..start].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let skip = |at: usize| at + source[at..until].len() - source[at..until].trim_start().len();
        let colon = skip(cursor);
        if !source[colon..until].starts_with(':') {
            continue;
        }
        let value_at = skip(colon + 1);
        if let Some((value, _)) = js_string(source, value_at) {
            return Some(value);
        }
    }
    None
}

/// What the script's `export const meta = { name, description, phases }` says, read without
/// running it: every field is a string literal there, which is what Claude requires of `meta`.
#[derive(Default)]
struct ScriptMeta {
    name: Option<String>,
    description: Option<String>,
    phases: Vec<(String, Option<String>)>,
}

fn script_meta(script: &str) -> ScriptMeta {
    let Some(start) = script.find("meta") else {
        return ScriptMeta::default();
    };
    let Some(open) = script[start..].find('{').map(|at| start + at) else {
        return ScriptMeta::default();
    };
    // The meta object ends at its matching brace; strings may hold braces of their own.
    let mut depth = 0usize;
    let mut end = script.len();
    let mut index = open;
    while index < script.len() {
        let character = script[index..].chars().next().unwrap_or(' ');
        if matches!(character, '\'' | '"' | '`') {
            match js_string(script, index) {
                Some((_, next)) => {
                    index = next;
                    continue;
                }
                None => break,
            }
        }
        match character {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = index + 1;
                    break;
                }
            }
            _ => {}
        }
        index += character.len_utf8();
    }
    let phases_at = script[open..end].find("phases").map(|at| open + at);
    let head_end = phases_at.unwrap_or(end);
    let mut meta = ScriptMeta {
        name: meta_string(script, "name", open, head_end),
        description: meta_string(script, "description", open, head_end),
        phases: Vec::new(),
    };
    if let Some(phases_at) = phases_at {
        let list_end = script[phases_at..end]
            .find(']')
            .map_or(end, |at| phases_at + at);
        let mut cursor = phases_at;
        while let Some(found) = script[cursor..list_end].find('{') {
            let entry = cursor + found;
            let entry_end = script[entry..list_end]
                .find('}')
                .map_or(list_end, |at| entry + at);
            if let Some(title) = meta_string(script, "title", entry, entry_end) {
                meta.phases
                    .push((title, meta_string(script, "detail", entry, entry_end)));
            }
            cursor = entry_end.max(entry + 1);
        }
    }
    meta
}

/// The text of a record's user message.
fn record_text(record: &Value) -> Option<&str> {
    match record.pointer("/message/content") {
        Some(Value::String(content)) => Some(content.as_str()),
        Some(Value::Array(blocks)) => blocks.iter().find_map(|block| text(block.get("text"))),
        _ => None,
    }
}

/// What an agent was asked to do, without Claude's framing: its transcript opens with the user
/// request the run relays (a `[Workflow harness …]` header naming the user request), then the
/// task the script computed (a header naming the computed task, then the task, indented).
fn agent_task(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("[Workflow harness") {
        return preview(text);
    }
    let header = trimmed
        .split_once(']')
        .map_or(trimmed, |(header, _)| header);
    if !header.contains("computed task") {
        return None;
    }
    preview(trimmed.split_once('\n')?.1)
}

/// The records an agent's transcript opens with that are read for its start and its task.
const AGENT_HEAD_RECORDS: usize = 6;

/// When an agent started and the task it was given, from its transcript's first records.
fn agent_start(path: &Path) -> (Option<i64>, Option<String>) {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (String, Option<i64>, Option<String>)>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let Ok(version) = read_transcript_file_version(path) else {
        return (None, None);
    };
    if let Some(hit) = cache.lock().ok().and_then(|cache| {
        cache
            .get(path)
            .filter(|(identity, _, _)| *identity == version.identity)
            .map(|(_, at, prompt)| (*at, prompt.clone()))
    }) {
        return hit;
    }
    let Ok(file) = File::open(path) else {
        return (None, None);
    };
    let mut reader = BufReader::new(file);
    let (mut at, mut prompt, mut complete) = (None, None, false);
    for index in 0..AGENT_HEAD_RECORDS {
        let mut line = Vec::new();
        let read = (&mut reader)
            .take(MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES as u64)
            .read_until(b'\n', &mut line);
        // A record still being written has nothing to say yet.
        if read.is_err() || line.last() != Some(&b'\n') {
            break;
        }
        let Ok(record) = serde_json::from_slice::<Value>(&line) else {
            break;
        };
        if index == 0 {
            at = timestamp(&record);
        }
        if record.get("type").and_then(Value::as_str) == Some("user") {
            if let Some(task) = record_text(&record).and_then(agent_task) {
                prompt = Some(task);
                break;
            }
        }
        complete = index + 1 == AGENT_HEAD_RECORDS;
    }
    // Kept only once it can no longer change: the task was found, or the head is all written.
    if prompt.is_some() || complete {
        if let Ok(mut cache) = cache.lock() {
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(path.to_path_buf(), (version.identity, at, prompt.clone()));
        }
    }
    (at, prompt)
}

fn run_status(status: Option<&str>) -> &'static str {
    match status {
        None => "running",
        Some("completed") => "completed",
        Some("failed") => "failed",
        Some(_) => "stopped",
    }
}

#[allow(clippy::too_many_arguments)]
fn agent_value(
    id: Option<&str>,
    label: &str,
    phase: Option<&str>,
    model: Option<&str>,
    status: &str,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    preview: Option<String>,
) -> Value {
    let mut agent = Map::new();
    if let Some(id) = id.filter(|id| safe_id(id)) {
        agent.insert("id".to_string(), json!(id));
    }
    agent.insert("label".to_string(), json!(label));
    if let Some(phase) = phase {
        agent.insert("phase".to_string(), json!(phase));
    }
    if let Some(model) = model {
        agent.insert("model".to_string(), json!(model));
    }
    agent.insert("status".to_string(), json!(status));
    if let Some(at) = started_at {
        agent.insert("startedAt".to_string(), json!(at));
    }
    if let Some(at) = finished_at {
        agent.insert("finishedAt".to_string(), json!(at));
    }
    if let Some(preview) = preview {
        agent.insert("preview".to_string(), json!(preview));
    }
    Value::Object(agent)
}

/// The finished run Claude wrote, when it belongs to this launch.
fn finished_run(path: &Path, launch: &WorkflowLaunch) -> Option<Value> {
    let run: Value = serde_json::from_str(&read_bounded(path, MAX_RUN_FILE_BYTES)?).ok()?;
    // A rerun of the same run id rewrites the file; an older launch keeps its own live reading.
    (text(run.get("taskId")) == Some(launch.task_id.as_str())).then_some(run)
}

fn from_finished_run(run: &Value, finished: bool) -> (Vec<Value>, Vec<Value>) {
    let mut phases = Vec::new();
    let mut agents = Vec::new();
    let details: HashMap<&str, &str> = run
        .get("phases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|phase| Some((text(phase.get("title"))?, text(phase.get("detail"))?)))
        .collect();
    for entry in run
        .get("workflowProgress")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match text(entry.get("type")) {
            Some("workflow_phase") => {
                if let Some(title) = text(entry.get("title")) {
                    let mut phase = json!({ "title": title });
                    if let Some(detail) = details.get(title) {
                        phase["detail"] = json!(detail);
                    }
                    phases.push(phase);
                }
            }
            Some("workflow_agent") => {
                let Some(label) = text(entry.get("label")) else {
                    continue;
                };
                let status = match text(entry.get("state")) {
                    Some("done") => "done",
                    Some("error") => "failed",
                    Some("queued") if !finished => "queued",
                    _ if finished => "stopped",
                    _ => "running",
                };
                let started_at = entry.get("startedAt").and_then(Value::as_i64);
                let finished_at = started_at
                    .zip(entry.get("durationMs").and_then(Value::as_i64))
                    .map(|(at, duration)| at + duration)
                    .filter(|_| status != "running" && status != "queued");
                let shown = text(entry.get("resultPreview"))
                    .or_else(|| text(entry.get("promptPreview")))
                    .and_then(preview);
                agents.push(agent_value(
                    text(entry.get("agentId")),
                    label,
                    text(entry.get("phaseTitle")),
                    text(entry.get("model")),
                    status,
                    started_at,
                    finished_at,
                    shown,
                ));
            }
            _ => {}
        }
    }
    (phases, agents)
}

/// A run that is still going (or ended without writing its run file), read from its folder.
fn from_run_folder(folder: &Path, finished: bool) -> Vec<Value> {
    struct Live {
        id: String,
        label: String,
        phase: Option<String>,
        result: Option<String>,
        failed: bool,
    }
    let mut order: Vec<String> = Vec::new();
    let mut live: HashMap<String, Live> = HashMap::new();
    if let Some(journal) = read_bounded(&folder.join("journal.jsonl"), MAX_RUN_FILE_BYTES) {
        for line in journal.lines() {
            let Ok(entry) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(id) = text(entry.get("agentId")).filter(|id| safe_id(id)) else {
                continue;
            };
            let agent = live.entry(id.to_string()).or_insert_with(|| {
                order.push(id.to_string());
                Live {
                    id: id.to_string(),
                    label: String::new(),
                    phase: None,
                    result: None,
                    failed: false,
                }
            });
            if let Some(label) = text(entry.get("label")) {
                agent.label = label.to_string();
            }
            if let Some(phase) = text(entry.get("phase")) {
                agent.phase = Some(phase.to_string());
            }
            match text(entry.get("type")) {
                Some("result") => {
                    agent.result = Some(match entry.get("result") {
                        Some(Value::String(result)) => result.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    });
                }
                Some(kind) if kind.contains("error") || kind.contains("fail") => {
                    agent.failed = true;
                }
                _ => {}
            }
        }
    }
    // An agent that started before its journal line was flushed still has its files.
    if let Ok(entries) = fs::read_dir(folder) {
        let mut found: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let id = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
                safe_id(id).then(|| id.to_string())
            })
            .filter(|id| !live.contains_key(id))
            .collect();
        found.sort();
        for id in found {
            order.push(id.clone());
            live.insert(
                id.clone(),
                Live {
                    id,
                    label: String::new(),
                    phase: None,
                    result: None,
                    failed: false,
                },
            );
        }
    }
    order
        .iter()
        .filter_map(|id| live.get(id))
        .map(|agent| {
            let transcript = folder.join(format!("agent-{}.jsonl", agent.id));
            let meta: Option<Value> = read_bounded(
                &folder.join(format!("agent-{}.meta.json", agent.id)),
                64 * 1024,
            )
            .and_then(|text| serde_json::from_str(&text).ok());
            let label = if agent.label.is_empty() {
                text(meta.as_ref().and_then(|meta| meta.get("description")))
                    .unwrap_or(agent.id.as_str())
                    .to_string()
            } else {
                agent.label.clone()
            };
            let phase = agent.phase.clone().or_else(|| {
                text(meta.as_ref().and_then(|meta| meta.get("workflowPhase"))).map(str::to_string)
            });
            let (started_at, prompt) = agent_start(&transcript);
            let status = if agent.failed {
                "failed"
            } else if agent.result.is_some() {
                "done"
            } else if finished {
                "stopped"
            } else {
                "running"
            };
            let finished_at = (status != "running")
                .then(|| modified_ms(&transcript))
                .flatten();
            let shown = agent.result.as_deref().and_then(preview).or(prompt);
            agent_value(
                Some(&agent.id),
                &label,
                phase.as_deref(),
                text(meta.as_ref().and_then(|meta| meta.get("model"))),
                status,
                started_at,
                finished_at,
                shown,
            )
        })
        .collect()
}

/// The script a launch ran: inline on the call, or the copy Claude saved for a rerun.
fn launch_script(
    session_folder: &Path,
    launch: &WorkflowLaunch,
    inline: Option<&str>,
) -> Option<String> {
    if let Some(inline) = inline {
        return Some(inline.to_string());
    }
    // Only a file in this session's own scripts folder, by name.
    let name = Path::new(launch.script_path.as_deref()?).file_name()?;
    read_bounded(
        &session_folder.join("workflows").join("scripts").join(name),
        MAX_SCRIPT_BYTES,
    )
}

fn read_launch(session_folder: &Path, launch: &WorkflowLaunch, inline: Option<&str>) -> Value {
    let script = launch_script(session_folder, launch, inline);
    let meta = script.as_deref().map(script_meta).unwrap_or_default();
    let finished = launch.finished_at.is_some();
    let run_file = session_folder
        .join("workflows")
        .join(format!("{}.json", launch.run_id));
    let run = finished_run(&run_file, launch);
    let mut status = run_status(launch.status.as_deref());
    let mut started_at = launch.launched_at;
    let mut finished_at = launch.finished_at;
    let (mut phases, agents) = match run.as_ref() {
        Some(run) => {
            // The run file is written when the run ends, a moment before the notification.
            if status == "running" {
                status = run_status(text(run.get("status")));
            }
            if let Some(start) = run.get("startTime").and_then(Value::as_i64) {
                started_at = start;
            }
            if let Some(duration) = run.get("durationMs").and_then(Value::as_i64) {
                finished_at = Some(started_at + duration);
            }
            from_finished_run(run, status != "running")
        }
        None => (
            Vec::new(),
            from_run_folder(
                &session_folder
                    .join("subagents")
                    .join("workflows")
                    .join(&launch.run_id),
                finished,
            ),
        ),
    };
    if status == "running" {
        finished_at = None;
    }
    if phases.is_empty() {
        phases = meta
            .phases
            .iter()
            .map(|(title, detail)| match detail {
                Some(detail) => json!({ "title": title, "detail": detail }),
                None => json!({ "title": title }),
            })
            .collect();
    }
    // A phase an agent names that the script's meta did not list still gets its heading.
    for agent in &agents {
        if let Some(phase) = text(agent.get("phase")) {
            if !phases
                .iter()
                .any(|known| text(known.get("title")) == Some(phase))
            {
                phases.push(json!({ "title": phase }));
            }
        }
    }
    let mut value = Map::new();
    value.insert("callId".to_string(), json!(launch.call_id));
    value.insert("runId".to_string(), json!(launch.run_id));
    value.insert(
        "name".to_string(),
        json!(meta.name.unwrap_or_else(|| launch.name.clone())),
    );
    if let Some(description) = launch.summary.clone().or(meta.description) {
        value.insert("description".to_string(), json!(description));
    }
    value.insert("status".to_string(), json!(status));
    value.insert("startedAt".to_string(), json!(started_at));
    if let Some(at) = finished_at {
        value.insert("finishedAt".to_string(), json!(at));
    }
    if let Some(script) = script {
        value.insert("script".to_string(), json!(script));
    }
    value.insert("phases".to_string(), Value::Array(phases));
    value.insert("agents".to_string(), Value::Array(agents));
    Value::Object(value)
}

/// Every Workflow call in a Claude session, or `None` when it made none.
pub fn read_session_chat_workflows(
    agent_session_path: Option<&str>,
) -> Option<SessionChatWorkflows> {
    let path = Path::new(agent_session_path?.trim());
    let runs = crate::session_chat_claude_fleet::read_claude_workflow_runs(path).ok()?;
    let session_folder = path.with_extension("");
    let launches: Vec<_> = runs.launches().collect();
    let skip = launches.len().saturating_sub(MAX_WORKFLOW_CARDS);
    let runs: Vec<Value> = launches
        .into_iter()
        .skip(skip)
        .map(|(launch, inline)| read_launch(&session_folder, launch, inline))
        .collect();
    (!runs.is_empty()).then_some(SessionChatWorkflows { runs })
}
