//! CDXC:SessionStatus 2026-11-28 WHY:
//! Antigravity (agy 1.5.18) starts a sub-agent with its `invoke_subagent` tool and runs it as a conversation of its own. The ACP stream shows only the parent's tool call (plus the child's calls that need an approval, under the parent's session with a `<childId>:` prefix on the call id), so the chat had no roster and nothing to open. agy records the link in plain files beside the parent's step log, which is what this reads (shapes from one recorded run, `server/tests/fixtures/antigravity_subagents/`):
//!
//!     ~/.gemini/antigravity-cli/brain/<parent>/.system_generated/subagents/<child>.json
//!         {"conversationId": "<child>", "subagentDescriptor": {"typeName": "research",
//!          "role": "File Researcher"}, "state": "SUBAGENT_STATE_ALIVE", "spawnStepIndex": 4, …}
//!     ~/.gemini/antigravity-cli/brain/<child>/.system_generated/logs/transcript_full.jsonl
//!
//! The child's log is the same step log the lead's chat mirrors, so the viewer reads it through the same mirror. `state` stays `ALIVE` after the child's last reply (it can still be messaged) and turns `KILLED` when it is stopped, so whether it is still working comes from its log's last step: a reply with no tool calls is a finished child, anything else is work in flight. A child with no step for ten minutes is a leftover, not work. The roster lists working children and the ones started since the lead's latest prompt, so a finished child stays openable from the menu for the rest of that turn, also after a resume.
//! SEE-ALSO: server/src/session_chat_fleet_status.rs (read_fleet), server/src/session_chat_subagent.rs (opening a child), server/src/session_chat_antigravity_mirror.rs (the step log and the `agentId:` line on the spawn row).

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_antigravity_mirror::{
    antigravity_app_data_dir, find_antigravity_transcript, is_safe_antigravity_session_id,
};

const STALE_CHILD_SECONDS: u64 = 600;
const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024;
const MAX_CHILDREN: usize = 64;
const MAX_DEPTH: usize = 4;
/// Enough of a log's end to hold its last step (a step is one line).
const TAIL_BYTES: u64 = 256 * 1024;

/// One `subagents/<child>.json` record.
pub(crate) struct AntigravityChild {
    pub id: String,
    pub type_name: Option<String>,
    pub role: Option<String>,
    killed: bool,
    spawn_step_index: Option<u64>,
    descriptor_modified: Option<SystemTime>,
}

fn subagents_dir(conversation_id: &str) -> PathBuf {
    antigravity_app_data_dir()
        .join("brain")
        .join(conversation_id)
        .join(".system_generated")
        .join("subagents")
}

fn text_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// The children `conversation_id` started, in spawn order. No folder is no children.
pub(crate) fn antigravity_children(conversation_id: &str) -> anyhow::Result<Vec<AntigravityChild>> {
    if !is_safe_antigravity_session_id(conversation_id) {
        return Ok(Vec::new());
    }
    let entries = match fs::read_dir(subagents_dir(conversation_id)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut children = Vec::new();
    for entry in entries.flatten() {
        if children.len() >= MAX_CHILDREN {
            break;
        }
        let file_name = entry.file_name();
        let Some(id) = file_name
            .to_str()
            .and_then(|name| name.strip_suffix(".json"))
            .filter(|id| is_safe_antigravity_session_id(id) && *id != conversation_id)
        else {
            continue;
        };
        let mut bytes = Vec::new();
        let Ok(file) = File::open(entry.path()) else {
            continue;
        };
        let modified = file.metadata().ok().and_then(|meta| meta.modified().ok());
        if file
            .take(MAX_DESCRIPTOR_BYTES)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        // The file is named for the child; a record naming another conversation is not trusted.
        if text_field(record.get("conversationId")).is_some_and(|named| named != id) {
            continue;
        }
        let descriptor = record.get("subagentDescriptor");
        children.push(AntigravityChild {
            id: id.to_string(),
            type_name: text_field(descriptor.and_then(|d| d.get("typeName"))),
            role: text_field(descriptor.and_then(|d| d.get("role"))),
            killed: record.get("state").and_then(Value::as_str) == Some("SUBAGENT_STATE_KILLED"),
            spawn_step_index: record.get("spawnStepIndex").and_then(Value::as_u64),
            descriptor_modified: modified,
        });
    }
    children.sort_by(|a, b| {
        a.spawn_step_index
            .cmp(&b.spawn_step_index)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(children)
}

/// `selector` when it is `root_id`'s child or a child's child (the viewer's Back stack opens
/// nested ones from inside their parent), refusing any other conversation.
pub(crate) fn antigravity_descendant(
    root_id: &str,
    selector: &str,
) -> anyhow::Result<AntigravityChild> {
    anyhow::ensure!(
        is_safe_antigravity_session_id(selector),
        "Not an Antigravity subagent id."
    );
    let mut level = vec![root_id.to_string()];
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for parent in &level {
            for child in antigravity_children(parent)? {
                if child.id == selector {
                    return Ok(child);
                }
                next.push(child.id);
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    anyhow::bail!("This subagent does not belong to this session.")
}

/// The last complete step of a step log.
fn last_step(path: &Path) -> Option<Value> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .find_map(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .filter(Value::is_object)
        })
}

fn first_step(path: &Path) -> Option<Value> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(TAIL_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    text.lines().find_map(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .filter(Value::is_object)
    })
}

/// The model of the newest step that names one, split into agy's model and level
/// (`gemini-3.8-flash-low` is `gemini-3.8-flash` at `low`).
fn latest_model(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(mut file) = File::open(path) else {
        return (None, None);
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if file
        .seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)))
        .and_then(|_| file.read_to_end(&mut bytes))
        .is_err()
    {
        return (None, None);
    }
    let raw = String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter(|line| line.contains("\"model\""))
        .find_map(|line| text_field(serde_json::from_str::<Value>(line).ok()?.get("model")));
    match raw {
        Some(raw) => match crate::agent_model_catalog::split_antigravity_model_id(&raw) {
            Some((model, effort)) => (Some(model), Some(effort)),
            None => (Some(raw), None),
        },
        None => (None, None),
    }
}

/// The model and level a child last ran on, for the viewer's header.
pub(crate) fn antigravity_child_model(child_id: &str) -> (Option<String>, Option<String>) {
    find_antigravity_transcript(child_id)
        .map(|path| latest_model(&path))
        .unwrap_or((None, None))
}

fn step_time_ms(step: &Value) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(step.get("created_at")?.as_str()?)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn is_final_reply(step: &Value) -> bool {
    step.get("source").and_then(Value::as_str) == Some("MODEL")
        && step.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE")
        && step.get("status").and_then(Value::as_str) != Some("RUNNING")
        && step
            .get("tool_calls")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
}

/// The step index of the lead's latest prompt, so children it started before that are history.
fn latest_prompt_step(root_id: &str) -> Option<u64> {
    let path = find_antigravity_transcript(root_id)?;
    let mut bytes = Vec::new();
    File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter(|line| line.contains("\"USER_EXPLICIT\""))
        .find_map(|line| {
            let step = serde_json::from_str::<Value>(line).ok()?;
            (step.get("source").and_then(Value::as_str) == Some("USER_EXPLICIT"))
                .then(|| step.get("step_index").and_then(Value::as_u64))
                .flatten()
        })
}

fn age_seconds(modified: Option<SystemTime>) -> Option<u64> {
    SystemTime::now()
        .duration_since(modified?)
        .ok()
        .map(|age| age.as_secs())
}

/// `Err` is unreadable evidence, so callers show the last roster as unavailable.
pub(crate) fn read_antigravity_fleet(
    session: &Value,
) -> anyhow::Result<Option<SessionChatAgentFleet>> {
    if session.get("lifecycleState").and_then(Value::as_str) != Some("running") {
        return Ok(None);
    }
    let Some(root_id) = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .filter(|id| is_safe_antigravity_session_id(id))
    else {
        return Ok(None);
    };
    let children = antigravity_children(root_id)?;
    if children.is_empty() {
        return Ok(None);
    }
    let turn_start = latest_prompt_step(root_id);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut agents = Vec::new();
    for child in children {
        let log = find_antigravity_transcript(&child.id);
        let log_modified = log
            .as_deref()
            .and_then(|path| fs::metadata(path).ok())
            .and_then(|meta| meta.modified().ok());
        let last = log.as_deref().and_then(last_step);
        let first = log.as_deref().and_then(first_step);
        let quiet = age_seconds(log_modified.or(child.descriptor_modified))
            .is_none_or(|age| age > STALE_CHILD_SECONDS);
        let finished = child.killed || last.as_ref().is_some_and(is_final_reply);
        let working = !finished && !quiet;
        let this_turn = match (turn_start, child.spawn_step_index) {
            (Some(prompt), Some(spawn)) => spawn > prompt,
            _ => !quiet,
        };
        if !working && !this_turn {
            continue;
        }
        let descriptor_ms = child
            .descriptor_modified
            .and_then(|at| at.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|at| at.as_millis() as i64);
        let started_at = first
            .as_ref()
            .and_then(step_time_ms)
            .or(descriptor_ms)
            .unwrap_or(now_ms);
        let ended_at = if working {
            now_ms
        } else {
            last.as_ref().and_then(step_time_ms).unwrap_or(started_at)
        };
        let (model, effort) = log.as_deref().map(latest_model).unwrap_or((None, None));
        let nested = antigravity_children(&child.id)
            .map(|nested| nested.len() as u32)
            .unwrap_or(0);
        agents.push(SessionChatSubAgent {
            id: Some(child.id.clone()),
            started_at,
            working,
            done: finished,
            name: child
                .type_name
                .clone()
                .unwrap_or_else(|| "subagent".to_string()),
            model,
            effort,
            task: child.role.clone(),
            elapsed_seconds: Some(((ended_at - started_at).max(0) / 1000) as u64),
            tokens: None,
            nested: (nested > 0).then_some(nested),
        });
    }
    Ok((!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents)))
}
