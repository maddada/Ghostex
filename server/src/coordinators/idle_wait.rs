//! A coordinator's thread that ended its turn and sits idle although something waits for it: task
//! notifications its agent queued and never took, or a last message saying it waits on a
//! background job that no longer runs. The supervisor (server/coordinator_runtime.rs) keeps one
//! `IdleWatch` per idle thread: after 10 minutes without a new turn it nudges the thread and
//! reports to its coordinator, again every 10 minutes until a new turn starts, and 10 minutes after
//! the nudge the thread counts as stuck (the red dot, a notification and the not-moving sound,
//! through the same quiet map as stall.rs).
//!
//! CDXC:Coordinators 2026-11-06 DECISION:
//! A thread (G7h13, 2026-11-05) ended its turn with "The Linux check and both watchers are still running in the background; I'll pick up the result when it lands." Its three jobs were stopped ten minutes later (Auto Sleep slept the session, see CDXC:SessionStatus 2026-11-06 in session_chat_terminal_activity/types.rs), their notifications sat queued in its transcript, and it stayed idle for three hours while the stall watch, which covers only working threads, said nothing. User, through the coordinator: an idle thread that has queued, undelivered task notifications or harness rows, or ended its last turn saying it waits on a background job, and has had no new turn for 10 minutes, gets the stall treatment: Ghostex nudges it ("Your background job looks finished: read its result and carry on.") and reports to the coordinator, again every 10 minutes until it moves, with the red dot after the nudge. A thread whose screen still shows its background work running is left alone for (b): its job has not finished, and Claude Code hands it the result itself when it does.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use serde_json::Value;

use super::records::ThreadRecord;
use super::stall::{test_scaled, SessionQuiet};

/// One round of idleness: nudged and reported after one, reported again after each further one.
pub const IDLE_ROUND_MS: i64 = 10 * 60_000;
/// Rounds before an idle thread counts as stuck (10 minutes after its nudge).
pub const IDLE_STUCK_ROUNDS: u32 = 2;
/// How often one idle thread's transcript tail is read.
const IDLE_CHECK_EVERY_MS: i64 = 60_000;
/// How much of a Claude transcript's end is read for queued notifications.
const RAW_TAIL_BYTES: u64 = 256 * 1024;
/// How much of the end of a last message is read for "I wait on a background job".
const LAST_WORDS_CHARS: usize = 600;

/// One round as it runs: `IDLE_ROUND_MS`, shortened only on a debug test server.
pub fn idle_round_ms() -> i64 {
    test_scaled(IDLE_ROUND_MS)
}

/// The message Ghostex sends an idle thread at its first round.
pub const IDLE_NUDGE_MESSAGE: &str =
    "Ghostex: your background job looks finished: read its result and carry on.";

/// Why an idle thread is watched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdleWaitReason {
    /// Task notifications (or other harness rows) its agent queued and never took.
    Queued(usize),
    /// Its last message says it waits on a background job, and none shows as running.
    SaidWaiting,
}

/// A report a watch says is due: when the idle stretch began (its turn's end), which round this
/// is, and why.
#[derive(Clone, Debug)]
pub struct IdleDue {
    pub since: i64,
    pub round: u32,
    pub reason: IdleWaitReason,
}

/// One idle thread's clock, from the end of the turn its last report came from.
pub struct IdleWatch {
    since: i64,
    path: Option<PathBuf>,
    checked_at: Option<i64>,
    reason: Option<IdleWaitReason>,
    reported_rounds: u32,
}

impl IdleWatch {
    pub fn new(since: i64) -> Self {
        Self {
            since,
            path: None,
            checked_at: None,
            reason: None,
            reported_rounds: 0,
        }
    }

    pub fn since(&self) -> i64 {
        self.since
    }

    /// BLOCKING (a transcript tail read), at most every `IDLE_CHECK_EVERY_MS`. Returns the report
    /// due now, if any.
    pub fn observe(&mut self, session: &Value, thread: &ThreadRecord, now: i64) -> Option<IdleDue> {
        if self
            .checked_at
            .is_none_or(|checked| now - checked >= test_scaled(IDLE_CHECK_EVERY_MS))
        {
            self.checked_at = Some(now);
            self.reason = idle_wait_reason(session, thread, &mut self.path);
        }
        let reason = self.reason.clone()?;
        let round = self.round(now);
        (round > self.reported_rounds).then(|| IdleDue {
            since: self.since,
            round,
            reason,
        })
    }

    fn round(&self, now: i64) -> u32 {
        ((now - self.since).max(0) / idle_round_ms()) as u32
    }

    pub fn mark_reported(&mut self, round: u32) {
        self.reported_rounds = self.reported_rounds.max(round);
    }

    /// What the sidebar, the board and `coordinator status` show, while something waits for it.
    pub fn view(&self, now: i64) -> Option<SessionQuiet> {
        self.reason.as_ref()?;
        let round = self.round(now);
        Some(SessionQuiet {
            last_output_at: Some(self.since),
            quiet_since: self.since,
            stalled: round >= 1,
            stuck: round >= IDLE_STUCK_ROUNDS,
        })
    }
}

/// BLOCKING. Why an idle thread should be taking a new turn, if anything says so.
pub fn idle_wait_reason(
    session: &Value,
    thread: &ThreadRecord,
    path: &mut Option<PathBuf>,
) -> Option<IdleWaitReason> {
    let queued = queued_harness_rows(session, path);
    if queued > 0 {
        return Some(IdleWaitReason::Queued(queued));
    }
    let background_runs =
        crate::presentation::session_background_work_detected_at(session).is_some();
    (!background_runs && thread.last_report.as_deref().is_some_and(says_it_waits))
        .then_some(IdleWaitReason::SaidWaiting)
}

/// Whether the end of a last message says the thread waits on a background job.
pub fn says_it_waits(message: &str) -> bool {
    const PHRASES: [&str; 16] = [
        "in the background",
        "background job",
        "background task",
        "background shell",
        "background build",
        "background run",
        "still running",
        "still runs",
        "watcher",
        "when it lands",
        "when it finishes",
        "once it finishes",
        "when it completes",
        "once it completes",
        "pick up the result",
        "check back",
    ];
    let chars: Vec<char> = message.chars().collect();
    let tail: String = chars[chars.len().saturating_sub(LAST_WORDS_CHARS)..]
        .iter()
        .collect::<String>()
        .to_lowercase();
    PHRASES.iter().any(|phrase| tail.contains(phrase))
}

/// BLOCKING. Task notifications (and other harness rows) a Claude thread queued after its last
/// turn and never took: `queue-operation` `enqueue` rows that no user or assistant row follows.
/// Other agents keep no such queue in their transcript, so they read as none.
fn queued_harness_rows(session: &Value, path: &mut Option<PathBuf>) -> usize {
    let Some(agent) = super::delivery::resolve_thread_transcript(session, path) else {
        return 0;
    };
    if agent != crate::session_chat::SessionChatTranscriptAgent::Claude {
        return 0;
    }
    let Some(path) = path.as_ref() else {
        return 0;
    };
    let Ok(mut file) = std::fs::File::open(path) else {
        return 0;
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(RAW_TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return 0;
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return 0;
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    // A read that starts mid-file starts mid-line.
    if start > 0 {
        lines.next();
    }
    let mut pending = 0;
    for line in lines {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match row.get("type").and_then(Value::as_str) {
            Some("user" | "assistant") => pending = 0,
            Some("queue-operation")
                if row.get("operation").and_then(Value::as_str) == Some("enqueue")
                    && row
                        .get("content")
                        .and_then(Value::as_str)
                        .is_some_and(|content| content.contains("<task-notification>")) =>
            {
                pending += 1;
            }
            _ => {}
        }
    }
    pending
}
