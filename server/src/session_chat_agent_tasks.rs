/*
CDXC:SessionChat 2026-09-03:
The task list Claude Code keeps for a session (its TaskCreate / TaskUpdate
tools, the block the CLI pins under its transcript and folds with `ctrl+t`):

      ◼ ⟳ MNS-40: dedupe start_adapter across four test suites
      ◻ ⌛ connection_audit_log flakes under full-suite load
      ✔ ✅ MNS-24: Close the unclassified-POST fail-open hole
        … +16 completed

None of it reaches transcript JSONL as a list: the tool calls do, but the
CURRENT state of the list is only ever the on-disk store the CLI writes to,
`~/.claude/tasks/<session id>/<n>.json`, one file per task. That store is what
this module reads, so the chat surface shows the same list the terminal does.

One file per task, read whole on every detection: the list is small (tens of
items at most) and the detector already runs on the option cache's cadence, so
a directory scan costs nothing next to the screen capture it rides beside.
`same_session_chat_agent_tasks` decides whether a reading is worth a frame; a
list that did not change is never republished.

A task file arrives as
  { "id": "9", "subject": "…", "description": "…", "activeForm": "…",
    "status": "in_progress", "blocks": [], "blockedBy": [] }
Only what the panel shows is carried: the description is the agent's working
notes, not something the user asked to read, and `blocks` is the inverse of
`blockedBy`, which is the one direction a reader cares about ("why is this
waiting").
*/

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// Where Claude Code keeps per-session task stores, under its config dir.
const CLAUDE_TASKS_DIRECTORY: &str = "tasks";

/// One task, in the order the CLI numbers them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatAgentTask {
    pub id: String,
    pub subject: String,
    /// The present-continuous label the CLI paints while the task is running.
    pub active_form: Option<String>,
    /// `pending`, `in_progress` or `completed`, verbatim from the file.
    pub status: String,
    /// Ids of tasks that must finish before this one can start.
    pub blocked_by: Vec<String>,
}

impl SessionChatAgentTask {
    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("subject".to_string(), json!(self.subject));
        if let Some(active_form) = self.active_form.as_ref() {
            map.insert("activeForm".to_string(), json!(active_form));
        }
        map.insert("status".to_string(), json!(self.status));
        if !self.blocked_by.is_empty() {
            map.insert("blockedBy".to_string(), json!(self.blocked_by));
        }
        Value::Object(map)
    }
}

/// The whole store, never empty: no task files means no list at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatAgentTasks {
    pub tasks: Vec<SessionChatAgentTask>,
}

impl SessionChatAgentTasks {
    pub fn to_value(&self) -> Value {
        json!({
            "tasks": self.tasks.iter().map(SessionChatAgentTask::to_value).collect::<Vec<_>>(),
        })
    }
}

/// True when both readings list the same tasks in the same states, so a
/// periodic re-read only emits a frame on a REAL change.
pub fn same_session_chat_agent_tasks(
    current: Option<&SessionChatAgentTasks>,
    published: Option<&SessionChatAgentTasks>,
) -> bool {
    current == published
}

/*
The config directory the session's OWN CLI writes under. Claude Code keeps
everything, transcripts and task stores alike, below one config root
(`CLAUDE_CONFIG_DIR`, default `~/.claude`), and that root is per-process: a
profile-swapped CLI runs with a different root than the daemon that watches
it, so the daemon's own environment is the wrong place to ask. The transcript
path the hooks reported IS that root, `<root>/projects/<slug>/<id>.jsonl`, so
the store is read beside it. Without a transcript path there is only the
daemon's environment to go on.
*/
pub(crate) fn claude_config_directory(agent_session_path: Option<&str>) -> PathBuf {
    if let Some(root) = agent_session_path.map(Path::new).and_then(|transcript| {
        let slug_dir = transcript.parent()?;
        let projects_dir = slug_dir.parent()?;
        (projects_dir.file_name()?.to_str()? == "projects").then(|| projects_dir.parent())?
    }) {
        return root.to_path_buf();
    }
    std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| crate::resume_lookup::expand_home(&value))
        .unwrap_or_else(|| crate::resume_lookup::home_dir().join(".claude"))
}

/// The task store of one Claude session, or `None` when it has no tasks.
pub fn read_session_chat_agent_tasks(
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<SessionChatAgentTasks> {
    let agent_session_id = agent_session_id?.trim();
    if agent_session_id.is_empty() {
        return None;
    }
    read_session_chat_agent_tasks_in(
        &claude_config_directory(agent_session_path)
            .join(CLAUDE_TASKS_DIRECTORY)
            .join(agent_session_id),
    )
}

pub(crate) fn read_session_chat_agent_tasks_in(store: &Path) -> Option<SessionChatAgentTasks> {
    let entries = fs::read_dir(store).ok()?;
    let mut tasks: Vec<(u64, SessionChatAgentTask)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            // The store also holds `.lock` and `.highwatermark`; only `<n>.json` is a task.
            let stem = path.file_stem()?.to_str()?;
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let text = fs::read_to_string(&path).ok()?;
            let task = parse_session_chat_agent_task(&text)?;
            // The file name is the CLI's own ordering; the id inside is the
            // same number and is what the panel shows.
            let order = stem
                .parse::<u64>()
                .or_else(|_| task.id.parse::<u64>())
                .unwrap_or(u64::MAX);
            Some((order, task))
        })
        .collect();
    if tasks.is_empty() {
        return None;
    }
    tasks.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
    Some(SessionChatAgentTasks {
        tasks: tasks.into_iter().map(|(_, task)| task).collect(),
    })
}

fn parse_session_chat_agent_task(text: &str) -> Option<SessionChatAgentTask> {
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let subject = object.get("subject")?.as_str()?.trim();
    if subject.is_empty() {
        return None;
    }
    let id = match object.get("id") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Number(id)) => id.to_string(),
        _ => return None,
    };
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .unwrap_or("pending")
        .to_string();
    let active_form = object
        .get("activeForm")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|form| !form.is_empty())
        .map(str::to_string);
    let blocked_by = object
        .get("blockedBy")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| match id {
                    Value::String(id) => Some(id.clone()),
                    Value::Number(id) => Some(id.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    Some(SessionChatAgentTask {
        id,
        subject: subject.to_string(),
        active_form,
        status,
        blocked_by,
    })
}
