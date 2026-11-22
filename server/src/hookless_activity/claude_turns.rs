//! The turns a Claude transcript records between two reads of its session record: a turn that
//! ended and the next one that started before the record was read again, and prompts the user
//! sent while a turn was running.
//!
//! CDXC:AgentHooks 2026-11-11 WHY:
//! A hooks-vs-observer comparison on Windows found two cases only the hooks caught, on every platform: a queued prompt restarts the turn 37 to 74 ms after it ended (16 ms seen live on macOS), so the session record goes busy, idle, busy between two 500 ms polls and the observer never saw the Stop; and a prompt sent while Claude works never changes the record's status, so no UserPromptSubmit was sent for it. Both are in the transcript. Claude writes a `system` `turn_duration` row when a turn finishes (a `[Request interrupted by user` row when it is interrupted), a `user` row when a turn starts, and a `queue-operation` row for its prompt queue: `enqueue` when a prompt is sent while it works, then `remove` when it is folded into the running turn, `dequeue` when it starts the next turn, or `popAll` when the user takes it back to edit. Claude's own UserPromptSubmit hook fires at the `enqueue` (seen live on 2026-11-11, 3 ms apart) and fires nothing when the queued prompt later starts its turn. Rule: each pane the observer follows reads only the bytes its transcript gained since the last pass. A user prompt enqueued while a turn runs and the record is not idle sends one UserPromptSubmit naming it. A turn end followed by a turn start sends Stop (or Interrupt, StopFailure), then UserPromptSubmit, or only activity (SessionBusy) when the turn's prompt is a queued one already submitted, unless the record itself showed idle during the ended turn (`statusUpdatedAt` after the ended turn started and no later than the new turn) or is idle now. When the record later shows an idle no later than a restart already sent, that idle is the one in the gap and sends nothing; when the record goes busy with a submitted prompt first in the queue, it sends only activity. So every finished turn sends one Stop and every prompt one UserPromptSubmit, whichever of the record and the transcript saw it first.
//! CDXC:AgentHooks 2026-11-12 WHY:
//! The day's comparison on macOS found Claude's UserPromptSubmit hook also fires when a background task's `<task-notification>` is folded into a running turn (at its `remove` row, 3 to 25 ms apart in all six cases; nothing at its `enqueue`). It is not the user's prompt and the session is already working, so the observer still drives nothing for it; it only reports the fold to the comparison (`FoldedNotice`, compared and never driven), so it stops counting as a hook-only prompt.
//! SEE-ALSO: server/src/hookless_activity/claude.rs (`ClaudeObserver::step` and `read_turns`, which read these after the records).

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// At most this much of a transcript is read in one pass; a larger jump skips to its end.
const MAX_READ: u64 = 2 * 1024 * 1024;
/// How far a session record's `statusUpdatedAt` may sit after the transcript row it goes with.
pub(super) const CLOCK_SLACK_MS: i64 = 250;

/// What new transcript rows say happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TurnMark {
    /// A turn ended (`end` is Stop, Interrupt or StopFailure) and the next one started.
    Restart {
        end: &'static str,
        /// When the ended turn started (ms since the epoch).
        ended_turn_started_ms: i64,
        /// The prompt that started the ended turn, when this cursor read it.
        ended_turn_prompt: Option<String>,
        prompt: Option<String>,
        /// When the new turn started.
        prompt_ms: i64,
        /// The new turn's prompt is a queued one whose UserPromptSubmit was already sent.
        prompt_submitted: bool,
    },
    /// A prompt the user sent while a turn ran.
    QueuedPrompt { prompt: String },
    /// A background task's notice folded into the running turn: Claude's UserPromptSubmit hook
    /// fires for it, and it changes nothing (compared only).
    FoldedNotice,
}

/// One entry of Claude's prompt queue.
#[derive(Debug)]
struct Queued {
    content: String,
    /// A prompt the user typed (not a background task's or another session's message) whose
    /// UserPromptSubmit was sent.
    submitted: bool,
}

/// One pane's read position in its conversation's transcript.
pub(super) struct TurnCursor {
    pub(super) conversation: String,
    pub(super) path: Option<PathBuf>,
    offset: u64,
    /// The transcript was not on disk when the pane was first followed: once it appears, all of
    /// it is new.
    read_from_start: bool,
    turn_started_ms: i64,
    turn_prompt: Option<String>,
    /// A turn end with no turn start after it yet: which hook event it is.
    pending_end: Option<&'static str>,
    last_reply_failed: bool,
    queue: VecDeque<Queued>,
    last_dequeued: Option<Queued>,
    /// `statusUpdatedAt` of the newest idle after a turn the session record showed.
    pub(super) record_idle_ms: Option<i64>,
    /// The newest prompt time of a restart this cursor sent.
    pub(super) restart_sent_ms: Option<i64>,
    pub(super) next_lookup_ms: i64,
}

impl TurnCursor {
    pub(super) fn new(conversation: &str, path: Option<PathBuf>, now_ms: i64) -> Self {
        let offset = path
            .as_deref()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len())
            .unwrap_or_default();
        Self {
            conversation: conversation.to_string(),
            read_from_start: path.is_none(),
            path,
            offset,
            turn_started_ms: now_ms,
            turn_prompt: None,
            pending_end: None,
            last_reply_failed: false,
            queue: VecDeque::new(),
            last_dequeued: None,
            record_idle_ms: None,
            restart_sent_ms: None,
            next_lookup_ms: now_ms,
        }
    }

    /// The transcript appeared after the pane was first followed.
    pub(super) fn found(&mut self, path: PathBuf) {
        self.offset = if self.read_from_start {
            0
        } else {
            std::fs::metadata(&path)
                .map(|meta| meta.len())
                .unwrap_or_default()
        };
        self.path = Some(path);
    }

    /// The next turn starts with a queued prompt whose UserPromptSubmit was already sent: one
    /// Claude took out of the queue at the turn end (it writes `dequeue` with `turn_duration`)
    /// whose turn row is not read yet, or else the first one still queued.
    pub(super) fn submitted_prompt_next(&self) -> bool {
        match &self.last_dequeued {
            Some(dequeued) => dequeued.submitted,
            None => self.queue.front().is_some_and(|queued| queued.submitted),
        }
    }

    /// The marks in the complete lines the transcript gained since the last read. `record_idle`:
    /// the session record shows Claude at its prompt now.
    pub(super) fn read_new(&mut self, record_idle: bool) -> Vec<TurnMark> {
        let Some(path) = self.path.clone() else {
            return Vec::new();
        };
        let Some(bytes) = self.new_bytes(&path) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&bytes);
        self.scan(text.lines(), record_idle)
    }

    fn new_bytes(&mut self, path: &Path) -> Option<Vec<u8>> {
        let mut file = std::fs::File::open(path).ok()?;
        let length = file.metadata().ok()?.len();
        if length < self.offset {
            // Rewritten in place: start over at its end.
            self.offset = length;
            return None;
        }
        if length == self.offset {
            return None;
        }
        let mut start = self.offset;
        let skipped = length - start > MAX_READ;
        if skipped {
            start = length - MAX_READ;
        }
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::new();
        file.take(length - start).read_to_end(&mut bytes).ok()?;
        // Only complete lines; a line still being written is read on the next pass.
        let end = bytes.iter().rposition(|b| *b == b'\n')? + 1;
        bytes.truncate(end);
        self.offset = start + end as u64;
        if skipped {
            let first = bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(0, |at| at + 1);
            bytes.drain(..first);
            self.pending_end = None;
            self.queue.clear();
            self.last_dequeued = None;
        }
        Some(bytes)
    }

    pub(super) fn scan<'a>(
        &mut self,
        lines: impl Iterator<Item = &'a str>,
        record_idle: bool,
    ) -> Vec<TurnMark> {
        let mut marks = Vec::new();
        for line in lines {
            let Ok(entry) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if entry["isSidechain"] == true {
                continue;
            }
            let at_ms = entry["timestamp"]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.timestamp_millis())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            match classify(&entry) {
                Row::Reply { failed } => self.last_reply_failed = failed,
                Row::TurnEnd => {
                    if self.pending_end.is_none() {
                        self.pending_end = Some(if self.last_reply_failed {
                            "StopFailure"
                        } else {
                            "Stop"
                        });
                    }
                }
                Row::Interrupt => self.pending_end = Some("Interrupt"),
                Row::Enqueue(content) => {
                    // Sent while a turn runs: Claude's hook submits it now. One enqueued at the
                    // prompt (another session's message) starts its turn at once, as the record
                    // shows.
                    let submitted =
                        is_typed_prompt(&content) && self.pending_end.is_none() && !record_idle;
                    if submitted {
                        marks.push(TurnMark::QueuedPrompt {
                            prompt: content.clone(),
                        });
                    }
                    self.queue.push_back(Queued { content, submitted });
                }
                Row::Unqueue { content, folded } => {
                    if folded && is_task_notice(&content) && self.pending_end.is_none() {
                        marks.push(TurnMark::FoldedNotice);
                    }
                    if let Some(at) = self.queue.iter().position(|q| q.content == content) {
                        self.queue.remove(at);
                    }
                }
                Row::Dequeue => self.last_dequeued = self.queue.pop_front(),
                Row::TurnStart(prompt) => {
                    let dequeued = self.last_dequeued.take();
                    if let Some(end) = self.pending_end.take() {
                        marks.push(TurnMark::Restart {
                            end,
                            ended_turn_started_ms: self.turn_started_ms,
                            ended_turn_prompt: self.turn_prompt.take(),
                            prompt: Some(prompt.clone()),
                            prompt_ms: at_ms,
                            prompt_submitted: dequeued.is_some_and(|q| q.submitted),
                        });
                    }
                    self.turn_started_ms = at_ms;
                    self.turn_prompt = Some(prompt);
                    self.last_reply_failed = false;
                }
                Row::Other => {}
            }
        }
        marks
    }
}

/// A background task's notice (a shell, monitor or agent run in the background finished).
fn is_task_notice(content: &str) -> bool {
    content.starts_with("<task-notification")
}

/// A queued prompt the user typed, not a background task's notice or another session's message.
fn is_typed_prompt(content: &str) -> bool {
    !content.is_empty()
        && !["<task-notification", "<cross-session-message"]
            .iter()
            .any(|prefix| content.starts_with(prefix))
}

enum Row {
    Reply {
        failed: bool,
    },
    TurnEnd,
    Interrupt,
    /// A prompt that starts a turn.
    TurnStart(String),
    Enqueue(String),
    /// Out of the queue without starting a turn: folded into the running one (`remove`) or taken
    /// back to edit (`popAll`).
    Unqueue {
        content: String,
        folded: bool,
    },
    Dequeue,
    Other,
}

fn classify(entry: &Value) -> Row {
    match entry["type"].as_str() {
        Some("system") if entry["subtype"] == "turn_duration" => Row::TurnEnd,
        Some("assistant") => Row::Reply {
            failed: entry["isApiErrorMessage"] == true,
        },
        Some("queue-operation") => {
            let content = entry["content"].as_str().map(str::trim).map(str::to_string);
            match (entry["operation"].as_str(), content) {
                (Some("enqueue"), Some(content)) => Row::Enqueue(content),
                (Some(operation @ ("remove" | "popAll")), Some(content)) => Row::Unqueue {
                    content,
                    folded: operation == "remove",
                },
                (Some("dequeue"), _) => Row::Dequeue,
                _ => Row::Other,
            }
        }
        Some("user") => {
            if entry["isMeta"] == true || entry["isCompactSummary"] == true {
                return Row::Other;
            }
            let text = match &entry["message"]["content"] {
                Value::String(text) => text.clone(),
                Value::Array(parts) => {
                    if parts.iter().any(|part| part["type"] == "tool_result") {
                        return Row::Other;
                    }
                    parts
                        .iter()
                        .filter(|part| part["type"] == "text")
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                _ => return Row::Other,
            };
            let text = text.trim();
            if text.starts_with("[Request interrupted by user") {
                return Row::Interrupt;
            }
            // Local commands and shell-mode lines run no turn.
            if text.is_empty()
                || [
                    "<command-name>",
                    "<local-command",
                    "<bash-input>",
                    "<bash-std",
                ]
                .iter()
                .any(|prefix| text.starts_with(prefix))
            {
                return Row::Other;
            }
            Row::TurnStart(text.to_string())
        }
        _ => Row::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn lines(rows: &[Value]) -> Vec<String> {
        rows.iter().map(Value::to_string).collect()
    }

    fn scan(cursor: &mut TurnCursor, rows: &[Value]) -> Vec<TurnMark> {
        let lines = lines(rows);
        cursor.scan(lines.iter().map(String::as_str), false)
    }

    #[test]
    fn a_turn_end_then_a_new_prompt_is_a_restart() {
        let mut cursor = TurnCursor::new("c", None, 1_000);
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(2_000), "message": {"content": "first"}}),
                json!({"type": "assistant", "timestamp": at(3_000), "message": {"content": []}}),
                json!({"type": "user", "timestamp": at(3_100), "message": {"content": [{"type": "tool_result", "tool_use_id": "a"}]}}),
                json!({"type": "system", "subtype": "turn_duration", "timestamp": at(4_000)}),
                json!({"type": "queue-operation", "operation": "dequeue", "timestamp": at(4_030)}),
                json!({"type": "user", "timestamp": at(4_050), "message": {"content": "queued"}}),
            ],
        );
        assert_eq!(
            marks,
            vec![TurnMark::Restart {
                end: "Stop",
                ended_turn_started_ms: 2_000,
                ended_turn_prompt: Some("first".to_string()),
                prompt: Some("queued".to_string()),
                prompt_ms: 4_050,
                prompt_submitted: false,
            }]
        );
        // A turn end with no new prompt yet waits for one.
        let marks = scan(
            &mut cursor,
            &[json!({"type": "system", "subtype": "turn_duration", "timestamp": at(6_000)})],
        );
        assert!(marks.is_empty());
        // Local commands start no turn; the next real prompt does.
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(7_000), "message": {"content": "<command-name>/model</command-name>"}}),
                json!({"type": "user", "isMeta": true, "timestamp": at(7_001), "message": {"content": "Caveat"}}),
                json!({"type": "user", "timestamp": at(8_000), "message": {"content": [{"type": "text", "text": "next"}]}}),
            ],
        );
        assert!(matches!(
            marks.as_slice(),
            [TurnMark::Restart {
                end: "Stop",
                ended_turn_started_ms: 4_050,
                prompt_ms: 8_000,
                ..
            }]
        ));
    }

    #[test]
    fn interrupts_failures_and_side_chains() {
        let mut cursor = TurnCursor::new("c", None, 1_000);
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(2_000), "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}}),
                json!({"type": "user", "timestamp": at(2_100), "message": {"content": "go on"}}),
                json!({"type": "assistant", "isApiErrorMessage": true, "timestamp": at(3_000), "message": {"content": []}}),
                json!({"type": "system", "subtype": "turn_duration", "timestamp": at(3_001)}),
                // A subagent's rows are not the main turn.
                json!({"type": "user", "isSidechain": true, "timestamp": at(3_002), "message": {"content": "sub prompt"}}),
                json!({"type": "user", "timestamp": at(3_050), "message": {"content": "retry"}}),
            ],
        );
        let ends: Vec<_> = marks
            .iter()
            .map(|mark| match mark {
                TurnMark::Restart { end, prompt, .. } => (*end, prompt.clone()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            ends,
            vec![
                ("Interrupt", Some("go on".to_string())),
                ("StopFailure", Some("retry".to_string())),
            ]
        );
    }

    fn enqueue(content: &str) -> Value {
        json!({"type": "queue-operation", "operation": "enqueue", "content": content})
    }

    fn queue_op(operation: &str, content: Option<&str>) -> Value {
        json!({"type": "queue-operation", "operation": operation, "content": content})
    }

    #[test]
    fn a_prompt_sent_while_claude_works_is_submitted_when_sent() {
        let mut cursor = TurnCursor::new("c", None, 1_000);
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(2_000), "message": {"content": "work"}}),
                enqueue("also this"),
                // Background task notices and other sessions' messages are not the user's.
                enqueue("<task-notification>\n<task-id>x</task-id>"),
                queue_op("remove", Some("also this")),
                json!({"type": "attachment", "timestamp": at(2_500), "attachment": {"type": "queued_command", "commandMode": "prompt", "prompt": "also this"}}),
            ],
        );
        assert_eq!(
            marks,
            vec![TurnMark::QueuedPrompt {
                prompt: "also this".to_string()
            }]
        );
        // Folded in, it is out of the queue: the next turn is not its.
        assert!(!cursor.submitted_prompt_next());
        // A message enqueued while Claude sits at its prompt starts its turn at once.
        let lines = lines(&[
            json!({"type": "system", "subtype": "turn_duration", "timestamp": at(3_000)}),
            enqueue("from another session"),
        ]);
        assert!(cursor
            .scan(lines.iter().map(String::as_str), true)
            .is_empty());
    }

    #[test]
    fn a_task_notice_folded_into_the_running_turn_is_reported_for_the_comparison() {
        let mut cursor = TurnCursor::new("c", None, 1_000);
        let notice = "<task-notification>\n<task-id>x</task-id>";
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(2_000), "message": {"content": "work"}}),
                enqueue(notice),
                queue_op("remove", Some(notice)),
                // Only task notices were seen firing the hook when folded; a prompt taken back to
                // edit is not folded.
                enqueue("<cross-session-message from=\"x\">hi</cross-session-message>"),
                queue_op(
                    "remove",
                    Some("<cross-session-message from=\"x\">hi</cross-session-message>"),
                ),
                enqueue("typed"),
                queue_op("popAll", Some("typed")),
            ],
        );
        assert_eq!(
            marks,
            vec![
                TurnMark::FoldedNotice,
                TurnMark::QueuedPrompt {
                    prompt: "typed".to_string()
                },
            ]
        );
        assert!(cursor.queue.is_empty());
        // After the turn ended, a notice starts the next turn as the record shows: no fold.
        let lines = lines(&[
            json!({"type": "system", "subtype": "turn_duration", "timestamp": at(3_000)}),
            enqueue(notice),
            queue_op("remove", Some(notice)),
        ]);
        assert!(cursor
            .scan(lines.iter().map(String::as_str), true)
            .is_empty());
    }

    #[test]
    fn a_queued_prompt_that_starts_the_next_turn_was_already_submitted() {
        let mut cursor = TurnCursor::new("c", None, 1_000);
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "user", "timestamp": at(2_000), "message": {"content": "story"}}),
                enqueue("now QUEUED"),
            ],
        );
        assert_eq!(marks.len(), 1);
        assert!(cursor.submitted_prompt_next());
        let marks = scan(
            &mut cursor,
            &[
                json!({"type": "assistant", "timestamp": at(3_000), "message": {"content": []}}),
                queue_op("dequeue", None),
                json!({"type": "system", "subtype": "turn_duration", "timestamp": at(3_010)}),
                json!({"type": "user", "timestamp": at(3_026), "message": {"content": "now QUEUED"}}),
            ],
        );
        assert!(matches!(
            marks.as_slice(),
            [TurnMark::Restart {
                end: "Stop",
                prompt_submitted: true,
                prompt_ms: 3_026,
                ..
            }]
        ));
        assert!(!cursor.submitted_prompt_next());
        // Taken back to edit, a queued prompt is out of the queue.
        scan(
            &mut cursor,
            &[enqueue("edit me"), queue_op("popAll", Some("edit me"))],
        );
        assert!(!cursor.submitted_prompt_next());
    }

    #[test]
    fn only_complete_new_lines_are_read() {
        let dir = std::env::temp_dir().join(format!("gx-claude-turns-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.jsonl");
        let old = json!({"type": "system", "subtype": "turn_duration", "timestamp": at(1_000)});
        std::fs::write(&path, format!("{old}\n")).unwrap();
        // Followed from here on: the old turn end is not news.
        let mut cursor = TurnCursor::new("c", Some(path.clone()), 1_500);
        let prompt = json!({"type": "user", "timestamp": at(2_000), "message": {"content": "hi"}});
        let end = json!({"type": "system", "subtype": "turn_duration", "timestamp": at(3_000)});
        let next = json!({"type": "user", "timestamp": at(3_040), "message": {"content": "again"}});
        let next_line = next.to_string();
        let (head, tail) = next_line.split_at(20);
        let append = |text: &str| {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            file.write_all(text.as_bytes()).unwrap();
        };
        append(&format!("{prompt}\n{end}\n{head}"));
        assert!(cursor.read_new(false).is_empty());
        append(&format!("{tail}\n"));
        assert!(matches!(
            cursor.read_new(false).as_slice(),
            [TurnMark::Restart {
                ended_turn_started_ms: 2_000,
                prompt_ms: 3_040,
                ..
            }]
        ));
        assert!(cursor.read_new(false).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
