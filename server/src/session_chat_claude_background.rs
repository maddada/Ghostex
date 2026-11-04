//! CDXC:AgentScreenDetection 2026-10-11 WHY:
//! The footer's "1 shell still running" says how many, never what. Claude's transcript names every background task twice: the launch (a `Bash`/`PowerShell` call whose result carries `toolUseResult.backgroundTaskId`, or a `Monitor` call whose result carries `toolUseResult.taskId`) and its end (a `<task-notification>` with that task id and a terminal status). Launched minus ended is what is still running. The footer's count stays the authority (a restarted Claude kills its shells without writing an end), so the roster is cut to the newest `count` launches.
//!
//! CDXC:AgentScreenDetection 2026-10-11 SEE-ALSO:
//! packages/gx-chat-core/src/extras/background_tasks.rs words the rows; apps/desktop/src/app/native_chat/working_strip.rs and apps/mobile/app/src/chat/native/cards/WorkingStrip.tsx draw them.

use std::collections::HashMap;

use serde_json::Value;

use crate::session_chat_terminal_activity::SessionChatBackgroundTask;

/// Longest command kept for the hover; the title line is cut far shorter by the renderers.
const MAX_COMMAND_CHARS: usize = 2_000;
/// A launch without a description is titled by its command's first line, cut to this.
const MAX_TITLE_CHARS: usize = 160;

#[derive(Clone, Debug)]
struct Launch {
    kind: &'static str,
    title: String,
    command: Option<String>,
    at: i64,
}

/// Background launches read off one parent transcript, kept in the fleet reader's incremental cache.
#[derive(Clone, Debug, Default)]
pub(crate) struct ClaudeBackgroundLaunches {
    /// Shell and monitor calls waiting for their result, by tool-use id. A result removes the
    /// entry whether or not it backgrounded, so this only ever holds calls still in flight.
    pending: HashMap<String, Launch>,
    /// Tasks started and not yet ended, by Claude's task id.
    running: HashMap<String, Launch>,
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\u{2026}", text[..end].trim_end()),
        None => text.to_string(),
    }
}

fn launch_of(block: &Value, at: i64) -> Option<Launch> {
    let kind = match block.get("name").and_then(Value::as_str)? {
        "Bash" | "PowerShell" => "shell",
        "Monitor" => "monitor",
        _ => return None,
    };
    let input = block.get("input")?;
    let command = input
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty());
    let description = input
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|description| !description.is_empty());
    let title = description
        .map(|description| cut(description, MAX_TITLE_CHARS))
        .or_else(|| {
            command.and_then(|command| {
                command
                    .lines()
                    .next()
                    .map(|line| cut(line.trim(), MAX_TITLE_CHARS))
            })
        })?;
    Some(Launch {
        kind,
        title,
        command: command.map(|command| cut(command, MAX_COMMAND_CHARS)),
        at,
    })
}

impl ClaudeBackgroundLaunches {
    /// Reads one parent transcript record.
    pub(crate) fn observe(&mut self, record: &Value, at: i64) {
        // An old transcript's hook line; Ghostex's own process starts arrive through
        // `process_started` (hookless_activity/process_starts.rs).
        if crate::hookless_activity::claude_hook_line_start(record)
            .is_some_and(|source| source.new_process())
        {
            self.process_started();
            return;
        }
        let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) else {
            return;
        };
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    if let (Some(id), Some(launch)) = (
                        block.get("id").and_then(Value::as_str),
                        launch_of(block, at),
                    ) {
                        self.pending.insert(id.to_string(), launch);
                    }
                }
                Some("tool_result") => {
                    let Some(launch) = block
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .and_then(|id| self.pending.remove(id))
                    else {
                        continue;
                    };
                    let task_id = record
                        .pointer("/toolUseResult/backgroundTaskId")
                        .or_else(|| record.pointer("/toolUseResult/taskId"))
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty());
                    if let Some(task_id) = task_id {
                        self.running.insert(task_id.to_string(), launch);
                    }
                }
                _ => {}
            }
        }
    }

    /// A new or resumed Claude process starts with no shells: the old process's died with it.
    pub(crate) fn process_started(&mut self) {
        self.pending.clear();
        self.running.clear();
    }

    /// A `<task-notification>` with a terminal status ended this task.
    pub(crate) fn finish(&mut self, task_id: &str) {
        self.running.remove(task_id);
    }

    /// The newest `count` tasks started at or after `boundary`, oldest first.
    pub(crate) fn running(&self, count: u64, boundary: i64) -> Vec<SessionChatBackgroundTask> {
        let mut tasks: Vec<_> = self
            .running
            .iter()
            .filter(|(_, launch)| launch.at >= boundary)
            .collect();
        tasks.sort_by(|a, b| b.1.at.cmp(&a.1.at).then_with(|| a.0.cmp(b.0)));
        tasks.truncate(usize::try_from(count).unwrap_or(usize::MAX));
        tasks.reverse();
        tasks
            .into_iter()
            .map(|(id, launch)| SessionChatBackgroundTask {
                id: id.clone(),
                kind: launch.kind,
                title: launch.title.clone(),
                command: launch.command.clone(),
                started_at: chrono::DateTime::from_timestamp_millis(launch.at)
                    .unwrap_or_default()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            })
            .collect()
    }
}
