/*
CDXC:SessionChat 2026-11-10 DECISION:
User: "i see 4 tasks on this thread but it's all old work not sure why i see them still". Claude Code's
on-disk task store (`~/.claude/tasks/<session id>/<n>.json`, read by `session_chat_agent_tasks`) is not
what the CLI itself lists: after the session restarts on a new process the CLI starts with an EMPTY list
(TaskList says "No tasks found", TaskUpdate says "Task not found") while the old `<n>.json` files stay on
disk forever, so the pill kept showing a release's four tasks two days later.

The transcript is the one place that records what the CLI answered, so the disk reading is filtered by it.
Replayed in order over the transcript, these decide which disk tasks the pill still shows:
  - a `SessionStart:resume` or `SessionStart:startup` hook row (the process restarted; `compact` is NOT a
    restart, the list survives it): every task is dropped until a later call names it again;
  - a TaskList answered "No tasks found": every task is dropped;
  - a TaskList that names tasks (`#7 [pending] ...`): exactly those stay;
  - a TaskUpdate or TaskGet answered "Task not found", or a TaskUpdate that deleted the task: that task
    is dropped;
  - a TaskCreate, TaskUpdate or TaskGet the CLI accepted: that task stays (or comes back).
A transcript with none of these rows (Claude without Ghostex's hooks, a brand new session, a transcript we
cannot read) leaves the disk reading untouched. The restart marker is the SessionStart hook row Ghostex
installs; the transcript carries no other restart row (no init row is written to it).

The transcript can be a gigabyte, so only its last `WINDOW_BYTES` are read the first time and every later
reading continues from where the last one stopped.
*/

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

/// How far back the first reading of a transcript looks.
const WINDOW_BYTES: u64 = 16 * 1024 * 1024;
/// A line longer than this is not a hook row or a short task call; it is skipped unparsed.
const MAX_LINE_BYTES: usize = 2 * 1024 * 1024;
/// Task calls waiting for their answer, per transcript.
const MAX_PENDING_CALLS: usize = 128;
/// Transcripts remembered at once.
const MAX_REMEMBERED: usize = 32;

/// What the replay concluded, applied to the disk reading with `keeps`.
#[derive(Clone, Debug, Default)]
pub(crate) struct TaskEvidence {
    /// Set by a restart or an empty/named TaskList: only `named` tasks stay.
    drop_all: bool,
    /// Tasks a call named since `drop_all` was set.
    named: BTreeSet<String>,
    /// Tasks the CLI said it does not have.
    gone: BTreeSet<String>,
}

impl TaskEvidence {
    pub(crate) fn keeps(&self, id: &str) -> bool {
        (!self.drop_all || self.named.contains(id)) && !self.gone.contains(id)
    }

    fn drop_everything(&mut self) {
        self.drop_all = true;
        self.named.clear();
        self.gone.clear();
    }

    fn keep_only(&mut self, ids: BTreeSet<String>) {
        self.drop_all = true;
        self.named = ids;
        self.gone.clear();
    }

    fn name(&mut self, id: &str) {
        self.gone.remove(id);
        self.named.insert(id.to_string());
    }

    fn drop_one(&mut self, id: &str) {
        self.named.remove(id);
        self.gone.insert(id.to_string());
    }
}

#[derive(Default)]
struct Replay {
    len_seen: u64,
    evidence: TaskEvidence,
    /// `tool_use` id -> (tool name, the task id the call was about, whether it deleted it).
    pending: HashMap<String, (String, Option<String>, bool)>,
}

fn replays() -> &'static Mutex<HashMap<String, Replay>> {
    static REPLAYS: OnceLock<Mutex<HashMap<String, Replay>>> = OnceLock::new();
    REPLAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The evidence the transcript holds about the CLI's task list, or `None` when it cannot be read.
pub(crate) fn read_task_evidence(transcript_path: &str) -> Option<TaskEvidence> {
    let mut file = File::open(transcript_path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut replays = replays().lock().ok()?;
    if replays.len() >= MAX_REMEMBERED && !replays.contains_key(transcript_path) {
        replays.clear();
    }
    let replay = replays.entry(transcript_path.to_string()).or_default();
    // A transcript that shrank was rewritten (compaction rewrite, new file): start over.
    if len < replay.len_seen {
        *replay = Replay::default();
    }
    if len == replay.len_seen && replay.len_seen != 0 {
        return Some(replay.evidence.clone());
    }
    let first_reading = replay.len_seen == 0;
    let start = if first_reading {
        len.saturating_sub(WINDOW_BYTES)
    } else {
        replay.len_seen
    };
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity((len - start).min(WINDOW_BYTES) as usize);
    file.take(len - start).read_to_end(&mut bytes).ok()?;
    // Only whole lines count; a half-written last line is read next time.
    let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        if first_reading {
            replay.len_seen = len;
        }
        return Some(replay.evidence.clone());
    };
    let consumed = &bytes[..=last_newline];
    let mut lines = consumed.split(|byte| *byte == b'\n');
    // The window began mid-line: its first piece is a fragment.
    if first_reading && start > 0 {
        lines.next();
    }
    for line in lines {
        replay.apply_line(line);
    }
    replay.len_seen = start + consumed.len() as u64;
    Some(replay.evidence.clone())
}

impl Replay {
    fn apply_line(&mut self, line: &[u8]) {
        if line.is_empty() || line.len() > MAX_LINE_BYTES {
            return;
        }
        let has = |needle: &[u8]| line.windows(needle.len()).any(|window| window == needle);
        let is_restart = has(b"\"hookName\":\"SessionStart:resume\"")
            || has(b"\"hookName\":\"SessionStart:startup\"");
        if !is_restart && !has(b"\"Task") && !has(b"\"tool_result\"") {
            return;
        }
        let Ok(row) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if is_restart {
            if row.get("type").and_then(Value::as_str) == Some("attachment") {
                self.evidence.drop_everything();
                self.pending.clear();
            }
            return;
        }
        // The `api-request-*` rows repeat the same messages as the rows around them.
        if !matches!(
            row.get("type").and_then(Value::as_str),
            Some("assistant") | Some("user")
        ) {
            return;
        }
        let Some(blocks) = row
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => self.note_call(block),
                Some("tool_result") => self.note_answer(block),
                _ => {}
            }
        }
    }

    fn note_call(&mut self, block: &Value) {
        let (Some(id), Some(name)) = (
            block.get("id").and_then(Value::as_str),
            block.get("name").and_then(Value::as_str),
        ) else {
            return;
        };
        if !matches!(name, "TaskCreate" | "TaskUpdate" | "TaskGet" | "TaskList") {
            return;
        }
        let input = block.get("input");
        let task_id = input
            .and_then(|input| input.get("taskId"))
            .and_then(|task_id| match task_id {
                Value::String(task_id) => Some(task_id.trim().to_string()),
                Value::Number(task_id) => Some(task_id.to_string()),
                _ => None,
            })
            .filter(|task_id| !task_id.is_empty());
        let deletes = input
            .and_then(|input| input.get("status"))
            .and_then(Value::as_str)
            == Some("deleted");
        if self.pending.len() >= MAX_PENDING_CALLS {
            self.pending.clear();
        }
        self.pending
            .insert(id.to_string(), (name.to_string(), task_id, deletes));
    }

    fn note_answer(&mut self, block: &Value) {
        let Some(call_id) = block.get("tool_use_id").and_then(Value::as_str) else {
            return;
        };
        let Some((name, task_id, deletes)) = self.pending.remove(call_id) else {
            return;
        };
        if block.get("is_error").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let text = answer_text(block.get("content"));
        let text = text.trim();
        let not_found = text.eq_ignore_ascii_case("task not found");
        match name.as_str() {
            "TaskList" => {
                if text.eq_ignore_ascii_case("no tasks found") {
                    self.evidence.drop_everything();
                } else {
                    let ids = listed_task_ids(text);
                    // A list we cannot read names nothing; it proves nothing either way.
                    if !ids.is_empty() {
                        self.evidence.keep_only(ids);
                    }
                }
            }
            "TaskCreate" => {
                if let Some(id) = created_task_id(text) {
                    self.evidence.name(&id);
                }
            }
            "TaskUpdate" | "TaskGet" => {
                let Some(task_id) = task_id else {
                    return;
                };
                if not_found || (name == "TaskUpdate" && deletes) {
                    self.evidence.drop_one(&task_id);
                } else if !text.is_empty() {
                    self.evidence.name(&task_id);
                }
            }
            _ => {}
        }
    }
}

/// A tool answer is a string or a list of `{ "type": "text", "text": ... }` blocks.
fn answer_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// "Task #11 created successfully: ..." -> "11".
fn created_task_id(text: &str) -> Option<String> {
    let digits: String = text
        .strip_prefix("Task #")?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// The ids a TaskList answer names: one `#<id> [status] subject` line per task.
fn listed_task_ids(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|line| {
            let digits: String = line
                .trim_start()
                .strip_prefix('#')?
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            (!digits.is_empty()).then_some(digits)
        })
        .collect()
}
