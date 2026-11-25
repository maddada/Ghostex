//! Transcript mirror paths, streaming delta assembly, usage/tasks/fleet extraction, and atomic JSONL writes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::super::host::now_ms;
use super::http::OpenCodeHttp;
use crate::session_chat::SessionChatRole;
use crate::session_chat_agent_fleet::{SessionChatAgentFleet, SessionChatSubAgent};
use crate::session_chat_agent_tasks::{SessionChatAgentTask, SessionChatAgentTasks};

/// Path to the Native OpenCode transcript mirror for `session_id`.
pub(crate) fn mirror_path(session_id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("acp-opencode-mirror")
        .join(format!("{session_id}.jsonl"))
}

/// Path to a mirrored child subagent transcript and its metadata file.
pub(crate) fn subagent_mirror_path(child_id: &str) -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("acp-opencode-mirror")
        .join("subagents")
        .join(format!("{child_id}.jsonl"))
}

fn subagent_meta_path(child_id: &str) -> PathBuf {
    subagent_mirror_path(child_id).with_extension("meta.json")
}

/// True when `path` is written by this Native driver (`acp-opencode-mirror/`), so
/// `session_chat_opencode::refresh_for_path` does not try to contact the shared
/// `opencode service` for it.
pub(crate) fn is_mirror_path(path: &Path) -> bool {
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    if parent_name == Some("acp-opencode-mirror") {
        return true;
    }
    if parent_name == Some("subagents") {
        return path
            .parent()
            .and_then(Path::parent)
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("acp-opencode-mirror");
    }
    false
}

/// Resolve a Native OpenCode child subagent mirror owned by `root_id`.
pub(crate) fn resolve_subagent_mirror(
    root_id: &str,
    selector: &str,
) -> Option<(PathBuf, String, Option<String>)> {
    if !crate::session_chat_opencode::safe_id(selector) {
        return None;
    }
    let path = subagent_mirror_path(selector);
    let meta_path = subagent_meta_path(selector);
    let meta: Value = fs::read_to_string(&meta_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())?;
    if meta.get("rootId").and_then(Value::as_str) != Some(root_id) {
        return None;
    }
    if !path.is_file() {
        return None;
    }
    let name = meta
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(selector)
        .to_string();
    let agent_type = meta
        .get("agent")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string);
    Some((path, name, agent_type))
}

/// Live streamed text/reasoning parts that OpenCode's `/message` endpoint still returns empty
/// until the part ends.
#[derive(Default, Clone)]
pub(super) struct LiveParts {
    parts: HashMap<(String, bool, u64), String>,
}

impl LiveParts {
    pub(super) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub(super) fn append(&mut self, message_id: &str, reasoning: bool, ordinal: u64, delta: &str) {
        self.parts
            .entry((message_id.to_string(), reasoning, ordinal))
            .or_default()
            .push_str(delta);
    }

    pub(super) fn prune(&mut self, messages: &[Value]) {
        self.parts.retain(|(msg_id, _, _), _| {
            !messages.iter().any(|row| {
                row["id"].as_str() == Some(msg_id.as_str()) && !row["time"]["completed"].is_null()
            })
        });
    }

    pub(super) fn clear(&mut self) {
        self.parts.clear();
    }

    pub(super) fn apply(&self, messages: &[Value]) -> Vec<Value> {
        if self.parts.is_empty() {
            return messages.to_vec();
        }
        messages
            .iter()
            .map(|row| {
                let Some(id) = row["id"].as_str().filter(|_| row["type"] == "assistant") else {
                    return row.clone();
                };
                if !self.parts.keys().any(|(m, _, _)| m == id) {
                    return row.clone();
                }
                let mut row = row.clone();
                let mut ordinals = [0u64; 2];
                for part in row["content"].as_array_mut().into_iter().flatten() {
                    let reasoning = match part["type"].as_str() {
                        Some("text") => false,
                        Some("reasoning") => true,
                        _ => continue,
                    };
                    let ordinal = ordinals[reasoning as usize];
                    ordinals[reasoning as usize] += 1;
                    if part["text"].as_str().is_some_and(str::is_empty) {
                        if let Some(text) = self.parts.get(&(id.to_string(), reasoning, ordinal)) {
                            part["text"] = Value::String(text.clone());
                        }
                    }
                }
                row
            })
            .collect()
    }
}

pub(super) fn extract_usage_from_messages(messages: &[Value]) -> Option<u64> {
    let msg = messages
        .iter()
        .rev()
        .take_while(|m| !(m["type"] == "compaction" && m["status"] == "completed"))
        .find(|m| m["type"] == "assistant" && m["tokens"].is_object())?;
    tokens_total(&msg["tokens"])
}

pub(super) fn tokens_total(t: &Value) -> Option<u64> {
    let used = [
        &t["input"],
        &t["output"],
        &t["reasoning"],
        &t["cache"]["read"],
        &t["cache"]["write"],
    ]
    .into_iter()
    .filter_map(Value::as_u64)
    .fold(0u64, u64::saturating_add);
    (used > 0).then_some(used)
}

pub(super) fn extract_tasks_from_messages(messages: &[Value]) -> Option<SessionChatAgentTasks> {
    let todos = messages
        .iter()
        .rev()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten().rev())
        .find(|p| {
            p["type"] == "tool" && p["name"] == "todowrite" && p["state"]["status"] == "completed"
        })?["state"]["input"]["todos"]
        .as_array()?;
    let tasks: Vec<SessionChatAgentTask> = todos
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            Some(SessionChatAgentTask {
                id: t["id"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| i.to_string()),
                subject: t["content"].as_str()?.into(),
                status: t["status"].as_str().unwrap_or("pending").into(),
                active_form: None,
                blocked_by: vec![],
            })
        })
        .collect();
    (!tasks.is_empty()).then_some(SessionChatAgentTasks { tasks })
}

fn finished_task_children(messages: &[Value]) -> HashSet<String> {
    let mut latest = HashMap::new();
    for part in messages
        .iter()
        .filter(|m| m["type"] == "assistant")
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .filter(|p| p["type"] == "tool" && matches!(p["name"].as_str(), Some("task" | "subagent")))
    {
        if let Some(child) = part["state"]["metadata"]["sessionId"]
            .as_str()
            .or_else(|| part["state"]["metadata"]["sessionID"].as_str())
        {
            latest.insert(child.to_string(), part["state"]["status"] == "completed");
        }
    }
    latest
        .into_iter()
        .filter_map(|(child, completed)| completed.then_some(child))
        .collect()
}

pub(super) fn build_fleet(
    children: &HashMap<String, Value>,
    active_sessions: &HashSet<String>,
    messages: &[Value],
) -> Option<SessionChatAgentFleet> {
    if children.is_empty() {
        return None;
    }
    let now = now_ms();
    let finished = finished_task_children(messages);
    let mut list: Vec<&Value> = children.values().collect();
    list.sort_by_key(|c| c["time"]["created"].as_i64().unwrap_or(0));
    let agents: Vec<SessionChatSubAgent> = list
        .into_iter()
        .filter_map(|child| {
            let id = child["id"].as_str()?;
            let started = child["time"]["created"].as_i64().unwrap_or(now);
            let working = active_sessions.contains(id);
            Some(SessionChatSubAgent {
                id: Some(id.to_string()),
                started_at: started,
                working,
                done: !working && finished.contains(id),
                name: child["agent"].as_str().unwrap_or("OpenCode agent").into(),
                task: child["title"].as_str().map(str::to_string),
                model: child["model"]["id"].as_str().map(str::to_string),
                effort: child["model"]["variant"].as_str().map(str::to_string),
                elapsed_seconds: Some(
                    (if working {
                        now
                    } else {
                        child["time"]["idle"].as_i64().unwrap_or(started)
                    })
                    .saturating_sub(started)
                    .max(0) as u64
                        / 1000,
                ),
                tokens: None,
                nested: None,
            })
        })
        .collect();
    (!agents.is_empty()).then(|| SessionChatAgentFleet::new(agents))
}

pub(super) fn write_atomic_mirror_bytes(path: &Path, output: &[u8]) -> std::io::Result<()> {
    let previous = fs::read(path).unwrap_or_default();
    if previous == output && path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(output)?;
        temp.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}

pub(super) fn serialize_mirror_jsonl(
    messages: &[Value],
    info: &Value,
    working: bool,
    interrupted: bool,
    live: &LiveParts,
) -> Vec<u8> {
    let mut output = Vec::new();
    let effective = if live.is_empty() {
        crate::session_chat_opencode::decode_messages(messages)
    } else {
        crate::session_chat_opencode::decode_messages(&live.apply(messages))
    };
    for row in &effective {
        let _ = serde_json::to_writer(&mut output, row);
        output.push(b'\n');
    }
    if let Some(turn) = effective
        .iter()
        .rev()
        .find(|row| row.role == SessionChatRole::User)
    {
        let idle = messages.iter().rev().find(|m| m["type"] == "idle");
        let state_str = if working {
            "working"
        } else if interrupted || idle.is_some_and(|m| m["outcome"] == "interrupted") {
            "interrupted"
        } else {
            "completed"
        };
        let _ = serde_json::to_writer(
            &mut output,
            &json!({
                "opencodeLifecycle": {
                    "state": state_str,
                    "turnId": turn.id,
                    "timestamp": info["time"]["idle"].as_i64().or(turn.timestamp),
                }
            }),
        );
        output.push(b'\n');
    }
    output
}

pub(super) fn sync_child_mirror(
    http: &OpenCodeHttp,
    root_id: &str,
    child_info: &Value,
    active: bool,
) {
    let Some(child_id) = child_info
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| crate::session_chat_opencode::safe_id(id))
    else {
        return;
    };
    let Ok(messages) = http.fetch_messages(child_id) else {
        return;
    };
    let bytes = serialize_mirror_jsonl(&messages, child_info, active, false, &LiveParts::default());
    let path = subagent_mirror_path(child_id);
    let _ = write_atomic_mirror_bytes(&path, &bytes);
    let meta = json!({
        "id": child_id,
        "rootId": root_id,
        "title": child_info.get("title").and_then(Value::as_str),
        "agent": child_info.get("agent").and_then(Value::as_str),
    });
    if let Ok(meta_bytes) = serde_json::to_vec(&meta) {
        let _ = write_atomic_mirror_bytes(&subagent_meta_path(child_id), &meta_bytes);
    }
}
