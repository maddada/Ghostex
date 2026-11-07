//! The output clock of a thread on another computer: the same 8-minute rounds as a local thread's
//! watch (stall.rs), judged from what that computer answers over the connection instead of from
//! files here. Its chat (`readSessionChat`: the newest rows and whether the transcript's newest
//! turn is still open) and, once it has been quiet a minute, its screen (`readSessionTerminalTail`)
//! are read at most every 30 seconds while it works.
//!
//! CDXC:RemoteMachines 2026-11-07 FIX:
//! Issue #475 (a Mac coordinator with threads on two headless Linux computers): three threads finished their turns and stayed "Working" for one to three hours with no report, and "Not moving" never fired for them. The supervisor for threads on another computer (server/coordinator_remote_runtime.rs) trusted that computer's hook state alone, which without hooks stayed working, and the stall watch only covered threads on this computer. Now a remote thread that has shown no output for a round is judged by its transcript: when the transcript's newest turn ended, it is reported finished with its last reply (and stays finished until its rows move again, whatever its hook state says); otherwise Ghostex nudges it and tells the coordinator, as for a local thread, and again every round until it moves. There is no red dot or notification for it on this computer: its row lives in the other computer's sidebar.
//! SEE-ALSO: server/src/coordinators/stall.rs (the local watch and the CDXC:Coordinators decision on the rounds), server/src/server/coordinator_remote_runtime.rs (where it runs).

use std::hash::{Hash, Hasher};

use serde_json::Value;

use super::stall::{
    describe_row, is_ghostex_nudge, quiet_round_ms, screen_fingerprint, test_scaled,
    NUDGE_GRACE_MS, SCREEN_SAMPLE_AFTER_MS, STALL_CHECK_EVERY_MS,
};
use crate::session_chat::SessionChatMessage;

/// Rows a check asks the other computer for.
pub const REMOTE_STALL_CHAT_ROWS: u64 = 12;

/// What one read of the other computer's chat says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteChatReading {
    /// The newest rows the agent wrote (Ghostex's nudge left out), hashed; `None` when the chat
    /// could not be read.
    pub rows: Option<u64>,
    /// The transcript's newest turn has ended (completed or interrupted).
    pub turn_ended: bool,
}

/// A round of quiet a remote watch says is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteStallDue {
    pub quiet_since: i64,
    pub round: u32,
    /// The transcript says its turn ended: a finished turn its hooks never reported.
    pub turn_ended: bool,
    /// The rows the round was judged on, so a report is dropped when they moved since.
    pub rows: Option<u64>,
}

pub struct RemoteStallWatch {
    watched_since: i64,
    rows: Option<u64>,
    turn_ended: bool,
    screen: Option<u64>,
    last_output_at: Option<i64>,
    checked_at: Option<i64>,
    reported: Option<(i64, u32)>,
    nudged_at: Option<i64>,
    /// The rows a finished report went out on: while they stand, the thread is finished whatever
    /// its hooks say.
    finished_on: Option<u64>,
}

impl RemoteStallWatch {
    pub fn new(now: i64) -> Self {
        Self {
            watched_since: now,
            rows: None,
            turn_ended: false,
            screen: None,
            last_output_at: None,
            checked_at: None,
            reported: None,
            nudged_at: None,
            finished_on: None,
        }
    }

    /// Whether the other computer is due to be read again.
    pub fn check_due(&self, now: i64) -> bool {
        self.checked_at
            .is_none_or(|checked| now - checked >= test_scaled(STALL_CHECK_EVERY_MS))
    }

    /// Whether this check should read the screen too: only once the rows stopped for a minute.
    pub fn wants_screen(&self, now: i64) -> bool {
        now - self.quiet_since() >= test_scaled(SCREEN_SAMPLE_AFTER_MS)
    }

    /// Takes one check's reading. A chat that could not be read changes nothing; the screen is
    /// `None` when it was not (or could not be) read.
    pub fn record(&mut self, now: i64, chat: RemoteChatReading, screen: Option<u64>) {
        self.checked_at = Some(now);
        let in_grace = self
            .nudged_at
            .is_some_and(|nudged| now - nudged < test_scaled(NUDGE_GRACE_MS));
        if let Some(rows) = chat.rows {
            if self.rows.is_some_and(|previous| previous != rows) {
                self.count_output(now);
                self.screen = None;
                if self.finished_on.is_some_and(|finished| finished != rows) {
                    self.finished_on = None;
                }
            }
            self.rows = Some(rows);
            self.turn_ended = chat.turn_ended;
        }
        if let Some(screen) = screen {
            if !in_grace && self.screen.is_some_and(|previous| previous != screen) {
                self.count_output(now);
            }
            self.screen = Some(screen);
        }
    }

    fn count_output(&mut self, at: i64) {
        if Some(at) > self.last_output_at {
            self.last_output_at = Some(at);
            self.nudged_at = None;
        }
    }

    pub fn quiet_since(&self) -> i64 {
        self.watched_since.max(self.last_output_at.unwrap_or(0))
    }

    fn round(&self, now: i64) -> u32 {
        ((now - self.quiet_since()).max(0) / quiet_round_ms()) as u32
    }

    /// The round due now, if one is: none while a finished report still stands, and none before
    /// the chat was read once.
    pub fn due(&self, now: i64) -> Option<RemoteStallDue> {
        if self.finished() || self.rows.is_none() {
            return None;
        }
        let quiet_since = self.quiet_since();
        let round = self.round(now);
        let reported = match self.reported {
            Some((since, rounds)) if since == quiet_since => rounds,
            _ => 0,
        };
        (round > reported).then_some(RemoteStallDue {
            quiet_since,
            round,
            turn_ended: self.turn_ended,
            rows: self.rows,
        })
    }

    /// A finished report went out on these rows: the thread counts as finished until they move.
    pub fn mark_finished(&mut self, rows: Option<u64>) {
        self.finished_on = rows.or(self.rows);
    }

    /// The thread's turn ended and was reported, and nothing it wrote moved since.
    pub fn finished(&self) -> bool {
        self.finished_on.is_some() && self.finished_on == self.rows
    }

    pub fn mark_reported(&mut self, quiet_since: i64, round: u32) {
        self.reported = Some((quiet_since, round));
    }

    pub fn note_nudged(&mut self, now: i64) {
        self.nudged_at = Some(now);
    }
}

/// The rows of a `readSessionChat` answer that the agent wrote or was sent, newest last, without
/// queued rows and Ghostex's own nudges. Rows another computer's gxserver shapes differently (an
/// older or newer version) are skipped.
fn chat_rows(chat: &Value) -> Vec<SessionChatMessage> {
    chat.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| serde_json::from_value::<SessionChatMessage>(row.clone()).ok())
        .filter(|message| !message.queued && !is_ghostex_nudge(message))
        .collect()
}

/// What a `readSessionChat` answer says for the watch; `None` (nothing read) for an answer that is
/// not a chat.
pub fn remote_chat_reading(chat: Option<&Value>) -> RemoteChatReading {
    let Some(chat) = chat.filter(|chat| chat.get("messages").is_some_and(Value::is_array)) else {
        return RemoteChatReading {
            rows: None,
            turn_ended: false,
        };
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for message in chat_rows(chat) {
        message.id.hash(&mut hasher);
        message.timestamp.hash(&mut hasher);
        serde_json::to_string(&message.blocks)
            .unwrap_or_default()
            .hash(&mut hasher);
    }
    let turn_ended = matches!(
        chat.pointer("/lifecycle/state").and_then(Value::as_str),
        Some("completed" | "interrupted")
    );
    RemoteChatReading {
        rows: Some(hasher.finish()),
        turn_ended,
    }
}

/// The last row worth naming in a stall report, from a `readSessionChat` answer.
pub fn remote_chat_last_row(chat: &Value) -> Option<String> {
    chat_rows(chat).iter().rev().find_map(describe_row)
}

/// A `readSessionTerminalTail` answer's lines, fingerprinted without what ticks on its own.
pub fn remote_screen_fingerprint(tail: &Value) -> Option<u64> {
    if tail.get("captured").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let lines = tail.get("lines")?.as_array()?;
    let text = lines
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    Some(screen_fingerprint(&text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MIN: i64 = 60_000;

    fn row(id: &str, role: &str, ts: i64, blocks: Value) -> Value {
        json!({"id": id, "role": role, "blocks": blocks, "timestamp": ts, "source": "transcript"})
    }

    fn chat(rows: Vec<Value>, state: &str) -> Value {
        json!({"messages": rows, "lifecycle": {"state": state, "turnId": "t", "timestamp": 1}})
    }

    fn reading(rows: u64, turn_ended: bool) -> RemoteChatReading {
        RemoteChatReading {
            rows: Some(rows),
            turn_ended,
        }
    }

    #[test]
    fn the_chat_reading_hashes_the_agents_rows_and_reads_the_turn() {
        let working = chat(
            vec![row(
                "a",
                "assistant",
                10,
                json!([{"type": "tool-call", "name": "Bash", "input": {"command": "cargo test"}}]),
            )],
            "working",
        );
        let first = remote_chat_reading(Some(&working));
        assert!(first.rows.is_some());
        assert!(!first.turn_ended);
        // Ghostex's nudge is not the agent moving.
        let mut nudged = working.clone();
        nudged["messages"].as_array_mut().unwrap().push(row(
            "n",
            "user",
            20,
            json!([{"type": "text", "text": "Ghostex: no new output from you for 8 minutes."}]),
        ));
        assert_eq!(remote_chat_reading(Some(&nudged)).rows, first.rows);
        let mut replied = nudged.clone();
        replied["messages"].as_array_mut().unwrap().push(row(
            "r",
            "assistant",
            30,
            json!([{"type": "text", "text": "Done: all green."}]),
        ));
        replied["lifecycle"]["state"] = json!("completed");
        let after = remote_chat_reading(Some(&replied));
        assert_ne!(after.rows, first.rows);
        assert!(after.turn_ended);
        assert_eq!(
            remote_chat_last_row(&replied).as_deref(),
            Some("It wrote: Done: all green.")
        );
        assert_eq!(
            remote_chat_last_row(&working).as_deref(),
            Some("It started Bash: cargo test")
        );
        // An answer that is not a chat reads nothing.
        assert_eq!(remote_chat_reading(Some(&json!({"error": "x"}))).rows, None);
        assert_eq!(remote_chat_reading(None).rows, None);
    }

    #[test]
    fn a_quiet_remote_thread_is_due_each_round_and_output_resets_the_clock() {
        let mut watch = RemoteStallWatch::new(0);
        assert!(watch.check_due(0));
        watch.record(0, reading(1, false), None);
        assert!(!watch.check_due(10_000));
        assert!(!watch.wants_screen(30_000));
        assert!(watch.wants_screen(MIN));
        watch.record(MIN, reading(1, false), Some(7));
        // Spinner noise is already out of the fingerprint; an unchanged screen is quiet.
        watch.record(2 * MIN, reading(1, false), Some(7));
        assert_eq!(watch.due(7 * MIN), None);
        let due = watch.due(8 * MIN).expect("first round");
        assert_eq!((due.quiet_since, due.round, due.turn_ended), (0, 1, false));
        watch.mark_reported(due.quiet_since, due.round);
        assert_eq!(watch.due(9 * MIN), None);
        assert_eq!(watch.due(16 * MIN).map(|due| due.round), Some(2));
        // A changed screen is output: a long build printing lines.
        watch.record(16 * MIN, reading(1, false), Some(8));
        assert_eq!(watch.quiet_since(), 16 * MIN);
        assert_eq!(watch.due(17 * MIN), None);
        // New rows are output too.
        watch.record(18 * MIN, reading(2, false), None);
        assert_eq!(watch.quiet_since(), 18 * MIN);
    }

    #[test]
    fn the_nudge_and_its_typing_are_not_the_thread_moving() {
        let mut watch = RemoteStallWatch::new(0);
        watch.record(0, reading(1, false), Some(1));
        watch.note_nudged(8 * MIN);
        // Typing the nudge changed the screen inside its grace.
        watch.record(8 * MIN + 30_000, reading(1, false), Some(2));
        assert_eq!(watch.quiet_since(), 0);
        // A screen change after the grace counts.
        watch.record(10 * MIN, reading(1, false), Some(3));
        assert_eq!(watch.quiet_since(), 10 * MIN);
    }

    #[test]
    fn a_turn_its_transcript_ended_is_finished_until_its_rows_move() {
        let mut watch = RemoteStallWatch::new(0);
        watch.record(0, reading(5, true), None);
        let due = watch.due(8 * MIN).expect("round");
        assert!(due.turn_ended);
        watch.mark_finished(due.rows);
        assert!(watch.finished());
        // Hours later its hooks still say working: nothing more is due.
        watch.record(3 * 60 * MIN, reading(5, true), Some(1));
        assert!(watch.finished());
        assert_eq!(watch.due(3 * 60 * MIN), None);
        // A new turn moves its rows: it works again and its clock restarts.
        watch.record(3 * 60 * MIN + 1, reading(6, false), None);
        assert!(!watch.finished());
        assert_eq!(watch.quiet_since(), 3 * 60 * MIN + 1);
    }

    #[test]
    fn the_screen_fingerprint_reads_a_captured_tail_only() {
        let tail = json!({"captured": true, "lines": ["✻ Working… (39s · ↓ 1.2k tokens)", "> "]});
        let ticked = json!({"captured": true, "lines": ["✶ Working… (41s · ↓ 1.3k tokens)", "> "]});
        assert_eq!(
            remote_screen_fingerprint(&tail),
            remote_screen_fingerprint(&ticked)
        );
        assert_eq!(
            remote_screen_fingerprint(&json!({"captured": false, "lines": []})),
            None
        );
    }
}
