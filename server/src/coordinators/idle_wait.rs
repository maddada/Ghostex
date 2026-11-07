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
//!
//! CDXC:Coordinators 2026-11-06 DECISION:
//! User, of a board that showed three threads as "parked" while one was put to sleep mid-command and another had stopped after "Now opening the fold.": "why is board not up to date?? what can we do to avoid this please?". Two more things leave a thread owing work after its turn: (c) its last message, which carries no handover marker, ends saying what it does next ("Now opening the fold."), so the agent stopped mid-task; (d) its transcript's newest turn never ended (the agent was put to sleep or died mid-turn, or a restart cut it off). Both get the same rounds as (a) and (b), with their own nudge, and while one holds, Auto Sleep leaves the thread awake and the board shows it under In progress with what it owes instead of as parked (board.rs, panel.rs).

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

/// The message Ghostex sends an idle thread at its first round, when a background job waits.
pub const IDLE_NUDGE_MESSAGE: &str =
    "Ghostex: your background job looks finished: read its result and carry on.";

/// The message Ghostex sends a thread that stopped mid-task or whose turn was cut off.
pub const IDLE_CARRY_ON_MESSAGE: &str =
    "Ghostex: your last turn stopped before your task was done: carry on where you left off, or say you're finished.";

/// Why an idle thread is watched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleWaitReason {
    /// Task notifications (or other harness rows) its agent queued and never took.
    Queued(usize),
    /// Its last message says it waits on a background job, and none shows as running.
    SaidWaiting,
    /// Its last message ends saying what it does next: the agent stopped mid-task.
    SaidCarryingOn,
    /// Its transcript's newest turn never ended: the agent was put to sleep, died or was
    /// restarted mid-turn.
    TurnCutOff,
    /// Its last message says it waits on a background job that still runs: nothing is wrong, it
    /// gets the result when the job ends. Shown on the board, never reported or nudged.
    JobRunning,
}

impl IdleWaitReason {
    /// The nudge Ghostex sends at the first round.
    pub fn nudge_message(self) -> &'static str {
        match self {
            Self::Queued(_) | Self::SaidWaiting | Self::JobRunning => IDLE_NUDGE_MESSAGE,
            Self::SaidCarryingOn | Self::TurnCutOff => IDLE_CARRY_ON_MESSAGE,
        }
    }

    /// The word the board and the Workers rows carry for it (`unfinished`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued(_) => "queued",
            Self::SaidWaiting => "waitsOnJob",
            Self::SaidCarryingOn => "carriesOn",
            Self::TurnCutOff => "cutOff",
            Self::JobRunning => "jobRunning",
        }
    }
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
    /// due now, if any. `turn_open` says the transcript's newest turn never ended.
    pub fn observe(
        &mut self,
        session: &Value,
        thread: &ThreadRecord,
        turn_open: bool,
        now: i64,
    ) -> Option<IdleDue> {
        if self
            .checked_at
            .is_none_or(|checked| now - checked >= test_scaled(IDLE_CHECK_EVERY_MS))
        {
            self.checked_at = Some(now);
            self.reason = idle_wait_reason(session, thread, turn_open, &mut self.path);
        }
        // A job that still runs hands the thread its result itself.
        let reason = self
            .reason
            .filter(|reason| *reason != IdleWaitReason::JobRunning)?;
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

    /// Why it is watched, as last checked, while something waits for it.
    pub fn reason(&self) -> Option<IdleWaitReason> {
        self.reason
    }

    pub fn mark_reported(&mut self, round: u32) {
        self.reported_rounds = self.reported_rounds.max(round);
    }

    /// What the sidebar, the board and `coordinator status` show, while something waits for it.
    pub fn view(&self, now: i64) -> Option<SessionQuiet> {
        self.reason
            .filter(|reason| *reason != IdleWaitReason::JobRunning)?;
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
/// `turn_open` says the transcript's newest turn never ended.
pub fn idle_wait_reason(
    session: &Value,
    thread: &ThreadRecord,
    turn_open: bool,
    path: &mut Option<PathBuf>,
) -> Option<IdleWaitReason> {
    session_idle_reason(session, thread.last_report.as_deref(), turn_open, path)
}

/// BLOCKING. [`idle_wait_reason`] for any session, judged by its last words (a thread's are its
/// last report); Autopilot reads every covered session this way (server/autopilot_runtime.rs).
pub fn session_idle_reason(
    session: &Value,
    last: Option<&str>,
    turn_open: bool,
    path: &mut Option<PathBuf>,
) -> Option<IdleWaitReason> {
    let queued = queued_harness_rows(session, path);
    if queued > 0 {
        return Some(IdleWaitReason::Queued(queued));
    }
    let background_runs =
        crate::presentation::session_background_work_detected_at(session).is_some();
    // Its job still runs: Claude Code hands it the result when it ends.
    if background_runs {
        return last
            .is_some_and(says_it_waits)
            .then_some(IdleWaitReason::JobRunning);
    }
    if turn_open {
        return Some(IdleWaitReason::TurnCutOff);
    }
    let last = last?;
    if says_it_waits(last) {
        return Some(IdleWaitReason::SaidWaiting);
    }
    says_it_carries_on(last).then_some(IdleWaitReason::SaidCarryingOn)
}

/// The handover markers a thread's final report opens its sections with (✅ 🚀 📣 🛫 🔄 ⏸️ 📦
/// 🧪 ⚠️ 🛑 💡 ❓): a message with any of them is a report, not a turn that stopped mid-task.
const HANDOVER_MARKERS: [char; 12] = [
    '\u{2705}',
    '\u{1f680}',
    '\u{1f4e3}',
    '\u{1f6eb}',
    '\u{1f504}',
    '\u{23f8}',
    '\u{1f4e6}',
    '\u{1f9ea}',
    '\u{26a0}',
    '\u{1f6d1}',
    '\u{1f4a1}',
    '\u{2753}',
];

/// Whether a last message stops mid-task: it carries no handover marker and its last sentence
/// says what the agent does next ("Now opening the fold.", "Let me check the log:", "Next I'll
/// run the tests.").
pub fn says_it_carries_on(message: &str) -> bool {
    if message.contains(HANDOVER_MARKERS) {
        return false;
    }
    let Some(line) = message
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty())
    else {
        return false;
    };
    // Its last sentence: a line of several ends at its last full stop before the end.
    let body = line
        .trim_end_matches(['.', '!', ':', '\u{2026}'])
        .trim_end();
    let sentence = body
        .rfind(". ")
        .map(|at| &body[at + 2..])
        .unwrap_or(body)
        .trim()
        .trim_start_matches(['-', '*'])
        .trim()
        .to_lowercase();
    if sentence.starts_with("let me know") || sentence.contains('?') {
        return false;
    }
    // A colon at the very end announces the command or read that never came.
    if line.ends_with(':') {
        return true;
    }
    const STARTS: [&str; 14] = [
        "now ",
        "next ",
        "next,",
        "then ",
        "let me ",
        "let's ",
        "i'll ",
        "i will ",
        "i'm going to ",
        "i am going to ",
        "going to ",
        "continuing",
        "moving on",
        "on to ",
    ];
    STARTS.iter().any(|start| sentence.starts_with(start))
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
    // "I didn't start anything in the background" waits on nothing: a phrase counts only when the
    // words of its sentence before it hold no negation.
    PHRASES.iter().any(|phrase| {
        tail.match_indices(phrase).any(|(at, _)| {
            let before = &tail[..at];
            let sentence = before
                .rfind(['.', '!', '?', '\n'])
                .map_or(before, |end| &before[end + 1..]);
            let words: Vec<&str> = sentence
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|word| !word.is_empty())
                .collect();
            !words.iter().any(|word| {
                matches!(*word, "not" | "no" | "never" | "nothing" | "without")
                    || word.ends_with("n't")
            })
        })
    })
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
