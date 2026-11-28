//! CDXC:AgentProviders 2026-11-28 WHY:
//! Native Cursor starts ACP with `_meta.subagents: true`, so `cursor-agent` (checked against
//! 2026.11.22) runs each Task child as its own ACP session: `subagent_spawned`
//! (`subagentSessionId`, `name`, `task`, `_meta.cursor.toolCallId`/`model`), then the child's own
//! `session/update`s under that session id, then `subagent_state_update` (`completed`, `failed`,
//! `cancelled`, `disconnected`). On `session/load` it replays the finished ones. The driver read
//! every update as the lead's, so a child's text and tool calls were written into the lead's
//! transcript and live message, and the chat had no Subagents pill and nothing to open. Each child
//! now gets its own Cursor-format transcript beside the lead's (`subagents/<id>.jsonl` plus a
//! `.meta.json` naming its lead), the roster is published as `agentFleet` (the shape Grok's driver
//! publishes, read by session_chat_options/detection.rs), and the Task row's result names the child
//! (`agentId: <id>`), so its chip opens the same transcript the menu row does.
//! SEE-ALSO: server/src/session_chat_subagent.rs (opening a child), reader.rs (routing).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};

use super::config_and_store::{cursor_native_transcript_dir, write_private_file};
use super::transcript::CursorTranscriptWriter;
use crate::acp::host::now_ms;

const CHILD_ID_MAX_LENGTH: usize = 160;

/// A child id Cursor minted (`<agentId>` or `<agentId>.<run>`) that is safe as a file name.
pub(crate) fn is_safe_cursor_child_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= CHILD_ID_MAX_LENGTH
        && !id.starts_with('.')
        && !id.contains("..")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn subagent_dir() -> PathBuf {
    cursor_native_transcript_dir().join("subagents")
}

/// The transcript a Native Cursor child writes, if `id` is safe.
pub(crate) fn cursor_subagent_transcript_path(id: &str) -> Option<PathBuf> {
    is_safe_cursor_child_id(id).then(|| subagent_dir().join(format!("{id}.jsonl")))
}

fn meta_path(id: &str) -> Option<PathBuf> {
    is_safe_cursor_child_id(id).then(|| subagent_dir().join(format!("{id}.meta.json")))
}

/// One Native Cursor child the viewer can open.
pub(crate) struct CursorSubagentRecord {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) agent_type: Option<String>,
    pub(crate) model: Option<String>,
}

fn read_record(id: &str, meta: &Value) -> Option<CursorSubagentRecord> {
    let path = cursor_subagent_transcript_path(id)?;
    let text = |key: &str| {
        meta.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(CursorSubagentRecord {
        id: id.to_string(),
        path,
        name: text("task")
            .or_else(|| text("name"))
            .unwrap_or_else(|| id.to_string()),
        agent_type: text("name"),
        model: text("model"),
    })
}

/// The child of `root_id` that `selector` names: its id, or else (a Task row read before Cursor
/// announced the child) the only child whose task or type is that text. A child of another lead is
/// never returned.
pub(crate) fn resolve_cursor_subagent(
    root_id: &str,
    selector: &str,
) -> anyhow::Result<CursorSubagentRecord> {
    let read_meta = |id: &str| -> Option<Value> {
        let raw = fs::read(meta_path(id)?).ok()?;
        serde_json::from_slice::<Value>(&raw)
            .ok()
            .filter(|meta| meta.get("rootId").and_then(Value::as_str) == Some(root_id))
    };
    if let Some(meta) = is_safe_cursor_child_id(selector)
        .then(|| read_meta(selector))
        .flatten()
    {
        return read_record(selector, &meta)
            .ok_or_else(|| anyhow::anyhow!("This subagent's transcript is not available yet."));
    }
    let mut matches = Vec::new();
    for entry in fs::read_dir(subagent_dir()).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".meta.json"))
        else {
            continue;
        };
        let Some(meta) = read_meta(id) else {
            continue;
        };
        let matches_text = ["task", "name"]
            .iter()
            .any(|key| meta.get(*key).and_then(Value::as_str).map(str::trim) == Some(selector));
        if matches_text {
            matches.extend(read_record(id, &meta));
        }
    }
    anyhow::ensure!(
        matches.len() <= 1,
        "Several subagents share this name. Open the agent from the Subagents menu."
    );
    matches
        .pop()
        .ok_or_else(|| anyhow::anyhow!("This subagent's transcript is not available yet."))
}

struct CursorSubagent {
    id: String,
    name: String,
    task: Option<String>,
    model: Option<String>,
    started_at_ms: i64,
    finished_at_ms: Option<i64>,
    /// `running` until Cursor reports how it ended.
    state: String,
    /// Shown in the Subagents menu: false for a child replayed by `session/load` and for one that
    /// ended before the current prompt.
    listed: bool,
    wrote_prompt: bool,
    writer: CursorTranscriptWriter,
}

impl CursorSubagent {
    fn working(&self) -> bool {
        self.finished_at_ms.is_none()
    }

    /// The child's task as its first line, unless Cursor already sent the prompt it was given.
    fn ensure_prompt(&mut self) {
        if self.wrote_prompt {
            return;
        }
        self.wrote_prompt = true;
        if let Some(task) = self.task.clone().filter(|task| !task.trim().is_empty()) {
            self.writer.write_user_message(&task);
        }
    }
}

/// Every child the lead started in this host run, in launch order.
#[derive(Default)]
pub(super) struct CursorSubagents {
    order: Vec<String>,
    by_id: HashMap<String, CursorSubagent>,
    /// True while `session/load` replays the conversation.
    pub(super) loading: bool,
}

fn text_field<'a>(update: &'a Value, key: &str) -> Option<&'a str> {
    update
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

impl CursorSubagents {
    /// `subagent_spawned`: start (or re-announce) a child of `lead_id`. Returns the child id and
    /// the lead tool call that started it.
    pub(super) fn spawned(
        &mut self,
        lead_id: Option<&str>,
        update: &Value,
    ) -> Option<(String, Option<String>)> {
        let id =
            text_field(update, "subagentSessionId").filter(|id| is_safe_cursor_child_id(id))?;
        let meta = update.pointer("/_meta/cursor");
        let tool_call_id = meta
            .and_then(|meta| text_field(meta, "toolCallId"))
            .map(str::to_string);
        let model = meta
            .and_then(|meta| text_field(meta, "model"))
            .map(str::to_string);
        let name = text_field(update, "name").unwrap_or("Subagent").to_string();
        let task = text_field(update, "task").map(str::to_string);
        if !self.by_id.contains_key(id) {
            let mut writer = CursorTranscriptWriter::new();
            let populated = cursor_subagent_transcript_path(id)
                .map(|path| writer.bind_path(path))
                .unwrap_or(false);
            // A transcript written live earlier is complete; `session/load` replays it again.
            writer.suppress_writes = populated;
            self.order.push(id.to_string());
            self.by_id.insert(
                id.to_string(),
                CursorSubagent {
                    id: id.to_string(),
                    name: name.clone(),
                    task: task.clone(),
                    model: model.clone(),
                    started_at_ms: now_ms(),
                    finished_at_ms: None,
                    state: "running".to_string(),
                    listed: !self.loading,
                    wrote_prompt: populated,
                    writer,
                },
            );
        }
        if let (Some(lead_id), Some(path)) = (lead_id, meta_path(id)) {
            let child = self.by_id.get(id)?;
            let meta = json!({
                "id": id,
                "rootId": lead_id,
                "name": child.name,
                "task": child.task,
                "model": child.model,
                "toolCallId": tool_call_id,
            });
            let _ = write_private_file(&path, meta.to_string().as_bytes());
        }
        Some((id.to_string(), tool_call_id))
    }

    /// `subagent_state_update`: a child ended (any state but a running one).
    pub(super) fn state_update(&mut self, update: &Value) {
        let Some(child) =
            text_field(update, "subagentSessionId").and_then(|id| self.by_id.get_mut(id))
        else {
            return;
        };
        let state = text_field(update, "state").unwrap_or("completed");
        if matches!(state, "running" | "started" | "in_progress" | "pending") {
            return;
        }
        if child.finished_at_ms.is_none() {
            child.finished_at_ms = Some(now_ms());
            child.state = state.to_string();
            child.writer.finish_turn(
                match state {
                    "completed" => "success",
                    "failed" => "error",
                    _ => "aborted",
                },
                None,
            );
        }
    }

    /// A `session/update` for `session_id` that is not the lead's: written to that child's
    /// transcript. Updates for a session nobody announced are dropped, never shown as the lead's.
    pub(super) fn route(&mut self, session_id: &str, update: &Value) {
        let Some(child) = self.by_id.get_mut(session_id) else {
            return;
        };
        let kind = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let chunk = || {
            update
                .pointer("/content/text")
                .or_else(|| update.get("text"))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
        };
        match kind {
            "user_message_chunk" => {
                if let Some(text) = chunk().filter(|text| !text.trim().is_empty()) {
                    child.wrote_prompt = true;
                    child.writer.write_user_message(text);
                }
            }
            "agent_message_chunk" => {
                if let Some(text) = chunk() {
                    child.ensure_prompt();
                    child.writer.push_text(text);
                }
            }
            "agent_thought_chunk" => {
                if let Some(text) = chunk() {
                    child.ensure_prompt();
                    child.writer.push_thought(text);
                }
            }
            "tool_call" | "tool_call_update" => {
                child.ensure_prompt();
                child.writer.handle_tool_update(update);
            }
            "plan" => {
                if let Some(entries) = update.get("entries").and_then(Value::as_array) {
                    child.ensure_prompt();
                    child.writer.write_plan_todos(entries);
                }
            }
            _ => {}
        }
    }

    /// A nested child's launch: the Task row in its parent child's transcript names it.
    pub(super) fn note_tool_result(&mut self, parent_id: &str, call_id: &str, child_id: &str) {
        if let Some(parent) = self.by_id.get_mut(parent_id) {
            parent
                .writer
                .note_tool_result(call_id, format!("agentId: {child_id}"));
        }
    }

    /// A new prompt: children that already ended leave the Subagents menu.
    pub(super) fn begin_turn(&mut self) {
        for child in self.by_id.values_mut() {
            if !child.working() {
                child.listed = false;
            }
        }
    }

    /// The host is going away: children still running are cut off with it.
    pub(super) fn end_all(&mut self) {
        for child in self.by_id.values_mut() {
            if child.finished_at_ms.is_none() {
                child.finished_at_ms = Some(now_ms());
                child.state = "disconnected".to_string();
                child.writer.finish_turn("aborted", None);
            }
        }
    }

    /// The roster as `agentFleet`, or `null` when no child is listed.
    pub(super) fn fleet_json(&self) -> Value {
        let now = now_ms();
        let agents: Vec<Value> = self
            .order
            .iter()
            .filter_map(|id| self.by_id.get(id))
            .filter(|child| child.listed)
            .map(|child| {
                let working = child.working();
                let ended = child.finished_at_ms.unwrap_or(now);
                let mut agent = Map::new();
                agent.insert("id".into(), json!(child.id));
                agent.insert("startedAt".into(), json!(child.started_at_ms));
                agent.insert(
                    "status".into(),
                    json!(if working { "working" } else { "idle" }),
                );
                // Always sent: detection reads a missing `done` as "not working", and a failed or
                // cut-off child did not finish its work.
                agent.insert("done".into(), json!(!working && child.state == "completed"));
                agent.insert("name".into(), json!(child.name));
                if let Some(model) = &child.model {
                    agent.insert("model".into(), json!(model));
                }
                if let Some(task) = &child.task {
                    agent.insert("task".into(), json!(task));
                }
                agent.insert(
                    "elapsedSeconds".into(),
                    json!((ended.saturating_sub(child.started_at_ms).max(0) / 1000) as u64),
                );
                Value::Object(agent)
            })
            .collect();
        if agents.is_empty() {
            return Value::Null;
        }
        json!({ "agents": agents })
    }
}
